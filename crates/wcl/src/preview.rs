//! Scratch preview tree for `wcl editor`.
//!
//! The editor renders its preview builds into a session-scoped temp output
//! tree (nothing touches disk or a real build output), serialized behind a
//! gate so concurrent previews don't double CPU or interleave writes.

use std::path::Path;

/// Per-session preview state: the scratch output tree plus the build gate.
pub(crate) struct Preview {
    dir: tempfile::TempDir,
    /// Serializes preview builds — a preview racing another preview would
    /// double CPU and interleave scratch-tree writes.
    gate: tokio::sync::Mutex<()>,
}

impl Preview {
    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            dir: tempfile::Builder::new().prefix("wdoc-preview-").tempdir()?,
            gate: tokio::sync::Mutex::new(()),
        })
    }

    /// The scratch tree a preview URL path resolves against.
    pub(crate) fn root(&self) -> &Path {
        self.dir.path()
    }

    pub(crate) async fn lock(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.gate.lock().await
    }
}
