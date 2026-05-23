//! File-loading abstraction. Lets callers (e.g. the LSP) overlay
//! in-memory buffer contents on top of the on-disk file tree so a
//! [`Document`](super::Document) parses against the user's *open*
//! source even before they hit save.
//!
//! The default loader is [`disk_loader`] — it just calls
//! `std::fs::read_to_string`. Anywhere `wcl_lang` would otherwise
//! read a file (top-level eager imports and in-block lazy imports
//! alike) goes through the document's `FileLoader` instead.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Reader of source files for the loader. Implementations are
/// expected to be cheap to clone and safe to share across threads —
/// they're shaped as `Arc<dyn Fn>` so the closure form composes
/// naturally with `move`-captured state (e.g. an editor's open
/// buffers).
pub type FileLoader = Arc<dyn Fn(&Path) -> std::io::Result<String> + Send + Sync>;

/// Default loader: read every file straight from disk.
pub fn disk_loader() -> FileLoader {
    Arc::new(|p: &Path| std::fs::read_to_string(p))
}

/// Loader that overlays an in-memory map on top of disk. Paths in
/// `overlay` are checked first (matched by canonicalised key when
/// possible, then by raw key); anything not found in the overlay
/// falls through to `std::fs::read_to_string`.
///
/// Keys should be canonical absolute paths (`std::fs::canonicalize`)
/// to match the way imports are resolved internally; the overlay
/// also accepts raw keys as a convenience for callers that have
/// not canonicalised.
pub fn overlay_loader(overlay: HashMap<PathBuf, String>) -> FileLoader {
    Arc::new(move |p: &Path| {
        if let Some(s) = overlay.get(p) {
            return Ok(s.clone());
        }
        if let Ok(canon) = std::fs::canonicalize(p)
            && let Some(s) = overlay.get(&canon)
        {
            return Ok(s.clone());
        }
        std::fs::read_to_string(p)
    })
}
