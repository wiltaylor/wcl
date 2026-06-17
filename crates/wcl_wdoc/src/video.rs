//! The `video` block: an embedded video on a page, shown first as a
//! click-to-play *facade* (a poster thumbnail + a play button) that the
//! bundled `_wdoc/wdoc-video.js` player swaps for a real `<video>` /
//! `<iframe>` when the reader clicks — so no video auto-loads.
//!
//! The `source` is classified into one of four kinds: a local file
//! (`<video>`, copied into `_wdoc/` like an `image`), a YouTube or Vimeo
//! URL (an embed `<iframe>`, with YouTube posters auto-derived), or any
//! other web URL (a generic embed `<iframe>`). Like `image` it's
//! special-cased in the renderer, so its WCL `lower` is a never-called
//! stub. The asset copy reuses `image`'s deterministic `_wdoc/` naming
//! ([`is_external`], [`sanitize`], [`fnv1a`]).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use wcl_lang::Block;

use crate::build::BuildError;
use crate::image::{fnv1a, is_external, sanitize};
use crate::render::{escape_html, field_f64, field_id, field_utf8, field_utf8_list, label_string};

/// How a `video` `source` is embedded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VideoSource {
    /// A local file (or a direct video-file URL) played via `<video>`.
    Local,
    /// A YouTube video, carrying its extracted id.
    YouTube(String),
    /// A Vimeo video, carrying its extracted numeric id.
    Vimeo(String),
    /// Any other web URL, embedded verbatim in an `<iframe>`.
    Generic,
}

/// Classify a `source` into the player kind. A `http(s)` URL is matched
/// against YouTube / Vimeo, then a direct video-file extension (played
/// natively), else treated as a generic embed; everything else (a
/// doc-relative path or a `data:` URI) plays natively as `Local`.
pub(crate) fn classify(source: &str) -> VideoSource {
    let lower = source.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        if let Some(id) = youtube_id(source) {
            return VideoSource::YouTube(id);
        }
        if let Some(id) = vimeo_id(source) {
            return VideoSource::Vimeo(id);
        }
        if is_video_file(source) {
            return VideoSource::Local;
        }
        return VideoSource::Generic;
    }
    VideoSource::Local
}

/// The human-facing URL to link to from the PDF, or `None` for a local
/// video (a `file:`-style path is useless in a distributed PDF, so a
/// local video gets only its poster — no link).
pub(crate) fn online_url(source: &str) -> Option<String> {
    match classify(source) {
        VideoSource::YouTube(id) => Some(format!("https://www.youtube.com/watch?v={id}")),
        VideoSource::Vimeo(id) => Some(format!("https://vimeo.com/{id}")),
        VideoSource::Generic => Some(source.to_string()),
        VideoSource::Local => None,
    }
}

/// `true` if any block in this subtree is a `video` (drives the per-page
/// inclusion of the bundled `wdoc-video.js` player).
pub(crate) fn uses_video(block: &Block<'_>) -> bool {
    crate::render::block_tree_any(block, &|b| b.kind() == "video")
}

/// One resolved local asset (a video file or a poster) to copy.
#[derive(Clone)]
struct VideoEntry {
    /// The `src` to emit (a `_wdoc/…` URL for a copied local file, or the
    /// verbatim source for an external URL).
    url: String,
    /// Output filename within `_wdoc/`; `None` ⇒ external (never copied).
    out_file: Option<String>,
    /// Source path on disk (`Some` for a local file to copy).
    src_path: Option<PathBuf>,
}

/// Lazily-populated registry of referenced local video files and posters,
/// keyed by raw source string so repeat references share one copied file.
/// Mirrors [`crate::image::ImageRegistry`] but carries no natural
/// dimensions (a video's size is the browser's concern).
pub(crate) struct VideoRegistry {
    base_dir: Option<PathBuf>,
    entries: RefCell<BTreeMap<String, VideoEntry>>,
}

impl VideoRegistry {
    pub(crate) fn new(base_dir: Option<PathBuf>) -> Self {
        VideoRegistry {
            base_dir,
            entries: RefCell::new(BTreeMap::new()),
        }
    }

    /// Resolve `source`, recording it for copying when local, and return
    /// the URL to emit — a `_wdoc/<prefix>-…` URL for a copied local file,
    /// or the verbatim source for an external URL / `data:` URI. Idempotent.
    pub(crate) fn register(&self, source: &str, prefix: &str) -> String {
        if let Some(e) = self.entries.borrow().get(source) {
            return e.url.clone();
        }
        let entry = self.build_entry(source, prefix);
        let url = entry.url.clone();
        self.entries.borrow_mut().insert(source.to_string(), entry);
        url
    }

    fn build_entry(&self, source: &str, prefix: &str) -> VideoEntry {
        if is_external(source) {
            return VideoEntry {
                url: source.to_string(),
                out_file: None,
                src_path: None,
            };
        }
        let src_path = match &self.base_dir {
            Some(dir) => dir.join(source),
            None => PathBuf::from(source),
        };
        let ext = src_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4");
        let stem = src_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(prefix);
        // Deterministic + collision-free, mirroring `image`'s scheme.
        let out_file = format!("{prefix}-{}-{:08x}.{ext}", sanitize(stem), fnv1a(source));
        let url = format!("{}/{}", crate::terminal::ASSET_DIR, out_file);
        VideoEntry {
            url,
            out_file: Some(out_file),
            src_path: Some(src_path),
        }
    }

    /// Copy every referenced local video file / poster into `<out>/_wdoc/`.
    /// No-op when nothing local was referenced.
    pub(crate) fn copy_used_assets(&self, out_dir: &Path) -> Result<(), BuildError> {
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

/// Render a page `@block("video")` to a click-to-play facade `<div>`. The
/// real player swaps in on click (driven by `wdoc-video.js`), reading
/// `data-kind` (which element to build) and `data-src` (the playable /
/// embed URL). The poster is an explicit `poster`, else a YouTube
/// auto-thumbnail, else a styled placeholder.
pub(crate) fn render_html(block: &Block<'_>, registry: &VideoRegistry) -> String {
    let Some(source) = label_string(block) else {
        return String::new();
    };
    if source.is_empty() {
        return String::new();
    }
    let kind = classify(&source);
    let (kind_str, play_url) = match &kind {
        VideoSource::Local => ("local", registry.register(&source, "video")),
        VideoSource::YouTube(id) => (
            "youtube",
            format!("https://www.youtube.com/embed/{id}?autoplay=1"),
        ),
        VideoSource::Vimeo(id) => (
            "vimeo",
            format!("https://player.vimeo.com/video/{id}?autoplay=1"),
        ),
        VideoSource::Generic => ("generic", source.clone()),
    };

    // Poster: explicit field (copied if local) → YouTube auto-thumbnail → none.
    let poster = field_utf8(block, "poster")
        .map(|p| registry.register(&p, "poster"))
        .or_else(|| match &kind {
            VideoSource::YouTube(id) => {
                Some(format!("https://img.youtube.com/vi/{id}/hqdefault.jpg"))
            }
            _ => None,
        });

    let mut classes = vec!["wdoc-video".to_string()];
    classes.extend(field_utf8_list(block, "class"));
    let cls = classes
        .iter()
        .map(|s| escape_html(s))
        .collect::<Vec<_>>()
        .join(" ");

    let title = field_utf8(block, "title");

    let mut out = format!(
        "<div class=\"{cls}\" data-kind=\"{kind_str}\" data-src=\"{}\"",
        escape_html(&play_url)
    );
    if let Some(t) = &title {
        let _ = write!(out, " aria-label=\"{}\"", escape_html(t));
    }
    // Sizing rides on an inline style (the facade is a `<div>`, not an
    // `<img>`), so the swapped-in player inherits the same box.
    let w = field_f64(block, "width");
    let h = field_f64(block, "height");
    if w.is_some() || h.is_some() {
        let mut style = String::new();
        if let Some(w) = w {
            let _ = write!(style, "width:{w}px;");
        }
        if let Some(h) = h {
            let _ = write!(style, "height:{h}px;");
        }
        let _ = write!(out, " style=\"{style}\"");
    }
    if let Some(id) = field_id(block, "id") {
        let _ = write!(out, " id=\"{}\"", escape_html(&id));
    }
    out.push('>');

    match poster {
        Some(url) => {
            let _ = write!(out, "<img src=\"{}\"", escape_html(&url));
            if let Some(t) = &title {
                let _ = write!(out, " alt=\"{}\"", escape_html(t));
            }
            out.push_str(" />");
        }
        None => out.push_str("<span class=\"wdoc-video-placeholder\"></span>"),
    }
    out.push_str("<span class=\"wdoc-video-play\" aria-hidden=\"true\"></span>");
    out.push_str("</div>");
    out
}

/// Split a URL into its lower-cased host (sans `www.`, port, userinfo) and
/// the remaining `/path?query` slice.
fn host_and_path(url: &str) -> (String, &str) {
    let after = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    let (host, path) = match after.find('/') {
        Some(i) => (&after[..i], &after[i..]),
        None => (after, ""),
    };
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    (host.trim_start_matches("www.").to_ascii_lowercase(), path)
}

/// Extract a YouTube video id from any of the common URL shapes
/// (`youtu.be/ID`, `watch?v=ID`, `/embed/ID`, `/shorts/ID`, `/v/ID`).
fn youtube_id(url: &str) -> Option<String> {
    let (host, path) = host_and_path(url);
    if host == "youtu.be" {
        let id = path.trim_start_matches('/');
        return valid_id(id.split(['?', '&', '/']).next().unwrap_or(""));
    }
    if matches!(
        host.as_str(),
        "youtube.com" | "m.youtube.com" | "youtube-nocookie.com"
    ) {
        if let Some(query) = path.split('?').nth(1) {
            for pair in query.split('&') {
                if let Some(v) = pair.strip_prefix("v=") {
                    return valid_id(v);
                }
            }
        }
        for seg in ["/embed/", "/shorts/", "/v/"] {
            if let Some(i) = path.find(seg) {
                let id = &path[i + seg.len()..];
                return valid_id(id.split(['?', '&', '/']).next().unwrap_or(""));
            }
        }
    }
    None
}

/// Extract a Vimeo numeric id (the first all-digit path segment of a
/// `vimeo.com` / `player.vimeo.com` URL).
fn vimeo_id(url: &str) -> Option<String> {
    let (host, path) = host_and_path(url);
    if host != "vimeo.com" && host != "player.vimeo.com" {
        return None;
    }
    for seg in path.split('/') {
        let seg = seg.split(['?', '&']).next().unwrap_or(seg);
        if !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit()) {
            return Some(seg.to_string());
        }
    }
    None
}

/// A video id is non-empty and limited to the URL-safe id alphabet.
fn valid_id(id: &str) -> Option<String> {
    if !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Some(id.to_string())
    } else {
        None
    }
}

/// `true` if `url`'s path ends in a directly-playable video extension.
fn is_video_file(url: &str) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    [".mp4", ".webm", ".ogg", ".ogv", ".mov", ".m4v"]
        .iter()
        .any(|e| path.ends_with(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_youtube_url_forms() {
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=dQw4w9WgXcQ&t=10s",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
        ] {
            assert_eq!(
                classify(url),
                VideoSource::YouTube("dQw4w9WgXcQ".to_string()),
                "{url}"
            );
        }
    }

    #[test]
    fn classifies_vimeo_url() {
        assert_eq!(
            classify("https://vimeo.com/76979871"),
            VideoSource::Vimeo("76979871".to_string())
        );
        assert_eq!(
            classify("https://player.vimeo.com/video/76979871"),
            VideoSource::Vimeo("76979871".to_string())
        );
    }

    #[test]
    fn classifies_generic_local_and_direct_file() {
        assert_eq!(
            classify("https://example.com/embed/x"),
            VideoSource::Generic
        );
        // A direct video-file URL plays natively (still external ⇒ not copied).
        assert_eq!(classify("https://example.com/clip.mp4"), VideoSource::Local);
        assert_eq!(classify("assets/clip.mp4"), VideoSource::Local);
        assert_eq!(classify("data:video/mp4;base64,AA"), VideoSource::Local);
    }

    #[test]
    fn online_url_links_only_for_remote_sources() {
        assert_eq!(
            online_url("https://youtu.be/dQw4w9WgXcQ").as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        assert_eq!(
            online_url("https://vimeo.com/76979871").as_deref(),
            Some("https://vimeo.com/76979871")
        );
        assert_eq!(
            online_url("https://example.com/embed/x").as_deref(),
            Some("https://example.com/embed/x")
        );
        assert_eq!(online_url("assets/clip.mp4"), None);
    }

    #[test]
    fn registry_copies_local_and_passes_external_through() {
        let reg = VideoRegistry::new(Some(PathBuf::from("/docs")));
        let local = reg.register("media/clip.mp4", "video");
        assert!(local.starts_with("_wdoc/video-clip-"));
        assert!(local.ends_with(".mp4"));
        // Idempotent + deterministic.
        assert_eq!(reg.register("media/clip.mp4", "video"), local);

        let poster = reg.register("media/cover.png", "poster");
        assert!(poster.starts_with("_wdoc/poster-cover-"));
    }

    #[test]
    fn external_sources_pass_through_and_copy_is_a_noop() {
        let reg = VideoRegistry::new(None);
        for ext in ["https://x/y.mp4", "data:video/mp4;base64,AA", "/abs.mp4"] {
            assert_eq!(reg.register(ext, "video"), ext, "{ext} should pass through");
        }
        // Nothing local ⇒ copy is a no-op (no missing-file error).
        let tmp = std::env::temp_dir().join("wdoc-video-test-noop");
        assert!(reg.copy_used_assets(&tmp).is_ok());
    }
}
