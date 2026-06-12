//! File-loading abstraction. Lets callers (e.g. the LSP) overlay
//! in-memory buffer contents on top of the on-disk file tree so a
//! [`Document`](super::Document) parses against the user's *open*
//! source even before they hit save.
//!
//! The default loader is [`disk_loader`] — it just calls
//! `std::fs::read_to_string`. Anywhere `wcl_lang` would otherwise
//! read a file (top-level eager imports and in-block lazy imports
//! alike) goes through the document's `FileLoader` instead.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::imports::SYSTEM_IMPORT_ROOT;

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

/// A set of named source files embedded in the binary, addressable from
/// WCL via the angle-bracket system import: `import <wdoc/core.wcl>`.
///
/// Register files under registry-relative keys, then turn the registry
/// into a [`FileLoader`] with [`Registry::loader`]. The resulting loader
/// serves `<wcl-system>`-rooted virtual paths (the form
/// [`resolve_import_path_kind`](super::imports) produces for system
/// imports) out of the registry and delegates every other path to a
/// fallback loader (usually [`disk_loader`]).
///
/// ```
/// # use wcl_lang::{Registry, disk_loader, Document, Environment};
/// let mut reg = Registry::new();
/// reg.register("lib/prelude.wcl", "@schemaless\nanswer = 42");
/// let loader = reg.loader(disk_loader());
/// let doc = Document::open_at_with_loader(
///     "import <lib/prelude.wcl>",
///     "<doc>",
///     None,
///     &Environment::new(),
///     loader,
/// )
/// .unwrap();
/// assert!(doc.get("answer").is_some());
/// ```
#[derive(Default, Clone)]
pub struct Registry {
    files: HashMap<String, Cow<'static, str>>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `content` under the registry-relative `name` (e.g.
    /// `"wdoc/core.wcl"`). A later registration under the same name wins.
    pub fn register(&mut self, name: impl Into<String>, content: impl Into<Cow<'static, str>>) {
        self.files.insert(name.into(), content.into());
    }

    /// `true` if no files have been registered.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Build a [`FileLoader`] that serves registered files for system
    /// imports and delegates everything else to `fallback`. A system
    /// import naming an unregistered file fails with a `NotFound` error.
    pub fn loader(self, fallback: FileLoader) -> FileLoader {
        let files = self.files;
        Arc::new(move |p: &Path| {
            if let Ok(rel) = p.strip_prefix(SYSTEM_IMPORT_ROOT) {
                // Registry names always use forward slashes, but the
                // resolved import path arrives with platform separators
                // (backslashes on Windows); normalise before lookup.
                let key = rel.to_string_lossy().replace('\\', "/");
                return files
                    .get(key.as_str())
                    .map(|c| c.clone().into_owned())
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("no system import registered for <{key}>"),
                        )
                    });
            }
            fallback(p)
        })
    }
}
