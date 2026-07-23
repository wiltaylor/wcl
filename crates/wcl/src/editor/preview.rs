//! The editor's wdoc preview machinery: the `/api/preview` full/targeted
//! build ([`handle_preview`] → [`preview_site`]), the per-slug
//! [`PreviewSession`] bookkeeping that remembers each build's inputs, and
//! the lazy per-page rebuild that materializes stale pages when the iframe
//! navigates to them ([`handle_preview_file`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::{EditorState, api_result, err_str, overlay_files};
use crate::serve::{content_type, json_error, parse_json_body};

/// What the last `/api/preview` build of one output slug rendered, and with
/// which inputs — enough to lazily materialize a stale page on request.
///
/// After a targeted rebuild only the named pages reflect the latest inputs;
/// every other `<name>.html` on disk is a leftover from an earlier build.
/// The `generation` counter (bumped per `/api/preview` POST) tracks that: a
/// page is fresh iff the last build was full (`full_gen == generation`) or
/// the page itself was rendered at the current generation. Lazy rebuilds
/// materialize the *current* generation, so they never bump it.
pub(crate) struct PreviewSession {
    entry_abs: PathBuf,
    site: Option<String>,
    /// Whether this slug is the merged all-views build (`merged: true` on
    /// the POST — visibility bypassed, per-block visibility stamped). Lazy
    /// rebuilds must reuse it or a stale merged page would silently
    /// re-render without the bypass.
    all_sites: bool,
    /// The unsaved buffers the last POST built with — lazy rebuilds reuse
    /// them so navigation stays consistent with the last Rebuild.
    overlay: std::collections::HashMap<PathBuf, String>,
    pub(crate) generation: u64,
    full_gen: u64,
    page_gen: std::collections::HashMap<String, u64>,
}

impl PreviewSession {
    fn is_fresh(&self, page: &str) -> bool {
        self.full_gen == self.generation || self.page_gen.get(page) == Some(&self.generation)
    }

    /// Record what a build pass rendered at the current generation.
    fn note(&mut self, result: &PreviewBuild) {
        match result {
            PreviewBuild::Full => {
                self.full_gen = self.generation;
                self.page_gen.clear();
            }
            PreviewBuild::Targeted(pages) => {
                for p in pages {
                    self.page_gen.insert(p.clone(), self.generation);
                }
            }
        }
    }
}

/// What a preview build pass did — the session bookkeeping half of
/// [`wcl_wdoc::RebuildOutcome`].
enum PreviewBuild {
    Full,
    Targeted(Vec<String>),
}

/// `POST /api/preview` — full build of the selected site (`entry` +
/// optional `site` name) with the posted unsaved buffers overlaid, into
/// the preview scratch tree. Manual-rebuild semantics: the client only
/// calls this from its Rebuild button, and the long-held POST is the
/// build's progress signal. Serialized behind the preview gate and run
/// off the async executor — a render is real work.
pub(super) async fn handle_preview(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let _gate = state.preview.lock().await;
    let state2 = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || preview_site(&state2, &v))
        .await
        .unwrap_or_else(|e| Err(format!("preview task failed: {e}")));
    api_result(result)
}

/// The blocking half of [`handle_preview`]: build the whole selected site
/// into a per-(entry, site) subdirectory of the scratch tree and answer
/// with the `/api/preview/…` href of its index page.
fn preview_site(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let entry = crate::edit::str_field(v, "entry")?;
    let entry_abs = crate::serve::sandboxed(&state.root_dir, &state.root_dir.join(entry))
        .ok_or_else(|| format!("file outside the served tree: {entry}"))?;
    let site = v
        .get("site")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    let overlay = overlay_files(state, v)?;

    // The merged all-views render (the content modal's Merged tab): every
    // block regardless of `@only` / `@except`, with per-block visibility
    // stamped on the edit-mode anchors.
    let merged = v.get("merged").and_then(serde_json::Value::as_bool) == Some(true);
    if merged && v.get("skill").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err("merged has no meaning for a skill build".to_string());
    }
    // A merged build may additionally ask for a synthetic page for one unit
    // (`unit: {kind, id}`) — the fallback for units no view builds a page
    // for (they render embedded elsewhere). The page projects the unit's
    // `body`, so its blocks keep their real file/span anchors and stay
    // editable.
    let unit = v.get("unit").filter(|u| u.is_object());
    if unit.is_some() && !merged {
        return Err("unit previews are only available for merged builds".to_string());
    }

    // A stable slug per (entry, site) so distinct selections coexist in
    // the scratch tree and re-selecting one reuses its output. A merged
    // build gets its own slug — its pages must never shadow the normal
    // per-view output (and vice versa); a synthetic unit page gets a slug
    // per unit so its extra page keeps each dir's page set stable.
    let unit_id = unit
        .map(|u| crate::edit::str_field(u, "id").map(str::to_string))
        .transpose()?;
    let slug: String = format!(
        "{entry}__{}{}{}",
        site.as_deref().unwrap_or(""),
        if merged { "__merged" } else { "" },
        unit_id
            .as_deref()
            .map(|id| format!("__unit_{id}"))
            .unwrap_or_default(),
    )
    .chars()
    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
    .collect();
    let out = state.preview.root().join("sites").join(&slug);

    // A skill view builds the actual skill folder (the Markdown backend's
    // SKILL.md + references/ + assets) instead of an HTML site, and answers
    // with the file listing so the client can browse it.
    if v.get("skill").and_then(serde_json::Value::as_bool) == Some(true) {
        // A fresh dir per build: stale files from renamed pages would
        // otherwise linger in the listing. Any recorded session refers to
        // the wiped output, so it goes too.
        state.preview_sessions.lock().unwrap().remove(&slug);
        let _ = std::fs::remove_dir_all(&out);
        wcl_wdoc::skill(&entry_abs, &out, site.as_deref()).map_err(|e| e.render_plain())?;
        let _ = wcl_wdoc::take_render_warnings();
        let mut files: Vec<String> = Vec::new();
        collect_files(&out, &out, &mut files);
        files.sort();
        return Ok(serde_json::json!({
            "ok": true,
            "mode": "skill",
            "base": format!("/api/preview/sites/{slug}/"),
            "files": files,
        }));
    }

    // Targeted fast path: the client posts the page it's looking at (the
    // manual Rebuild) or the pages a design-mode commit touched, plus the
    // changed files — when the output dir is warm from a prior full build,
    // only those pages re-render in place. [`run_preview_build`] falls back
    // to a full build whenever that isn't safe.
    let pages: Option<std::collections::HashSet<String>> = v
        .get("pages")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect()
        })
        .filter(|s: &std::collections::HashSet<String>| !s.is_empty());
    let changed: Vec<PathBuf> = v
        .get("changed")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| p.as_str())
                .filter_map(|p| crate::serve::sandboxed(&state.root_dir, &state.root_dir.join(p)))
                .collect()
        })
        .unwrap_or_default();
    let mut overlay = overlay;
    if let Some(u) = unit {
        let kind = crate::edit::str_field(u, "kind")?;
        let id = unit_id.as_deref().unwrap_or_default();
        append_synthetic_unit_page(&entry_abs, &mut overlay, kind, id)?;
    }
    let built = run_preview_build(
        &entry_abs,
        &out,
        site.as_deref(),
        merged,
        overlay.clone(),
        pages,
        &changed,
    )?;
    let mode = match &built {
        PreviewBuild::Targeted(_) => "targeted",
        PreviewBuild::Full => "full",
    };

    // Record the session so navigating to a page this build left stale can
    // lazily re-render it with the same inputs. A POST is a new generation:
    // its inputs (buffers, saved files) supersede whatever came before.
    let mut sessions = state.preview_sessions.lock().unwrap();
    let session = sessions
        .entry(slug.clone())
        .or_insert_with(|| PreviewSession {
            entry_abs: entry_abs.clone(),
            site: site.clone(),
            all_sites: merged,
            overlay: std::collections::HashMap::new(),
            generation: 0,
            full_gen: 0,
            page_gen: std::collections::HashMap::new(),
        });
    session.entry_abs = entry_abs;
    session.site = site;
    session.all_sites = merged;
    session.overlay = overlay;
    session.generation += 1;
    session.note(&built);
    drop(sessions);

    let index = index_page(&out).ok_or("the site built but produced no HTML page")?;
    Ok(serde_json::json!({
        "ok": true,
        "mode": mode,
        "href": format!("/api/preview/sites/{slug}/{index}"),
    }))
}

/// One preview build pass into `out`: targeted when the dir is warm and the
/// caller named pages (or changed files map cleanly onto pages), else a full
/// build. Shared by the `/api/preview` POST and the lazy per-page rebuild in
/// [`handle_preview_file`].
///
/// comment_mode stamps the `data-wcl-block` / `data-wcl-page-*` anchors the
/// preview pane's comment UI keys on; edit_mode adds the per-block
/// `data-wcl-span` / `data-wcl-file` anchors and the `edit_object` "Edit
/// this …" buttons the pane resolves via `/api/object/locate` — still no
/// injected scripts. `all_sites` is the merged all-views render (the content
/// modal's Merged tab): `@only` / `@except` visibility is bypassed and each
/// block's anchor carries its visibility stamps for the client's per-view
/// indicator gutter.
///
/// An explicit `pages` filter that renders nothing (the viewed page isn't
/// one of this entry's site pages — a chooser index, an included sub-site's
/// page, a renamed page) means the targeted path can't refresh it, so it
/// falls back to a full build rather than reporting a no-op as success. The
/// incremental engine itself also self-falls-back on structural change
/// (page set, decks, new icons).
fn run_preview_build(
    entry_abs: &Path,
    out: &Path,
    site: Option<&str>,
    all_sites: bool,
    overlay: std::collections::HashMap<PathBuf, String>,
    pages: Option<std::collections::HashSet<String>>,
    changed: &[PathBuf],
) -> Result<PreviewBuild, String> {
    let mut opts = wcl_wdoc::BuildOptions {
        overlay: Some(overlay),
        comment_mode: true,
        edit_mode: true,
        all_sites,
        ..Default::default()
    };
    let explicit = pages.is_some();
    let warm = out.join("index.html").is_file() || index_page(out).is_some();
    if warm && (explicit || !changed.is_empty()) {
        opts.page_filter = pages;
        let outcome = wcl_wdoc::build_incremental(entry_abs, out, site, &opts, changed)
            .map_err(|e| e.render_plain())?;
        // Per-render warnings would otherwise pile up for whoever drains next.
        let _ = wcl_wdoc::take_render_warnings();
        match outcome {
            wcl_wdoc::RebuildOutcome::Targeted { pages: rendered } => {
                if !rendered.is_empty() || !explicit {
                    return Ok(PreviewBuild::Targeted(rendered));
                }
                // Explicit filter matched nothing — full-build below.
                opts.page_filter = None;
            }
            wcl_wdoc::RebuildOutcome::Full { .. } => return Ok(PreviewBuild::Full),
        }
    }
    wcl_wdoc::build_with_options(entry_abs, out, site, &opts).map_err(|e| e.render_plain())?;
    let _ = wcl_wdoc::take_render_warnings();
    Ok(PreviewBuild::Full)
}

/// The page name synthetic unit previews build under (the content modal's
/// merged tab for units with no page of their own).
pub(crate) const UNIT_PREVIEW_PAGE: &str = "__wcl_unit_preview";

/// Append the merged preview's synthetic unit page to the entry's overlay:
/// a `page __wcl_unit_preview { project { from = <gather>.<id>.body } }`
/// that renders the unit's addressable `body` via a static label path — the
/// projected blocks are the unit file's own doc-view blocks, so their
/// edit-mode anchors carry the REAL file/span and every editing op works.
/// Errors when the kind has no document gather or the instance carries no
/// `body` (nothing to render — the client keeps its list fallback).
fn append_synthetic_unit_page(
    entry_abs: &Path,
    overlay: &mut std::collections::HashMap<PathBuf, String>,
    kind: &str,
    id: &str,
) -> Result<(), String> {
    if id.is_empty()
        || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || id.starts_with(|c: char| c.is_ascii_digit())
    {
        return Err(format!("bad unit id `{id}`"));
    }
    let doc =
        wcl_wdoc::open_doc_for_edit_with_overlay(entry_abs, overlay.clone()).map_err(err_str)?;
    let gather = doc
        .type_decls()
        .filter(|d| d.decorators().any(|dec| dec.name() == "document"))
        .flat_map(|d| d.effective_fields())
        .find(|f| f.children_block_kind().as_deref() == Some(kind))
        .map(|f| f.name().to_string())
        .ok_or_else(|| format!("no document gather for kind `{kind}`"))?;
    let unit = doc
        .blocks()
        .find(|b| {
            b.kind() == kind
                && b.labels().ok().is_some_and(|ls| {
                    matches!(
                        ls.first(),
                        Some(
                            wcl_lang::Value::Utf8(s)
                                | wcl_lang::Value::Ascii(s)
                                | wcl_lang::Value::Identifier(s)
                        ) if s == id
                    )
                })
        })
        .ok_or_else(|| format!("no `{kind}` with id `{id}`"))?;
    if !unit.blocks().any(|c| c.kind() == "body") {
        return Err(format!(
            "`{kind} {id}` has no body — nothing to render standalone"
        ));
    }
    let base = match overlay.get(entry_abs) {
        Some(text) => text.clone(),
        None => std::fs::read_to_string(entry_abs)
            .map_err(|e| format!("read {}: {e}", entry_abs.display()))?,
    };
    let synthetic = format!(
        "\n\npage {UNIT_PREVIEW_PAGE} {{\n  title = \"{id}\"\n  project {{ from = {gather}.{id}.body }}\n}}\n"
    );
    overlay.insert(entry_abs.to_path_buf(), base + &synthetic);
    Ok(())
}

/// Every file under `root`, as `/`-separated paths relative to it.
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The page the preview iframe should open: `index.html` when the build
/// produced one, else the first `.html` directly under `out`.
fn index_page(out: &Path) -> Option<String> {
    if out.join("index.html").is_file() {
        return Some("index.html".to_string());
    }
    let mut pages: Vec<String> = std::fs::read_dir(out)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (name.ends_with(".html") && e.file_type().ok()?.is_file()).then_some(name)
        })
        .collect();
    pages.sort();
    pages.into_iter().next()
}

/// `GET /api/preview/{*path}` — serve a rendered file from the preview
/// scratch tree (the iframe target). No editor/reload scripts are involved:
/// the scratch tree holds a plain wdoc build.
///
/// A page left stale by a targeted rebuild is lazily re-rendered (with the
/// same inputs as the last `/api/preview` build) before it's served, so
/// navigating the preview always shows the last Rebuild's state without
/// paying for a full site build up front.
pub(super) async fn handle_preview_file(
    State(state): State<Arc<EditorState>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    let rel = Path::new(&path);
    if rel
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return json_error(StatusCode::BAD_REQUEST, "bad preview path");
    }
    let file = state.preview.root().join(rel);
    lazy_page_rebuild(&state, &path, &file).await;
    match tokio::fs::read(&file).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type(&file))],
            bytes,
        )
            .into_response(),
        Err(_) => json_error(StatusCode::NOT_FOUND, "no such preview file"),
    }
}

/// Lazily materialize a stale preview page before it's served. Only
/// `sites/<slug>/…/<stem>.html` requests with a recorded [`PreviewSession`]
/// participate; everything else (assets, skill files, unknown slugs) serves
/// as-is. Best-effort: a failing build logs and serves the stale file — a
/// broken document surfaces through the Rebuild button, not navigation.
async fn lazy_page_rebuild(state: &Arc<EditorState>, rel: &str, file: &Path) {
    let Some(slug) = rel
        .strip_prefix("sites/")
        .and_then(|r| r.split('/').next())
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    if !rel.ends_with(".html") {
        return;
    }
    let Some(stem) = file
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
    else {
        return;
    };
    let slug = slug.to_string();

    // Cheap pre-check without the build gate: right after a full build
    // (`full_gen == generation`, the common case) everything is fresh.
    // `index.html` is a copy of the start page, so its freshness needs the
    // manifest — resolved under the gate below.
    {
        let sessions = state.preview_sessions.lock().unwrap();
        let Some(s) = sessions.get(&slug) else { return };
        if s.full_gen == s.generation || (stem != "index" && s.is_fresh(&stem)) {
            return;
        }
    }

    // Serialize behind the preview gate, re-check (another request may have
    // materialized the page while we waited), then build off the executor.
    let _gate = state.preview.lock().await;
    let state2 = Arc::clone(state);
    let file2 = file.to_path_buf();
    let rel2 = rel.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        // Map the html file back to its page through the sibling manifest a
        // full build wrote. No manifest / unknown stem (a chooser index, an
        // included sub-site's page the root entry's targeted path can't
        // reach) ⇒ `None` ⇒ full rebuild.
        let page = manifest_page(&file2, &stem);
        let (entry_abs, site, all_sites, overlay) = {
            let sessions = state2.preview_sessions.lock().unwrap();
            let Some(s) = sessions.get(&slug) else { return };
            let fresh = match &page {
                Some(p) => s.is_fresh(p),
                None => s.full_gen == s.generation,
            };
            if fresh {
                return;
            }
            (
                s.entry_abs.clone(),
                s.site.clone(),
                s.all_sites,
                s.overlay.clone(),
            )
        };
        let out = state2.preview.root().join("sites").join(&slug);
        let filter = page.map(|p| std::collections::HashSet::from([p]));
        match run_preview_build(
            &entry_abs,
            &out,
            site.as_deref(),
            all_sites,
            overlay,
            filter,
            &[],
        ) {
            Ok(built) => {
                let mut sessions = state2.preview_sessions.lock().unwrap();
                if let Some(s) = sessions.get_mut(&slug) {
                    s.note(&built);
                }
            }
            Err(e) => eprintln!("preview: lazy rebuild for {rel2} failed: {e}"),
        }
    })
    .await;
}

/// The page name behind a built `<stem>.html`, read from the sibling
/// `_wdoc/pages.json` manifest the full build wrote: `index.html` maps to
/// the site's start page, any other stem must be listed. `None` when the
/// manifest is missing or the stem is unknown.
fn manifest_page(file: &Path, stem: &str) -> Option<String> {
    let manifest = file.parent()?.join(wcl_wdoc::PAGES_MANIFEST_HREF);
    let text = std::fs::read_to_string(manifest).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    if stem == "index" {
        return v.get("start")?.as_str().map(str::to_string);
    }
    v.get("pages")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find(|p| *p == stem)
        .map(str::to_string)
}
