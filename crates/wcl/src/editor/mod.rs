//! `wcl editor` — a browser-based editor for the current directory.
//!
//! Serves a single-page app (SolidJS + the Forge design system, embedded
//! from `editor-ui/dist`) with a gitignore-aware file tree, CodeMirror
//! editing of any text file, LSP support for `.wcl` (a WebSocket bridge to
//! an in-process [`wcl_lsp`] session), and a wdoc preview pane. Preview is
//! site-scoped: `/api/sites` discovers every `site`-declaring entry under
//! the served tree (nested by `include` membership), and `/api/preview`
//! full-builds the selected one on demand — the client's Rebuild button,
//! not a per-edit loop. The root document follows the LSP's model: the
//! explicit argument, else `./main.wcl` when present. Without one the
//! editor still works — schema-validated saves and the LSP root degrade
//! gracefully.
//!
//! Unlike the `wcl wdoc serve` dev server there is no watcher and no
//! rebuild loop: saves are client-driven, external changes surface as etag
//! conflicts on save, and previews always overlay the live buffers.

mod assets;
mod comments;
mod files;
mod lsp_bridge;
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
    /// Review handshake pairing a blocked `wcl wdoc review <root>` with the
    /// preview pane's "Send to agent" button. `None` without a root document
    /// (`review` then falls back to its non-blocking listing).
    review: Option<wcl_wdoc::Handshake>,
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

    // A stable slug per (entry, site) so distinct selections coexist in
    // the scratch tree and re-selecting one reuses its output.
    let slug: String = format!("{entry}__{}", site.as_deref().unwrap_or(""))
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let out = state.preview.root().join("sites").join(&slug);

    // comment_mode stamps the `data-wcl-block` / `data-wcl-page-*` anchors
    // the preview pane's comment UI keys on; edit_mode adds the per-block
    // `data-wcl-span` / `data-wcl-file` anchors and the `edit_object`
    // "Edit this …" buttons the pane resolves via `/api/object/locate` —
    // still no injected scripts.
    let opts = wcl_wdoc::BuildOptions {
        overlay: Some(overlay),
        comment_mode: true,
        edit_mode: true,
        ..Default::default()
    };
    wcl_wdoc::build_with_options(&entry_abs, &out, site.as_deref(), &opts)
        .map_err(|e| e.render_plain())?;
    // Per-render warnings would otherwise pile up for whoever drains next.
    let _ = wcl_wdoc::take_render_warnings();

    let index = index_page(&out).ok_or("the site built but produced no HTML page")?;
    Ok(serde_json::json!({
        "ok": true,
        "href": format!("/api/preview/sites/{slug}/{index}"),
    }))
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
