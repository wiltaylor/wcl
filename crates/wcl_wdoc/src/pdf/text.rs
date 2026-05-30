//! Font management and the cosmic-text → krilla glyph bridge.
//!
//! [`FontBook`] owns a `cosmic_text::FontSystem` holding only the bundled Noto
//! faces (no system fonts, for deterministic output) plus a cache of the
//! matching `krilla` [`Font`]s. Because both the shaper and krilla parse the
//! *same* embedded font bytes, the glyph ids cosmic-text produces line up with
//! the ids krilla expects — so shaping in cosmic-text and drawing in krilla
//! Just Work together.
//!
//! Shaping returns a [`ShapedParagraph`]: line-broken runs of positioned
//! glyphs, each already resolved to a krilla `Font`. Hyperlink membership is
//! threaded through cosmic-text's per-glyph `metadata` slot, so the layout pass
//! can recover which glyphs belong to which link (and build the PDF link
//! annotations) without re-walking the run tree.

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, Weight, Wrap, fontdb,
};
use krilla::text::Font;
use usvg::Tree;

use super::ir::{FontFamily, InlineRun, TextStyle};
use super::svg_embed::SvgEmbedder;

/// Metadata offset distinguishing an inline-object index from a link index in
/// cosmic-text's per-glyph `metadata` slot (links use `1..OBJECT_META_BASE`).
const OBJECT_META_BASE: usize = 1 << 24;

// The bundled body/heading/mono faces (OFL, see assets/fonts/OFL-Noto.txt).
// Both cosmic-text (via `fontdb::Source::Binary`) and krilla (via `Font::new`)
// read these exact static slices, which keeps their glyph ids in lockstep.
const SERIF_REGULAR: &[u8] = include_bytes!("../../assets/fonts/NotoSerif-Regular.ttf");
const SERIF_BOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSerif-Bold.ttf");
const SERIF_ITALIC: &[u8] = include_bytes!("../../assets/fonts/NotoSerif-Italic.ttf");
const SERIF_BOLD_ITALIC: &[u8] = include_bytes!("../../assets/fonts/NotoSerif-BoldItalic.ttf");
const SANS_REGULAR: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");
const SANS_BOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Bold.ttf");
const SANS_ITALIC: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Italic.ttf");
const SANS_BOLD_ITALIC: &[u8] = include_bytes!("../../assets/fonts/NotoSans-BoldItalic.ttf");
const MONO_REGULAR: &[u8] = include_bytes!("../../assets/fonts/NotoSansMono-Regular.ttf");

pub(crate) const SERIF_NAME: &str = "Noto Serif";
pub(crate) const SANS_NAME: &str = "Noto Sans";
pub(crate) const MONO_NAME: &str = "Noto Sans Mono";

/// All bundled faces, shared with [`super::svg_embed`] so embedded SVG `<text>`
/// (diagram/timeline labels) shapes with the same fonts as native prose.
pub(crate) const FONT_FACES: [&[u8]; 9] = [
    SERIF_REGULAR,
    SERIF_BOLD,
    SERIF_ITALIC,
    SERIF_BOLD_ITALIC,
    SANS_REGULAR,
    SANS_BOLD,
    SANS_ITALIC,
    SANS_BOLD_ITALIC,
    MONO_REGULAR,
];

/// A single positioned glyph, resolved to a krilla font.
pub(crate) struct ShapedGlyph {
    pub font: Font,
    pub glyph_id: u16,
    /// Pen x relative to the paragraph's left edge.
    pub x: f32,
    /// Vertical offset from the line baseline (positive = down).
    pub dy: f32,
    /// Index into [`ShapedParagraph::hrefs`] when this glyph is part of a link.
    pub link: Option<usize>,
    /// Index into [`ShapedParagraph::objects`] when this glyph is an inline-SVG
    /// placeholder.
    pub obj: Option<usize>,
    /// The text covered by this glyph's cluster, for the PDF `ToUnicode` map.
    pub cluster: String,
}

/// An inline SVG object (icon / equation) to overlay at a placeholder glyph.
#[derive(Clone)]
pub(crate) struct InlineObject {
    pub tree: Tree,
    pub w: f32,
    pub h: f32,
}

/// One visual (line-broken) line of shaped glyphs.
pub(crate) struct ShapedLine {
    /// Distance from the line top to its baseline.
    pub ascent: f32,
    /// Full line advance (top of this line to top of the next).
    pub height: f32,
    /// Painted width of the line (advance of its glyphs).
    pub width: f32,
    pub glyphs: Vec<ShapedGlyph>,
}

/// A shaped, line-broken paragraph.
pub(crate) struct ShapedParagraph {
    pub lines: Vec<ShapedLine>,
    /// Resolved hrefs, indexed by [`ShapedGlyph::link`].
    pub hrefs: Vec<String>,
    /// Inline SVG objects, indexed by [`ShapedGlyph::obj`].
    pub objects: Vec<InlineObject>,
}

/// Owns the shaper and the krilla font cache.
pub(crate) struct FontBook {
    fs: FontSystem,
    cache: HashMap<fontdb::ID, Font>,
}

impl FontBook {
    pub(crate) fn new() -> Self {
        let sources = [
            SERIF_REGULAR,
            SERIF_BOLD,
            SERIF_ITALIC,
            SERIF_BOLD_ITALIC,
            SANS_REGULAR,
            SANS_BOLD,
            SANS_ITALIC,
            SANS_BOLD_ITALIC,
            MONO_REGULAR,
        ]
        .into_iter()
        .map(|bytes| fontdb::Source::Binary(Arc::new(bytes)));
        // `new_with_fonts` does *not* load system fonts — only what we pass.
        let fs = FontSystem::new_with_fonts(sources);
        Self {
            fs,
            cache: HashMap::new(),
        }
    }

    /// Shape `runs` into a paragraph wrapped to `width`, at `size` points with
    /// the given line-height multiple. Inline-SVG objects are embedded via
    /// `embedder` and reserved as placeholder space in the flow.
    pub(crate) fn shape_paragraph(
        &mut self,
        runs: &[InlineRun],
        width: f32,
        size: f32,
        line_height: f32,
        embedder: &SvgEmbedder,
    ) -> ShapedParagraph {
        self.shape(runs, Some(width), true, size, line_height, Some(embedder))
    }

    /// Shape a single short string with no wrapping (header / footer chrome,
    /// list markers, code spans) — no inline objects.
    pub(crate) fn shape_label(&mut self, text: &str, style: TextStyle, size: f32) -> ShapedLine {
        let runs = [InlineRun::Text {
            text: text.to_string(),
            style,
        }];
        let mut p = self.shape(&runs, None, false, size, 1.2, None);
        p.lines.pop().unwrap_or(ShapedLine {
            ascent: size,
            height: size * 1.2,
            width: 0.0,
            glyphs: Vec::new(),
        })
    }

    fn shape(
        &mut self,
        runs: &[InlineRun],
        width: Option<f32>,
        wrap: bool,
        size: f32,
        line_height: f32,
        embedder: Option<&SvgEmbedder>,
    ) -> ShapedParagraph {
        let (leaves, hrefs, objects) = flatten(runs, embedder, size);

        let metrics = Metrics::relative(size, line_height);
        let mut buffer = Buffer::new(&mut self.fs, metrics);
        let mut b = buffer.borrow_with(&mut self.fs);
        b.set_wrap(if wrap { Wrap::Word } else { Wrap::None });
        b.set_size(width, None);
        let default = attrs_for(TextStyle::body());
        let spans: Vec<(&str, Attrs)> = leaves
            .iter()
            .map(|leaf| {
                let mut attrs = attrs_for(leaf.style);
                // metadata 0 = none; objects use OBJECT_META_BASE+idx; links
                // use idx+1 (below OBJECT_META_BASE).
                attrs.metadata = match (leaf.obj, leaf.link) {
                    (Some(o), _) => OBJECT_META_BASE + o,
                    (None, Some(l)) => l + 1,
                    (None, None) => 0,
                };
                (leaf.text.as_str(), attrs)
            })
            .collect();
        b.set_rich_text(spans, &default, Shaping::Advanced, None);

        // Collect the raw glyph geometry first; resolving krilla fonts borrows
        // `self.fs` again, which would conflict with the `borrow_with` above.
        let mut raw_lines: Vec<RawLine> = Vec::new();
        for run in b.layout_runs() {
            let glyphs = run
                .glyphs
                .iter()
                .map(|g| {
                    let (link, obj) = decode_meta(g.metadata);
                    RawGlyph {
                        font_id: g.font_id,
                        glyph_id: g.glyph_id,
                        x: g.x + g.x_offset,
                        dy: g.y_offset,
                        link,
                        obj,
                        cluster: run.text.get(g.start..g.end).unwrap_or("").to_string(),
                    }
                })
                .collect();
            raw_lines.push(RawLine {
                ascent: run.line_y - run.line_top,
                height: run.line_height,
                width: run.line_w,
                glyphs,
            });
        }
        drop(buffer);

        let lines = raw_lines
            .into_iter()
            .map(|rl| ShapedLine {
                ascent: rl.ascent,
                height: rl.height,
                width: rl.width,
                glyphs: rl
                    .glyphs
                    .into_iter()
                    .filter_map(|rg| {
                        Some(ShapedGlyph {
                            font: self.krilla_font(rg.font_id)?,
                            glyph_id: rg.glyph_id,
                            x: rg.x,
                            dy: rg.dy,
                            link: rg.link,
                            obj: rg.obj,
                            cluster: rg.cluster,
                        })
                    })
                    .collect(),
            })
            .collect();
        ShapedParagraph {
            lines,
            hrefs,
            objects,
        }
    }

    /// Build (and cache) the krilla `Font` for a fontdb face from its bytes.
    fn krilla_font(&mut self, id: fontdb::ID) -> Option<Font> {
        if let Some(f) = self.cache.get(&id) {
            return Some(f.clone());
        }
        let font = self
            .fs
            .db()
            .with_face_data(id, |data, index| Font::new(data.to_vec().into(), index))
            .flatten()?;
        self.cache.insert(id, font.clone());
        Some(font)
    }
}

fn attrs_for(style: TextStyle) -> Attrs<'static> {
    let family = match style.family {
        FontFamily::Serif => Family::Name(SERIF_NAME),
        FontFamily::Sans => Family::Name(SANS_NAME),
        FontFamily::Mono => Family::Name(MONO_NAME),
    };
    Attrs::new()
        .family(family)
        .weight(if style.bold {
            Weight::BOLD
        } else {
            Weight::NORMAL
        })
        .style(if style.italic {
            Style::Italic
        } else {
            Style::Normal
        })
}

/// A flattened inline leaf: contiguous text in one style, optionally inside a
/// link, or a placeholder reserving space for an inline object.
struct Leaf {
    text: String,
    style: TextStyle,
    link: Option<usize>,
    obj: Option<usize>,
}

/// Decode a cosmic-text glyph `metadata` slot into (link, object) indices.
fn decode_meta(meta: usize) -> (Option<usize>, Option<usize>) {
    if meta >= OBJECT_META_BASE {
        (None, Some(meta - OBJECT_META_BASE))
    } else if meta > 0 {
        (Some(meta - 1), None)
    } else {
        (None, None)
    }
}

/// Flatten the [`InlineRun`] tree into styled leaves, the link href table, and
/// the embedded inline objects. Each object becomes a run of placeholder
/// spaces sized to its embedded width (rounded to the space advance), tagged so
/// the layout pass can overlay the SVG.
fn flatten(
    runs: &[InlineRun],
    embedder: Option<&SvgEmbedder>,
    size: f32,
) -> (Vec<Leaf>, Vec<String>, Vec<InlineObject>) {
    let mut ctx = FlattenCtx {
        leaves: Vec::new(),
        hrefs: Vec::new(),
        objects: Vec::new(),
        embedder,
        size,
    };
    ctx.run(runs, None);
    (ctx.leaves, ctx.hrefs, ctx.objects)
}

struct FlattenCtx<'a> {
    leaves: Vec<Leaf>,
    hrefs: Vec<String>,
    objects: Vec<InlineObject>,
    embedder: Option<&'a SvgEmbedder>,
    size: f32,
}

impl FlattenCtx<'_> {
    fn run(&mut self, runs: &[InlineRun], cur_link: Option<usize>) {
        for run in runs {
            match run {
                InlineRun::Text { text, style } => self.leaves.push(Leaf {
                    text: text.clone(),
                    style: *style,
                    link: cur_link,
                    obj: None,
                }),
                InlineRun::Link { runs: inner, href } => {
                    let idx = self.hrefs.len();
                    self.hrefs.push(href.clone());
                    self.run(inner, Some(idx));
                }
                InlineRun::Object { svg } => self.object(svg, cur_link),
            }
        }
    }

    fn object(&mut self, svg: &str, cur_link: Option<usize>) {
        let Some(embedder) = self.embedder else {
            return;
        };
        let Some((tree, (tw, th))) = embedder.embed(svg) else {
            return;
        };
        if tw <= 0.0 || th <= 0.0 {
            return;
        }
        // Size the object to one em tall, preserving aspect ratio.
        let h = self.size;
        let w = tw * (h / th);
        let idx = self.objects.len();
        self.objects.push(InlineObject { tree, w, h });
        // Reserve roughly `w` of horizontal space with placeholder spaces (a
        // space advance is ~0.25em); a leading hair-space pads the icon.
        let space_w = (self.size * 0.25).max(1.0);
        let n = ((w / space_w).round() as usize).max(1) + 1;
        self.leaves.push(Leaf {
            text: " ".repeat(n),
            style: TextStyle::body(),
            link: cur_link,
            obj: Some(idx),
        });
    }
}

struct RawGlyph {
    font_id: fontdb::ID,
    glyph_id: u16,
    x: f32,
    dy: f32,
    link: Option<usize>,
    obj: Option<usize>,
    cluster: String,
}

struct RawLine {
    ascent: f32,
    height: f32,
    width: f32,
    glyphs: Vec<RawGlyph>,
}
