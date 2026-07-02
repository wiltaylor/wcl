//! Preview-without-saving for `wcl wdoc serve --edit`.
//!
//! The source editor POSTs its unsaved buffers to `/__wdoc_preview`; the
//! server renders the affected page into a session-scoped scratch output
//! tree with the buffers overlaid (nothing touches disk or the live build),
//! and the client shows the result in an iframe served from
//! `GET /__wdoc_preview/{path}` — with no reload/edit scripts injected.
//!
//! Cost model: the first preview of a document warms its scratch tree with a
//! full build (shared assets, search index); every preview after that is a
//! targeted single-page render (`BuildOptions::page_filter`), which the build
//! automatically escalates back to a full render when the edit changes the
//! page set or shared state.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use wcl_wdoc::BuildOptions;

/// Per-serve preview state: the scratch output tree plus which entry
/// documents have already had their warm full build.
pub(crate) struct Preview {
    dir: tempfile::TempDir,
    warm: Mutex<HashSet<PathBuf>>,
    /// Serializes preview builds — a preview racing another preview would
    /// double CPU and interleave scratch-tree writes.
    gate: tokio::sync::Mutex<()>,
}

impl Preview {
    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            dir: tempfile::Builder::new().prefix("wdoc-preview-").tempdir()?,
            warm: Mutex::new(HashSet::new()),
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

/// Render a preview. `body` carries the current page (`page`, its `page_file`)
/// and the unsaved buffers (`files: [{path, text}]`). Returns
/// `{ ok, href, full }` — `href` is the `/__wdoc_preview/…` URL of the
/// rendered page, `full` whether this pass was a warm/fallback full build.
///
/// Blocking (a real render) — call from `spawn_blocking` under the preview
/// gate.
pub(crate) fn preview_build(
    preview: &Preview,
    root_file: &Path,
    watch_root: &Path,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let page = body
        .get("page")
        .and_then(serde_json::Value::as_str)
        .ok_or("missing page")?;
    let page_file = body.get("page_file").and_then(serde_json::Value::as_str);

    // Overlay: every posted buffer, sandbox-checked and canonically keyed.
    let mut overlay: HashMap<PathBuf, String> = HashMap::new();
    for f in body
        .get("files")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or_default()
    {
        let path = f
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or("file entry missing path")?;
        let text = f
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or("file entry missing text")?;
        let canon = crate::serve::sandboxed(watch_root, Path::new(path))
            .ok_or_else(|| format!("file outside the served tree: {path}"))?;
        overlay.insert(canon, text.to_string());
    }

    // Scope to the page's owning sub-site (like the Rebuild button): its
    // entry builds into the matching subdir of the scratch tree.
    let sub = page_file.and_then(|pf| wcl_wdoc::subsite_for_page(root_file, Path::new(pf)));
    let (entry, out_subdir, site) = match &sub {
        Some(s) => (s.entry.clone(), s.out_subdir.clone(), s.site.clone()),
        None => (root_file.to_path_buf(), PathBuf::new(), None),
    };
    let out = preview.dir.path().join(&out_subdir);

    let warm_needed = {
        let warm = preview.warm.lock().unwrap_or_else(|e| e.into_inner());
        !warm.contains(&entry)
    };
    let opts = BuildOptions {
        overlay: Some(overlay),
        page_filter: (!warm_needed).then(|| HashSet::from([page.to_string()])),
        ..Default::default()
    };
    wcl_wdoc::build_with_options(&entry, &out, site.as_deref(), &opts)
        .map_err(|e| e.render_plain())?;
    // Ignore non-fatal per-render warnings so they don't pile up for the
    // dev server's next real build report.
    let _ = wcl_wdoc::take_render_warnings();
    if warm_needed {
        preview
            .warm
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(entry);
    }

    let prefix = out_subdir.to_string_lossy().replace('\\', "/");
    let href = if prefix.is_empty() {
        format!("/__wdoc_preview/{page}.html")
    } else {
        format!("/__wdoc_preview/{prefix}/{page}.html")
    };
    Ok(serde_json::json!({ "ok": true, "href": href, "full": warm_needed }))
}
