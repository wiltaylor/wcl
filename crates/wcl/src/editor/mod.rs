//! `wcl editor` — a browser-based editor for the current directory.
//!
//! Serves a single-page app (SolidJS + the Forge design system, embedded
//! from `editor-ui/dist`) with a gitignore-aware file tree, CodeMirror
//! editing of any text file, LSP support for `.wcl` (a WebSocket bridge to
//! an in-process [`wcl_lsp`] session), and a wdoc preview pane. Preview is
//! site-scoped: `/api/sites` discovers every `site`-declaring entry under
//! the served tree (nested by `include` membership), and `/api/preview`
//! builds the selected one on demand — the client's Rebuild button, not a
//! per-edit loop. On a warm output dir the build targets only the page the
//! client is viewing; pages that leaves stale are lazily re-rendered when
//! the iframe navigates to them (a per-slug [`PreviewSession`] remembers
//! the last build's inputs), with an automatic full rebuild whenever a
//! targeted one isn't possible. The root document follows the LSP's model: the
//! explicit argument, else `./main.wcl` when present. Without one the
//! editor still works — schema-validated saves and the LSP root degrade
//! gracefully.
//!
//! Unlike the `wcl wdoc serve` dev server there is no watcher and no
//! rebuild loop: saves are client-driven, external changes surface as etag
//! conflicts on save, and previews always overlay the live buffers.

mod assets;
mod blocks;
mod comments;
mod data;
mod files;
mod graph;
mod lsp_bridge;
mod nav;
mod profiles;
mod sites;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

use crate::serve::{
    BindSpec, DEFAULT_BIND, bind_auto, content_type, json_error, json_response, parse_json_body,
    query_param,
};

pub(crate) struct EditorState {
    /// Canonical directory the editor serves — the sandbox root for every
    /// file operation.
    root_dir: PathBuf,
    /// Canonical root `.wcl` document, when one exists. Drives the preview
    /// build, schema-validated saves, and the LSP session's root.
    root_file: Option<PathBuf>,
    /// Scratch output tree for preview renders.
    preview: crate::preview::Preview,
    /// Warm preview state per `sites/<slug>` output dir, so a stale page can
    /// be lazily re-rendered when the iframe navigates to it. Guarded by a
    /// plain mutex — only ever held briefly; builds themselves serialize
    /// behind the preview gate.
    preview_sessions: std::sync::Mutex<std::collections::HashMap<String, PreviewSession>>,
    /// Review handshake pairing a blocked `wcl wdoc review <root>` with the
    /// preview pane's "Send to agent" button. `None` without a root document
    /// (`review` then falls back to its non-blocking listing).
    review: Option<wcl_wdoc::Handshake>,
}

/// What the last `/api/preview` build of one output slug rendered, and with
/// which inputs — enough to lazily materialize a stale page on request.
///
/// After a targeted rebuild only the named pages reflect the latest inputs;
/// every other `<name>.html` on disk is a leftover from an earlier build.
/// The `generation` counter (bumped per `/api/preview` POST) tracks that: a
/// page is fresh iff the last build was full (`full_gen == generation`) or
/// the page itself was rendered at the current generation. Lazy rebuilds
/// materialize the *current* generation, so they never bump it.
struct PreviewSession {
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
    generation: u64,
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

pub(crate) async fn serve(
    root: Option<PathBuf>,
    addr: BindSpec,
) -> Result<(), Box<dyn std::error::Error>> {
    let root_dir = std::fs::canonicalize(".")?;
    let root_file = resolve_root_file(&root_dir, root)?;

    // Register as the live review peer for the root document, so a blocked
    // `wcl wdoc review <root>` pairs with this editor's "Send to agent".
    let review = root_file.as_deref().map(wcl_wdoc::Handshake::new);
    if let Some(hs) = &review
        && let Err(e) = hs.serve_started()
    {
        eprintln!("warning: could not initialise the review handshake: {e}");
    }

    // Hard stop on Ctrl-C: nothing here needs graceful teardown (the preview
    // TempDir is cleaned by the OS temp reaper if the guard's Drop is
    // skipped), and an open WebSocket or long request must not delay exit —
    // only the review marker is cleared so `wcl wdoc review` doesn't wait on
    // a dead editor.
    let review_cleanup = review.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down");
        if let Some(hs) = &review_cleanup {
            hs.serve_stopped();
        }
        std::process::exit(0);
    });

    let state = Arc::new(EditorState {
        root_dir: root_dir.clone(),
        root_file: root_file.clone(),
        preview: crate::preview::Preview::new()?,
        preview_sessions: std::sync::Mutex::new(std::collections::HashMap::new()),
        review,
    });

    let app = router(state);
    let listener = match addr {
        BindSpec::Auto => bind_auto(DEFAULT_BIND).await?,
        BindSpec::Fixed(a) => tokio::net::TcpListener::bind(a).await?,
    };
    let bound = listener.local_addr()?;
    println!(
        "wcl editor at http://{bound}  (dir: {})",
        root_dir.display()
    );
    match &root_file {
        Some(f) => println!(
            "root document: {}  (review: `wcl wdoc review {}` pairs with this editor)",
            f.display(),
            f.display()
        ),
        None => println!(
            "no root document (no ./main.wcl) — schema-validated saves and cross-file LSP \
             are off (preview discovers sites itself); pass one: wcl editor path/to/root.wcl"
        ),
    }
    axum::serve(listener, app).await?;
    Ok(())
}

/// The editor's API router plus the embedded SPA fallback. Split from
/// [`serve`] so tests can drive it with `tower::ServiceExt::oneshot`.
pub(crate) fn router(state: Arc<EditorState>) -> Router {
    Router::new()
        .route("/api/files", get(handle_files))
        .route("/api/file", get(handle_file_get).post(handle_file_post))
        .route("/api/format", post(handle_format))
        .route("/api/lsp", get(lsp_bridge::handle_lsp_ws))
        .route("/api/sites", get(handle_sites))
        .route(
            "/api/comments",
            get(comments::handle_comments_list).post(comments::handle_comment_add),
        )
        .route(
            "/api/comments/resolve",
            post(comments::handle_comment_resolve),
        )
        .route("/api/comments/edit", post(comments::handle_comment_edit))
        .route("/api/review/status", get(comments::handle_review_status))
        .route("/api/review/ready", post(comments::handle_review_ready))
        .route("/api/preview", post(handle_preview))
        .route("/api/preview/{*path}", get(handle_preview_file))
        .route("/api/object/locate", post(handle_object_locate))
        .route("/api/block/source", post(blocks::handle_block_source))
        .route("/api/block/ops", post(blocks::handle_block_ops))
        .route("/api/unit/field", post(blocks::handle_unit_field))
        .route("/api/unit/create", post(blocks::handle_unit_create))
        .route("/api/palette", get(blocks::handle_palette))
        .route("/api/nav", get(nav::handle_nav))
        .route("/api/nav/op", post(nav::handle_nav_op))
        .route("/api/wskill/profile", post(profiles::handle_profile))
        .route("/api/graph", get(graph::handle_graph))
        .route("/api/data/types", get(data::handle_data_types))
        .route("/api/data/rows", get(data::handle_data_rows))
        .route("/api/raw", get(handle_raw))
        .fallback(get(assets::spa_fallback))
        .with_state(state)
}

/// Resolve the root document: the explicit argument (must exist inside the
/// served directory), else `./main.wcl` when present, else none.
fn resolve_root_file(root_dir: &Path, arg: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    match arg {
        Some(p) => {
            let canon = std::fs::canonicalize(&p)
                .map_err(|e| format!("root document {}: {e}", p.display()))?;
            if !canon.starts_with(root_dir) {
                return Err(format!(
                    "root document {} is outside the served directory {}",
                    canon.display(),
                    root_dir.display()
                ));
            }
            Ok(Some(canon))
        }
        None => {
            let main = root_dir.join("main.wcl");
            Ok(main.is_file().then_some(main))
        }
    }
}

/// Map a `Result<json, message>` to an HTTP response; save conflicts get a
/// real 409 so the client can offer reload-vs-overwrite.
fn api_result(r: Result<serde_json::Value, String>) -> Response {
    match r {
        Ok(v) => json_response(StatusCode::OK, &v),
        Err(e) if e.starts_with("conflict:") => json_error(StatusCode::CONFLICT, &e),
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e),
    }
}

/// `GET /api/files` — the gitignore-aware repo tree.
async fn handle_files(State(state): State<Arc<EditorState>>) -> Response {
    let state2 = Arc::clone(&state);
    run_blocking(move || files::list_tree(&state2.root_dir)).await
}

/// `GET /api/file?path=` — a text file plus its save etag.
async fn handle_file_get(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let Some(path) = query_param(&uri, "path") else {
        return json_error(StatusCode::BAD_REQUEST, "missing path");
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || files::read_text(&state2.root_dir, &path)).await
}

/// `POST /api/file` — save a buffer (validating pipeline for `.wcl`).
async fn handle_file_post(State(state): State<Arc<EditorState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || files::save_file(&state2.root_dir, state2.root_file.as_deref(), &v)).await
}

/// `POST /api/format` — canonically format a WCL buffer (`wcl fmt` core).
async fn handle_format(body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    api_result(crate::edit::format_source(&v))
}

/// `GET /api/raw?path=` — raw bytes with a best-effort content type.
async fn handle_raw(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let Some(path) = query_param(&uri, "path") else {
        return json_error(StatusCode::BAD_REQUEST, "missing path");
    };
    let state2 = Arc::clone(&state);
    let result =
        tokio::task::spawn_blocking(move || files::read_raw(&state2.root_dir, &path)).await;
    match result.unwrap_or_else(|e| Err(format!("task failed: {e}"))) {
        Ok((file, bytes)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type(&file))],
            bytes,
        )
            .into_response(),
        Err(e) => json_error(StatusCode::NOT_FOUND, &e),
    }
}

/// `GET /api/sites` — every previewable wdoc site under the served
/// directory, nested by `include` membership (the picker's tree).
async fn handle_sites(State(state): State<Arc<EditorState>>) -> Response {
    let state2 = Arc::clone(&state);
    run_blocking(move || sites::scan_sites(&state2.root_dir, state2.root_file.as_deref())).await
}

/// `POST /api/preview` — full build of the selected site (`entry` +
/// optional `site` name) with the posted unsaved buffers overlaid, into
/// the preview scratch tree. Manual-rebuild semantics: the client only
/// calls this from its Rebuild button, and the long-held POST is the
/// build's progress signal. Serialized behind the preview gate and run
/// off the async executor — a render is real work.
async fn handle_preview(State(state): State<Arc<EditorState>>, body: String) -> Response {
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
    let doc = wcl_wdoc::open_doc_for_edit_with_overlay(entry_abs, overlay.clone())
        .map_err(|e| e.to_string())?;
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

/// The posted unsaved buffers (`files: [{path, text}]`) as an overlay map,
/// sandbox-checked and canonically keyed.
fn overlay_files(
    state: &EditorState,
    v: &serde_json::Value,
) -> Result<std::collections::HashMap<PathBuf, String>, String> {
    let mut overlay = std::collections::HashMap::new();
    for f in v
        .get("files")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or_default()
    {
        let path = crate::edit::str_field(f, "path")?;
        let text = crate::edit::str_field(f, "text")?;
        let canon = crate::serve::sandboxed(&state.root_dir, &state.root_dir.join(path))
            .ok_or_else(|| format!("file outside the served tree: {path}"))?;
        overlay.insert(canon, text.to_string());
    }
    Ok(overlay)
}

/// `POST /api/object/locate` — resolve an `edit_object` button's `kind` +
/// optional `target` to the declaring `.wcl` file (repo-relative) and byte
/// span, so the client can open the source at that instance. Unsaved buffers
/// (`files`) overlay disk; `page_file` scopes the lookup to the page's owning
/// included sub-site (a wskill page resolves against its own document).
async fn handle_object_locate(State(state): State<Arc<EditorState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || locate_object(&state2, &v)).await
}

/// The blocking half of [`handle_object_locate`].
fn locate_object(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let entry = crate::edit::str_field(v, "entry")?;
    let entry_abs = crate::serve::sandboxed(&state.root_dir, &state.root_dir.join(entry))
        .ok_or_else(|| format!("file outside the served tree: {entry}"))?;
    let kind = crate::edit::str_field(v, "kind")?;
    let target = v
        .get("target")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty());

    // A page inside an included sub-site resolves against that sub-site's
    // own document, so a wskill page's kinds match its own schema.
    let doc_entry = v
        .get("page_file")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .and_then(|pf| crate::serve::sandboxed(&state.root_dir, Path::new(pf)))
        .map(|pf| wcl_wdoc::doc_entry_for_page(&entry_abs, &pf))
        .unwrap_or_else(|| entry_abs.clone());

    let overlay = overlay_files(state, v)?;
    let (file, span) = crate::edit::locate_object(&doc_entry, kind, target, overlay)?;
    let canon = std::fs::canonicalize(&file).unwrap_or(file);
    let rel = canon.strip_prefix(&state.root_dir).map_err(|_| {
        format!(
            "{} is outside the served directory — not editable here",
            canon.display()
        )
    })?;
    Ok(serde_json::json!({
        "ok": true,
        "file": rel.to_string_lossy().replace('\\', "/"),
        "span": { "start": span.start, "end": span.end },
    }))
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
async fn handle_preview_file(
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

/// Run a filesystem-touching operation off the async executor and map its
/// result to a response.
async fn run_blocking<F>(f: F) -> Response
where
    F: FnOnce() -> Result<serde_json::Value, String> + Send + 'static,
{
    let result = tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(format!("task failed: {e}")));
    api_result(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn state_for(dir: &Path, root_file: Option<PathBuf>) -> Arc<EditorState> {
        state_with_review(dir, root_file, None)
    }

    fn state_with_review(
        dir: &Path,
        root_file: Option<PathBuf>,
        review: Option<wcl_wdoc::Handshake>,
    ) -> Arc<EditorState> {
        Arc::new(EditorState {
            root_dir: std::fs::canonicalize(dir).unwrap(),
            root_file,
            preview: crate::preview::Preview::new().unwrap(),
            preview_sessions: std::sync::Mutex::new(std::collections::HashMap::new()),
            review,
        })
    }

    async fn send(
        app: Router,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(match &body {
                Some(v) => Body::from(v.to_string()),
                None => Body::empty(),
            })
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn files_file_roundtrip_and_conflict() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("a.txt"), "one").unwrap();
        let state = state_for(td.path(), None);

        let (status, v) = send(router(Arc::clone(&state)), "GET", "/api/files", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            v["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["path"] == "a.txt")
        );

        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/file?path=a.txt",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["text"], "one");
        let etag = v["etag"].as_str().unwrap().to_string();

        // Save with the right etag succeeds; a stale etag is a 409.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/file",
            Some(serde_json::json!({ "path": "a.txt", "text": "two", "base_etag": etag })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/file",
            Some(serde_json::json!({ "path": "a.txt", "text": "three", "base_etag": etag })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(v["error"].as_str().unwrap().contains("conflict"));
    }

    #[tokio::test]
    async fn format_formats_wcl() {
        let td = tempfile::tempdir().unwrap();
        let app = router(state_for(td.path(), None));
        let (status, v) = send(
            app,
            "POST",
            "/api/format",
            Some(serde_json::json!({ "text": "name=\"x\"" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["text"], "name = \"x\"\n");
    }

    const SITE_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hello preview\"\n}\n";

    #[tokio::test]
    async fn preview_builds_selected_site_with_overlay() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), SITE_DOC).unwrap();
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({ "entry": "main.wcl", "site": "docs", "files": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let href = v["href"].as_str().unwrap().to_string();
        assert!(href.starts_with("/api/preview/"), "{href}");

        let (status, _) = send(router(Arc::clone(&state)), "GET", &href, None).await;
        assert_eq!(status, StatusCode::OK);

        // Unsaved buffers overlay disk: the served HTML shows the edit.
        let edited = SITE_DOC.replace("Hello preview", "Overlaid text");
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl",
                "site": "docs",
                "files": [{ "path": "main.wcl", "text": edited }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let href = v["href"].as_str().unwrap();
        let req = Request::builder().uri(href).body(Body::empty()).unwrap();
        let resp = router(Arc::clone(&state)).oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 22)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&bytes);
        assert!(html.contains("Overlaid text"), "overlay not applied");
        // Comment anchors for the preview pane's comment UI, plus the
        // edit-mode source anchors the edit_object jump keys on — but still
        // no injected scripts (the SPA drives the iframe from outside).
        assert!(html.contains("data-wcl-page-file"), "missing page anchor");
        assert!(html.contains("data-wcl-block"), "missing block anchors");
        assert!(
            html.contains("data-wcl-span"),
            "missing source span anchors"
        );
        assert!(!html.contains("<script"), "scripts injected into preview");
    }

    /// Two-page site for the targeted-rebuild / lazy-materialization tests:
    /// `alpha` is the start page (so `index.html` is its copy), `beta` the
    /// page a targeted alpha rebuild leaves stale.
    const TWO_PAGE_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage alpha {\n  start = true\n\n  h1 \"Alpha\"\n\n  p \"alpha original\"\n}\n\npage beta {\n  h1 \"Beta\"\n\n  p \"beta original\"\n}\n";

    async fn fetch_preview(state: &Arc<EditorState>, href: &str) -> String {
        let req = Request::builder().uri(href).body(Body::empty()).unwrap();
        let resp = router(Arc::clone(state)).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "GET {href}");
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 22)
            .await
            .unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn preview_targeted_rebuild_lazily_materializes_stale_pages() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), TWO_PAGE_DOC).unwrap();
        let state = state_for(td.path(), None);

        // Cold dir: even with a page hint the first build is full.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl", "files": [], "pages": ["alpha"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["mode"], "full");
        let base = v["href"]
            .as_str()
            .unwrap()
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();

        // Warm dir + page hint: only alpha re-renders; beta.html goes stale.
        let edited = TWO_PAGE_DOC
            .replace("alpha original", "alpha edited")
            .replace("beta original", "beta edited");
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl",
                "files": [{ "path": "main.wcl", "text": edited }],
                "pages": ["alpha"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["mode"], "targeted");
        let alpha = fetch_preview(&state, &format!("{base}/alpha.html")).await;
        assert!(alpha.contains("alpha edited"), "targeted page not rebuilt");

        // Navigating to the stale page materializes it on request with the
        // same overlay the last POST built with.
        let beta = fetch_preview(&state, &format!("{base}/beta.html")).await;
        assert!(
            beta.contains("beta edited"),
            "stale page not lazily rebuilt"
        );

        // `index.html` is a copy of the start page: a targeted beta rebuild
        // leaves it stale, and a lazy fetch resolves it to `alpha` through
        // the pages.json manifest (re-copying the landing file).
        let edited2 = edited.replace("alpha edited", "alpha edited again");
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl",
                "files": [{ "path": "main.wcl", "text": edited2 }],
                "pages": ["beta"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["mode"], "targeted");
        let index = fetch_preview(&state, &format!("{base}/index.html")).await;
        assert!(
            index.contains("alpha edited again"),
            "stale start-page copy not lazily rebuilt"
        );

        // A page name that matches nothing in this entry's sites can't be
        // refreshed by the targeted path — automatic full fallback.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl", "files": [], "pages": ["no-such-page"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["mode"], "full");
    }

    /// Two-page site with per-site-hidden blocks for the merged all-views
    /// preview tests: both pages carry an `@except(sites = [:docs])` block
    /// a normal `docs` build drops.
    const MERGED_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage alpha {\n  start = true\n\n  h1 \"Alpha\"\n\n  p \"alpha original\"\n\n  @except(sites = [:docs]) p \"ALPHA_HIDDEN\"\n}\n\npage beta {\n  h1 \"Beta\"\n\n  p \"beta original\"\n\n  @except(sites = [:docs]) p \"BETA_HIDDEN\"\n}\n";

    #[tokio::test]
    async fn preview_merged_renders_all_blocks_in_its_own_slug() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), MERGED_DOC).unwrap();
        let state = state_for(td.path(), None);

        // Normal build: the `@except(:docs)` block is dropped.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({ "entry": "main.wcl", "site": "docs", "files": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let normal_base = v["href"]
            .as_str()
            .unwrap()
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();
        assert!(!normal_base.contains("__merged"), "{normal_base}");
        let alpha = fetch_preview(&state, &format!("{normal_base}/alpha.html")).await;
        assert!(!alpha.contains("ALPHA_HIDDEN"), "normal build filters");

        // Merged build: its own slug, every block rendered, visibility
        // stamped on the anchors.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl", "site": "docs", "merged": true, "files": [],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let merged_base = v["href"]
            .as_str()
            .unwrap()
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();
        assert!(merged_base.contains("__merged"), "{merged_base}");
        let alpha = fetch_preview(&state, &format!("{merged_base}/alpha.html")).await;
        assert!(alpha.contains("ALPHA_HIDDEN"), "merged build shows all");
        assert!(
            alpha.contains("data-wcl-except=\"docs\""),
            "merged build stamps visibility: {alpha}"
        );

        // The normal slug's output is untouched by the merged build.
        let alpha = fetch_preview(&state, &format!("{normal_base}/alpha.html")).await;
        assert!(!alpha.contains("ALPHA_HIDDEN"), "normal output poisoned");

        // A targeted merged rebuild of alpha leaves beta stale; the lazy
        // materialization must remember the merged flag (or beta would
        // silently re-render filtered).
        let edited = MERGED_DOC.replace("beta original", "beta edited");
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl",
                "site": "docs",
                "merged": true,
                "files": [{ "path": "main.wcl", "text": edited }],
                "pages": ["alpha"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["mode"], "targeted");
        let beta = fetch_preview(&state, &format!("{merged_base}/beta.html")).await;
        assert!(
            beta.contains("beta edited"),
            "stale page not lazily rebuilt"
        );
        assert!(
            beta.contains("BETA_HIDDEN"),
            "lazy merged rebuild lost the all-sites flag"
        );

        // merged + skill is meaningless — an explicit error.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl", "site": "docs", "merged": true, "skill": true, "files": [],
            })),
        )
        .await;
        assert_ne!(status, StatusCode::OK);
        assert!(v["error"].as_str().unwrap().contains("merged"), "{v}");
    }

    /// A document with a gathered unit kind carrying an addressable `body`
    /// — but NO page for the unit — for the synthetic merged unit preview.
    const UNIT_BODY_DOC: &str = "import <wdoc.wcl>\n\n\
        @document\ntype Doc {\n  @children(\"thing\") things: list<Thing>\n}\n\n\
        @block(\"thing\")\ntype Thing {\n  @inline(0) id: identifier\n  @child(\"body\") body: wdoc.WdocAddressableBody?\n}\n\n\
        site docs {\n  title = \"The Docs\"\n  root = true\n}\n\n\
        thing alpha {\n  body {\n    p \"ALPHA_BODY_TEXT\"\n  }\n}\n\n\
        thing beta {\n}\n\n\
        page index {\n  start = true\n\n  h1 \"Home\"\n}\n";

    #[tokio::test]
    async fn preview_merged_unit_builds_synthetic_body_page() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), UNIT_BODY_DOC).unwrap();
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl", "site": "docs", "merged": true,
                "unit": { "kind": "thing", "id": "alpha" },
                "pages": [UNIT_PREVIEW_PAGE], "files": [],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let base = v["href"]
            .as_str()
            .unwrap()
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();
        assert!(base.contains("__unit_alpha"), "own slug per unit: {base}");
        let page = fetch_preview(&state, &format!("{base}/{UNIT_PREVIEW_PAGE}.html")).await;
        assert!(page.contains("ALPHA_BODY_TEXT"), "body projected: {page}");
        // The projected blocks anchor to the REAL declaring file, so every
        // editing op (reorder, visibility, text) targets real source.
        assert!(
            page.contains("main.wcl\""),
            "anchors point at the unit's file: {page}"
        );

        // A unit without a body has nothing to render standalone.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl", "site": "docs", "merged": true,
                "unit": { "kind": "thing", "id": "beta" }, "files": [],
            })),
        )
        .await;
        assert_ne!(status, StatusCode::OK);
        assert!(v["error"].as_str().unwrap().contains("no body"), "{v}");

        // `unit` without `merged` is an explicit error.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl", "site": "docs",
                "unit": { "kind": "thing", "id": "alpha" }, "files": [],
            })),
        )
        .await;
        assert_ne!(status, StatusCode::OK);
    }

    /// A document with a user schema kind (`thing`) plus `edit_object`
    /// buttons targeting its instances.
    const OBJECT_DOC: &str = "import <wdoc.wcl>\n\n\
        @document\ntype Doc {\n  @children(\"thing\") things: list<Thing>\n}\n\n\
        @block(\"thing\")\ntype Thing {\n  @inline(0) name: utf8\n  note: utf8?\n}\n\n\
        site docs {\n  title = \"The Docs\"\n  root = true\n}\n\n\
        thing \"alpha\" {\n  note = \"first\"\n}\n\n\
        thing \"beta\" {\n  note = \"second\"\n}\n\n\
        page index {\n  title = \"Hi\"\n\n  h1 \"Hello\"\n\n  edit_object {\n    kind = \"thing\"\n    target = \"alpha\"\n  }\n}\n";

    #[tokio::test]
    async fn preview_renders_edit_object_button() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), OBJECT_DOC).unwrap();
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({ "entry": "main.wcl", "site": "docs", "files": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let href = v["href"].as_str().unwrap();
        let req = Request::builder().uri(href).body(Body::empty()).unwrap();
        let resp = router(Arc::clone(&state)).oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 22)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&bytes);
        assert!(
            html.contains("data-wcl-edit-kind=\"thing\""),
            "missing edit_object button: {html}"
        );
        assert!(html.contains("data-wcl-edit-target=\"alpha\""));
    }

    #[tokio::test]
    async fn object_locate_finds_instance_by_label() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), OBJECT_DOC).unwrap();
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/object/locate",
            Some(serde_json::json!({
                "entry": "main.wcl", "kind": "thing", "target": "beta", "files": [],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["file"], "main.wcl");
        let (start, end) = (
            v["span"]["start"].as_u64().unwrap() as usize,
            v["span"]["end"].as_u64().unwrap() as usize,
        );
        let src = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        let text = &src[start..end];
        assert!(text.starts_with("thing \"beta\""), "span slices {text:?}");
    }

    #[tokio::test]
    async fn object_locate_respects_overlay_and_errors() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), OBJECT_DOC).unwrap();
        let state = state_for(td.path(), None);

        // Overlay renames beta → gamma: gamma resolves, beta no longer does.
        let edited = OBJECT_DOC.replace("thing \"beta\"", "thing \"gamma\"");
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/object/locate",
            Some(serde_json::json!({
                "entry": "main.wcl", "kind": "thing", "target": "gamma",
                "files": [{ "path": "main.wcl", "text": edited }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let (start, end) = (
            v["span"]["start"].as_u64().unwrap() as usize,
            v["span"]["end"].as_u64().unwrap() as usize,
        );
        assert!(edited[start..end].starts_with("thing \"gamma\""));

        // Unknown target → clean 400 naming the kind.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/object/locate",
            Some(serde_json::json!({
                "entry": "main.wcl", "kind": "thing", "target": "nope", "files": [],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("thing"));

        // No target with two instances → 400 listing the labels.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/object/locate",
            Some(serde_json::json!({ "entry": "main.wcl", "kind": "thing", "files": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("alpha"));

        // An escaping entry → 400.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/object/locate",
            Some(serde_json::json!({ "entry": "../x.wcl", "kind": "thing", "files": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn comments_add_list_edit_resolve_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), SITE_DOC).unwrap();
        let state = state_for(td.path(), None);
        let page_file = state.root_dir.join("main.wcl").display().to_string();

        // Page comment + block comment.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/comments",
            Some(serde_json::json!({
                "page": "index", "page_file": page_file, "body": "page note",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let page_id = v["id"].as_str().unwrap().to_string();
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/comments",
            Some(serde_json::json!({
                "page": "index", "page_file": page_file, "loc": "0",
                "target": "h1 — \"Hello preview\"", "body": "block note", "quote": "Hello",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let block_id = v["id"].as_str().unwrap().to_string();

        let (status, v) = send(router(Arc::clone(&state)), "GET", "/api/comments", None).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let list = v["comments"].as_array().unwrap();
        assert_eq!(list.len(), 2, "{v:#}");
        let block = list.iter().find(|c| c["id"] == block_id.as_str()).unwrap();
        assert_eq!(block["scope"], "block");
        assert_eq!(block["loc"], "0");
        assert_eq!(block["quote"], "Hello");
        let page = list.iter().find(|c| c["id"] == page_id.as_str()).unwrap();
        assert_eq!(page["scope"], "page");
        assert!(page["loc"].is_null());

        // Edit the block comment's body; resolve the page comment.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/comments/edit",
            Some(serde_json::json!({ "id": block_id, "body": "sharper note" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/comments/resolve",
            Some(serde_json::json!({ "id": page_id })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, v) = send(router(Arc::clone(&state)), "GET", "/api/comments", None).await;
        let list = v["comments"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["body"], "sharper note");

        // Unknown ids are clean 404s.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/comments/resolve",
            Some(serde_json::json!({ "id": "cnothere" })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn comment_add_scopes_to_wskill_sidecar_and_validates() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/wskill.wcl"), "// wskill marker\n").unwrap();
        std::fs::write(root.join("sub/page.wcl"), "// page source\n").unwrap();
        let state = state_for(root, None);
        let page_file = state.root_dir.join("sub/page.wcl").display().to_string();

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/comments",
            Some(serde_json::json!({ "page": "p", "page_file": page_file, "body": "note" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert!(state.root_dir.join("sub/comments.wcl").is_file());
        assert!(!state.root_dir.join("comments.wcl").exists());

        // A page_file outside the served tree falls back to the root sidecar.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/comments",
            Some(serde_json::json!({ "page": "p", "page_file": "/nowhere/x.wcl", "body": "n" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(state.root_dir.join("comments.wcl").is_file());

        // Missing page / body are 400s.
        for bad in [
            serde_json::json!({ "body": "n" }),
            serde_json::json!({ "page": "p", "body": "  " }),
        ] {
            let (status, _) = send(
                router(Arc::clone(&state)),
                "POST",
                "/api/comments",
                Some(bad),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test]
    async fn review_status_and_ready_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), "x = 1\n").unwrap();
        let hs = wcl_wdoc::Handshake::new(&td.path().join("main.wcl"));
        hs.serve_started().unwrap();
        let state = state_with_review(td.path(), None, Some(hs.clone()));

        // No agent yet: round 0 matches, but the long-poll deadline would
        // park — ask with a mismatched round to get an immediate answer.
        let round = hs.begin_wait().unwrap();
        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/review/status?round=0",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["waiting"], true);
        // The round is serialized as a string — a u64 nanosecond stamp
        // exceeds JS number precision, so it must travel opaquely.
        assert_eq!(v["round"].as_str().unwrap(), round.to_string());

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/review/ready",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert!(hs.released(round));
        hs.end_wait();
        hs.serve_stopped();
    }

    #[tokio::test]
    async fn review_endpoints_without_root_doc() {
        let td = tempfile::tempdir().unwrap();
        let state = state_for(td.path(), None);
        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/review/status?round=0",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["waiting"], false);
        let (status, _) = send(router(state), "POST", "/api/review/ready", None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// A skill view previews as the built skill folder: the file listing +
    /// browsable contents, not an HTML site.
    #[tokio::test]
    async fn preview_skill_builds_folder_listing() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join("main.wcl"),
            "import <wdoc.wcl>\n\nsite skill {\n  default_template = :ai_skill\n  root = true\n\n  skill {\n    name = \"demo\"\n    description = \"A demo skill.\"\n  }\n}\n\npage index {\n  title = \"Demo skill\"\n  start = true\n\n  h1 \"Demo\"\n\n  p \"Start page prose.\"\n}\n\npage extra {\n  title = \"Extra\"\n\n  p \"Reference prose.\"\n}\n",
        )
        .unwrap();
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl", "site": "skill", "files": [], "skill": true,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["mode"], "skill");
        let files: Vec<&str> = v["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f.as_str())
            .collect();
        assert!(files.contains(&"SKILL.md"), "{files:?}");
        assert!(
            files.iter().any(|f| f.starts_with("references/")),
            "{files:?}"
        );
        // The listed files serve through the preview file route.
        let href = format!("{}SKILL.md", v["base"].as_str().unwrap());
        let req = Request::builder().uri(&href).body(Body::empty()).unwrap();
        let resp = router(Arc::clone(&state)).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Start page prose."), "{text}");
    }

    #[tokio::test]
    async fn preview_rejects_escaping_entry() {
        let td = tempfile::tempdir().unwrap();
        let app = router(state_for(td.path(), None));
        let (status, v) = send(
            app,
            "POST",
            "/api/preview",
            Some(serde_json::json!({ "entry": "../escape.wcl", "site": null, "files": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("outside"));
    }

    #[tokio::test]
    async fn sites_lists_nested_sites() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::write(
            root.join("main.wcl"),
            "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\nsite deck {\n  default_template = :ai_skill\n}\n\npage index {\n  title = \"Hi\"\n  sites = [:docs]\n\n  h1 \"Hi\"\n}\n\ninclude \"members\" {\n  entry = \"main.wcl\"\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("members/alpha")).unwrap();
        std::fs::write(
            root.join("members/alpha/main.wcl"),
            "import <wdoc.wcl>\n\nsite book {\n  title = \"Alpha Book\"\n}\n\npage index {\n  title = \"Alpha\"\n\n  h1 \"Alpha\"\n}\n",
        )
        .unwrap();

        let (status, v) = send(router(state_for(root, None)), "GET", "/api/sites", None).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let sites = v["sites"].as_array().unwrap();
        // The member is claimed by the include, so only main.wcl's two
        // sites list at the top level.
        assert_eq!(sites.len(), 2, "{v:#}");
        let docs = &sites[0];
        assert_eq!(docs["entry"], "main.wcl");
        assert_eq!(docs["site"], "docs");
        assert_eq!(docs["label"], "The Docs");
        assert_eq!(docs["skill"], false);
        let children = docs["children"].as_array().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["entry"], "members/alpha/main.wcl");
        assert_eq!(children[0]["label"], "Alpha Book");
        let deck = &sites[1];
        assert_eq!(deck["site"], "deck");
        assert_eq!(deck["skill"], true);
        assert!(deck["children"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn raw_serves_bytes_and_rejects_escape() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("img.bin"), b"\x00\x01\x02").unwrap();
        let state = state_for(td.path(), None);

        let req = Request::builder()
            .uri("/api/raw?path=img.bin")
            .body(Body::empty())
            .unwrap();
        let resp = router(Arc::clone(&state)).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        assert_eq!(&bytes[..], b"\x00\x01\x02");

        let (status, _) = send(router(state), "GET", "/api/raw?path=../escape", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn spa_fallback_serves_index_for_routes() {
        let td = tempfile::tempdir().unwrap();
        let app = router(state_for(td.path(), None));
        // Any extension-less path outside /api serves the embedded index.
        let req = Request::builder()
            .uri("/some/client/route")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers()[axum::http::header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .to_string();
        assert!(ct.starts_with("text/html"), "{ct}");
    }

    /// Find the span of the first block matching `pred` in `text`, walking
    /// the parsed AST the way the edit path does.
    fn span_of(text: &str, pred: impl Fn(&wcl_lang::ast::Block) -> bool) -> wcl_lang::Span {
        fn walk<'a>(
            items: &'a [wcl_lang::ast::Item],
            pred: &impl Fn(&wcl_lang::ast::Block) -> bool,
        ) -> Option<&'a wcl_lang::ast::Block> {
            for item in items {
                if let wcl_lang::ast::Item::Block(b) = item {
                    if pred(b) {
                        return Some(b);
                    }
                    if let Some(found) = walk(&b.items, pred) {
                        return Some(found);
                    }
                }
            }
            None
        }
        let src = wcl_lang::parse_for_edit(text, "t").unwrap();
        walk(&src.items, &pred).expect("no block matched").span
    }

    fn span_json(span: wcl_lang::Span) -> serde_json::Value {
        serde_json::json!({ "start": span.start, "end": span.end })
    }

    const BODY_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  p \"First paragraph\"\n\n  p \"Second paragraph\"\n}\n";

    #[tokio::test]
    async fn block_ops_edit_insert_move_delete() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), BODY_DOC).unwrap();
        let state = state_for(td.path(), None);
        let disk = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        let etag = crate::edit::content_etag(&disk);
        let first_p = span_of(&disk, |b| {
            b.kind == "p"
                && matches!(b.labels.first(), Some(wcl_lang::ast::Expr::Utf8(s)) if s.starts_with("First"))
        });

        // Edit the paragraph text + insert a callout after it, atomically.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl", "etag": etag,
                "ops": [
                    { "op": "set_label", "span": span_json(first_p), "slot": 0,
                      "text": "Edited paragraph" },
                    { "op": "insert_after", "span": span_json(first_p),
                      "source": "callout \"Note\" {\n  body = \"hi\"\n}" },
                ],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let new_text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert_eq!(v["file_text"], new_text.as_str());
        assert_eq!(v["etag"], crate::edit::content_etag(&new_text).as_str());
        assert!(new_text.contains("Edited paragraph"), "{new_text}");
        // The response spans slice the *new* text at the right blocks.
        let spans = v["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 2, "{v:#}");
        let slice = |s: &serde_json::Value| {
            let (a, b) = (
                s["span"]["start"].as_u64().unwrap() as usize,
                s["span"]["end"].as_u64().unwrap() as usize,
            );
            new_text[a..b].to_string()
        };
        assert_eq!(spans[0]["role"], "edited");
        assert!(slice(&spans[0]).starts_with("p \"Edited paragraph\""));
        assert_eq!(spans[1]["role"], "inserted");
        assert!(slice(&spans[1]).starts_with("callout \"Note\""));
        // Order in the page: edited p, callout, second p.
        let ei = new_text.find("Edited paragraph").unwrap();
        let ci = new_text.find("callout").unwrap();
        let si = new_text.find("Second paragraph").unwrap();
        assert!(ei < ci && ci < si, "{new_text}");

        // Move the callout below the second paragraph, then delete it.
        let callout = span_of(&new_text, |b| b.kind == "callout");
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "move", "span": span_json(callout), "dir": "down" }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(text.find("Second paragraph").unwrap() < text.find("callout").unwrap());
        let callout = span_of(&text, |b| b.kind == "callout");
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "delete", "span": span_json(callout) }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(!text.contains("callout"), "{text}");
    }

    #[tokio::test]
    async fn block_source_classifies_literal_list_tables() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  table {\n    header = [\"Signal\", \"Plain\"]\n    rows = [[\"Audience\", \"AI agents\"], [\"Lifespan\", \"Long-lived\"]]\n  }\n}\n";
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), doc).unwrap();
        let state = state_for(td.path(), None);
        let disk = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        let table = span_of(&disk, |b| b.kind == "table");

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/source",
            Some(serde_json::json!({ "file": "main.wcl", "span": span_json(table) })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        // All-string list → `list` with items; list-of-lists → `rows`.
        assert_eq!(v["fields"]["header"]["state"], "list", "{v:#}");
        assert_eq!(v["fields"]["header"]["items"][1], "Plain");
        assert_eq!(v["fields"]["rows"]["state"], "rows", "{v:#}");
        assert_eq!(v["fields"]["rows"]["rows"][1][0], "Lifespan");

        // A computed rows expression stays `computed` (no grid).
        let doc2 = doc.replace(
            "rows = [[\"Audience\", \"AI agents\"], [\"Lifespan\", \"Long-lived\"]]",
            "rows = map([\"x\"], fn(s: utf8) -> list<utf8> { [s, s] })",
        );
        std::fs::write(td.path().join("main.wcl"), &doc2).unwrap();
        let table = span_of(&doc2, |b| b.kind == "table");
        let (_, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/source",
            Some(serde_json::json!({ "file": "main.wcl", "span": span_json(table) })),
        )
        .await;
        assert_eq!(v["fields"]["rows"]["state"], "computed", "{v:#}");
    }

    #[tokio::test]
    async fn block_ops_remove_field_on_nested_shape() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  diagram {\n    width = 320\n    height = 160\n\n    rect {\n      id = a\n      x = 20.0\n      y = 30.0\n      width = 80.0\n      height = 50.0\n      fill = \"#88c0d0\"\n    }\n  }\n}\n";
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), doc).unwrap();
        let state = state_for(td.path(), None);
        let disk = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        let rect = span_of(&disk, |b| b.kind == "rect");

        // Reset-position batch: drop x/y plus a field that was never there —
        // absent fields are tolerated so clients can batch removals blindly.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [
                    { "op": "remove_field", "span": span_json(rect), "field": "x" },
                    { "op": "remove_field", "span": span_json(rect), "field": "y" },
                    { "op": "remove_field", "span": span_json(rect), "field": "cx" },
                ],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(!text.contains("x = 20"), "{text}");
        assert!(!text.contains("y = 30"), "{text}");
        assert!(text.contains("fill = \"#88c0d0\""), "{text}");
        // The edited span in the response slices the rect in the new text.
        let s = &v["spans"].as_array().unwrap()[0];
        let (a, b) = (
            s["span"]["start"].as_u64().unwrap() as usize,
            s["span"]["end"].as_u64().unwrap() as usize,
        );
        assert!(text[a..b].starts_with("rect"), "{}", &text[a..b]);
    }

    #[tokio::test]
    async fn block_ops_conflict_and_rollback() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), BODY_DOC).unwrap();
        let state = state_for(td.path(), None);
        let disk = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        let p = span_of(&disk, |b| b.kind == "p");

        // A stale etag is a 409 and leaves the file untouched.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl", "etag": "stale",
                "ops": [{ "op": "delete", "span": span_json(p) }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{v}");
        assert_eq!(
            std::fs::read_to_string(td.path().join("main.wcl")).unwrap(),
            disk
        );

        // An edit that breaks the schema (a `page` needs a `title`) rolls
        // back: 400, disk unchanged.
        let page = span_of(&disk, |b| b.kind == "page");
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "replace_source", "span": span_json(page),
                          "source": "page index {\n  title = 42\n}" }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
        assert_eq!(
            std::fs::read_to_string(td.path().join("main.wcl")).unwrap(),
            disk,
            "schema-breaking edit must roll back"
        );

        // A bad fragment (two blocks) is rejected before anything happens.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "insert_after", "span": span_json(p),
                          "source": "p \"a\"\n\np \"b\"" }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("exactly one block"));
    }

    #[tokio::test]
    async fn block_source_classifies_slots() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\nlet greeting = \"hi\"\n\npage index {\n  title = \"Hi\"\n\n  p \"Literal text\"\n\n  p $\"computed ${greeting}\"\n}\n";
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), doc).unwrap();
        let state = state_for(td.path(), None);
        let literal = span_of(doc, |b| {
            b.kind == "p" && matches!(b.labels.first(), Some(wcl_lang::ast::Expr::Utf8(_)))
        });
        let computed = span_of(doc, |b| {
            b.kind == "p"
                && matches!(
                    b.labels.first(),
                    Some(wcl_lang::ast::Expr::InterpolatedString { .. })
                )
        });

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/source",
            Some(serde_json::json!({ "file": "main.wcl", "span": span_json(literal) })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["kind"], "p");
        assert_eq!(v["source"], "p \"Literal text\"");
        assert_eq!(v["labels"][0]["state"], "literal");
        assert_eq!(v["labels"][0]["text"], "Literal text");

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/source",
            Some(serde_json::json!({ "file": "main.wcl", "span": span_json(computed) })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["labels"][0]["state"], "computed");
        assert!(v["labels"][0]["text"].is_null());
    }

    #[tokio::test]
    async fn unit_field_and_unit_create_append_mode() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), OBJECT_DOC).unwrap();
        let state = state_for(td.path(), None);

        // Set a field on a located object.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/unit/field",
            Some(serde_json::json!({
                "entry": "main.wcl", "kind": "thing", "target": "alpha",
                "field": "note", "value": "updated note",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(text.contains("note = \"updated note\""), "{text}");

        // Create a new instance: appended to the file already holding the
        // most `thing`s (main.wcl), duplicate ids rejected.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/unit/create",
            Some(serde_json::json!({
                "entry": "main.wcl",
                "unit": { "kind": "thing", "id": "gamma",
                          "fields": { "note": "third" } },
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["file"], "main.wcl");
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(text.contains("thing \"gamma\""), "{text}");
        assert!(text.contains("note = \"third\""), "{text}");
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/unit/create",
            Some(serde_json::json!({
                "entry": "main.wcl",
                "unit": { "kind": "thing", "id": "gamma" },
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("already exists"));
    }

    /// A miniature wskill: a gathered `topic`, `concept` units one-per-file
    /// under `data/concepts/` with a `main.wcl` aggregator, and an `index`
    /// pinning them.
    fn write_mini_wskill(root: &Path) {
        std::fs::write(
            root.join("main.wcl"),
            "import <wdoc.wcl>\nimport \"data/concepts/main.wcl\"\nimport \"data/indexes.wcl\"\n\n\
             @document\ntype Doc {\n  @children(\"topic\") topics: list<Topic>\n  @children(\"concept\") concepts: list<Concept>\n  @children(\"index\") indexes: list<Index>\n}\n\n\
             @block(\"topic\")\ntype Topic {\n  @inline(0) id: identifier\n}\n\n\
             @block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n}\n\n\
             @block(\"index\")\ntype Index {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n}\n\n\
             topic mini {}\n\n\
             site book {\n  title = \"Mini\"\n  root = true\n  toc {\n    chapter \"Overview\" {\n      page = index\n    }\n  }\n}\n\n\
             page index {\n  title = \"Hi\"\n\n  h1 \"Mini\"\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("data/concepts")).unwrap();
        std::fs::write(
            root.join("data/concepts/main.wcl"),
            "import \"./alpha.wcl\"\nimport \"./beta.wcl\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("data/concepts/alpha.wcl"),
            "concept alpha {\n  name = \"Alpha\"\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("data/concepts/beta.wcl"),
            "concept beta {\n  name = \"Beta\"\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("data/indexes.wcl"),
            "index lang {\n  name = \"Language\"\n  related = [alpha, beta]\n}\n",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn wskill_nav_model_and_related_ops() {
        let td = tempfile::tempdir().unwrap();
        write_mini_wskill(td.path());
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/nav?entry=main.wcl&site=book",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["wskill"], true);
        assert_eq!(v["site_type"], "book");
        let nav = v["nav"].as_array().unwrap();
        assert_eq!(nav.len(), 1, "{v:#}");
        assert_eq!(nav[0]["kind"], "index");
        assert_eq!(nav[0]["title"], "Language");
        let children = nav[0]["children"].as_array().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0]["title"], "Alpha");
        assert_eq!(children[0]["page"], "concept_alpha");
        assert_eq!(children[0]["source"]["file"], "data/concepts/alpha.wcl");
        let units = v["units"].as_array().unwrap();
        assert_eq!(units.len(), 2);

        // Reorder, unpin, re-pin.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/nav/op",
            Some(serde_json::json!({
                "entry": "main.wcl", "op": "reorder_children",
                "index_id": "lang", "order": ["beta", "alpha"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let text = std::fs::read_to_string(td.path().join("data/indexes.wcl")).unwrap();
        assert!(text.contains("related = [beta, alpha]"), "{text}");
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/nav/op",
            Some(serde_json::json!({
                "entry": "main.wcl", "op": "unpin_unit",
                "index_id": "lang", "unit_id": "alpha",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let text = std::fs::read_to_string(td.path().join("data/indexes.wcl")).unwrap();
        assert!(text.contains("related = [beta]"), "{text}");
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/nav/op",
            Some(serde_json::json!({
                "entry": "main.wcl", "op": "pin_unit",
                "index_id": "lang", "unit_id": "alpha",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let text = std::fs::read_to_string(td.path().join("data/indexes.wcl")).unwrap();
        assert!(text.contains("related = [beta, alpha]"), "{text}");

        // A bad permutation is rejected.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/nav/op",
            Some(serde_json::json!({
                "entry": "main.wcl", "op": "reorder_children",
                "index_id": "lang", "order": ["beta"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("permutation"));
    }

    #[tokio::test]
    async fn unit_create_per_file_layout_with_pin() {
        let td = tempfile::tempdir().unwrap();
        write_mini_wskill(td.path());
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/unit/create",
            Some(serde_json::json!({
                "entry": "main.wcl",
                "unit": { "kind": "concept", "id": "gamma",
                          "fields": { "name": "Gamma" } },
                "pin": { "index_id": "lang" },
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["file"], "data/concepts/gamma.wcl");
        // One-per-file layout: its own file, imported by the aggregator,
        // pinned into the index — all in one commit.
        let unit = std::fs::read_to_string(td.path().join("data/concepts/gamma.wcl")).unwrap();
        assert!(unit.contains("concept gamma"), "{unit}");
        assert!(unit.contains("name = \"Gamma\""), "{unit}");
        let agg = std::fs::read_to_string(td.path().join("data/concepts/main.wcl")).unwrap();
        assert!(agg.contains("import \"./gamma.wcl\""), "{agg}");
        let idx = std::fs::read_to_string(td.path().join("data/indexes.wcl")).unwrap();
        assert!(idx.contains("related = [alpha, beta, gamma]"), "{idx}");
        // The new unit shows up in the nav model.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/nav?entry=main.wcl&site=book",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let units = v["units"].as_array().unwrap();
        assert!(units.iter().any(|u| u["id"] == "gamma"), "{v:#}");
    }

    #[tokio::test]
    async fn palette_lists_kinds_and_components() {
        let td = tempfile::tempdir().unwrap();
        let doc = format!(
            "{OBJECT_DOC}\nwdoc_component metric_card {{\n  wdoc_slot label\n  wdoc_slot status {{\n    default = \"ok\"\n  }}\n  wdoc_body {{\n    p $\"${{label}}\"\n  }}\n}}\n"
        );
        std::fs::write(td.path().join("main.wcl"), doc).unwrap();
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/palette?entry=main.wcl&site=docs",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["site_type"], "book");
        assert_eq!(v["wskill"], false);
        // The user schema kind, with introspected fields.
        let kinds = v["unit_kinds"].as_array().unwrap();
        let thing = kinds
            .iter()
            .find(|k| k["kind"] == "thing")
            .unwrap_or_else(|| panic!("no thing kind: {v:#}"));
        let fields = thing["fields"].as_array().unwrap();
        let name = fields.iter().find(|f| f["name"] == "name").unwrap();
        assert_eq!(name["inline_slot"], 0);
        assert_eq!(name["optional"], false);
        let note = fields.iter().find(|f| f["name"] == "note").unwrap();
        assert_eq!(note["optional"], true);
        // wdoc's own document gathers (site, page, …) are not offered.
        assert!(
            !kinds
                .iter()
                .any(|k| k["kind"] == "site" || k["kind"] == "page"),
            "{v:#}"
        );
        // Curated body kinds carry insertion snippets.
        let body = v["body_kinds"].as_array().unwrap();
        assert!(body.iter().any(|k| k["kind"] == "p"));
        assert!(
            body.iter()
                .all(|k| k["template_source"].as_str().is_some_and(|s| !s.is_empty()))
        );
        // Diagram shape kinds: SvgBlock descendants with introspected fields.
        let shapes = v["diagram_kinds"].as_array().unwrap();
        let process = shapes
            .iter()
            .find(|k| k["kind"] == "process")
            .unwrap_or_else(|| panic!("no process shape kind: {v:#}"));
        let pf = process["fields"].as_array().unwrap();
        for want in ["x", "y", "width", "height"] {
            assert!(pf.iter().any(|f| f["name"] == want), "{v:#}");
        }
        assert!(shapes.iter().any(|k| k["kind"] == "rect"), "{v:#}");
        // Page-level HTML blocks don't extend SvgBlock.
        assert!(!shapes.iter().any(|k| k["kind"] == "diagram"), "{v:#}");
        // The authored component with its slot contract.
        let comps = v["components"].as_array().unwrap();
        let card = comps.iter().find(|c| c["name"] == "metric_card").unwrap();
        let slots = card["slots"].as_array().unwrap();
        assert_eq!(slots.len(), 2, "{v:#}");
        let label = slots.iter().find(|s| s["name"] == "label").unwrap();
        assert_eq!(label["required"], true);
        let status_slot = slots.iter().find(|s| s["name"] == "status").unwrap();
        assert_eq!(status_slot["required"], false);
        assert_eq!(status_slot["default"], "ok");
    }

    #[tokio::test]
    async fn static_book_nav_and_ops() {
        let td = tempfile::tempdir().unwrap();
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n  toc {\n    chapter \"Start\" {\n      page = index\n    }\n    chapter \"Guides\" {\n      chapter \"Deep\" {\n        page = deep\n      }\n    }\n  }\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hello\"\n}\n\npage deep {\n  title = \"Deep\"\n\n  h1 \"Deep\"\n}\n";
        std::fs::write(td.path().join("main.wcl"), doc).unwrap();
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/nav?entry=main.wcl&site=docs",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["site_type"], "book");
        let nav = v["nav"].as_array().unwrap();
        assert_eq!(nav.len(), 2);
        assert_eq!(nav[0]["kind"], "chapter");
        assert_eq!(nav[0]["title"], "Start");
        assert_eq!(nav[0]["page"], "index");
        assert_eq!(nav[1]["children"][0]["title"], "Deep");
        assert!(v["container"]["span"]["start"].is_u64());
        let pages = v["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 2);

        // Rename a chapter, move it, then add a page linked into the toc.
        let start = &nav[0]["source"];
        let (status, v2) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/nav/op",
            Some(serde_json::json!({
                "entry": "main.wcl", "op": "rename", "kind": "chapter",
                "file": start["file"], "span": start["span"], "title": "Begin",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v2}");
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(text.contains("chapter \"Begin\""), "{text}");

        // Re-read (spans shifted), then move "Begin" down.
        let (_, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/nav?entry=main.wcl&site=docs",
            None,
        )
        .await;
        let begin = &v["nav"][0]["source"];
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/nav/op",
            Some(serde_json::json!({
                "entry": "main.wcl", "op": "move", "dir": "down",
                "file": begin["file"], "span": begin["span"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(
            text.find("chapter \"Guides\"").unwrap() < text.find("chapter \"Begin\"").unwrap(),
            "{text}"
        );

        // Add a page + its chapter entry in one op.
        let (_, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/nav?entry=main.wcl&site=docs",
            None,
        )
        .await;
        let container = &v["container"];
        let (status, v2) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/nav/op",
            Some(serde_json::json!({
                "entry": "main.wcl", "op": "add_page",
                "name": "faq", "title": "FAQ",
                "nav": { "container_span": container["span"], "kind": "chapter" },
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v2}");
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(text.contains("page faq"), "{text}");
        assert!(text.contains("chapter \"FAQ\""), "{text}");
        // The new page + entry are in the model.
        let (_, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/nav?entry=main.wcl&site=docs",
            None,
        )
        .await;
        assert!(
            v["nav"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["title"] == "FAQ" && n["page"] == "faq"),
            "{v:#}"
        );
    }

    #[tokio::test]
    async fn sites_group_wskill_views() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("wdoc/book")).unwrap();
        std::fs::create_dir_all(root.join("wdoc/skill")).unwrap();
        std::fs::write(
            root.join("wskill.wcl"),
            "topic demo {\n  name = \"Demo Topic\"\n}\n\n\
             artifact book {\n  kind = :book\n  entry = \"wdoc/book/main.wcl\"\n}\n\n\
             artifact ai_skill {\n  kind = :ai_skill\n  entry = \"wdoc/skill/main.wcl\"\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("wdoc/book/main.wcl"),
            "import <wdoc.wcl>\n\nsite book {\n  title = \"Demo Book\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("wdoc/skill/main.wcl"),
            "import <wdoc.wcl>\n\nsite skill {\n  default_template = :ai_skill\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n",
        )
        .unwrap();
        // A plain, unrelated site stays a normal node.
        std::fs::write(
            root.join("other.wcl"),
            "import <wdoc.wcl>\n\nsite docs {\n  title = \"Other\"\n  root = true\n}\n\npage index {\n  title = \"O\"\n\n  h1 \"O\"\n}\n",
        )
        .unwrap();

        let (status, v) = send(router(state_for(root, None)), "GET", "/api/sites", None).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let sites = v["sites"].as_array().unwrap();
        let wskill = sites
            .iter()
            .find(|s| s["wskill"] == true)
            .unwrap_or_else(|| panic!("no grouped wskill node: {v:#}"));
        assert_eq!(wskill["label"], "Demo Topic");
        assert_eq!(wskill["root"], "");
        let views = wskill["views"].as_array().unwrap();
        assert_eq!(views.len(), 2, "{v:#}");
        assert_eq!(views[0]["kind"], "book");
        assert_eq!(views[0]["entry"], "wdoc/book/main.wcl");
        assert_eq!(views[0]["site"], "book");
        assert_eq!(views[0]["skill"], false);
        assert_eq!(views[1]["kind"], "ai_skill");
        assert_eq!(views[1]["skill"], true);
        // The projections no longer list as separate top-level nodes; the
        // unrelated site does.
        assert!(
            !sites
                .iter()
                .any(|s| s["entry"] == "wdoc/book/main.wcl" || s["entry"] == "wdoc/skill/main.wcl"),
            "{v:#}"
        );
        assert!(sites.iter().any(|s| s["entry"] == "other.wcl"));
    }

    /// A projection pulled in by a parent site's `include` must not list
    /// twice: it moves into the grouped wskill node and disappears from
    /// the parent's children.
    #[tokio::test]
    async fn sites_group_wskill_views_nested_under_include() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("skills/demo/wdoc/book")).unwrap();
        std::fs::write(
            root.join("main.wcl"),
            "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n\ninclude \"skills\" {\n  entry = \"wdoc/book/main.wcl\"\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("skills/demo/wskill.wcl"),
            "topic demo {\n  name = \"Demo Topic\"\n}\n\nartifact book {\n  kind = :book\n  entry = \"wdoc/book/main.wcl\"\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("skills/demo/wdoc/book/main.wcl"),
            "import <wdoc.wcl>\n\nsite book {\n  title = \"Demo Book\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n",
        )
        .unwrap();

        let (status, v) = send(router(state_for(root, None)), "GET", "/api/sites", None).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let sites = v["sites"].as_array().unwrap();
        let docs = sites.iter().find(|s| s["entry"] == "main.wcl").unwrap();
        // The included book left the docs node's children…
        assert!(
            docs["children"].as_array().unwrap().is_empty(),
            "included projection must move into the wskill node: {v:#}"
        );
        // …and lives exactly once, as the grouped wskill's view.
        let wskill = sites.iter().find(|s| s["wskill"] == true).unwrap();
        assert_eq!(wskill["label"], "Demo Topic");
        let views = wskill["views"].as_array().unwrap();
        assert_eq!(views.len(), 1, "{v:#}");
        assert_eq!(views[0]["entry"], "skills/demo/wdoc/book/main.wcl");
        fn count_entry(nodes: &[serde_json::Value], entry: &str) -> usize {
            nodes
                .iter()
                .map(|n| {
                    usize::from(n["entry"] == entry)
                        + n["children"]
                            .as_array()
                            .map(|c| count_entry(c, entry))
                            .unwrap_or(0)
                })
                .sum()
        }
        assert_eq!(count_entry(sites, "skills/demo/wdoc/book/main.wcl"), 0);
    }

    #[tokio::test]
    async fn visibility_toggle_round_trip() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), BODY_DOC).unwrap();
        let state = state_for(td.path(), None);
        let disk = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        let p = span_of(&disk, |b| b.kind == "p");

        // Hide the paragraph from the deck + training views.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "set_visibility", "span": span_json(p),
                          "except_sites": ["deck", "training"] }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(
            text.contains("@except(sites = [:deck, :training])"),
            "{text}"
        );

        // The classification reflects it.
        let p2 = span_of(&text, |b| b.kind == "p");
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/source",
            Some(serde_json::json!({ "file": "main.wcl", "span": span_json(p2) })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["visibility"]["custom"], false);
        assert_eq!(
            v["visibility"]["except_sites"],
            serde_json::json!(["deck", "training"])
        );

        // Empty list removes the decorator again.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "set_visibility", "span": span_json(p2),
                          "except_sites": [] }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let text = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(!text.contains("@except"), "{text}");

        // A block with @only is custom: classified, and the toggle refuses.
        let custom_doc = BODY_DOC.replace(
            "  p \"First paragraph\"",
            "  @only(sites = [:docs])\n  p \"First paragraph\"",
        );
        std::fs::write(td.path().join("main.wcl"), &custom_doc).unwrap();
        let pc = span_of(&custom_doc, |b| b.kind == "p");
        let (_, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/source",
            Some(serde_json::json!({ "file": "main.wcl", "span": span_json(pc) })),
        )
        .await;
        assert_eq!(v["visibility"]["custom"], true);
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "set_visibility", "span": span_json(pc),
                          "except_sites": ["deck"] }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("custom"));
    }

    /// The Data mode surface: `@wdoc.editable` registers a type; rows list
    /// with cell classification; adds honour the decorator's target file
    /// (creating + importing it on first use); edits/deletes reuse block ops.
    #[tokio::test]
    async fn data_editor_types_rows_and_crud() {
        let td = tempfile::tempdir().unwrap();
        let doc = "import <wdoc.wcl>\n\n\
            @document\ntype Doc {\n  @children(\"character\") characters: list<Character>\n}\n\n\
            @block(\"character\") @wdoc.editable(\"data/characters.wcl\")\ntype Character {\n  @inline(0) id: identifier\n  name: utf8\n  hp: i64?\n}\n\n\
            site docs {\n  title = \"D\"\n  root = true\n}\n\n\
            character hero {\n  name = \"Hero\"\n  hp = 10\n}\n\n\
            page index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n";
        std::fs::write(td.path().join("main.wcl"), doc).unwrap();
        let state = state_for(td.path(), None);

        // Types: the registered kind with metadata + resolved target file.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/data/types?entry=main.wcl",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let types = v["types"].as_array().unwrap();
        assert_eq!(types.len(), 1, "{v:#}");
        assert_eq!(types[0]["kind"], "character");
        assert_eq!(types[0]["file"], "data/characters.wcl");
        assert!(
            types[0]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["name"] == "hp")
        );

        // Rows: the existing instance, cells classified.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/data/rows?entry=main.wcl&kind=character",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let rows = v["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["label"], "hero");
        assert_eq!(rows[0]["cells"]["name"]["state"], "literal");
        assert_eq!(rows[0]["cells"]["name"]["text"], "Hero");
        // Numbers are editable too — written back as parsed WCL.
        assert_eq!(rows[0]["cells"]["hp"]["state"], "literal");
        assert_eq!(rows[0]["cells"]["hp"]["text"], "10");
        assert_eq!(rows[0]["cells"]["hp"]["expr"], true);

        // Add a row into the decorator's target file (created + imported).
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/unit/create",
            Some(serde_json::json!({
                "entry": "main.wcl",
                "unit": { "kind": "character", "id": "villain",
                          "fields": { "name": "Villain", "hp": 13 },
                          "file": "data/characters.wcl" },
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["file"], "data/characters.wcl");
        let created = std::fs::read_to_string(td.path().join("data/characters.wcl")).unwrap();
        assert!(created.contains("character villain"), "{created}");
        assert!(created.contains("hp = 13"), "{created}");
        let main = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        assert!(main.contains("import \"data/characters.wcl\""), "{main}");

        // Both rows list now; edit one cell + delete the other via block ops.
        let (_, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/data/rows?entry=main.wcl&kind=character",
            None,
        )
        .await;
        let rows = v["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2, "{v:#}");
        let villain = rows.iter().find(|r| r["label"] == "villain").unwrap();
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": villain["file"], "etag": villain["etag"],
                "ops": [{ "op": "set_field", "span": villain["span"],
                          "field": "name", "text": "Big Bad" }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            std::fs::read_to_string(td.path().join("data/characters.wcl"))
                .unwrap()
                .contains("name = \"Big Bad\""),
        );
        let (_, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/data/rows?entry=main.wcl&kind=character",
            None,
        )
        .await;
        let hero = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["label"] == "hero")
            .unwrap();
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": hero["file"],
                "ops": [{ "op": "delete", "span": hero["span"] }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/data/rows?entry=main.wcl&kind=character",
            None,
        )
        .await;
        assert_eq!(v["rows"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn graph_lists_units_edges_and_view_visibility() {
        let td = tempfile::tempdir().unwrap();
        write_mini_wskill(td.path());
        // Give alpha a body with one deck-hidden paragraph.
        std::fs::write(
            td.path().join("data/concepts/alpha.wcl"),
            "concept alpha {\n  name = \"Alpha\"\n  body {\n    p \"Everywhere\"\n\n    @except(sites = [:deck])\n    p \"Book only\"\n  }\n}\n",
        )
        .unwrap();
        // The mini-wskill schema has no body child; extend it.
        let main = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        let main = main.replace(
            "@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n}",
            "@block(\"body\") @schemaless\ntype UnitBody {\n}\n\n@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n  @child(\"body\") body: UnitBody?\n}",
        );
        std::fs::write(td.path().join("main.wcl"), main).unwrap();
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/graph?entry=main.wcl&sites=book,deck",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let nodes = v["nodes"].as_array().unwrap();
        // alpha, beta, and the lang index.
        let alpha = nodes.iter().find(|n| n["id"] == "alpha").unwrap();
        assert_eq!(alpha["type"], "unit");
        assert_eq!(alpha["kind"], "concept");
        assert_eq!(alpha["title"], "Alpha");
        assert!(alpha["x"].is_number() && alpha["y"].is_number());
        let idx = nodes.iter().find(|n| n["id"] == "lang").unwrap();
        assert_eq!(idx["type"], "index");
        // Ordered pin list — the index panel edits this order.
        assert_eq!(idx["pinned"], serde_json::json!(["alpha", "beta"]));
        // Edges: the index pins alpha + beta.
        let edges = v["edges"].as_array().unwrap();
        assert!(
            edges.iter().any(|e| e["from"] == "index:lang"
                && e["to"] == "concept:alpha"
                && e["kind"] == "pin"),
            "{v:#}"
        );
        // Block-level per-view visibility: the body's paragraphs, with the
        // second hidden from the deck.
        let blocks = alpha["blocks"].as_array().unwrap();
        let hidden = blocks
            .iter()
            .find(|b| b["preview"] == "Book only")
            .unwrap_or_else(|| panic!("no body block listing: {v:#}"));
        assert_eq!(hidden["views"]["book"], true);
        assert_eq!(hidden["views"]["deck"], false);
        assert_eq!(hidden["visibility"]["custom"], false);
        let shown = blocks
            .iter()
            .find(|b| b["preview"] == "Everywhere")
            .unwrap();
        assert_eq!(shown["views"]["deck"], true);
    }

    /// The mini wskill with a nested sub-index: `alpha` pinned at the top
    /// level, `beta` (and later `alpha` too) inside `lang_sub`.
    fn write_mini_wskill_nested(root: &Path) {
        write_mini_wskill(root);
        let main = std::fs::read_to_string(root.join("main.wcl")).unwrap();
        let main = main.replace(
            "@block(\"index\")\ntype Index {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n}",
            "@block(\"index\")\ntype Index {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n  @children(\"index\") children: list<Index>?\n}",
        );
        std::fs::write(root.join("main.wcl"), main).unwrap();
        std::fs::write(
            root.join("data/indexes.wcl"),
            "index lang {\n  name = \"Language\"\n  related = [alpha]\n\n  index lang_sub {\n    name = \"Sub\"\n    related = [beta]\n  }\n}\n",
        )
        .unwrap();
    }

    /// Sub-indexes ride the graph payload as the index node's nested
    /// `children` tree, and their pins become edges attributed to the
    /// owning level via `index_id` (a unit pinned at two levels yields
    /// two edges).
    #[tokio::test]
    async fn graph_nested_index_children_and_pins() {
        let td = tempfile::tempdir().unwrap();
        write_mini_wskill_nested(td.path());
        let state = state_for(td.path(), None);

        let (status, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/graph?entry=main.wcl&sites=book",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let nodes = v["nodes"].as_array().unwrap();
        let idx = nodes.iter().find(|n| n["id"] == "lang").unwrap();
        assert_eq!(idx["pinned"], serde_json::json!(["alpha"]));
        let children = idx["children"].as_array().unwrap();
        assert_eq!(children.len(), 1, "{v:#}");
        assert_eq!(children[0]["id"], "lang_sub");
        assert_eq!(children[0]["title"], "Sub");
        assert_eq!(children[0]["pinned"], serde_json::json!(["beta"]));
        assert_eq!(children[0]["related_editable"], true);
        assert_eq!(children[0]["children"], serde_json::json!([]));
        // The sub-index never becomes a node of its own.
        assert!(!nodes.iter().any(|n| n["id"] == "lang_sub"), "{v:#}");
        let edges = v["edges"].as_array().unwrap();
        assert!(
            edges.iter().any(|e| e["from"] == "index:lang"
                && e["to"] == "concept:alpha"
                && e["kind"] == "pin"
                && e["index_id"] == "lang"),
            "{v:#}"
        );
        assert!(
            edges.iter().any(|e| e["from"] == "index:lang"
                && e["to"] == "concept:beta"
                && e["kind"] == "pin"
                && e["index_id"] == "lang_sub"),
            "{v:#}"
        );

        // Pin alpha into the sub-index too: two pin edges to alpha, one
        // per owning level.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/nav/op",
            Some(serde_json::json!({
                "entry": "main.wcl", "op": "pin_unit",
                "index_id": "lang_sub", "unit_id": "alpha",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/graph?entry=main.wcl&sites=book",
            None,
        )
        .await;
        let alpha_pins: Vec<&serde_json::Value> = v["edges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "pin" && e["to"] == "concept:alpha")
            .collect();
        assert_eq!(alpha_pins.len(), 2, "{v:#}");
    }

    /// The id-addressed related ops reach nested sub-indexes (the owning
    /// file used to be resolved with a top-level-only scan, so these all
    /// errored for sub-index ids).
    #[tokio::test]
    async fn nav_op_targets_sub_index() {
        let td = tempfile::tempdir().unwrap();
        write_mini_wskill_nested(td.path());
        let state = state_for(td.path(), None);

        let op = |body: serde_json::Value| {
            let state = Arc::clone(&state);
            async move { send(router(state), "POST", "/api/nav/op", Some(body)).await }
        };
        let (status, v) = op(serde_json::json!({
            "entry": "main.wcl", "op": "pin_unit",
            "index_id": "lang_sub", "unit_id": "alpha",
        }))
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let (status, v) = op(serde_json::json!({
            "entry": "main.wcl", "op": "reorder_children",
            "index_id": "lang_sub", "order": ["alpha", "beta"],
        }))
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let (status, v) = op(serde_json::json!({
            "entry": "main.wcl", "op": "unpin_unit",
            "index_id": "lang_sub", "unit_id": "beta",
        }))
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");

        // The writes landed on the NESTED list; the top-level one is
        // untouched.
        let (_, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/graph?entry=main.wcl&sites=book",
            None,
        )
        .await;
        let idx = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "lang")
            .unwrap();
        assert_eq!(idx["pinned"], serde_json::json!(["alpha"]), "{v:#}");
        assert_eq!(
            idx["children"][0]["pinned"],
            serde_json::json!(["alpha"]),
            "{v:#}"
        );
    }

    /// The graph view's edge writes: `related_add` / `related_remove` block
    /// ops on unit and index blocks, plus the `related_editable` flag.
    #[tokio::test]
    async fn graph_related_add_remove_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        write_mini_wskill(td.path());
        // Concepts need a `related` field for unit→unit edges.
        let main = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        let main = main.replace(
            "@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n}",
            "@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n}",
        );
        std::fs::write(td.path().join("main.wcl"), main).unwrap();
        let state = state_for(td.path(), None);

        let graph = |state: &Arc<EditorState>| {
            let state = Arc::clone(state);
            async move {
                let (status, v) = send(
                    router(state),
                    "GET",
                    "/api/graph?entry=main.wcl&sites=book",
                    None,
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{v}");
                v
            }
        };
        let v = graph(&state).await;
        let alpha = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "alpha")
            .unwrap();
        assert_eq!(alpha["related_editable"], true);
        assert!(
            !v["edges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "related"),
            "{v:#}"
        );

        // Connect alpha → beta.
        let ops = |file: &serde_json::Value, op: &str, span: &serde_json::Value, id: &str| {
            serde_json::json!({
                "entry": "main.wcl", "file": file,
                "ops": [{ "op": op, "span": span, "id": id }],
            })
        };
        let (status, v2) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(ops(&alpha["file"], "related_add", &alpha["span"], "beta")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v2}");
        let text = std::fs::read_to_string(td.path().join("data/concepts/alpha.wcl")).unwrap();
        assert!(text.contains("related = [beta]"), "{text}");
        let v = graph(&state).await;
        assert!(
            v["edges"].as_array().unwrap().iter().any(|e| {
                e["from"] == "concept:alpha" && e["to"] == "concept:beta" && e["kind"] == "related"
            }),
            "{v:#}"
        );

        // Duplicate, self-loop, and bad-id are refused (fresh spans each time).
        let alpha = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "alpha")
            .unwrap()
            .clone();
        for (id, msg) in [
            ("beta", "already related"),
            ("alpha", "itself"),
            ("not an id", "not a valid"),
        ] {
            let (status, v2) = send(
                router(Arc::clone(&state)),
                "POST",
                "/api/block/ops",
                Some(ops(&alpha["file"], "related_add", &alpha["span"], id)),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{id}: {v2}");
            assert!(v2["error"].as_str().unwrap().contains(msg), "{id}: {v2}");
        }

        // Disconnect again; a second remove is refused.
        let (status, v2) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(ops(
                &alpha["file"],
                "related_remove",
                &alpha["span"],
                "beta",
            )),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v2}");
        let v = graph(&state).await;
        assert!(
            !v["edges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "related"),
            "{v:#}"
        );
        let alpha = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "alpha")
            .unwrap()
            .clone();
        let (status, v2) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(ops(
                &alpha["file"],
                "related_remove",
                &alpha["span"],
                "beta",
            )),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v2["error"].as_str().unwrap().contains("not in the related"));

        // The same ops drive index pins: unpin beta, then re-pin it.
        let idx = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "lang")
            .unwrap()
            .clone();
        let (status, v2) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(ops(&idx["file"], "related_remove", &idx["span"], "beta")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v2}");
        let v = graph(&state).await;
        let pins = |v: &serde_json::Value| {
            v["edges"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "pin")
                .count()
        };
        assert_eq!(pins(&v), 1, "{v:#}");
        let idx = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "lang")
            .unwrap()
            .clone();
        let (status, v2) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(ops(&idx["file"], "related_add", &idx["span"], "beta")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v2}");
        assert_eq!(pins(&graph(&state).await), 2);

        // A computed related list: flagged not-editable, and the op refuses.
        std::fs::write(
            td.path().join("data/concepts/alpha.wcl"),
            "concept alpha {\n  name = \"Alpha\"\n  related = concat([], [])\n}\n",
        )
        .unwrap();
        let v = graph(&state).await;
        let alpha = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "alpha")
            .unwrap()
            .clone();
        assert_eq!(alpha["related_editable"], false, "{v:#}");
        let (status, v2) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(ops(&alpha["file"], "related_add", &alpha["span"], "beta")),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v2["error"].as_str().unwrap().contains("computed"), "{v2}");
    }

    /// Profile toggles on a real template-scaffolded wskill: disable
    /// removes the artifact + projection folder; enable scaffolds them
    /// back (files, aggregator imports, artifact block) and the document
    /// still validates.
    #[tokio::test]
    async fn wskill_profile_disable_then_enable() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        // Scaffold a full wskill (with the presentation view) from the
        // built-in template.
        let answers = std::collections::BTreeMap::from([
            ("topic_id".to_string(), "demo".to_string()),
            ("topic_name".to_string(), "Demo".to_string()),
            ("include_presentation".to_string(), "yes".to_string()),
            ("include_training".to_string(), "no".to_string()),
        ]);
        let (files, folders) = crate::scaffold::evaluate_template_tree("wskill", answers).unwrap();
        for dir in &folders {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for (rel, content) in &files {
            // The scaffold with training off still emits only wanted files.
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        assert!(root.join("wdoc/presentation/main.wcl").is_file());
        let state = state_for(root, None);

        // Disable the presentation profile.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/wskill/profile",
            Some(serde_json::json!({
                "registry": "wskill.wcl", "kind": "presentation", "enable": false,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let reg = std::fs::read_to_string(root.join("wskill.wcl")).unwrap();
        assert!(!reg.contains(":presentation"), "{reg}");
        assert!(!root.join("wdoc/presentation").exists());
        // The book view is untouched.
        assert!(root.join("wdoc/book/main.wcl").is_file());
        assert!(reg.contains(":book"));

        // Re-enable it: files + artifact come back, doc still validates.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/wskill/profile",
            Some(serde_json::json!({
                "registry": "wskill.wcl", "kind": "presentation", "enable": true,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let reg = std::fs::read_to_string(root.join("wskill.wcl")).unwrap();
        assert!(reg.contains(":presentation"), "{reg}");
        assert!(root.join("wdoc/presentation/main.wcl").is_file());
        // Validating open of the registry succeeds (commit already gated
        // this; double-check directly).
        let doc = wcl_wdoc::open_doc_for_edit(&root.join("wskill.wcl")).unwrap();
        assert!(doc.schema_errors().is_empty());
        // Idempotence guards.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/wskill/profile",
            Some(serde_json::json!({
                "registry": "wskill.wcl", "kind": "presentation", "enable": true,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
        assert!(v["error"].as_str().unwrap().contains("already exists"));
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/wskill/profile",
            Some(serde_json::json!({
                "registry": "wskill.wcl", "kind": "training", "enable": false,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn preview_targeted_rebuild_after_edit() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), BODY_DOC).unwrap();
        let state = state_for(td.path(), None);

        // Cold: full build.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({ "entry": "main.wcl", "site": "docs", "files": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["mode"], "full");
        let href = v["href"].as_str().unwrap().to_string();

        // Edit a paragraph through block ops, then rebuild just that page.
        let disk = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        let p = span_of(&disk, |b| b.kind == "p");
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/block/ops",
            Some(serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "set_label", "span": span_json(p), "slot": 0,
                          "text": "Rebuilt text" }],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({
                "entry": "main.wcl", "site": "docs", "files": [],
                "pages": ["index"], "changed": ["main.wcl"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let req = Request::builder().uri(&href).body(Body::empty()).unwrap();
        let resp = router(Arc::clone(&state)).oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 22)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&bytes);
        assert!(
            html.contains("Rebuilt text"),
            "targeted rebuild missed the edit"
        );
    }

    #[test]
    fn root_resolution_prefers_arg_then_main_wcl() {
        let td = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(td.path()).unwrap();
        assert_eq!(resolve_root_file(&dir, None).unwrap(), None);
        std::fs::write(dir.join("main.wcl"), "x = 1\n").unwrap();
        assert_eq!(
            resolve_root_file(&dir, None).unwrap(),
            Some(dir.join("main.wcl"))
        );
        std::fs::write(dir.join("other.wcl"), "y = 2\n").unwrap();
        assert_eq!(
            resolve_root_file(&dir, Some(dir.join("other.wcl"))).unwrap(),
            Some(dir.join("other.wcl"))
        );
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("out.wcl"), "z = 3\n").unwrap();
        assert!(resolve_root_file(&dir, Some(outside.path().join("out.wcl"))).is_err());
        assert!(resolve_root_file(&dir, Some(dir.join("missing.wcl"))).is_err());
    }
}
