//! Embed wdoc SVG output (diagrams, charts, timelines, equations) into the PDF
//! via krilla-svg, vector-preserving.
//!
//! wdoc's SVG is built for the browser: strokes/text use `currentColor` (themed
//! by CSS), and chart series carry CSS classes with no inline fill. usvg
//! resolves neither against an external stylesheet, so before parsing we
//! substitute `currentColor` with the concrete foreground colour and hand usvg
//! a `style_sheet` carrying the class fills (chart palette). `<text>` labels
//! shape against the same bundled Noto fonts as native prose.

use std::cell::RefCell;
use std::sync::Arc;

use usvg::{Options, Tree, fontdb};

use crate::icons::IconRegistry;
use crate::image::ImageRegistry;
use crate::tileset::TilesetRegistry;

use super::palette::Palette;
use super::text::{FONT_FACES, SANS_NAME, SERIF_NAME};

/// The external icon-sprite href the web build emits in `<use>` elements.
const SPRITE_HREF: &str = "_wdoc/icons.svg#";

thread_local! {
    /// First usvg parse failure recorded during the current PDF pass.
    /// Every internal SVG producer emits well-formed SVG, so a parse
    /// failure means a renderer regression or a corrupt embedded asset
    /// — a PDF silently missing a diagram is data loss, so the entry
    /// point turns this into a hard error after the pass. First one
    /// wins (mirrors the render sinks in `render::lower`).
    static EMBED_ERR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Record an SVG-embed failure. First message wins; cleared by
/// [`take_embed_error`].
fn record_embed_error(msg: String) {
    EMBED_ERR.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(msg);
        }
    });
}

/// Take and clear the first embed error recorded during the current pass.
pub(crate) fn take_embed_error() -> Option<String> {
    EMBED_ERR.with(|slot| slot.borrow_mut().take())
}

// JetBrains Mono Nerd Font (Mono variant) TTFs, for embedded-SVG monospace —
// chiefly terminals, whose grid uses Nerd Font box-drawing / powerline / icon
// glyphs that the plain Noto mono lacks. The web terminal ships the woff2 form;
// usvg/fontdb need raw sfnt. Loaded only into the SVG embed fontdb (native code
// keeps Noto mono).
const NERD_REGULAR: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");
const NERD_BOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf");
const NERD_ITALIC: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf");
/// The Nerd Font's internal family name (shared with the terminal renderer,
/// which names it on every cell `<text>`). Registered as the fontdb monospace
/// family too, so even a bare `monospace` request resolves to the real Nerd
/// Font.
const NERD_FAMILY: &str = crate::terminal::NERD_FONT_FAMILY;

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
    /// Fill / stroke for diagram card boxes (the native replacement for each
    /// dropped `<foreignObject>`), themed from the site's light palette.
    card_fill: String,
    card_stroke: String,
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
        card_fill: String,
        card_stroke: String,
    ) -> Self {
        let mut db = fontdb::Database::new();
        for bytes in FONT_FACES {
            db.load_font_data(bytes.to_vec());
        }
        for bytes in [NERD_REGULAR, NERD_BOLD, NERD_ITALIC] {
            db.load_font_data(bytes.to_vec());
        }
        db.set_serif_family(SERIF_NAME);
        db.set_sans_serif_family(SANS_NAME);
        // Terminal text resolves through the generic `monospace` fallback.
        db.set_monospace_family(NERD_FAMILY);
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
            card_fill,
            card_stroke,
        }
    }

    /// Parse `svg` into a usvg tree and its intrinsic `(width, height)` in px.
    /// Returns `None` if the string holds no `<svg>` at all (benign — e.g. a
    /// math-error marker). A string that *does* carry an `<svg>` but fails to
    /// parse records a fatal embed error (see [`take_embed_error`]) and
    /// returns `None` so the layout can skip it before the build fails.
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
            inner = replace_foreign_objects(&inner, &self.card_fill, &self.card_stroke);
        }
        // Inline `<image href="_wdoc/…">` as data URIs (the copied asset files
        // don't exist for PDF; usvg's default resolver decodes `data:`).
        if inner.contains("<image") {
            inner = self.inline_images(&inner);
        }
        // A diagram with no explicit `width`/`height` emits
        // `width="0" height="0"` by convention (the browser sizes it from
        // CSS + viewBox), which usvg rejects as an invalid size — and the
        // diagram used to vanish from the PDF silently. Rewrite the root
        // dimensions from the viewBox so the embed sees real geometry.
        let inner = fix_zero_size(inner);
        let prepared = inner.replace("currentColor", &self.fg);
        let opt = Options {
            fontdb: self.fontdb.clone(),
            style_sheet: self.style_sheet.clone(),
            ..Options::default()
        };
        let tree = match Tree::from_str(&prepared, &opt) {
            Ok(t) => t,
            Err(e) => {
                record_embed_error(format!(
                    "embedding an SVG into the PDF failed: {e} (svg starts: {})",
                    prepared.chars().take(80).collect::<String>()
                ));
                return None;
            }
        };
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
/// (which usvg ignores) with just a native rounded card **box**. The card body
/// is painted natively on top by the PDF layout/paint pass (see `collect.rs`'s
/// diagram card extraction), so the box here carries no text.
fn replace_foreign_objects(svg: &str, fill: &str, stroke: &str) -> String {
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
        let x = attr_f32(open_tag, "x").unwrap_or(0.0);
        let y = attr_f32(open_tag, "y").unwrap_or(0.0);
        let w = attr_f32(open_tag, "width").unwrap_or(0.0);
        let h = attr_f32(open_tag, "height").unwrap_or(0.0);
        out.push_str(&card_box(x, y, w, h, fill, stroke));
        rest = &rest[body_start + close_rel + CLOSE.len()..];
    }
    out.push_str(rest);
    out
}

/// A rounded card box (no content — the body is painted natively over it).
/// `fill`/`stroke` come from the site theme's light palette (bg-alt / border),
/// matching the web `.wdoc-card`.
fn card_box(x: f32, y: f32, w: f32, h: f32, fill: &str, stroke: &str) -> String {
    format!(
        "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"4\" \
         fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"1\"/>"
    )
}

/// Read a numeric SVG attribute (`name="12.5"`) from an opening tag.
fn attr_f32(tag: &str, name: &str) -> Option<f32> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    tag[start..end].trim().parse().ok()
}

/// Parse an SVG `viewBox="minX minY width height"` into floats. Diagram cards'
/// positions are in this coordinate space.
/// Rewrite a root `width="0" height="0"` (wdoc's "size me from CSS"
/// convention for diagrams without explicit dimensions) to the viewBox's
/// width/height, which is what the browser effectively renders. Leaves
/// any other markup untouched; without a parseable viewBox the SVG passes
/// through unchanged (and usvg then reports the invalid size loudly).
fn fix_zero_size(svg: String) -> String {
    let Some(tag_end) = svg.find('>') else {
        return svg;
    };
    let root = &svg[..tag_end];
    if !root.contains("width=\"0\"") || !root.contains("height=\"0\"") {
        return svg;
    }
    let Some((_, _, vbw, vbh)) = parse_viewbox(root) else {
        return svg;
    };
    if vbw <= 0.0 || vbh <= 0.0 {
        return svg;
    }
    let fixed_root = root
        .replacen("width=\"0\"", &format!("width=\"{vbw}\""), 1)
        .replacen("height=\"0\"", &format!("height=\"{vbh}\""), 1);
    format!("{fixed_root}{}", &svg[tag_end..])
}

pub(crate) fn parse_viewbox(svg: &str) -> Option<(f32, f32, f32, f32)> {
    let key = "viewBox=\"";
    let start = svg.find(key)? + key.len();
    let end = svg[start..].find('"')? + start;
    let mut it = svg[start..end].split_whitespace();
    let min_x = it.next()?.parse().ok()?;
    let min_y = it.next()?.parse().ok()?;
    let w = it.next()?.parse().ok()?;
    let h = it.next()?.parse().ok()?;
    Some((min_x, min_y, w, h))
}

/// Each `<foreignObject>`'s `(x, y, width, height)` in document order — the card
/// boxes, in the same render order as the diagram's card blocks.
pub(crate) fn card_rects(svg: &str) -> Vec<(f32, f32, f32, f32)> {
    let mut rects = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<foreignObject") {
        let after = &rest[start..];
        let Some(tag_end) = after.find('>') else {
            break;
        };
        let open_tag = &after[..tag_end];
        rects.push((
            attr_f32(open_tag, "x").unwrap_or(0.0),
            attr_f32(open_tag, "y").unwrap_or(0.0),
            attr_f32(open_tag, "width").unwrap_or(0.0),
            attr_f32(open_tag, "height").unwrap_or(0.0),
        ));
        rest = &after[tag_end + 1..];
    }
    rects
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

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_lang::Document;

    /// Build an embedder over empty registries (no icons / images /
    /// tilesets declared) — enough to exercise the parse path.
    fn with_embedder(f: impl FnOnce(&SvgEmbedder)) {
        let doc = Document::open("", "test.wcl").expect("empty doc parses");
        let icons = IconRegistry::load(&doc);
        let images = ImageRegistry::new(None);
        let Ok(tilesets) = TilesetRegistry::load(&doc, None) else {
            panic!("empty doc declares no tilesets");
        };
        let palette = Palette::default();
        let embedder = SvgEmbedder::new(
            &palette,
            "",
            &icons,
            &images,
            &tilesets,
            "#ffffff".to_string(),
            "#cccccc".to_string(),
        );
        f(&embedder);
    }

    #[test]
    fn malformed_svg_records_an_embed_error() {
        with_embedder(|embedder| {
            let _ = take_embed_error();
            assert!(embedder.embed("<svg><unclosed</svg>").is_none());
            let err = take_embed_error().expect("embed error recorded");
            assert!(
                err.contains("embedding an SVG into the PDF failed"),
                "{err}"
            );
            // Well-formed input parses and leaves the slot empty.
            assert!(
                embedder
                    .embed("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"></svg>")
                    .is_some()
            );
            assert!(take_embed_error().is_none());
        });
    }

    #[test]
    fn no_svg_at_all_is_benign() {
        with_embedder(|embedder| {
            let _ = take_embed_error();
            assert!(embedder.embed("math error: bad input").is_none());
            assert!(
                take_embed_error().is_none(),
                "no error for a non-SVG marker"
            );
        });
    }

    #[test]
    fn zero_size_root_resizes_from_the_viewbox() {
        // wdoc's CSS-sized diagram convention: width/height 0 + viewBox.
        let fixed = fix_zero_size(
            "<svg xmlns=\"x\" width=\"0\" height=\"0\" viewBox=\"-10 -10 200 60\"></svg>"
                .to_string(),
        );
        assert!(fixed.contains("width=\"200\""), "{fixed}");
        assert!(fixed.contains("height=\"60\""), "{fixed}");
        // Real dimensions pass through untouched.
        let kept = "<svg width=\"32\" height=\"16\" viewBox=\"0 0 32 16\"></svg>".to_string();
        assert_eq!(fix_zero_size(kept.clone()), kept);
    }
}
