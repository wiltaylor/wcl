//! Wire-level tests: a real server on an in-memory stream, driven by a
//! minimal JSON-RPC client. Diagnostics are notifications the server
//! pushes on its own schedule, so publishing can only be observed here.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tower_lsp_server::ls_types::Uri;

/// The client half of an LSP session.
struct Session {
    /// Messages to the server.
    write: tokio::io::WriteHalf<DuplexStream>,
    /// Messages from the server.
    read: BufReader<tokio::io::ReadHalf<DuplexStream>>,
    /// Id of the next request.
    next_id: i64,
    /// Every `publishDiagnostics` received so far, oldest first.
    published: Vec<Value>,
}

impl Session {
    /// Start a server and run `initialize` / `initialized` against it,
    /// with `root` as the workspace folder when given.
    async fn start(root: Option<&Path>, capabilities: Value) -> Self {
        let (client, server) = tokio::io::duplex(1 << 20);
        let (server_read, server_write) = tokio::io::split(server);
        tokio::spawn(wcl_lsp::serve_stream(
            server_read,
            server_write,
            wcl_lsp::Host::new(wcl_wdoc::wdoc_environment(), wcl_wdoc::schema_registry()),
        ));
        let (read, write) = tokio::io::split(client);
        let mut session = Self {
            write,
            read: BufReader::new(read),
            next_id: 1,
            published: Vec::new(),
        };
        let folders = root.map(|dir| json!([{ "uri": uri(dir).as_str(), "name": "ws" }]));
        session
            .request(
                "initialize",
                json!({ "capabilities": capabilities, "workspaceFolders": folders }),
            )
            .await;
        session.notify("initialized", json!({})).await;
        session
    }

    async fn send(&mut self, message: Value) {
        let body = message.to_string();
        let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        self.write.write_all(frame.as_bytes()).await.unwrap();
    }

    async fn receive(&mut self) -> Value {
        let mut length = 0;
        loop {
            let mut line = String::new();
            self.read.read_line(&mut line).await.unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                length = value.parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        self.read.read_exact(&mut body).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    /// Read one message, recording diagnostics and answering any
    /// server → client request with `null`.
    async fn pump(&mut self) -> Value {
        let message = self.receive().await;
        if message["method"] == "textDocument/publishDiagnostics" {
            self.published.push(message["params"].clone());
        } else if message.get("method").is_some() && message.get("id").is_some() {
            let reply = json!({ "jsonrpc": "2.0", "id": message["id"], "result": null });
            self.send(reply).await;
        }
        message
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.send(request).await;
        loop {
            let message = self.pump().await;
            if message["id"] == id && message.get("method").is_none() {
                return message["result"].clone();
            }
        }
    }

    async fn notify(&mut self, method: &str, params: Value) {
        let notification = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.send(notification).await;
    }

    async fn open(&mut self, uri: &Uri, text: &str) {
        let params = json!({ "textDocument": {
            "uri": uri.as_str(), "languageId": "wcl", "version": 1, "text": text,
        }});
        self.notify("textDocument/didOpen", params).await;
    }

    async fn replace_all(&mut self, uri: &Uri, version: i32, text: &str) {
        let params = json!({
            "textDocument": { "uri": uri.as_str(), "version": version },
            "contentChanges": [{ "text": text }],
        });
        self.notify("textDocument/didChange", params).await;
    }

    /// The latest diagnostics published for `uri` once they satisfy
    /// `accept`. Panics after a generous timeout.
    async fn diagnostics_until(
        &mut self,
        uri: &Uri,
        accept: impl Fn(&[Value]) -> bool,
    ) -> Vec<Value> {
        let latest = |published: &[Value]| {
            published
                .iter()
                .rev()
                .find(|p| p["uri"] == uri.as_str())
                .map(|p| p["diagnostics"].as_array().cloned().unwrap_or_default())
        };
        let wait = async {
            loop {
                if let Some(diagnostics) = latest(&self.published)
                    && accept(&diagnostics)
                {
                    return diagnostics;
                }
                self.pump().await;
            }
        };
        match tokio::time::timeout(Duration::from_secs(20), wait).await {
            Ok(diagnostics) => diagnostics,
            Err(_) => panic!(
                "no acceptable diagnostics for {}; published: {:#?}",
                uri.as_str(),
                self.published
            ),
        }
    }
}

fn uri(path: &Path) -> Uri {
    Uri::from_file_path(path).expect("absolute path")
}

fn messages(diagnostics: &[Value]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| d["message"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// A rooted workspace whose imported file uses an undeclared decorator
/// on its third line. The URIs are in the tempdir's own spelling, as an
/// editor opening that folder would name them — not canonicalised,
/// which on macOS (`/private/var`) and Windows (long names) differs.
fn workspace_with_broken_import() -> (tempfile::TempDir, Uri, Uri) {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    std::fs::write(
        &main,
        "import \"./shared.wcl\"\n@document type Root { title: utf8 }\n",
    )
    .unwrap();
    std::fs::write(&shared, "// shared\n\n@missing\ntitle = \"Hi\"\n").unwrap();
    let main = uri(&main);
    let shared = uri(&shared);
    (dir, main, shared)
}

#[tokio::test]
async fn imported_file_errors_are_published_to_that_file() {
    let (dir, main, shared) = workspace_with_broken_import();
    let mut session = Session::start(Some(dir.path()), json!({})).await;
    session
        .open(
            &main,
            &std::fs::read_to_string(dir.path().join("main.wcl")).unwrap(),
        )
        .await;

    // The error lives in shared.wcl — not open — at its own line.
    let in_shared = session.diagnostics_until(&shared, |d| !d.is_empty()).await;
    assert!(
        messages(&in_shared)
            .iter()
            .any(|m| m.contains("decorator 'missing'")),
        "{in_shared:#?}"
    );
    let range = &in_shared[0]["range"];
    assert_eq!(range["start"], json!({ "line": 2, "character": 1 }));
    // The root itself stays clean.
    let in_main = session.diagnostics_until(&main, |_| true).await;
    assert!(in_main.is_empty(), "{in_main:#?}");
}

#[tokio::test]
async fn every_kind_of_schema_error_in_an_import_is_published_to_that_file() {
    // A type mismatch, an unknown field, a missing required field and an
    // out-of-range number, all in data.wcl, which is never opened.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("types.wcl"),
        "@block(\"server\", required_fields = [\"host\"])\n\
         type Server {\n  port: u16\n  host: utf8\n  @max(10) workers: i64\n}\n\
         @document type Root { @children(\"server\") servers: list<Server> }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("data.wcl"),
        "// data\n\nserver web {\n  port = \"eighty\"\n  colour = \"red\"\n  workers = 99\n}\n",
    )
    .unwrap();
    let main_text = "import \"./types.wcl\"\nimport \"./data.wcl\"\n";
    std::fs::write(dir.path().join("main.wcl"), main_text).unwrap();
    let main = uri(&dir.path().join("main.wcl"));
    let data = uri(&dir.path().join("data.wcl"));

    let mut session = Session::start(Some(dir.path()), json!({})).await;
    session.open(&main, main_text).await;

    let in_data = session.diagnostics_until(&data, |d| d.len() == 4).await;
    let line_of = |needle: &str| {
        let found = in_data
            .iter()
            .find(|d| d["message"].as_str().unwrap_or_default().contains(needle))
            .unwrap_or_else(|| panic!("no diagnostic mentioning {needle}: {in_data:#?}"));
        found["range"]["start"]["line"].clone()
    };
    assert_eq!(line_of("'port' declared as u16"), json!(3));
    assert_eq!(line_of("'colour' is not declared"), json!(4));
    assert_eq!(line_of("above @max(10)"), json!(5));
    assert_eq!(line_of("missing required field 'host'"), json!(2));
    // None of them lands on the root.
    let in_main = session.diagnostics_until(&main, |_| true).await;
    assert!(in_main.is_empty(), "{in_main:#?}");
}

#[tokio::test]
async fn editing_an_import_republishes_the_root_and_clears_fixed_files() {
    let (dir, main, shared) = workspace_with_broken_import();
    let mut session = Session::start(Some(dir.path()), json!({})).await;
    let main_text = std::fs::read_to_string(dir.path().join("main.wcl")).unwrap();
    session.open(&main, &main_text).await;
    session
        .open(&shared, "// shared\n\n@missing\ntitle = \"Hi\"\n")
        .await;
    session.diagnostics_until(&shared, |d| !d.is_empty()).await;

    // Fixing the import clears its diagnostics.
    session.replace_all(&shared, 2, "title = \"Hi\"\n").await;
    session
        .diagnostics_until(&shared, <[Value]>::is_empty)
        .await;

    // Breaking a declaration the root depends on — from the imported
    // file — republishes the root with the error it now has.
    session
        .replace_all(
            &main,
            2,
            "import \"./shared.wcl\"\n@document type Root { title: utf8 note: shared.Note }\n",
        )
        .await;
    session
        .replace_all(
            &shared,
            3,
            "namespace shared\ntype Note { text: utf8 }\ntitle = \"Hi\"\n",
        )
        .await;
    session.diagnostics_until(&main, <[Value]>::is_empty).await;
    session
        .replace_all(&shared, 4, "namespace shared\ntitle = \"Hi\"\n")
        .await;
    let in_main = session.diagnostics_until(&main, |d| !d.is_empty()).await;
    assert!(
        messages(&in_main).iter().any(|m| m.contains("Note")),
        "{in_main:#?}"
    );
}

#[tokio::test]
async fn per_file_mode_places_imported_errors_in_the_imported_file() {
    let (dir, main, shared) = workspace_with_broken_import();
    // No workspace folder: no root, so the open file is its own document.
    let mut session = Session::start(None, json!({})).await;
    let main_text = std::fs::read_to_string(dir.path().join("main.wcl")).unwrap();
    session.open(&main, &main_text).await;
    let in_shared = session.diagnostics_until(&shared, |d| !d.is_empty()).await;
    assert_eq!(in_shared[0]["range"]["start"]["line"], 2, "{in_shared:#?}");
}

#[tokio::test]
async fn syntax_errors_in_files_outside_the_root_graph_are_published() {
    let (dir, main, _) = workspace_with_broken_import();
    let mut session = Session::start(Some(dir.path()), json!({})).await;
    session
        .open(
            &main,
            &std::fs::read_to_string(dir.path().join("main.wcl")).unwrap(),
        )
        .await;
    let orphan = uri(&dir.path().join("orphan.wcl"));
    session.open(&orphan, "\n@schemaless x = {\n").await;
    let diagnostics = session.diagnostics_until(&orphan, |d| !d.is_empty()).await;
    assert_eq!(diagnostics[0]["code"], "wcl::parse", "{diagnostics:#?}");
}

#[tokio::test]
async fn a_burst_of_edits_publishes_the_latest_version_only() {
    let dir = tempfile::tempdir().unwrap();
    let file = uri(&dir.path().join("burst.wcl"));
    let mut session = Session::start(None, json!({})).await;
    session.open(&file, "@schemaless x = 1\n").await;
    session.diagnostics_until(&file, |_| true).await;
    let before = session.published.len();

    // Versions 2..=6, each a syntax error; only the last survives.
    for version in 2..=6 {
        let text = format!("@schemaless x = {{ {version}\n");
        session.replace_all(&file, version, &text).await;
    }
    session.diagnostics_until(&file, |d| !d.is_empty()).await;
    // Give any overtaken pass time to (wrongly) publish.
    let _ = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            session.pump().await;
        }
    })
    .await;
    let versions: Vec<i64> = session.published[before..]
        .iter()
        .filter(|p| p["uri"] == file.as_str())
        .map(|p| p["version"].as_i64().unwrap_or_default())
        .collect();
    assert_eq!(versions.last(), Some(&6), "{versions:?}");
    assert!(
        versions.windows(2).all(|pair| pair[0] <= pair[1]),
        "a stale pass published after a newer one: {versions:?}"
    );
    assert!(versions.len() < 5, "edits were not debounced: {versions:?}");
}

/// The zero-based `{line, character}` of the first `needle` in `text`,
/// `into` characters in (ASCII text only).
fn position(text: &str, needle: &str, into: usize) -> Value {
    let offset = text.find(needle).expect("needle") + into;
    let line = text[..offset].matches('\n').count();
    let character = offset - text[..offset].rfind('\n').map_or(0, |i| i + 1);
    json!({ "line": line, "character": character })
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_workspace_is_answered_in_the_client_spelling() {
    // The editor reaches the workspace through a symlink (as macOS
    // reaches every temp dir: /var → /private/var). The server works in
    // canonical paths; every URI it sends back must use the spelling the
    // editor opened, or the editor opens a second tab or drops edits.
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let main_text = "import \"./shared.wcl\"\nimport \"./data.wcl\"\n\
                     @document type Root { title: utf8 }\ntype Wrap { c: shared.Color }\n";
    let shared_text = "namespace shared\ntype Color { name: utf8 }\n";
    std::fs::write(real.join("main.wcl"), main_text).unwrap();
    std::fs::write(real.join("shared.wcl"), shared_text).unwrap();
    std::fs::write(
        real.join("data.wcl"),
        "// data\n\n@missing\ntitle = \"Hi\"\n",
    )
    .unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let main = uri(&link.join("main.wcl"));
    let shared = uri(&link.join("shared.wcl"));
    let data = uri(&link.join("data.wcl"));

    let mut session = Session::start(Some(&link), json!({})).await;
    session.open(&main, main_text).await;

    // An error in a file that is not open is published under the
    // spelling of the directory the editor knows.
    let in_data = session.diagnostics_until(&data, |d| !d.is_empty()).await;
    assert_eq!(in_data[0]["range"]["start"]["line"], 2, "{in_data:#?}");

    // Go-to-definition into an import that is not open.
    let at_color = json!({
        "textDocument": { "uri": main.as_str() },
        "position": position(main_text, "Color", 1),
    });
    let definition = session
        .request("textDocument/definition", at_color.clone())
        .await;
    assert_eq!(definition["uri"], shared.as_str(), "{definition:#?}");
    assert_eq!(definition["range"]["start"]["line"], 1, "{definition:#?}");

    // The import opened with unsaved text: its buffer is found, and the
    // answer names it the way it was opened.
    let unsaved = format!("\n\n{shared_text}");
    session.open(&shared, &unsaved).await;
    let definition = session
        .request("textDocument/definition", at_color.clone())
        .await;
    assert_eq!(definition["uri"], shared.as_str(), "{definition:#?}");
    assert_eq!(definition["range"]["start"]["line"], 3, "{definition:#?}");

    // Rename across both files edits each once, under its open URI.
    let mut params = at_color;
    params["newName"] = json!("Hue");
    let edit = session.request("textDocument/rename", params).await;
    let changes = edit["changes"].as_object().expect("changes");
    let mut targets: Vec<&str> = changes.keys().map(String::as_str).collect();
    targets.sort_unstable();
    let mut expected = vec![main.as_str(), shared.as_str()];
    expected.sort_unstable();
    assert_eq!(targets, expected, "{edit:#?}");
    assert_eq!(changes[shared.as_str()][0]["range"]["start"]["line"], 3);

    // Nothing was ever published under the canonical spelling.
    let canonical: Vec<&Value> = session
        .published
        .iter()
        .filter(|p| p["uri"].as_str().is_some_and(|u| u.contains("/real/")))
        .collect();
    assert!(canonical.is_empty(), "{canonical:#?}");
}
