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

use crate::icons::IconRegistry;

use super::palette::Palette;
use super::text::{FONT_FACES, MONO_NAME, SANS_NAME, SERIF_NAME};

/// The external icon-sprite href the web build emits in `<use>` elements.
const SPRITE_HREF: &str = "_wdoc/icons.svg#";

/// Parses wdoc SVG strings into positioned-ready usvg trees. Borrows the
/// [`IconRegistry`] so embedded diagram icons (which reference the shared
/// sprite) can be inlined at embed time, after their diagram has recorded its
/// icon usage.
pub(crate) struct SvgEmbedder<'a> {
    fontdb: Arc<fontdb::Database>,
    fg: String,
    style_sheet: Option<String>,
    icons: &'a IconRegistry,
}

impl<'a> SvgEmbedder<'a> {
    /// `user_css` is the document's own `class` rules (concatenated), so
    /// custom-coloured diagram shapes pick up their fills. `currentColor` in
    /// both the palette defaults and the user CSS resolves to the foreground.
    pub(crate) fn new(palette: &Palette, user_css: &str, icons: &'a IconRegistry) -> Self {
        let mut db = fontdb::Database::new();
        for bytes in FONT_FACES {
            db.load_font_data(bytes.to_vec());
        }
        db.set_serif_family(SERIF_NAME);
        db.set_sans_serif_family(SANS_NAME);
        db.set_monospace_family(MONO_NAME);
        let fg = palette.fg_hex();
        let mut sheet = palette.svg_style_sheet();
        sheet.push_str(user_css);
        let sheet = sheet.replace("currentColor", &fg);
        Self {
            fontdb: Arc::new(db),
            fg,
            style_sheet: Some(sheet),
            icons,
        }
    }

    /// Parse `svg` into a usvg tree and its intrinsic `(width, height)` in px.
    /// Returns `None` if the string holds no `<svg>` or fails to parse (e.g. a
    /// math-error marker).
    pub(crate) fn embed(&self, svg: &str) -> Option<(Tree, (f32, f32))> {
        let mut inner = extract_svg(svg)?;
        // Inline the icon sprite: splice the recorded `<symbol>`s into the SVG's
        // own defs and rewrite `<use href="_wdoc/icons.svg#id">` to `#id` so
        // usvg resolves them locally (there is no sprite file for PDF).
        if inner.contains(SPRITE_HREF)
            && let Some(defs) = self.icons.symbol_defs()
        {
            inner = splice_defs(&inner, &defs);
            inner = inner.replace(SPRITE_HREF, "#");
        }
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

/// Insert `<defs>{defs}</defs>` immediately after the opening `<svg …>` tag.
fn splice_defs(svg: &str, defs: &str) -> String {
    if let Some(start) = svg.find("<svg")
        && let Some(gt) = svg[start..].find('>')
    {
        let pos = start + gt + 1;
        return format!("{}<defs>{defs}</defs>{}", &svg[..pos], &svg[pos..]);
    }
    svg.to_string()
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
