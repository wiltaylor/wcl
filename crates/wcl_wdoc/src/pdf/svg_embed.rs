//! Embed wdoc SVG output (diagrams, charts, timelines, equations) into the PDF
//! via krilla-svg, vector-preserving.
//!
//! wdoc's SVG is built for the browser: strokes/text use `currentColor` (themed
//! by CSS), and chart series carry CSS classes with no inline fill. usvg
//! resolves neither against an external stylesheet, so before parsing we
//! substitute `currentColor` with the concrete foreground colour and hand usvg
//! a `style_sheet` carrying the class fills (chart palette). `<text>` labels
//! shape against the same bundled Noto fonts as native prose.

use std::sync::Arc;

use usvg::{Options, Tree, fontdb};

use super::palette::Palette;
use super::text::{FONT_FACES, MONO_NAME, SANS_NAME, SERIF_NAME};

/// Parses wdoc SVG strings into positioned-ready usvg trees.
pub(crate) struct SvgEmbedder {
    fontdb: Arc<fontdb::Database>,
    fg: String,
    style_sheet: Option<String>,
}

impl SvgEmbedder {
    pub(crate) fn new(palette: &Palette) -> Self {
        let mut db = fontdb::Database::new();
        for bytes in FONT_FACES {
            db.load_font_data(bytes.to_vec());
        }
        db.set_serif_family(SERIF_NAME);
        db.set_sans_serif_family(SANS_NAME);
        db.set_monospace_family(MONO_NAME);
        Self {
            fontdb: Arc::new(db),
            fg: palette.fg_hex(),
            style_sheet: Some(palette.svg_style_sheet()),
        }
    }

    /// Parse `svg` into a usvg tree and its intrinsic `(width, height)` in px.
    /// Returns `None` if the string holds no `<svg>` or fails to parse (e.g. a
    /// math-error marker).
    pub(crate) fn embed(&self, svg: &str) -> Option<(Tree, (f32, f32))> {
        let inner = extract_svg(svg)?;
        let prepared = inner.replace("currentColor", &self.fg);
        let opt = Options {
            fontdb: self.fontdb.clone(),
            style_sheet: self.style_sheet.clone(),
            ..Options::default()
        };
        let tree = Tree::from_str(&prepared, &opt).ok()?;
        let size = tree.size();
        Some((tree, (size.width(), size.height())))
    }
}

/// Extract the outermost balanced `<svg>…</svg>` from a render string, which may
/// be wrapped in a `<div>` (interactive diagrams) or `<div class="wdoc-math">`
/// (block equations) and may itself contain nested `<svg>` (tilemaps). Scans on
/// bytes; `<svg`/`</svg>` are ASCII so all slice indices land on char
/// boundaries.
fn extract_svg(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let start = find_from(b, b"<svg", 0)?;
    let mut depth = 0usize;
    let mut i = start;
    while i < b.len() {
        if b[i..].starts_with(b"<svg") {
            depth += 1;
            i += 4;
        } else if b[i..].starts_with(b"</svg>") {
            depth -= 1;
            i += 6;
            if depth == 0 {
                return Some(s[start..i].to_string());
            }
        } else {
            i += 1;
        }
    }
    None
}

fn find_from(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}
