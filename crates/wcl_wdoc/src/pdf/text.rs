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

use super::ir::{FontFamily, InlineRun, TextStyle};

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

const SERIF_NAME: &str = "Noto Serif";
const SANS_NAME: &str = "Noto Sans";
const MONO_NAME: &str = "Noto Sans Mono";

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
    /// The text covered by this glyph's cluster, for the PDF `ToUnicode` map.
    pub cluster: String,
}

/// One visual (line-broken) line of shaped glyphs.
pub(crate) struct ShapedLine {
    /// Distance from the line top to its baseline.
    pub ascent: f32,
    /// Full line advance (top of this line to top of the next).
    pub height: f32,
    pub glyphs: Vec<ShapedGlyph>,
}

/// A shaped, line-broken paragraph.
pub(crate) struct ShapedParagraph {
    pub lines: Vec<ShapedLine>,
    /// Resolved hrefs, indexed by [`ShapedGlyph::link`].
    pub hrefs: Vec<String>,
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
    /// the given line-height multiple.
    pub(crate) fn shape_paragraph(
        &mut self,
        runs: &[InlineRun],
        width: f32,
        size: f32,
        line_height: f32,
    ) -> ShapedParagraph {
        self.shape(runs, Some(width), true, size, line_height)
    }

    /// Shape a single short string with no wrapping (header / footer chrome).
    pub(crate) fn shape_label(&mut self, text: &str, style: TextStyle, size: f32) -> ShapedLine {
        let runs = [InlineRun::Text {
            text: text.to_string(),
            style,
        }];
        let mut p = self.shape(&runs, None, false, size, 1.2);
        p.lines.pop().unwrap_or(ShapedLine {
            ascent: size,
            height: size * 1.2,
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
    ) -> ShapedParagraph {
        let (leaves, hrefs) = flatten(runs);

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
                // metadata 0 means "no link"; link indices are stored +1.
                attrs.metadata = leaf.link.map_or(0, |i| i + 1);
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
                .map(|g| RawGlyph {
                    font_id: g.font_id,
                    glyph_id: g.glyph_id,
                    x: g.x + g.x_offset,
                    dy: g.y_offset,
                    link: g.metadata.checked_sub(1),
                    cluster: run.text.get(g.start..g.end).unwrap_or("").to_string(),
                })
                .collect();
            raw_lines.push(RawLine {
                ascent: run.line_y - run.line_top,
                height: run.line_height,
                glyphs,
            });
        }
        drop(buffer);

        let lines = raw_lines
            .into_iter()
            .map(|rl| ShapedLine {
                ascent: rl.ascent,
                height: rl.height,
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
                            cluster: rg.cluster,
                        })
                    })
                    .collect(),
            })
            .collect();
        ShapedParagraph { lines, hrefs }
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
/// link.
struct Leaf {
    text: String,
    style: TextStyle,
    link: Option<usize>,
}

/// Flatten the [`InlineRun`] tree into a list of styled leaves plus the link
/// href table they reference.
fn flatten(runs: &[InlineRun]) -> (Vec<Leaf>, Vec<String>) {
    let mut leaves = Vec::new();
    let mut hrefs = Vec::new();
    flatten_into(runs, None, &mut leaves, &mut hrefs);
    (leaves, hrefs)
}

fn flatten_into(
    runs: &[InlineRun],
    cur_link: Option<usize>,
    leaves: &mut Vec<Leaf>,
    hrefs: &mut Vec<String>,
) {
    for run in runs {
        match run {
            InlineRun::Text { text, style } => leaves.push(Leaf {
                text: text.clone(),
                style: *style,
                link: cur_link,
            }),
            InlineRun::Link { runs: inner, href } => {
                let idx = hrefs.len();
                hrefs.push(href.clone());
                flatten_into(inner, Some(idx), leaves, hrefs);
            }
        }
    }
}

struct RawGlyph {
    font_id: fontdb::ID,
    glyph_id: u16,
    x: f32,
    dy: f32,
    link: Option<usize>,
    cluster: String,
}

struct RawLine {
    ascent: f32,
    height: f32,
    glyphs: Vec<RawGlyph>,
}
