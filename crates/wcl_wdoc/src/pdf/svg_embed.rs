//! Embed wdoc SVG output (diagrams, charts, timelines, equations) into the PDF
//! via krilla-svg, vector-preserving.
//!
//! wdoc's SVG is built for the browser: strokes/text use `currentColor` (themed
//! by CSS), and chart series carry CSS classes with no inline fill. usvg
//! resolves neither against an external stylesheet, so before parsing we
//! substitute `currentColor` with the concrete foreground colour and hand usvg
//! a `style_sheet` carrying the class fills (chart palette). `<text>` labels
//! shape against the same bundled Noto fonts as native prose.

use std::fmt::Write as _;
use std::sync::Arc;

use usvg::{Options, Tree, fontdb};

use crate::icons::IconRegistry;
use crate::image::ImageRegistry;
use crate::tileset::TilesetRegistry;

use super::palette::Palette;
use super::text::{FONT_FACES, MONO_NAME, SANS_NAME, SERIF_NAME};

/// The external icon-sprite href the web build emits in `<use>` elements.
const SPRITE_HREF: &str = "_wdoc/icons.svg#";

/// Parses wdoc SVG strings into positioned-ready usvg trees. Borrows the
/// icon / image / tileset registries so embedded diagram icons (sprite `<use>`)
/// and images (`<image href="_wdoc/…">`) can be inlined at embed time, after
/// their diagram has recorded its usage.
pub(crate) struct SvgEmbedder<'a> {
    fontdb: Arc<fontdb::Database>,
    fg: String,
    style_sheet: Option<String>,
    icons: &'a IconRegistry,
    images: &'a ImageRegistry,
    tilesets: &'a TilesetRegistry,
}

impl<'a> SvgEmbedder<'a> {
    /// `user_css` is the document's own `class` rules (concatenated), so
    /// custom-coloured diagram shapes pick up their fills. `currentColor` in
    /// both the palette defaults and the user CSS resolves to the foreground.
    pub(crate) fn new(
        palette: &Palette,
        user_css: &str,
        icons: &'a IconRegistry,
        images: &'a ImageRegistry,
        tilesets: &'a TilesetRegistry,
    ) -> Self {
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
            images,
            tilesets,
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
        // usvg drops `<foreignObject>` (used by cards / timeline event cards /
        // map pins), so convert each to a native SVG box + wrapped text.
        if inner.contains("<foreignObject") {
            inner = replace_foreign_objects(&inner);
        }
        // Inline `<image href="_wdoc/…">` as data URIs (the copied asset files
        // don't exist for PDF; usvg's default resolver decodes `data:`).
        if inner.contains("<image") {
            inner = self.inline_images(&inner);
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

impl SvgEmbedder<'_> {
    /// Rewrite every `href="…"` / `xlink:href="…"` that names a copied asset
    /// (`_wdoc/…`) into a `data:` URI. Splitting on `href="` also catches the
    /// `xlink:` form (the preceding text keeps its `xlink:` prefix).
    fn inline_images(&self, svg: &str) -> String {
        let parts: Vec<&str> = svg.split("href=\"").collect();
        let mut out = String::with_capacity(svg.len());
        out.push_str(parts[0]);
        for seg in &parts[1..] {
            out.push_str("href=\"");
            match seg.split_once('"') {
                Some((value, rest)) => {
                    match self.image_data_uri(value) {
                        Some(uri) => out.push_str(&uri),
                        None => out.push_str(value),
                    }
                    out.push('"');
                    out.push_str(rest);
                }
                None => out.push_str(seg),
            }
        }
        out
    }

    /// A `data:` URI for an asset href, or `None` to leave it unchanged.
    fn image_data_uri(&self, href: &str) -> Option<String> {
        if href.starts_with('#') || href.starts_with("data:") || href.contains("://") {
            return None;
        }
        let bytes = self
            .images
            .bytes_for_url(href)
            .or_else(|| self.tilesets.bytes_for_url(href))?;
        let mime = image_mime(&bytes)?;
        Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
    }
}

/// Standard base64 encoding (no dependency).
fn base64_encode(data: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        s.push(B64[((n >> 18) & 63) as usize] as char);
        s.push(B64[((n >> 12) & 63) as usize] as char);
        s.push(if chunk.len() > 1 {
            B64[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        s.push(if chunk.len() > 2 {
            B64[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    s
}

/// Detect a raster image's MIME type from its magic bytes.
fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Replace every `<foreignObject x y width height>…XHTML…</foreignObject>`
/// (which usvg ignores) with a native rounded card box plus the inner text,
/// word-wrapped. Rich formatting (lists, nested blocks) is flattened to text —
/// a readable snapshot rather than a blank box.
fn replace_foreign_objects(svg: &str) -> String {
    const OPEN: &str = "<foreignObject";
    const CLOSE: &str = "</foreignObject>";
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(tag_end) = after.find('>') else {
            out.push_str(after);
            return out;
        };
        let open_tag = &after[..tag_end];
        let body_start = start + tag_end + 1;
        let Some(close_rel) = rest[body_start..].find(CLOSE) else {
            out.push_str(&rest[start..]);
            return out;
        };
        let body = &rest[body_start..body_start + close_rel];
        let x = attr_f32(open_tag, "x").unwrap_or(0.0);
        let y = attr_f32(open_tag, "y").unwrap_or(0.0);
        let w = attr_f32(open_tag, "width").unwrap_or(0.0);
        let h = attr_f32(open_tag, "height").unwrap_or(0.0);
        out.push_str(&card_svg(x, y, w, h, &strip_tags(body)));
        rest = &rest[body_start + close_rel + CLOSE.len()..];
    }
    out.push_str(rest);
    out
}

/// Read a numeric SVG attribute (`name="12.5"`) from an opening tag.
fn attr_f32(tag: &str, name: &str) -> Option<f32> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    tag[start..end].trim().parse().ok()
}

/// Strip HTML tags to plain text, mapping block-ish tags to spaces and decoding
/// the common entities the inline engine emits.
fn strip_tags(html: &str) -> String {
    let mut s = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                s.push(' ');
            }
            _ if in_tag => {}
            _ => s.push(ch),
        }
    }
    let s = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Emit a native card box + word-wrapped text for one former `<foreignObject>`.
fn card_svg(x: f32, y: f32, w: f32, h: f32, text: &str) -> String {
    const PAD: f32 = 6.0;
    const FS: f32 = 11.0;
    const LH: f32 = 14.0;
    let mut out = format!(
        "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"4\" \
         fill=\"#ffffff\" stroke=\"#cccccc\" stroke-width=\"1\"/>"
    );
    if text.is_empty() || w <= 2.0 * PAD {
        return out;
    }
    let max_chars = ((w - 2.0 * PAD) / (FS * 0.5)).floor().max(1.0) as usize;
    let tx = x + PAD;
    let mut ty = y + PAD + FS;
    let _ = write!(
        out,
        "<text x=\"{tx}\" font-family=\"sans-serif\" font-size=\"{FS}\" fill=\"#1a1a1a\">"
    );
    for line in wrap_text(text, max_chars) {
        if ty > y + h - 2.0 {
            break;
        }
        let _ = write!(
            out,
            "<tspan x=\"{tx}\" y=\"{ty}\">{}</tspan>",
            xml_escape(&line)
        );
        ty += LH;
    }
    out.push_str("</text>");
    out
}

/// Greedy word-wrap to at most `max_chars` per line.
fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur.len() + 1 + word.len() <= max_chars {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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
