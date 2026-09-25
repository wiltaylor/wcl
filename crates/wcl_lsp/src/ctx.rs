//! The per-request analysis context: what every handler needs beyond
//! its own arguments, and the one way the server opens a buffer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use tower_lsp_server::ls_types::Uri;
use wcl_lang::{Document, FileLoader, ParseError};

use crate::convert::{LineIndex, PositionEncoding, path_to_uri, uri_to_path};
use crate::host::Host;

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
    /// Canonical directory → the spelling an open buffer's path reaches
    /// it by, for every directory above a buffer opened through a
    /// symlink; deepest first. A file that is not open is reported
    /// under the spelling its nearest such directory gives it.
    dir_aliases: Arc<Vec<(PathBuf, PathBuf)>>,
    /// The host every document opens with.
    host: Arc<Host>,
    /// The host's system imports over an overlay of `buffers` on disk.
    loader: FileLoader,
}

impl Ctx {
    /// A context under the wdoc host with no open buffers: every import
    /// reads from disk.
    #[cfg(test)]
    pub(crate) fn new(encoding: PositionEncoding) -> Self {
        Self::with_buffers(encoding, HashMap::new(), crate::host::wdoc())
    }

    /// A context whose loader serves `buffers` in place of their files
    /// on disk, and `host`'s registry for system imports
    /// (`import <name.wcl>`). The snapshot is shared, not copied.
    pub(crate) fn with_buffers(
        encoding: PositionEncoding,
        buffers: impl Into<Arc<HashMap<PathBuf, String>>>,
        host: Arc<Host>,
    ) -> Self {
        let buffers = buffers.into();
        let opened_as: Arc<HashMap<PathBuf, PathBuf>> = Arc::new(
            buffers
                .keys()
                .map(|path| (canonical(path), path.clone()))
                .collect(),
        );
        let dir_aliases = Arc::new(dir_aliases(&opened_as));
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
            dir_aliases,
            loader: host.loader(overlay),
            host,
        }
    }

    /// The host every document in this context opens with.
    pub(crate) fn host(&self) -> &Arc<Host> {
        &self.host
    }

    /// The environment every document in this context opens with.
    pub(crate) fn environment(&self) -> &wcl_lang::Environment {
        self.host.environment()
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

    /// The spelling to report `path` under, whichever spelling it
    /// arrives in: the one its buffer was opened with when it is open;
    /// else the one an open buffer's symlinked directory gives it; else
    /// `path` unchanged. Every path sent back to the client goes through
    /// here, so the editor matches it to the buffer it already has.
    pub(crate) fn client_path(&self, path: &Path) -> PathBuf {
        let real = canonical(path);
        if let Some(opened) = self.opened_as.get(&real) {
            return opened.clone();
        }
        self.dir_aliases
            .iter()
            .find_map(|(dir, alias)| Some(alias.join(real.strip_prefix(dir).ok()?)))
            .unwrap_or_else(|| path.to_path_buf())
    }

    /// The URI to report `path` under: [`Ctx::client_path`] as a URI.
    pub(crate) fn uri_for(&self, path: &Path) -> Option<Uri> {
        path_to_uri(&self.client_path(path))
    }

    /// A loader over `buffers` in place of this context's own, matching
    /// every spelling of a path the way this context's loader does.
    pub(crate) fn loader_over(&self, buffers: HashMap<PathBuf, String>) -> FileLoader {
        Self::with_buffers(self.encoding, buffers, Arc::clone(&self.host)).loader()
    }

    /// Open `source` (named `uri`) the way the host's build would: system
    /// imports resolve through the host's registry, relative imports
    /// resolve against the file's directory with open buffers overlaid,
    /// and the host's [`Environment`](wcl_lang::Environment) supplies its
    /// builtins and the expander for `@contextual` kinds. A bare
    /// `Document::open` would flag all three as errors in a host's valid
    /// documents. Every handler opens buffers through here so they agree
    /// with the diagnostics.
    pub(crate) fn open(&self, source: &str, uri: &str) -> Result<Document, ParseError> {
        let base_dir = Uri::from_str(uri)
            .ok()
            .and_then(|uri| uri_to_path(&uri))
            .and_then(|path| path.parent().map(Path::to_path_buf));
        Document::open_at_with_loader(source, uri, base_dir, self.environment(), self.loader())
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

/// Canonical directory → client spelling for every directory above an
/// open buffer whose spelling differs from its canonical path, deepest
/// first. A buffer already spelled canonically has no such directory.
fn dir_aliases(opened_as: &HashMap<PathBuf, PathBuf>) -> Vec<(PathBuf, PathBuf)> {
    let mut aliases: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (real, opened) in opened_as {
        if real == opened {
            continue;
        }
        for dir in opened.ancestors().skip(1) {
            let real_dir = canonical(dir);
            if real_dir != dir && !aliases.iter().any(|(known, _)| *known == real_dir) {
                aliases.push((real_dir, dir.to_path_buf()));
            }
        }
    }
    aliases.sort_by_key(|(dir, _)| std::cmp::Reverse(dir.components().count()));
    aliases
}

/// `path` with symlinks and `..` resolved, or unchanged when it does not
/// exist — the key two spellings of one file agree on.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
