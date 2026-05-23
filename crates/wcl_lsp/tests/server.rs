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

/// Build an `InitializeParams` whose workspace folder points at
/// `dir`. The first folder is what `Backend::resolve_root` checks.
fn init_params_for(dir: &std::path::Path) -> InitializeParams {
    use tower_lsp::lsp_types::WorkspaceFolder;
    InitializeParams {
        workspace_folders: Some(vec![WorkspaceFolder {
            uri: Url::from_directory_path(dir).expect("dir url"),
            name: "ws".into(),
        }]),
        ..Default::default()
    }
}

#[tokio::test]
async fn initialize_discovers_main_wcl_at_workspace_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    std::fs::write(&main, "@document\ntype App { name: utf8 }\nname = \"x\"\n").unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    let root = backend.root_path().expect("root discovered");
    assert_eq!(
        std::fs::canonicalize(&root).unwrap(),
        std::fs::canonicalize(&main).unwrap()
    );
}

#[tokio::test]
async fn initialize_honours_initialization_options_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    let custom = dir.path().join("custom.wcl");
    std::fs::write(&custom, "@document\ntype A {}\n").unwrap();

    let svc = service();
    let backend = svc.inner();
    let mut params = init_params_for(dir.path());
    params.initialization_options = Some(serde_json::json!({"root": "custom.wcl"}));
    backend.initialize(params).await.expect("initialize");

    assert_eq!(
        std::fs::canonicalize(backend.root_path().expect("root")).unwrap(),
        std::fs::canonicalize(&custom).unwrap()
    );
}

#[tokio::test]
async fn completion_surfaces_types_from_imported_files() {
    // Workspace: main.wcl imports shared.wcl. Editing main.wcl,
    // completion at a type-ref position should include `Color`
    // declared *in shared.wcl* — only visible when the root document
    // is consulted.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    std::fs::write(
        &main,
        "import \"./shared.wcl\"\ntype Brand { name: utf8 }\ntype Wrap { x: utf8 }\n",
    )
    .unwrap();
    std::fs::write(&shared, "namespace shared\ntype Color { name: utf8 }\n").unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    let main_uri = Url::from_file_path(&main).unwrap();
    let edited = std::fs::read_to_string(&main).unwrap();
    open(backend, &main_uri, &edited).await;

    // Cursor sits just after the `:` in `x: utf8`. preceding_non_ws
    // sees `:` and the completion handler returns type_items, which
    // should include both local types (`Brand`, `Wrap`) and the
    // imported `shared.Color`.
    let line = edited[..edited.find("x: utf8").unwrap()]
        .matches('\n')
        .count() as u32;
    let character = (edited
        .lines()
        .nth(line as usize)
        .unwrap()
        .find("x: utf8")
        .unwrap()
        + 3) as u32;
    let resp = backend
        .completion(CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: main_uri.clone(),
                },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        })
        .await
        .expect("completion");
    let Some(CompletionResponse::Array(items)) = resp else {
        panic!("expected array, got {resp:?}");
    };
    let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        labels.iter().any(|l| *l == "shared.Color" || *l == "Color"),
        "imported type Color should appear in completions: {labels:?}"
    );
    assert!(labels.contains(&"Brand"), "local type Brand: {labels:?}");
}

#[tokio::test]
async fn goto_definition_crosses_into_imported_file() {
    // main.wcl declares `@block("color") type Color`; shared.wcl
    // uses the `color` block kind. Editing shared.wcl, a goto-def
    // on `color` should land in main.wcl — requires the root doc
    // for both the per-file `local_doc` fallback (shared.wcl alone
    // doesn't know the `color` kind) and for the cross-file
    // symbol-source lookup.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    std::fs::write(
        &main,
        "import \"./shared.wcl\"\n@block(\"color\")\ntype Color { name: utf8 }\n",
    )
    .unwrap();
    std::fs::write(&shared, "@schemaless color \"x\" { name = \"y\" }\n").unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    let shared_uri = Url::from_file_path(&shared).unwrap();
    let shared_src = std::fs::read_to_string(&shared).unwrap();
    open(backend, &shared_uri, &shared_src).await;

    // Cursor over `color` (the block kind) in shared.wcl.
    let needle = "color";
    let needle_pos = shared_src.find(needle).unwrap();
    let line = shared_src[..needle_pos].matches('\n').count() as u32;
    let line_start = shared_src[..needle_pos].rfind('\n').map_or(0, |p| p + 1);
    let character = (needle_pos - line_start + 2) as u32;
    let resp = backend
        .goto_definition(tower_lsp::lsp_types::GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: shared_uri.clone(),
                },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("goto");

    let main_uri = Url::from_file_path(&main).unwrap();
    let loc = match resp.expect("definition found") {
        tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(l) => l,
        other => panic!("expected scalar, got {other:?}"),
    };
    assert_eq!(loc.uri, main_uri, "definition should live in main.wcl");
}

#[tokio::test]
async fn overlay_lets_root_see_unsaved_edits_in_imported_file() {
    // shared.wcl on disk: no `Color`. Edited buffer adds `Color`.
    // main.wcl imports shared.wcl. With overlay enabled, the root
    // document sees the in-memory `Color` and goto-def from main.wcl
    // lands in shared.wcl.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    std::fs::write(&main, "import \"./shared.wcl\"\n").unwrap();
    std::fs::write(&shared, "namespace shared\n").unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    // Open shared.wcl with an unsaved `Color` declaration.
    let shared_uri = Url::from_file_path(&shared).unwrap();
    open(
        backend,
        &shared_uri,
        "namespace shared\ntype Color { name: utf8 }\n",
    )
    .await;

    // Root document built with overlay should now see `shared.Color`.
    let root_doc = backend.root_document().expect("root opens");
    assert!(
        root_doc.find_symbol("shared.Color").is_some(),
        "overlayed Color should appear in root's symbol index"
    );
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
