//! End-to-end test for `textDocument/codeAction`: drive `Backend`
//! with a document that triggers an unknown-field violation and
//! confirm a quick-fix comes back.

use tower_lsp_server::LanguageServer;
use tower_lsp_server::LspService;
use tower_lsp_server::ls_types::{
    CodeActionContext, CodeActionOrCommand, CodeActionParams, Diagnostic, DiagnosticSeverity,
    DidOpenTextDocumentParams, PartialResultParams, Position, Range, TextDocumentIdentifier,
    TextDocumentItem, Uri, WorkDoneProgressParams,
};
use wcl_lsp::{Backend, Host};

/// The wdoc host `wcl lsp` runs with, so wdoc documents open as they
/// would in the editor.
fn wdoc_host() -> Host {
    Host::new(wcl_wdoc::wdoc_environment(), wcl_wdoc::schema_registry())
}

fn service() -> LspService<Backend> {
    let (svc, _socket) = LspService::new(|client| Backend::new(client, wdoc_host()));
    svc
}

async fn open(b: &Backend, uri: &Uri, text: &str) {
    b.did_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "wcl".into(),
            version: 1,
            text: text.into(),
        },
    })
    .await;
}

#[tokio::test]
async fn unknown_field_code_action_round_trip() {
    let svc = service();
    let backend = svc.inner();
    let uri = "file:///cfg.wcl".parse::<Uri>().unwrap();
    // The `bogus` field isn't on Config — the validator emits a
    // wcl::eval::schema_violation that the editor hands back via
    // `params.context.diagnostics`. We construct that diagnostic here
    // to keep the test self-contained, including the structured `data`
    // payload the server attaches (and editors round-trip verbatim).
    let src = "@document\ntype Config {\n  name: utf8\n}\nname = \"a\"\nbogus = 1\n";
    open(backend, &uri, src).await;
    let line_with_bogus = src[..src.find("bogus").unwrap()].matches('\n').count() as u32;
    let diag = Diagnostic {
        range: Range {
            start: Position {
                line: line_with_bogus,
                character: 0,
            },
            end: Position {
                line: line_with_bogus,
                character: 5,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("wcl".into()),
        message: "unknown field 'bogus' in document".into(),
        data: Some(serde_json::json!({ "kind": "UnknownField", "name": "bogus" })),
        ..Default::default()
    };
    let resp = backend
        .code_action(CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: diag.range,
            context: CodeActionContext {
                diagnostics: vec![diag.clone()],
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("code_action")
        .expect("at least one action");
    assert_eq!(resp.len(), 1);
    let CodeActionOrCommand::CodeAction(action) = &resp[0] else {
        panic!("expected action")
    };
    assert!(
        action.title.to_lowercase().contains("bogus"),
        "title: {}",
        action.title
    );
    let edit = action.edit.as_ref().unwrap();
    let changes = edit.changes.as_ref().unwrap();
    let edits = changes.get(&uri).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "");
}
