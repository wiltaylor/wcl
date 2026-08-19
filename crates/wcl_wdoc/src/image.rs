//! The `image` block: a raster image embeddable on a page (`<img>`) and
//! as a placeable shape inside a diagram (SVG `<image>`).
//!
//! Like tilesets, a referenced local image is copied into `_wdoc/` and
//! referenced by relative URL, so it resolves when the output is *served*
//! (not via `file://`). The [`ImageRegistry`] is populated lazily on
//! reference (during rendering / the collect pass) rather than from
//! up-front declarations — every entry it holds was referenced, so
//! [`ImageRegistry::copy_used_images`] copies them all. An external
//! source (`http(s)://`, `data:`, or a leading `/`) passes through
//! verbatim and is never copied.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use wcl_lang::Block;

use crate::build::BuildError;
use crate::render::{escape_html, field_f64, field_id, field_utf8, field_utf8_list, label_string};

/// One resolved image reference.
#[derive(Clone)]
pub(crate) struct ImageEntry {
    /// The `src` / `href` to emit (a `_wdoc/…` URL for a copied local
    /// file, or the verbatim source for an external URL).
    pub url: String,
    /// Natural pixel dimensions, read from the file header. `None` for an
    /// external source or an unreadable / unrecognised file.
    pub dims: Option<(u32, u32)>,
    /// Output filename within `_wdoc/`; `None` ⇒ external (never copied).
    out_file: Option<String>,
    /// Source path on disk (`Some` for a local file to copy).
    src_path: Option<PathBuf>,
}

/// Lazily-populated registry of referenced images. Keyed by the raw
/// `source` string so repeat references share one copied file.
pub(crate) struct ImageRegistry {
    /// Directory relative sources resolve against. `None` when the
    /// document was opened without one.
    base_dir: Option<PathBuf>,
    /// Resolved entries by source, so one image referenced twice is read
    /// and copied once.
    entries: RefCell<BTreeMap<String, ImageEntry>>,
}

impl ImageRegistry {
    /// An empty registry resolving relative sources against `base_dir`.
    pub(crate) fn new(base_dir: Option<PathBuf>) -> Self {
        ImageRegistry {
            base_dir,
            entries: RefCell::new(BTreeMap::new()),
        }
    }

    /// Resolve `source`, recording it for copying (when local), and
    /// return its entry. Idempotent — the file header is read once.
    pub(crate) fn register(&self, source: &str) -> ImageEntry {
        if let Some(e) = self.entries.borrow().get(source) {
            return e.clone();
        }
        let entry = self.build_entry(source);
        self.entries
            .borrow_mut()
            .insert(source.to_string(), entry.clone());
        entry
    }

    /// The natural pixel dimensions of `source` (resolving + recording it
    /// if new). Used by the diagram bbox pass when `width`/`height` are
    /// omitted.
    pub(crate) fn dims(&self, source: &str) -> Option<(u32, u32)> {
        self.register(source).dims
    }

    /// Resolve an emitted `<image href>` (a `_wdoc/…` URL) back to its source
    /// file's raw bytes, for inlining into PDF-embedded SVG as a data URI.
    pub(crate) fn bytes_for_url(&self, href: &str) -> Option<Vec<u8>> {
        let entries = self.entries.borrow();
        let entry = entries.values().find(|e| e.url == href)?;
        std::fs::read(entry.src_path.as_ref()?).ok()
    }

    /// Resolve one image: read it, measure it, and decide its output
    /// path.
    fn build_entry(&self, source: &str) -> ImageEntry {
        if is_external(source) {
            return ImageEntry {
                url: source.to_string(),
                dims: None,
                out_file: None,
                src_path: None,
            };
        }
        let src_path = match &self.base_dir {
            Some(dir) => dir.join(source),
            None => PathBuf::from(source),
        };
        let dims = fs::read(&src_path)
            .ok()
            .and_then(|b| crate::tileset::image_dims(&b));
        let ext = src_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        let stem = src_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("img");
        // Deterministic + collision-free: a readable stem plus a hash of
        // the full source path (so two `logo.png`s in different dirs don't
        // clash).
        let out_file = format!("image-{}-{:08x}.{ext}", sanitize(stem), fnv1a(source));
        let url = format!("{}/{}", crate::terminal::ASSET_DIR, out_file);
        ImageEntry {
            url,
            dims,
            out_file: Some(out_file),
            src_path: Some(src_path),
        }
    }

    /// Copy every referenced local image into `<out>/_wdoc/`. No-op when
    /// no local image was referenced.
    pub(crate) fn copy_used_images(&self, out_dir: &Path) -> Result<(), BuildError> {
        let entries = self.entries.borrow();
        if entries.values().all(|e| e.out_file.is_none()) {
            return Ok(());
        }
        let dir = out_dir.join(crate::terminal::ASSET_DIR);
        fs::create_dir_all(&dir)
            .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
        for entry in entries.values() {
            let (Some(out_file), Some(src)) = (&entry.out_file, &entry.src_path) else {
                continue;
            };
            let dest = dir.join(out_file);
            fs::copy(src, &dest).map_err(|e| {
                BuildError::Io(e, format!("copy {} -> {}", src.display(), dest.display()))
            })?;
        }
        Ok(())
    }
}

/// `true` for a source that should pass through unchanged (an absolute
/// URL or path, or an inline data URI) rather than being copied.
pub(crate) fn is_external(source: &str) -> bool {
    source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("data:")
        || source.starts_with('/')
}

/// Sanitise a file stem into the `_wdoc/` filename: alphanumerics kept,
/// everything else collapsed to `-`, capped so the name stays short.
pub(crate) fn sanitize(stem: &str) -> String {
    let s: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(24)
        .collect();
    if s.is_empty() { "img".to_string() } else { s }
}

/// 32-bit FNV-1a hash — a tiny, dependency-free, deterministic hash for
/// the output filename's collision-avoidance suffix.
pub(crate) fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Render a page `@block("image")` to an `<img>`. `register` records the
/// source for copying and rewrites it to the `_wdoc/` URL. A bare-named
/// source becomes the `src`; `width`/`height` are optional (the browser
/// uses the intrinsic size otherwise).
pub(crate) fn render_html(block: &Block<'_>, registry: &ImageRegistry) -> String {
    let Some(source) = label_string(block) else {
        return String::new();
    };
    if source.is_empty() {
        return String::new();
    }
    let entry = registry.register(&source);
    let mut classes = vec!["wdoc-image".to_string()];
    classes.extend(field_utf8_list(block, "class"));
    let cls = classes
        .iter()
        .map(|s| escape_html(s))
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = format!("<img class=\"{cls}\" src=\"{}\"", escape_html(&entry.url));
    if let Some(alt) = field_utf8(block, "alt") {
        let _ = write!(out, " alt=\"{}\"", escape_html(&alt));
    }
    if let Some(w) = field_f64(block, "width") {
        let _ = write!(out, " width=\"{w}\"");
    }
    if let Some(h) = field_f64(block, "height") {
        let _ = write!(out, " height=\"{h}\"");
    }
    if let Some(id) = field_id(block, "id") {
        let _ = write!(out, " id=\"{}\"", escape_html(&id));
    }
    out.push_str(" />");
    out
}

/// Render a diagram `@block("image")` shape to an SVG `<image>`. Sized
/// by `width`/`height` (× `scale`) or the natural dimensions when
/// omitted, and positioned by `x`/`y` + anchors (shared with `tilemap`).
pub(crate) fn render_svg(
    block: &Block<'_>,
    registry: &ImageRegistry,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let Some(source) = label_string(block) else {
        return String::new();
    };
    if source.is_empty() {
        return String::new();
    }
    let entry = registry.register(&source);
    let (x, y, w, h) = box_for(block, entry.dims, parent_w, parent_h);
    let mut out = format!(
        "<image href=\"{}\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" \
         preserveAspectRatio=\"none\"",
        escape_html(&entry.url)
    );
    let classes = field_utf8_list(block, "class");
    if !classes.is_empty() {
        let joined = classes
            .iter()
            .map(|s| escape_html(s))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = write!(out, " class=\"{joined}\"");
    }
    if let Some(id) = field_id(block, "id") {
        let _ = write!(out, " id=\"{}\"", escape_html(&id));
    }
    out.push_str(" />");
    out
}

/// The absolute-in-parent bounding box of a diagram `image`, for the
/// collect pass (edge routing + viewBox fit). Reads natural dims via the
/// registry when `width`/`height` are omitted.
pub(crate) fn image_bbox(
    block: &Block<'_>,
    registry: &ImageRegistry,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
    let dims = label_string(block).and_then(|s| registry.dims(&s));
    box_for(block, dims, parent_w, parent_h)
}

/// Shared geometry for `render_svg` + `image_bbox`: display size from
/// `width`/`height` (or `dims`) × `scale`, positioned via `tileset::place`.
fn box_for(
    block: &Block<'_>,
    dims: Option<(u32, u32)>,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
    let scale = field_f64(block, "scale").unwrap_or(1.0);
    let (nat_w, nat_h) = dims.map_or((0.0, 0.0), |(w, h)| (w as f64, h as f64));
    let w = field_f64(block, "width").unwrap_or(nat_w) * scale;
    let h = field_f64(block, "height").unwrap_or(nat_h) * scale;
    // No declared size and no readable intrinsic size collapses the box
    // to 0×0 — the image is invisible in the diagram. Warn (the sink
    // dedups the bbox-pass + render-pass double call) instead of
    // rendering nothing silently.
    if w <= 0.0 || h <= 0.0 {
        let source = label_string(block).unwrap_or_default();
        crate::render::record_render_warning(format!(
            "image \"{source}\": no intrinsic size available (unreadable or unsupported \
             header, or an external URL) — set `width`/`height` or the diagram image \
             renders invisible"
        ));
    }
    let (x, y) = crate::tileset::place(block, parent_w, parent_h, w, h);
    (x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_sources_pass_through_uncopied() {
        let reg = ImageRegistry::new(None);
        for src in [
            "https://x/y.png",
            "http://x/y.png",
            "data:image/png;base64,AA",
            "/abs.png",
        ] {
            let e = reg.register(src);
            assert_eq!(e.url, src, "{src} should pass through verbatim");
        }
        // Nothing local ⇒ copy is a no-op.
        let tmp = std::env::temp_dir().join("wdoc-image-test-noop");
        assert!(reg.copy_used_images(&tmp).is_ok());
    }

    #[test]
    fn local_source_gets_a_wdoc_url_and_is_deterministic() {
        let reg = ImageRegistry::new(Some(PathBuf::from("/docs")));
        let a = reg.register("pics/logo.png");
        assert!(a.url.starts_with("_wdoc/image-logo-"));
        assert!(a.url.ends_with(".png"));
        // Same source ⇒ same url (idempotent + deterministic).
        assert_eq!(reg.register("pics/logo.png").url, a.url);
    }
}
