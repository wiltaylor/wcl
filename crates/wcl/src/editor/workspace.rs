//! The **workspace** — the served tree and its root document.
//!
//! This is the context a handler gets when all it does is read or write
//! documents: the sandbox root every path is resolved against, and the
//! optional root `.wcl` the validating commit pipeline and the LSP session
//! hang off. Endpoints that never touch built output take one of these
//! instead of the whole [`super::EditorState`], so a signature says what a
//! handler can reach — and so a test for such an endpoint costs a temporary
//! directory rather than a directory plus a preview scratch tree.
//!
//! Write paths additionally take [`super::preview::Sessions`], the handle
//! that marks built previews stale; only the preview module itself takes the
//! full editor state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::serve::{sandboxed, sandboxed_create};

/// Cloneable so a connection-lived task (the LSP bridge) can hold its own
/// rather than the whole [`super::EditorState`] — it is two paths.
#[derive(Clone)]
pub(crate) struct Workspace {
    /// Canonical directory the editor serves — the sandbox root for every
    /// file operation.
    root_dir: PathBuf,
    /// Canonical root `.wcl` document, when one exists. Drives schema-validated
    /// saves, the preview build and the LSP session's root.
    root_file: Option<PathBuf>,
}

impl Workspace {
    pub(crate) fn new(root_dir: PathBuf, root_file: Option<PathBuf>) -> Self {
        Self {
            root_dir,
            root_file,
        }
    }

    /// The served tree's canonical root.
    pub(crate) fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// The root document, when the editor was given (or found) one.
    pub(crate) fn root_file(&self) -> Option<&Path> {
        self.root_file.as_deref()
    }

    /// A request-relative path resolved to an existing file inside the served
    /// tree. Anything that escapes is refused by name.
    pub(crate) fn abs(&self, rel: &str) -> Result<PathBuf, String> {
        sandboxed(&self.root_dir, &self.root_dir.join(rel))
            .ok_or_else(|| format!("file outside the served tree: {rel}"))
    }

    /// [`Self::abs`] for a file that need not exist yet (a new unit, a new
    /// data file): the deepest existing ancestor must still be inside.
    pub(crate) fn abs_new(&self, rel: &str) -> Result<PathBuf, String> {
        sandboxed_create(&self.root_dir, &self.root_dir.join(rel))
            .ok_or_else(|| format!("file outside the served tree: {rel}"))
    }

    /// An absolute path back as the `/`-normalized repo-relative one every
    /// response speaks.
    pub(crate) fn rel(&self, file: &Path) -> Result<String, String> {
        let canon = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        canon
            .strip_prefix(&self.root_dir)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .map_err(|_| {
                format!(
                    "{} is outside the served directory — not editable here",
                    canon.display()
                )
            })
    }

    /// The document a request resolves against: `entry` sandbox-checked,
    /// scoped to `page_file`'s owning included sub-site when given — a page
    /// inside an included sub-site (a wskill's) resolves against that
    /// sub-site's own document, so its kinds match its own schema.
    pub(crate) fn doc_entry(
        &self,
        entry: &str,
        page_file: Option<&str>,
    ) -> Result<PathBuf, String> {
        let entry_abs = self.abs(entry)?;
        Ok(page_file
            .filter(|s| !s.is_empty())
            .and_then(|pf| sandboxed(&self.root_dir, Path::new(pf)))
            .map(|pf| wcl_wdoc::doc_entry_for_page(&entry_abs, &pf))
            .unwrap_or(entry_abs))
    }

    /// [`Self::doc_entry`] reading `entry` / `page_file` from a JSON body.
    pub(crate) fn doc_entry_from(&self, v: &serde_json::Value) -> Result<PathBuf, String> {
        let entry = crate::edit::str_field(v, "entry")?;
        let page_file = v.get("page_file").and_then(serde_json::Value::as_str);
        self.doc_entry(entry, page_file)
    }

    /// The posted unsaved buffers (`files: [{path, text}]`) as an overlay map,
    /// sandbox-checked and canonically keyed.
    pub(crate) fn overlay(
        &self,
        v: &serde_json::Value,
    ) -> Result<HashMap<PathBuf, String>, String> {
        let mut overlay = HashMap::new();
        for f in v
            .get("files")
            .and_then(serde_json::Value::as_array)
            .map(|a| a.as_slice())
            .unwrap_or_default()
        {
            let path = crate::edit::str_field(f, "path")?;
            let text = crate::edit::str_field(f, "text")?;
            overlay.insert(self.abs(path)?, text.to_string());
        }
        Ok(overlay)
    }
}

#[cfg(test)]
impl Workspace {
    /// A workspace over `dir` with no root document — the one-liner most
    /// module-local tests need (every endpoint that matters names its own
    /// `entry`).
    pub(crate) fn at(dir: &Path) -> Self {
        Self::new(std::fs::canonicalize(dir).unwrap(), None)
    }
}
