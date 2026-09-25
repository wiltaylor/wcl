//! The per-request analysis context: what every handler needs beyond
//! its own arguments, and the one way the server opens a buffer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use tower_lsp_server::ls_types::Uri;
use wcl_lang::{Document, FileLoader, ParseError, overlay_loader};

use crate::convert::{LineIndex, PositionEncoding, uri_to_path};

/// Settings and a snapshot of the open buffers, shared by one request's
/// (or one diagnostics pass's) analysis.
pub(crate) struct Ctx {
    /// Unit LSP `character` values count in, negotiated at `initialize`.
    pub encoding: PositionEncoding,
    /// Every open buffer with a filesystem path, as `path → text`.
    pub buffers: Arc<HashMap<PathBuf, String>>,
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
    /// and the other `<wdoc/…>` system imports.
    pub(crate) fn with_buffers(
        encoding: PositionEncoding,
        buffers: HashMap<PathBuf, String>,
    ) -> Self {
        let loader = wcl_wdoc::schema_registry().loader(overlay_loader(buffers.clone()));
        Self {
            encoding,
            buffers: Arc::new(buffers),
            loader,
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
        self.buffers
            .get(path)
            .cloned()
            .or_else(|| std::fs::read_to_string(path).ok())
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
