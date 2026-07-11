//! Review-comment and review-handshake endpoints for `wcl editor`.
//!
//! The preview pane's comment UI drives these: comments are stored in
//! `comments.wcl` sidecars via the shared [`wcl_wdoc::comments`] core (the
//! sidecar beside the owning wskill, else the served root), and the review
//! handshake pairs a blocked `wcl wdoc review <root>` process with the
//! editor's "Send to agent" button through the same marker files the old
//! `wdoc serve --comment` toolbar used.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::Response;
use wcl_wdoc::comments;

use super::{EditorState, run_blocking};
use crate::serve::{POLL_TIMEOUT, json_error, json_response, parse_json_body, sandboxed};

/// `GET /api/comments` — every stored comment under the served root.
pub(super) async fn handle_comments_list(State(state): State<Arc<EditorState>>) -> Response {
    let state2 = Arc::clone(&state);
    run_blocking(move || match comments::list(&state2.root_dir) {
        Ok(recs) => Ok(serde_json::json!({
            "comments": recs.iter().map(crate::comment_record_json).collect::<Vec<_>>(),
        })),
        Err(e) => Err(e.render_plain()),
    })
    .await
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
    run_blocking(move || add_comment(&state2, &v)).await
}

fn add_comment(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
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
    let sidecar = match page_file.and_then(|f| sandboxed(&state.root_dir, &state.root_dir.join(f)))
    {
        Some(pf) => comments::comments_path(&pf, &state.root_dir),
        None => state.root_dir.join("comments.wcl"),
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
    let result = tokio::task::spawn_blocking(move || comments::resolve(&state2.root_dir, &id))
        .await
        .unwrap_or_else(|e| Err(wcl_wdoc::BuildError::BadPage(format!("task failed: {e}"))));
    match result {
        Ok(true) => json_response(StatusCode::OK, &serde_json::json!({ "resolved": true })),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "no such comment"),
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e.render_plain()),
    }
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
    let result = tokio::task::spawn_blocking(move || comments::edit(&state2.root_dir, &id, &text))
        .await
        .unwrap_or_else(|e| Err(wcl_wdoc::BuildError::BadPage(format!("task failed: {e}"))));
    match result {
        Ok(true) => json_response(StatusCode::OK, &serde_json::json!({ "edited": true })),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "no such comment"),
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e.render_plain()),
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
    let Some(hs) = &state.review else {
        return json_response(
            StatusCode::OK,
            &serde_json::json!({ "waiting": false, "round": "0" }),
        );
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
    json_response(
        StatusCode::OK,
        &serde_json::json!({ "waiting": current != 0, "round": current.to_string() }),
    )
}

/// `POST /api/review/ready` — release a blocked `wcl wdoc review` (the
/// preview pane's "Send to agent" button).
pub(super) async fn handle_review_ready(State(state): State<Arc<EditorState>>) -> Response {
    let Some(hs) = &state.review else {
        return json_error(
            StatusCode::BAD_REQUEST,
            "review handshake is not active (no root document)",
        );
    };
    match hs.signal_ready() {
        Ok(()) => json_response(StatusCode::OK, &serde_json::json!({ "ok": true })),
        Err(e) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not signal the agent: {e}"),
        ),
    }
}
