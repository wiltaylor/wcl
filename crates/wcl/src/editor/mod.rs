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
mod preview;
mod profiles;
mod sites;
mod systems;
#[cfg(test)]
mod testsupport;
mod workspace;

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
pub(crate) use workspace::Workspace;

/// The editor's API router plus the embedded SPA fallback — the whole HTTP
/// surface, first in the file because it is what this module is *for*.
/// Split from [`serve`] so the transport-level tests can drive it with
/// `tower::ServiceExt::oneshot`; everything below a route is tested by the
/// module that owns it, through inner functions that take a [`Workspace`]
/// rather than a request.
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
        .route("/api/preview", post(preview::handle_preview))
        .route("/api/preview/{*path}", get(preview::handle_preview_file))
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
        .route("/api/systems", get(systems::handle_systems))
        .route("/api/systems/detail", post(systems::handle_systems_detail))
        .route("/api/data/types", get(data::handle_data_types))
        .route("/api/data/rows", get(data::handle_data_rows))
        .route("/api/raw", get(handle_raw))
        .fallback(get(assets::spa_fallback))
        .with_state(state)
}

/// Everything a request *could* reach. Almost nothing takes it: an endpoint
/// that only reads or writes documents takes [`Workspace`] (plus
/// [`preview::Sessions`] when it writes), so a handler's signature states
/// what it can affect. Only the preview module needs the whole of this.
pub(crate) struct EditorState {
    /// The served tree and its root document.
    ws: Workspace,
    /// Scratch output tree for preview renders.
    preview: crate::preview::Preview,
    /// Warm preview state per `sites/<slug>` output dir, so a stale page can
    /// be lazily re-rendered when the iframe navigates to it.
    sessions: preview::Sessions,
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
        ws: Workspace::new(root_dir.clone(), root_file.clone()),
        preview: crate::preview::Preview::new()?,
        sessions: preview::Sessions::default(),
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
    run_blocking(move || files::list_tree(&state2.ws)).await
}

/// `GET /api/file?path=` — a text file plus its save etag.
async fn handle_file_get(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let Some(path) = query_param(&uri, "path") else {
        return json_error(StatusCode::BAD_REQUEST, "missing path");
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || files::read_text(&state2.ws, &path)).await
}

/// `POST /api/file` — save a buffer (validating pipeline for `.wcl`).
async fn handle_file_post(State(state): State<Arc<EditorState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || files::save_file(&state2.ws, &state2.sessions, &v)).await
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
    let result = tokio::task::spawn_blocking(move || files::read_raw(&state2.ws, &path)).await;
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
    run_blocking(move || sites::scan_sites(&state2.ws)).await
}

/// Uniform endpoint error mapping: any displayable error as its message.
pub(crate) fn err_str(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A [`wcl_lang::Span`] as the `{ "start", "end" }` object every editor
/// endpoint's JSON uses.
pub(crate) fn span_json(span: wcl_lang::Span) -> serde_json::Value {
    serde_json::json!({ "start": span.start, "end": span.end })
}

/// The first block in `items` satisfying `pred`, searched recursively —
/// the one AST descent the editor's read paths share.
pub(crate) fn find_block<'a>(
    items: &'a [wcl_lang::ast::Item],
    pred: &impl Fn(&wcl_lang::ast::Block) -> bool,
) -> Option<&'a wcl_lang::ast::Block> {
    for item in items {
        if let wcl_lang::ast::Item::Block(b) = item {
            if pred(b) {
                return Some(b);
            }
            if let Some(found) = find_block(&b.items, pred) {
                return Some(found);
            }
        }
    }
    None
}

/// The block whose span is exactly `span`.
pub(crate) fn find_block_at(
    items: &[wcl_lang::ast::Item],
    span: wcl_lang::Span,
) -> Option<&wcl_lang::ast::Block> {
    find_block(items, &|b| b.span == span)
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
    run_blocking(move || locate_object(&state2.ws, &v)).await
}

/// The blocking half of [`handle_object_locate`].
fn locate_object(ws: &Workspace, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let doc_entry = ws.doc_entry_from(v)?;
    let kind = crate::edit::str_field(v, "kind")?;
    let target = v
        .get("target")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty());
    let overlay = ws.overlay(v)?;
    let (file, span) = crate::edit::locate_object(&doc_entry, kind, target, overlay)?;
    Ok(serde_json::json!({
        "ok": true,
        "file": ws.rel(&file)?,
        "span": span_json(span),
    }))
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
    //! Transport-level tests only. Everything a module can assert about its
    //! own behaviour lives with that module — these are the assertions that
    //! are genuinely about the HTTP surface: that the routes resolve, that
    //! errors map to the right status, that a path outside the served tree
    //! is refused, that unknown paths fall through to the SPA, that the
    //! language-server bridge upgrades — plus the end-to-end guard that a
    //! write invalidates already-built previews.

    use super::testsupport::{BODY_DOC, OBJECT_DOC, SITE_DOC, state_at};
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn send(
        app: Router,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let (status, bytes) = raw_send(app, method, uri, body).await;
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    async fn raw_send(
        app: Router,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, Vec<u8>) {
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
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 22)
            .await
            .unwrap();
        (status, bytes.to_vec())
    }

    /// The routing table resolves and the shared error mapping holds: a save
    /// conflict is a 409 (so the client can offer reload-vs-overwrite), a
    /// missing comment a 404, anything else a 400.
    #[tokio::test]
    async fn routes_resolve_and_errors_map_to_statuses() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), SITE_DOC).unwrap();
        let state = state_at(td.path());

        for uri in ["/api/files", "/api/file?path=main.wcl", "/api/sites"] {
            let (status, v) = send(router(Arc::clone(&state)), "GET", uri, None).await;
            assert_eq!(status, StatusCode::OK, "GET {uri}: {v}");
        }
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/format",
            Some(serde_json::json!({ "text": "name=\"x\"" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["text"], "name = \"x\"\n");

        // A save with the current etag lands; replaying it is a conflict,
        // and the `conflict:` prefix is what makes it a 409.
        let (_, v) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/file?path=main.wcl",
            None,
        )
        .await;
        let etag = v["etag"].as_str().unwrap().to_string();
        let save = |text: String| serde_json::json!({ "path": "main.wcl", "text": text, "base_etag": etag });
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/file",
            Some(save(SITE_DOC.replace("Hello preview", "Saved once"))),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        // The etag is stale now — the same base can't be saved against twice.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/file",
            Some(save(SITE_DOC.replace("Hello preview", "Saved twice"))),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(v["error"].as_str().unwrap().contains("conflict"));

        // A missing record is a 404, not a 400.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/comments/resolve",
            Some(serde_json::json!({ "id": "cnothere" })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // A malformed body is a plain 400.
        let (status, _) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/format",
            Some(serde_json::json!({ "nope": true })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn raw_serves_bytes_and_refuses_paths_outside_the_tree() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("img.bin"), b"\x00\x01\x02").unwrap();
        let state = state_at(td.path());

        let (status, bytes) = raw_send(
            router(Arc::clone(&state)),
            "GET",
            "/api/raw?path=img.bin",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(&bytes[..], b"\x00\x01\x02");

        let (status, _) = send(
            router(Arc::clone(&state)),
            "GET",
            "/api/raw?path=../escape",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // The same refusal on a body-carrying endpoint, where it is an
        // explicit error rather than a missing file.
        let (status, v) = send(
            router(state),
            "POST",
            "/api/object/locate",
            Some(serde_json::json!({ "entry": "../x.wcl", "kind": "thing", "files": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("outside"), "{v}");
    }

    #[tokio::test]
    async fn spa_fallback_serves_index_for_routes() {
        let td = tempfile::tempdir().unwrap();
        let app = router(state_at(td.path()));
        // Any extension-less path outside /api serves the embedded index.
        let req = Request::builder()
            .uri("/some/client/route")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers()[header::CONTENT_TYPE].to_str().unwrap();
        assert!(ct.starts_with("text/html"), "{ct}");
    }

    /// The language-server bridge is a protocol upgrade, so it is genuinely
    /// transport: the route exists and refuses a plain GET rather than
    /// falling through to the SPA.
    #[tokio::test]
    async fn lsp_route_requires_a_websocket_upgrade() {
        let td = tempfile::tempdir().unwrap();
        let state = state_at(td.path());

        let (status, _) = raw_send(router(Arc::clone(&state)), "GET", "/api/lsp", None).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "a plain GET must be rejected by the upgrade extractor, not fall \
             through to the SPA"
        );

        let req = Request::builder()
            .uri("/api/lsp")
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, "websocket")
            .header(header::SEC_WEBSOCKET_VERSION, "13")
            .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
            .body(Body::empty())
            .unwrap();
        let resp = router(state).oneshot(req).await.unwrap();
        // A well-formed handshake gets past every header check and fails
        // only on the one thing a `oneshot` harness cannot provide: an
        // upgradable connection. That is as far as the route can be driven
        // without a real socket, and it proves the bridge is wired to the
        // upgrade extractor.
        assert_eq!(
            resp.status(),
            StatusCode::UPGRADE_REQUIRED,
            "the handshake must reach the upgrade itself"
        );
    }

    /// The regression guard for the staleness defect: a write through an
    /// endpoint OTHER than `/api/block/ops` must invalidate every built
    /// preview. Deliberately end to end — a write followed by a bare page
    /// fetch with no rebuild request — because that is exactly the sequence
    /// that used to serve pre-commit HTML.
    #[tokio::test]
    async fn a_write_invalidates_built_previews() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), BODY_DOC).unwrap();
        let state = state_at(td.path());

        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/preview",
            Some(serde_json::json!({ "entry": "main.wcl", "site": "docs", "files": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let href = v["href"].as_str().unwrap().to_string();
        let (_, bytes) = raw_send(router(Arc::clone(&state)), "GET", &href, None).await;
        assert!(String::from_utf8_lossy(&bytes).contains("First paragraph"));

        // Save the file through `/api/file` — no rebuild is requested.
        let (status, v) = send(
            router(Arc::clone(&state)),
            "POST",
            "/api/file",
            Some(serde_json::json!({
                "path": "main.wcl",
                "text": BODY_DOC.replace("First paragraph", "Invalidated text"),
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");

        let (_, bytes) = raw_send(router(state), "GET", &href, None).await;
        let html = String::from_utf8_lossy(&bytes);
        assert!(
            html.contains("Invalidated text"),
            "a built page stayed fresh across a write: {html}"
        );
    }

    #[tokio::test]
    async fn object_locate_finds_instance_by_label() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), OBJECT_DOC).unwrap();
        let ws = Workspace::at(td.path());

        let v = locate_object(
            &ws,
            &serde_json::json!({
                "entry": "main.wcl", "kind": "thing", "target": "beta", "files": [],
            }),
        )
        .expect("locate");
        assert_eq!(v["file"], "main.wcl");
        let (start, end) = (
            v["span"]["start"].as_u64().unwrap() as usize,
            v["span"]["end"].as_u64().unwrap() as usize,
        );
        let text = &OBJECT_DOC[start..end];
        assert!(text.starts_with("thing \"beta\""), "span slices {text:?}");
    }

    #[tokio::test]
    async fn object_locate_respects_overlay_and_errors() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), OBJECT_DOC).unwrap();
        let ws = Workspace::at(td.path());

        // Overlay renames beta → gamma: gamma resolves, beta no longer does.
        let edited = OBJECT_DOC.replace("thing \"beta\"", "thing \"gamma\"");
        let v = locate_object(
            &ws,
            &serde_json::json!({
                "entry": "main.wcl", "kind": "thing", "target": "gamma",
                "files": [{ "path": "main.wcl", "text": edited }],
            }),
        )
        .expect("locate through the overlay");
        let (start, end) = (
            v["span"]["start"].as_u64().unwrap() as usize,
            v["span"]["end"].as_u64().unwrap() as usize,
        );
        assert!(edited[start..end].starts_with("thing \"gamma\""));

        // Unknown target → an error naming the kind.
        let e = locate_object(
            &ws,
            &serde_json::json!({
                "entry": "main.wcl", "kind": "thing", "target": "nope", "files": [],
            }),
        )
        .unwrap_err();
        assert!(e.contains("thing"), "{e}");

        // No target with two instances → an error listing the labels.
        let e = locate_object(
            &ws,
            &serde_json::json!({ "entry": "main.wcl", "kind": "thing", "files": [] }),
        )
        .unwrap_err();
        assert!(e.contains("alpha"), "{e}");
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
