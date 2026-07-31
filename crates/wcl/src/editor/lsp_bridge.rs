//! WebSocket ↔ LSP bridge for `wcl editor`.
//!
//! The browser LSP client speaks JSON-RPC as plain WebSocket text frames;
//! `wcl_lsp` speaks the standard `Content-Length`-framed LSP wire protocol
//! over a byte stream. Each WebSocket connection gets a fresh in-process
//! LSP session (via [`wcl_lsp::serve_stream`] over a `tokio::io::duplex`
//! pipe) and this module pumps between the two, adding/stripping the
//! framing.
//!
//! The server also rewrites the client's `initialize` request to inject
//! `initializationOptions.root` (the editor's root document) and a
//! `rootUri` fallback, so the browser client needs no knowledge of how
//! `wcl_lsp` resolves its root document.

use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use super::{EditorState, Workspace};

/// Size of the in-memory pipe between the WebSocket pump and the LSP
/// session. Big enough that a bulky payload (workspace symbols, semantic
/// tokens) never stalls one side while the other waits.
const DUPLEX_BUF: usize = 1024 * 1024;

/// `GET /api/lsp` — upgrade to a WebSocket and run a fresh LSP session
/// bridged to it for the connection's lifetime.
pub(crate) async fn handle_lsp_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<EditorState>>,
) -> Response {
    // The bridge only ever reads the root document and served tree, so the
    // connection-lived task holds a workspace rather than the whole state.
    let workspace = state.ws.clone();
    ws.on_upgrade(move |socket| bridge(socket, workspace))
}

async fn bridge(mut socket: WebSocket, workspace: Workspace) {
    let (ws_io, lsp_io) = tokio::io::duplex(DUPLEX_BUF);
    let (lsp_read, lsp_write) = tokio::io::split(lsp_io);
    let session = tokio::spawn(wcl_lsp::serve_stream(lsp_read, lsp_write));

    let (ws_read, mut ws_write) = tokio::io::split(ws_io);
    // Dedicated reader: `read_frame` performs several reads per message, so
    // it is not cancellation-safe inside `select!`; an mpsc recv is.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(16);
    let reader = tokio::spawn(async move {
        let mut reader = BufReader::new(ws_read);
        while let Ok(Some(payload)) = read_frame(&mut reader).await {
            if tx.send(payload).await.is_err() {
                break;
            }
        }
    });

    // `initialize` is the first client message; rewrite only until seen.
    let mut initialized = false;
    loop {
        tokio::select! {
            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(text))) => {
                    let payload = if initialized {
                        text.to_string()
                    } else {
                        initialized = true;
                        inject_root(&text, workspace.root_file(), workspace.root_dir())
                    };
                    if write_frame(&mut ws_write, &payload).await.is_err() {
                        break;
                    }
                }
                // Binary frames aren't part of the protocol; ping/pong is
                // answered by the WebSocket layer itself.
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            out = rx.recv() => match out {
                Some(payload) => {
                    if socket.send(Message::Text(payload.into())).await.is_err() {
                        break;
                    }
                }
                None => break, // LSP session ended
            },
        }
    }

    // Dropping the write half EOFs the LSP session's input; its task ends on
    // its own. The reader is parked on a dead pipe — abort it.
    drop(ws_write);
    reader.abort();
    session.abort();
}

/// Write one LSP wire message: `Content-Length` header (byte length, not
/// chars) then the payload.
async fn write_frame<W>(w: &mut W, payload: &str) -> std::io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    w.write_all(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes())
        .await?;
    w.write_all(payload.as_bytes()).await?;
    w.flush().await
}

/// Read one LSP wire message: headers up to the blank line (only
/// `Content-Length` matters), then exactly that many payload bytes.
/// `Ok(None)` on clean EOF at a message boundary.
async fn read_frame<R>(r: &mut R) -> std::io::Result<Option<String>>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).await? == 0 {
            return Ok(None); // EOF
        }
        let line = line.trim_end();
        if line.is_empty() {
            break; // end of headers
        }
        if let Some(v) = line
            .split_once(':')
            .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .map(|(_, v)| v.trim())
        {
            content_length = v.parse().ok();
        }
    }
    let len = content_length.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "missing Content-Length")
    })?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    String::from_utf8(buf)
        .map(Some)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Rewrite an `initialize` request so the in-process server resolves the
/// editor's root document: set `initializationOptions.root` when the editor
/// has one, and default `rootUri` to the served directory when the client
/// sent none (that keeps `wcl_lsp`'s own `<workspace>/main.wcl` fallback
/// working). Anything that isn't an `initialize` request passes through
/// untouched.
fn inject_root(msg: &str, root_file: Option<&Path>, root_dir: &Path) -> String {
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(msg) else {
        return msg.to_string();
    };
    if v.get("method").and_then(serde_json::Value::as_str) != Some("initialize") {
        return msg.to_string();
    }
    let params = v
        .as_object_mut()
        .expect("parsed JSON message is an object")
        .entry("params")
        .or_insert_with(|| serde_json::json!({}));
    if !params.is_object() {
        *params = serde_json::json!({});
    }
    let params = params.as_object_mut().expect("params forced to an object");
    if let Some(root) = root_file {
        let opts = params
            .entry("initializationOptions")
            .or_insert_with(|| serde_json::json!({}));
        if !opts.is_object() {
            *opts = serde_json::json!({});
        }
        opts.as_object_mut()
            .expect("initializationOptions forced to an object")
            .insert(
                "root".to_string(),
                serde_json::Value::String(root.display().to_string()),
            );
    }
    let has_root_uri = params.get("rootUri").is_some_and(|u| !u.is_null());
    let has_folders = params
        .get("workspaceFolders")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|a| !a.is_empty());
    if !has_root_uri && !has_folders {
        params.insert(
            "rootUri".to_string(),
            serde_json::Value::String(dir_uri(root_dir)),
        );
    }
    v.to_string()
}

/// `file://` URI for a directory path (best-effort; enough for local use).
fn dir_uri(dir: &Path) -> String {
    let p = dir.display().to_string().replace('\\', "/");
    if p.starts_with('/') {
        format!("file://{p}")
    } else {
        format!("file:///{p}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frame_round_trip_including_multibyte_utf8() {
        let payloads = [
            r#"{"jsonrpc":"2.0","id":1,"method":"x"}"#.to_string(),
            r#"{"msg":"héllo → 世界 🚀"}"#.to_string(),
            String::new(),
        ];
        let (mut a, b) = tokio::io::duplex(4096);
        for p in &payloads {
            write_frame(&mut a, p).await.unwrap();
        }
        drop(a);
        let mut r = BufReader::new(b);
        for p in &payloads {
            assert_eq!(
                read_frame(&mut r).await.unwrap().as_deref(),
                Some(p.as_str())
            );
        }
        assert_eq!(read_frame(&mut r).await.unwrap(), None);
    }

    #[tokio::test]
    async fn read_frame_ignores_extra_headers_and_case() {
        let (mut a, b) = tokio::io::duplex(4096);
        a.write_all(b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc\r\n\r\n{}")
            .await
            .unwrap();
        drop(a);
        let mut r = BufReader::new(b);
        assert_eq!(read_frame(&mut r).await.unwrap().as_deref(), Some("{}"));
    }

    #[test]
    fn inject_root_sets_options_and_root_uri() {
        let msg = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"capabilities":{}}}"#;
        let out = inject_root(msg, Some(Path::new("/repo/main.wcl")), Path::new("/repo"));
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["params"]["initializationOptions"]["root"],
            "/repo/main.wcl"
        );
        assert_eq!(v["params"]["rootUri"], "file:///repo");
        assert_eq!(v["params"]["capabilities"], serde_json::json!({}));
    }

    #[test]
    fn inject_root_preserves_existing_options_and_client_root_uri() {
        let msg = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"rootUri":"file:///elsewhere","initializationOptions":{"keep":true}}}"#;
        let out = inject_root(msg, Some(Path::new("/repo/main.wcl")), Path::new("/repo"));
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["params"]["initializationOptions"]["keep"], true);
        assert_eq!(
            v["params"]["initializationOptions"]["root"],
            "/repo/main.wcl"
        );
        assert_eq!(v["params"]["rootUri"], "file:///elsewhere");
    }

    #[test]
    fn inject_root_without_root_file_leaves_options_alone() {
        let msg = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}"#;
        let out = inject_root(msg, None, Path::new("/repo"));
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["params"].get("initializationOptions").is_none());
        assert_eq!(v["params"]["rootUri"], "file:///repo");
    }

    #[test]
    fn non_initialize_messages_pass_through_verbatim() {
        let msg = r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"x":1}}"#;
        assert_eq!(
            inject_root(msg, Some(Path::new("/r/main.wcl")), Path::new("/r")),
            msg
        );
    }
}
