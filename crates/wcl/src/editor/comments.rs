//! Review-comment and review-handshake endpoints for `wcl editor`.
//!
//! The preview pane's comment UI drives these: comments are stored in
//! `comments.wcl` sidecars via the shared [`wcl_wdoc::comments`] core (the
//! sidecar beside the owning wskill, else the served root), and the review
//! handshake pairs a blocked `wcl wdoc review <root>` process with the
//! editor's "Send to agent" button through marker files (see
//! [`wcl_wdoc::Handshake`]).

use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::Response;
use wcl_wdoc::comments;

use super::{EditorState, Workspace, run_blocking};
use crate::serve::{POLL_TIMEOUT, json_error, json_response, parse_json_body};

/// `GET /api/comments` — every stored comment under the served root.
pub(super) async fn handle_comments_list(State(state): State<Arc<EditorState>>) -> Response {
    let state2 = Arc::clone(&state);
    run_blocking(move || list_comments(&state2.ws)).await
}

fn list_comments(ws: &Workspace) -> Result<serde_json::Value, String> {
    match comments::list(ws.root_dir()) {
        Ok(recs) => Ok(serde_json::json!({
            "comments": recs.iter().map(crate::comment_record_json).collect::<Vec<_>>(),
        })),
        Err(e) => Err(e.render_plain()),
    }
}

/// `POST /api/comments` — add a comment. Body:
/// `{ page, page_file?, loc?, target?, body, quote?, author? }` → `{ id }`.
/// The page key routes the record to the sidecar that owns the page's
/// source file; a block comment additionally carries its locator + target.
pub(super) async fn handle_comment_add(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || add_comment(&state2.ws, &v)).await
}

fn add_comment(ws: &Workspace, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let str_of = |k: &str| v.get(k).and_then(serde_json::Value::as_str);
    let page = str_of("page")
        .filter(|s| !s.is_empty())
        .ok_or("missing page")?;
    let body_text = str_of("body")
        .filter(|s| !s.trim().is_empty())
        .ok_or("missing comment body")?;
    let page_file = str_of("page_file");
    // Sandbox the page's source file, then derive the sidecar from it. A
    // page_file that doesn't resolve inside the served tree means "page
    // source unknown" — fall back to the root sidecar, not an error.
    let sidecar = match page_file.and_then(|f| ws.abs(f).ok()) {
        Some(pf) => comments::comments_path(&pf, ws.root_dir(), wcl_wskill::ROOT_MARKER),
        None => ws.root_dir().join("comments.wcl"),
    };
    let id = comments::add(
        &sidecar,
        page,
        page_file,
        str_of("loc"),
        str_of("target"),
        body_text,
        str_of("author"),
        str_of("quote"),
    )
    .map_err(|e| e.render_plain())?;
    Ok(serde_json::json!({ "id": id }))
}

/// `POST /api/comments/resolve` — `{ id }` deletes the record.
pub(super) async fn handle_comment_resolve(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let Some(id) = v
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
    else {
        return json_error(StatusCode::BAD_REQUEST, "missing id");
    };
    let state2 = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || resolve_comment(&state2.ws, &id))
        .await
        .unwrap_or_else(|e| Err(format!("task failed: {e}")));
    found_response(result, "resolved")
}

/// Delete the record with `id`; `false` when no such comment exists.
fn resolve_comment(ws: &Workspace, id: &str) -> Result<bool, String> {
    comments::resolve(ws.root_dir(), id).map_err(|e| e.render_plain())
}

/// `POST /api/comments/edit` — `{ id, body }` replaces the record's body.
pub(super) async fn handle_comment_edit(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let str_of = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let Some(id) = str_of("id") else {
        return json_error(StatusCode::BAD_REQUEST, "missing id");
    };
    let Some(text) = str_of("body").filter(|s| !s.trim().is_empty()) else {
        return json_error(StatusCode::BAD_REQUEST, "missing comment body");
    };
    let state2 = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || edit_comment(&state2.ws, &id, &text))
        .await
        .unwrap_or_else(|e| Err(format!("task failed: {e}")));
    found_response(result, "edited")
}

/// Replace the body of the record with `id`; `false` when there is none.
fn edit_comment(ws: &Workspace, id: &str, text: &str) -> Result<bool, String> {
    comments::edit(ws.root_dir(), id, text).map_err(|e| e.render_plain())
}

/// The did-it-exist responses `resolve` / `edit` share: a missing record is
/// a 404, a failure a 400.
fn found_response(result: Result<bool, String>, verb: &str) -> Response {
    match result {
        Ok(true) => json_response(StatusCode::OK, &serde_json::json!({ verb: true })),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "no such comment"),
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e),
    }
}

/// `GET /api/review/status?round=N` — review-handshake long-poll. The client
/// passes its last-known wait round (0 = not waiting); the request parks up
/// to [`POLL_TIMEOUT`] polling the `agent` marker and answers
/// `{ waiting, round }` as soon as the round changes (a fresh `wcl wdoc
/// review` wait, or its end). A new round each `review` run is what re-shows
/// the banner after the agent's changes.
pub(super) async fn handle_review_status(
    State(state): State<Arc<EditorState>>,
    uri: Uri,
) -> Response {
    let asked: u64 = uri
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("round=")))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    json_response(
        StatusCode::OK,
        &review_status(state.review.as_ref(), asked).await,
    )
}

/// The long-poll itself: park until the handshake's wait round differs from
/// the one the client echoed, or the deadline passes.
async fn review_status(hs: Option<&wcl_wdoc::Handshake>, asked: u64) -> serde_json::Value {
    let Some(hs) = hs else {
        return serde_json::json!({ "waiting": false, "round": "0" });
    };
    let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
    let mut current = hs.agent_waiting().unwrap_or(0);
    while current == asked && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        current = hs.agent_waiting().unwrap_or(0);
    }
    // `round` travels as a string: it is a u64 nanosecond stamp, which JSON
    // number parsing in the browser would round (> Number.MAX_SAFE_INTEGER),
    // making the echoed round never match and the long-poll spin.
    serde_json::json!({ "waiting": current != 0, "round": current.to_string() })
}

/// `POST /api/review/ready` — release a blocked `wcl wdoc review` (the
/// preview pane's "Send to agent" button).
pub(super) async fn handle_review_ready(State(state): State<Arc<EditorState>>) -> Response {
    match review_ready(state.review.as_ref()) {
        Ok(v) => json_response(StatusCode::OK, &v),
        Err(e) if e.starts_with("could not signal") => {
            json_error(StatusCode::INTERNAL_SERVER_ERROR, &e)
        }
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e),
    }
}

/// Release a blocked `wcl wdoc review`. Without a root document there is no
/// handshake to release.
fn review_ready(hs: Option<&wcl_wdoc::Handshake>) -> Result<serde_json::Value, String> {
    let hs = hs.ok_or("review handshake is not active (no root document)")?;
    hs.signal_ready()
        .map(|()| serde_json::json!({ "ok": true }))
        .map_err(|e| format!("could not signal the agent: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::testsupport::{SITE_DOC, workspace_built_by, workspace_with};

    fn add(ws: &Workspace, body: serde_json::Value) -> Result<serde_json::Value, String> {
        add_comment(ws, &body)
    }

    #[test]
    fn add_list_edit_resolve_roundtrip() {
        let (_td, ws) = workspace_with(SITE_DOC);
        let page_file = ws.root_dir().join("main.wcl").display().to_string();

        // Page comment + block comment.
        let v = add(
            &ws,
            serde_json::json!({
                "page": "index", "page_file": page_file, "body": "page note",
            }),
        )
        .expect("page comment");
        let page_id = v["id"].as_str().unwrap().to_string();
        let v = add(
            &ws,
            serde_json::json!({
                "page": "index", "page_file": page_file, "loc": "0",
                "target": "h1 — \"Hello preview\"", "body": "block note", "quote": "Hello",
            }),
        )
        .expect("block comment");
        let block_id = v["id"].as_str().unwrap().to_string();

        let v = list_comments(&ws).expect("list");
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
        assert!(edit_comment(&ws, &block_id, "sharper note").expect("edit"));
        assert!(resolve_comment(&ws, &page_id).expect("resolve"));
        let v = list_comments(&ws).expect("list");
        let list = v["comments"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["body"], "sharper note");

        // Unknown ids report "no such record" rather than failing — the
        // router turns that into a 404.
        assert!(!resolve_comment(&ws, "cnothere").expect("resolve unknown"));
        assert!(!edit_comment(&ws, "cnothere", "x").expect("edit unknown"));
    }

    #[test]
    fn add_scopes_to_wskill_sidecar_and_validates() {
        let (_td, ws) = workspace_built_by(|root| {
            std::fs::create_dir_all(root.join("sub")).unwrap();
            std::fs::write(root.join("sub/wskill.wcl"), "// wskill marker\n").unwrap();
            std::fs::write(root.join("sub/page.wcl"), "// page source\n").unwrap();
        });
        let page_file = ws.root_dir().join("sub/page.wcl").display().to_string();

        add(
            &ws,
            serde_json::json!({ "page": "p", "page_file": page_file, "body": "note" }),
        )
        .expect("comment");
        assert!(ws.root_dir().join("sub/comments.wcl").is_file());
        assert!(!ws.root_dir().join("comments.wcl").exists());

        // A page_file outside the served tree falls back to the root sidecar.
        add(
            &ws,
            serde_json::json!({ "page": "p", "page_file": "/nowhere/x.wcl", "body": "n" }),
        )
        .expect("fallback comment");
        assert!(ws.root_dir().join("comments.wcl").is_file());

        // Missing page / body are refused.
        for bad in [
            serde_json::json!({ "body": "n" }),
            serde_json::json!({ "page": "p", "body": "  " }),
        ] {
            assert!(add(&ws, bad.clone()).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn review_status_and_ready_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("main.wcl"), "x = 1\n").unwrap();
        let hs = wcl_wdoc::Handshake::new(&td.path().join("main.wcl"));
        hs.serve_started().unwrap();

        // No agent yet: round 0 matches, but the long-poll deadline would
        // park — ask with a mismatched round to get an immediate answer.
        let round = hs.begin_wait().unwrap();
        let v = review_status(Some(&hs), 0).await;
        assert_eq!(v["waiting"], true);
        // The round is serialized as a string — a u64 nanosecond stamp
        // exceeds JS number precision, so it must travel opaquely.
        assert_eq!(v["round"].as_str().unwrap(), round.to_string());

        review_ready(Some(&hs)).expect("ready");
        assert!(hs.released(round));
        hs.end_wait();
        hs.serve_stopped();
    }

    #[tokio::test]
    async fn review_endpoints_without_root_doc() {
        let v = review_status(None, 0).await;
        assert_eq!(v["waiting"], false);
        assert_eq!(v["round"], "0");
        assert!(review_ready(None).is_err());
    }
}
