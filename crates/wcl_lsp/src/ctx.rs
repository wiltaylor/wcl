//! The per-request analysis context: what every handler needs beyond
//! its own arguments, and the one way the server opens a buffer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use tower_lsp_server::ls_types::Uri;
use wcl_lang::{Document, FileLoader, ParseError};

use crate::convert::{LineIndex, PositionEncoding, path_to_uri, uri_to_path};

/// Settings and a snapshot of the open buffers, shared by one request's
/// (or one diagnostics pass's) analysis.
pub(crate) struct Ctx {
    /// Unit LSP `character` values count in, negotiated at `initialize`.
    pub encoding: PositionEncoding,
    /// Every open buffer with a filesystem path, as `path → text`.
    pub buffers: Arc<HashMap<PathBuf, String>>,
    /// Canonical path of every open buffer → the path it was opened as,
    /// so a file reached through an import (canonical) and the same file
    /// opened through a symlink are recognised as one.
    opened_as: Arc<HashMap<PathBuf, PathBuf>>,
    /// The embedded wdoc library over an overlay of `buffers` on disk.
    loader: FileLoader,
}

impl Ctx {
    /// A context with no open buffers: every import reads from disk.
    #[cfg(test)]
    pub(crate) fn new(encoding: PositionEncoding) -> Self {
        Self::with_buffers(encoding, HashMap::new())
    }

    /// A context whose loader serves `buffers` in place of their files
    /// on disk, and the embedded wdoc library for `import <wdoc.wcl>`
    /// and the other `<wdoc/…>` system imports. The snapshot is shared,
    /// not copied.
    pub(crate) fn with_buffers(
        encoding: PositionEncoding,
        buffers: impl Into<Arc<HashMap<PathBuf, String>>>,
    ) -> Self {
        let buffers = buffers.into();
        let opened_as: Arc<HashMap<PathBuf, PathBuf>> = Arc::new(
            buffers
                .keys()
                .map(|path| (canonical(path), path.clone()))
                .collect(),
        );
        let overlay = {
            let buffers = Arc::clone(&buffers);
            let opened_as = Arc::clone(&opened_as);
            let loader: FileLoader =
                Arc::new(
                    move |path: &Path| match buffer(&buffers, &opened_as, path) {
                        Some(text) => Ok(text.clone()),
                        None => std::fs::read_to_string(path),
                    },
                );
            loader
        };
        Self {
            encoding,
            buffers,
            opened_as,
            loader: wcl_wdoc::schema_registry().loader(overlay),
        }
    }

    /// The loader every document in this context opens through.
    pub(crate) fn loader(&self) -> FileLoader {
        self.loader.clone()
    }

    /// Index `text` for span ↔ position conversion in this context's
    /// encoding.
    pub(crate) fn index<'a>(&self, text: &'a str) -> LineIndex<'a> {
        LineIndex::new(text, self.encoding)
    }

    /// The text of `path`: its open buffer, else the file on disk.
    pub(crate) fn text(&self, path: &Path) -> Option<String> {
        buffer(&self.buffers, &self.opened_as, path)
            .cloned()
            .or_else(|| std::fs::read_to_string(path).ok())
    }

    /// The URI to report `path` under: the spelling its buffer was opened
    /// with when it is open, so the editor matches it to that buffer.
    pub(crate) fn uri_for(&self, path: &Path) -> Option<Uri> {
        let path = self
            .opened_as
            .get(&canonical(path))
            .map_or(path, PathBuf::as_path);
        path_to_uri(path)
    }

    /// Open `source` (named `uri`) the way the wdoc build would: system
    /// imports resolve through the embedded registry, relative imports
    /// resolve against the file's directory with open buffers overlaid,
    /// and the wdoc [`Environment`](wcl_lang::Environment) supplies
    /// builtins like `page_metadata` and expanders for `@contextual`
    /// kinds. A bare `Document::open` would flag all three as errors in
    /// perfectly valid documents. Every handler opens buffers through
    /// here so they agree with the diagnostics.
    pub(crate) fn open(&self, source: &str, uri: &str) -> Result<Document, ParseError> {
        let base_dir = Uri::from_str(uri)
            .ok()
            .and_then(|uri| uri_to_path(&uri))
            .and_then(|path| path.parent().map(Path::to_path_buf));
        Document::open_at_with_loader(
            source,
            uri,
            base_dir,
            &wcl_wdoc::wdoc_environment(),
            self.loader(),
        )
    }
}

/// The open buffer for `path`, under the spelling it was opened with or
/// any other spelling of the same file.
fn buffer<'a>(
    buffers: &'a HashMap<PathBuf, String>,
    opened_as: &HashMap<PathBuf, PathBuf>,
    path: &Path,
) -> Option<&'a String> {
    buffers
        .get(path)
        .or_else(|| buffers.get(opened_as.get(&canonical(path))?))
}

/// `path` with symlinks and `..` resolved, or unchanged when it does not
/// exist — the key two spellings of one file agree on.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
