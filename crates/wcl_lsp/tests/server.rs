//! End-to-end tests that drive the `Backend` through `tower-lsp`'s
//! `LanguageServer` trait. Diagnostics publication isn't exercised
//! here — the underlying `diagnostics::compute` already has unit
//! coverage and publishing depends on the in-memory `ClientSocket`
//! which we'd otherwise need to drain.

use tower_lsp::LanguageServer;
use tower_lsp::LspService;
use tower_lsp::lsp_types::{
    CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, FormattingOptions, HoverParams, InitializeParams,
    PartialResultParams, Position, Range, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentPositionParams, Url, VersionedTextDocumentIdentifier,
    WorkDoneProgressParams,
};
use wcl_lsp::Backend;

/// Construct an `LspService` so its inner `Backend` is wired to a
/// real `Client` (one half of an unused in-memory channel). Tests
/// keep the service value alive and call `LanguageServer` methods on
/// the service's inner backend — no transport is driven.
fn service() -> LspService<Backend> {
    let (svc, _socket) = LspService::new(Backend::new);
    svc
}

async fn open(b: &Backend, uri: &Url, text: &str) {
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
async fn initialize_advertises_expected_capabilities() {
    let svc = service();
    let backend = svc.inner();
    let resp = backend
        .initialize(InitializeParams::default())
        .await
        .expect("initialize");
    let caps = resp.capabilities;
    assert!(caps.completion_provider.is_some());
    assert!(caps.definition_provider.is_some());
    assert!(caps.references_provider.is_some());
    assert!(caps.hover_provider.is_some());
    assert!(caps.document_symbol_provider.is_some());
    assert!(caps.document_formatting_provider.is_some());
    assert!(caps.semantic_tokens_provider.is_some());
}

#[tokio::test]
async fn formatting_emits_canonical_source() {
    let svc = service();
    let backend = svc.inner();
    let uri = Url::parse("file:///a.wcl").unwrap();
    open(backend, &uri, "@schemaless foo  =   1\n").await;
    let edits = backend
        .formatting(DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri },
            options: FormattingOptions::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("formatting")
        .expect("some edits");
    assert_eq!(edits.len(), 1);
    let new = &edits[0].new_text;
    // Canonical output collapses multi-space runs.
    assert!(new.contains("foo = 1"), "got: {new:?}");
}

#[tokio::test]
async fn completion_after_at_lists_builtin_decorators() {
    let svc = service();
    let backend = svc.inner();
    let uri = Url::parse("file:///a.wcl").unwrap();
    let src = "@\ntype Trailing {\n}\n";
    open(backend, &uri, src).await;
    let resp = backend
        .completion(CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: 0,
                    character: 1,
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        })
        .await
        .expect("completion");
    let Some(CompletionResponse::Array(items)) = resp else {
        panic!("expected array response, got {resp:?}");
    };
    let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"block"), "{labels:?}");
    assert!(labels.contains(&"document"), "{labels:?}");
}

#[tokio::test]
async fn hover_on_block_kind_returns_decl_snippet() {
    let svc = service();
    let backend = svc.inner();
    let uri = Url::parse("file:///a.wcl").unwrap();
    let src = "@document\ntype Root {\n  c: Config\n}\n@block(\"config\")\ntype Config {\n  region: utf8\n}\nconfig {\n  region = \"x\"\n}\n";
    open(backend, &uri, src).await;
    // Position the cursor over the lowercase `config` block kind.
    let line = src[..src.find("config {").unwrap()].matches('\n').count() as u32;
    let character = 2; // a few chars into "config"
    let resp = backend
        .hover(HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("hover");
    let body = match resp.expect("hover present").contents {
        tower_lsp::lsp_types::HoverContents::Markup(m) => m.value,
        other => panic!("expected markdown, got {other:?}"),
    };
    assert!(body.contains("block kind"), "{body}");
    assert!(body.contains("type Config"), "{body}");
}

async fn format_source(backend: &Backend, uri: Url) -> String {
    backend
        .formatting(DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri },
            options: FormattingOptions::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("formatting")
        .and_then(|edits| edits.into_iter().next().map(|e| e.new_text))
        .unwrap_or_default()
}

#[tokio::test]
async fn did_change_applies_ranged_edit() {
    let svc = service();
    let backend = svc.inner();
    let uri = Url::parse("file:///inc.wcl").unwrap();
    open(backend, &uri, "@schemaless\nfoo = 1\n").await;
    // Replace the `1` at line 1, col 6..7 with `42`.
    backend
        .did_change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 1,
                        character: 6,
                    },
                    end: Position {
                        line: 1,
                        character: 7,
                    },
                }),
                range_length: None,
                text: "42".into(),
            }],
        })
        .await;
    let new = format_source(backend, uri).await;
    assert!(new.contains("foo = 42"), "got: {new:?}");
}

#[tokio::test]
async fn did_change_full_replace_resets_doc() {
    let svc = service();
    let backend = svc.inner();
    let uri = Url::parse("file:///rep.wcl").unwrap();
    open(backend, &uri, "@schemaless\nfoo = 1\n").await;
    backend
        .did_change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "@schemaless\nbar = 2\n".into(),
            }],
        })
        .await;
    let new = format_source(backend, uri).await;
    assert!(new.contains("bar = 2"), "got: {new:?}");
    assert!(!new.contains("foo"), "stale content: {new:?}");
}
