//! End-to-end tests that drive the `Backend` through `tower-lsp-server`'s
//! `LanguageServer` trait. Diagnostics publication isn't exercised
//! here — the client only sends notifications once the service has seen
//! `initialized`, so `tests/protocol.rs` covers it over a real stream.

use std::path::PathBuf;

use tower_lsp_server::LanguageServer;
use tower_lsp_server::LspService;
use tower_lsp_server::ls_types::{
    CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, FormattingOptions, HoverParams, InitializeParams,
    PartialResultParams, Position, Range, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentPositionParams, Uri, VersionedTextDocumentIdentifier,
    WorkDoneProgressParams,
};
use wcl_lsp::{Backend, Host};

/// The wdoc host `wcl lsp` runs with, so wdoc documents open as they
/// would in the editor.
fn wdoc_host() -> Host {
    Host::new(wcl_wdoc::wdoc_environment(), wcl_wdoc::schema_registry())
}

/// A `file:` URI naming `name` in a fresh temp directory, which lives
/// as long as the returned guard. A literal `file:///a.wcl` names an
/// absolute path on Unix but only the relative `a.wcl` on Windows.
fn scratch_uri(name: &str) -> (tempfile::TempDir, Uri) {
    let dir = tempfile::tempdir().expect("tempdir");
    let uri = Uri::from_file_path(dir.path().join(name)).expect("absolute path");
    (dir, uri)
}

/// Construct an `LspService` so its inner `Backend` is wired to a
/// real `Client` (one half of an unused in-memory channel). Tests
/// keep the service value alive and call `LanguageServer` methods on
/// the service's inner backend — no transport is driven.
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

struct QualifiedDecoratorFixture {
    _dir: tempfile::TempDir,
    service: LspService<Backend>,
    main_uri: Uri,
    decorator_position: Position,
    selected_schema: PathBuf,
}

async fn qualified_decorator_fixture() -> QualifiedDecoratorFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let one = dir.path().join("one.wcl");
    let two = dir.path().join("two.wcl");
    std::fs::write(
        &one,
        "namespace one\n@decorator(\"note\") type OneNote { code: i64 }\n",
    )
    .unwrap();
    std::fs::write(
        &two,
        "namespace two\n@decorator(\"note\") type TwoNote { message: utf8 }\n",
    )
    .unwrap();
    let src = "import \"./one.wcl\"\nimport \"./two.wcl\"\n@two.note(message = \"selected\") type Target {}\n";
    std::fs::write(&main, src).unwrap();

    let service = service();
    let backend = service.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");
    let main_uri = Uri::from_file_path(&main).unwrap();
    open(backend, &main_uri, src).await;
    let needle_pos = src.find("note(message").unwrap();
    let line = src[..needle_pos].matches('\n').count() as u32;
    let line_start = src[..needle_pos].rfind('\n').map_or(0, |p| p + 1);
    let character = (needle_pos - line_start + 2) as u32;

    QualifiedDecoratorFixture {
        _dir: dir,
        service,
        main_uri,
        decorator_position: Position { line, character },
        selected_schema: two,
    }
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
    assert!(caps.workspace_symbol_provider.is_some());
    let sig = caps.signature_help_provider.expect("signature help");
    assert_eq!(
        sig.trigger_characters,
        Some(vec!["(".to_string(), ",".to_string()])
    );
}

#[tokio::test]
async fn formatting_emits_canonical_source() {
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("a.wcl");
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
    let (_dir, uri) = scratch_uri("a.wcl");
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
    let (_dir, uri) = scratch_uri("a.wcl");
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
        tower_lsp_server::ls_types::HoverContents::Markup(m) => m.value,
        other => panic!("expected markdown, got {other:?}"),
    };
    assert!(body.contains("block kind"), "{body}");
    assert!(body.contains("type Config"), "{body}");
}

#[tokio::test]
async fn hover_on_qualified_decorator_shows_qualified_schema() {
    let fixture = qualified_decorator_fixture().await;

    let response = fixture
        .service
        .inner()
        .hover(HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture.main_uri.clone(),
                },
                position: fixture.decorator_position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("hover request")
        .expect("hover present");
    let body = match response.contents {
        tower_lsp_server::ls_types::HoverContents::Markup(markup) => markup.value,
        other => panic!("expected markdown, got {other:?}"),
    };

    assert!(body.contains("decorator** `two.TwoNote`"), "{body}");
    assert!(body.contains("type TwoNote"), "{body}");
    assert!(!body.contains("type OneNote"), "{body}");
}

#[tokio::test]
async fn goto_definition_on_qualified_decorator_opens_qualified_schema() {
    let fixture = qualified_decorator_fixture().await;

    let response = fixture
        .service
        .inner()
        .goto_definition(tower_lsp_server::ls_types::GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture.main_uri.clone(),
                },
                position: fixture.decorator_position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("goto request")
        .expect("definition present");
    let location = match response {
        tower_lsp_server::ls_types::GotoDefinitionResponse::Scalar(location) => location,
        other => panic!("expected scalar location, got {other:?}"),
    };

    assert_eq!(
        location.uri,
        Uri::from_file_path(fixture.selected_schema).unwrap()
    );
}

async fn format_source(backend: &Backend, uri: Uri) -> String {
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
    let (_dir, uri) = scratch_uri("inc.wcl");
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
    use tower_lsp_server::ls_types::WorkspaceFolder;
    InitializeParams {
        workspace_folders: Some(vec![WorkspaceFolder {
            uri: Uri::from_file_path(dir).expect("dir url"),
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

    let main_uri = Uri::from_file_path(&main).unwrap();
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

    let shared_uri = Uri::from_file_path(&shared).unwrap();
    let shared_src = std::fs::read_to_string(&shared).unwrap();
    open(backend, &shared_uri, &shared_src).await;

    // Cursor over `color` (the block kind) in shared.wcl.
    let needle = "color";
    let needle_pos = shared_src.find(needle).unwrap();
    let line = shared_src[..needle_pos].matches('\n').count() as u32;
    let line_start = shared_src[..needle_pos].rfind('\n').map_or(0, |p| p + 1);
    let character = (needle_pos - line_start + 2) as u32;
    let resp = backend
        .goto_definition(tower_lsp_server::ls_types::GotoDefinitionParams {
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

    let main_uri = Uri::from_file_path(&main).unwrap();
    let loc = match resp.expect("definition found") {
        tower_lsp_server::ls_types::GotoDefinitionResponse::Scalar(l) => l,
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
    let shared_uri = Uri::from_file_path(&shared).unwrap();
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
async fn root_resolves_embedded_wdoc_library() {
    // A wdoc document opts into the stdlib with `import <wdoc.wcl>`.
    // The LSP must serve that system import from the embedded registry
    // (chained into its loader) so the document opens and `page`/`h1`
    // validate — exactly as `wcl wdoc build` resolves it.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    std::fs::write(
        &main,
        "import <wdoc.wcl>\npage index {\n  h1 \"Hello\"\n}\n",
    )
    .unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    // `root_document` returns `None` if the `<wdoc.wcl>` import fails to
    // load, so opening at all proves the registry is wired in.
    let root_doc = backend
        .root_document()
        .expect("root opens with the embedded wdoc import resolved");
    assert!(
        root_doc.schema_errors().is_empty(),
        "wdoc blocks should validate via the embedded library: {:?}",
        root_doc.schema_errors()
    );
}

#[tokio::test]
async fn a_plain_host_serves_wcl_without_wdoc() {
    // The server carries no vocabulary of its own. Under the default host
    // a plain WCL document validates, and wdoc's system import is not
    // there to resolve.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    std::fs::write(&main, "@document type D { port: u16 }\nport = 8080u16\n").unwrap();
    let (svc, _socket) = LspService::new(|client| Backend::new(client, Host::default()));
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");
    let root_doc = backend.root_document().expect("plain root opens");
    assert!(root_doc.schema_errors().is_empty());
    assert!(
        root_doc
            .environment()
            .builtins()
            .all(|(name, _)| name != "__wdoc_slot"),
        "no wdoc builtin reaches a plain host"
    );

    std::fs::write(&main, "import <wdoc.wcl>\n").unwrap();
    let (svc, _socket) = LspService::new(|client| Backend::new(client, Host::default()));
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");
    assert!(
        backend.root_document().is_none(),
        "`<wdoc.wcl>` is wdoc's import, not the language's"
    );
}

#[tokio::test]
async fn did_change_full_replace_resets_doc() {
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("rep.wcl");
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

#[tokio::test]
async fn folding_ranges_cover_blocks_and_type_decls() {
    use tower_lsp_server::ls_types::FoldingRangeParams;
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("fold.wcl");
    let src = "type Server {\n  name: utf8\n  port: u16\n}\n\
               @schemaless web service {\n  name = \"web\"\n  nested box {\n    size = 1\n  }\n}\n\
               one_liner = 1\n";
    open(backend, &uri, src).await;
    let ranges = backend
        .folding_range(FoldingRangeParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("folding ok")
        .expect("some ranges");
    // The type decl, the outer block, and the nested block fold; the
    // single-line field does not.
    assert_eq!(ranges.len(), 3, "{ranges:?}");
    let type_fold = &ranges[0];
    assert_eq!((type_fold.start_line, type_fold.end_line), (0, 3));
    let outer = ranges
        .iter()
        .find(|r| r.start_line == 4)
        .expect("outer block fold");
    assert_eq!(outer.end_line, 9);
    let nested = ranges
        .iter()
        .find(|r| r.start_line == 6)
        .expect("nested block fold");
    assert_eq!(nested.end_line, 8);
}

#[tokio::test]
async fn rename_rewrites_every_reference_in_one_file() {
    use tower_lsp_server::ls_types::RenameParams;
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("rn.wcl");
    let src =
        "@schemaless base = 2\n@schemaless doubled = base * 2\n@schemaless tripled = base * 3\n";
    open(backend, &uri, src).await;
    // Cursor on the `base` declaration.
    let edit = backend
        .rename(RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position {
                    line: 0,
                    character: 13,
                },
            },
            new_name: "seed".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("rename ok")
        .expect("workspace edit");
    let changes = edit.changes.expect("changes map");
    let edits = changes.get(&uri).expect("edits for the file");
    // Declaration + two references.
    assert_eq!(edits.len(), 3, "{edits:?}");
    assert!(edits.iter().all(|e| e.new_text == "seed"));
}

#[tokio::test]
async fn rename_without_configured_root_preserves_embedded_imports() {
    use tower_lsp_server::ls_types::RenameParams;
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("embedded-rename.wcl");
    let source = "import <wdoc.wcl>\nlet value = 7\n@schemaless result = value\n";
    open(backend, &uri, source).await;
    let edit = backend
        .rename(RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position {
                    line: 1,
                    character: 5,
                },
            },
            new_name: "amount".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("rename with embedded schema")
        .expect("workspace edit");
    let changes = edit.changes.unwrap();
    assert_eq!(changes.len(), 1);
    let mut edits = changes[&uri].clone();
    assert_eq!(edits.len(), 2);
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));
    let mut updated = source.to_string();
    for edit in edits {
        let offset = |position: Position| {
            source
                .split_inclusive('\n')
                .take(position.line as usize)
                .map(str::len)
                .sum::<usize>()
                + position.character as usize
        };
        let start = offset(edit.range.start);
        let end = offset(edit.range.end);
        updated.replace_range(start..end, &edit.new_text);
    }
    let path = uri.to_file_path().unwrap().into_owned();
    let loader = wcl_wdoc::schema_registry().loader(wcl_lang::overlay_loader(
        std::collections::HashMap::from([(path.clone(), updated)]),
    ));
    let doc =
        wcl_lang::Document::from_file_with_loader(&path, &wcl_wdoc::wdoc_environment(), loader)
            .unwrap();
    assert!(doc.schema_errors().is_empty());
    assert_eq!(
        doc.field("result").unwrap().value().unwrap(),
        &wcl_lang::Value::I64(7)
    );
}

#[tokio::test]
async fn rename_rejects_an_invalid_identifier() {
    use tower_lsp_server::ls_types::RenameParams;
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("rn2.wcl");
    open(backend, &uri, "@schemaless base = 2\n").await;
    let res = backend
        .rename(RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: 0,
                    character: 13,
                },
            },
            new_name: "not valid".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await;
    assert!(res.is_err(), "invalid identifier must be rejected: {res:?}");
}

#[tokio::test]
async fn rename_crosses_into_imported_file() {
    use tower_lsp_server::ls_types::RenameParams;
    // main.wcl uses `Color` declared in shared.wcl; renaming at the
    // use site must edit both files.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    std::fs::write(&main, "import \"./shared.wcl\"\ntype Wrap { c: Color }\n").unwrap();
    std::fs::write(&shared, "type Color { name: utf8 }\n").unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    let main_uri = Uri::from_file_path(&main).unwrap();
    let text = std::fs::read_to_string(&main).unwrap();
    open(backend, &main_uri, &text).await;

    // Cursor on `Color` in `c: Color`.
    let edit = backend
        .rename(RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: main_uri.clone(),
                },
                position: Position {
                    line: 1,
                    character: 15,
                },
            },
            new_name: "Hue".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("rename ok")
        .expect("workspace edit");
    let changes = edit.changes.expect("changes map");
    assert!(
        changes.contains_key(&main_uri),
        "request file edited: {changes:?}"
    );
    let shared_uri = Uri::from_file_path(&shared).unwrap();
    let shared_edits = changes
        .get(&shared_uri)
        .unwrap_or_else(|| panic!("declaration file edited: {changes:?}"));
    assert!(shared_edits.iter().all(|e| e.new_text == "Hue"));
}

#[tokio::test]
async fn signature_help_for_builtin_after_open_paren() {
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("sig.wcl");
    let src = "@schemaless x = len(";
    open(backend, &uri, src).await;
    let help = backend
        .signature_help(tower_lsp_server::ls_types::SignatureHelpParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: 0,
                    character: src.len() as u32,
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            context: None,
        })
        .await
        .expect("signature_help rpc")
        .expect("builtin signature found");
    assert_eq!(help.signatures.len(), 1);
    assert!(
        help.signatures[0].label.starts_with("len("),
        "{}",
        help.signatures[0].label
    );
    assert_eq!(help.active_parameter, Some(0));
}

#[tokio::test]
async fn signature_help_tracks_active_param_for_user_fn() {
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("sig2.wcl");
    let src = "fn add(a: i64, b: i64) -> i64 { a + b }\n@schemaless x = add(1, ";
    open(backend, &uri, src).await;
    let last_line = src.lines().count() as u32 - 1;
    let character = src.lines().last().unwrap().len() as u32;
    let help = backend
        .signature_help(tower_lsp_server::ls_types::SignatureHelpParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position {
                    line: last_line,
                    character,
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            context: None,
        })
        .await
        .expect("signature_help rpc")
        .expect("fn signature found");
    assert_eq!(help.signatures[0].label, "add(a: i64, b: i64) -> i64");
    assert_eq!(help.active_parameter, Some(1));
}

#[tokio::test]
async fn signature_help_resolves_fn_from_imported_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    std::fs::write(
        &main,
        "import \"./shared.wcl\"\n@schemaless x = shared.scale(",
    )
    .unwrap();
    std::fs::write(
        &shared,
        "namespace shared\nfn scale(v: f64, by: f64) -> f64 { v * by }\n",
    )
    .unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    let main_uri = Uri::from_file_path(&main).unwrap();
    let text = std::fs::read_to_string(&main).unwrap();
    open(backend, &main_uri, &text).await;
    let last_line = text.lines().count() as u32 - 1;
    let character = text.lines().last().unwrap().len() as u32;
    let help = backend
        .signature_help(tower_lsp_server::ls_types::SignatureHelpParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: main_uri },
                position: Position {
                    line: last_line,
                    character,
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            context: None,
        })
        .await
        .expect("signature_help rpc")
        .expect("cross-file fn signature found");
    assert_eq!(help.signatures[0].label, "scale(v: f64, by: f64) -> f64");
}

/// The flat symbol list inside a `workspace/symbol` response.
fn flat_symbols(
    response: tower_lsp_server::ls_types::WorkspaceSymbolResponse,
) -> Vec<tower_lsp_server::ls_types::SymbolInformation> {
    match response {
        tower_lsp_server::ls_types::WorkspaceSymbolResponse::Flat(symbols) => symbols,
        other => panic!("expected flat symbols, got {other:?}"),
    }
}

#[tokio::test]
async fn workspace_symbols_span_the_import_graph() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    std::fs::write(&main, "import \"./shared.wcl\"\ntype Local {}\n").unwrap();
    std::fs::write(&shared, "namespace shared\ntype Color { name: utf8 }\n").unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    let hits = backend
        .symbol(tower_lsp_server::ls_types::WorkspaceSymbolParams {
            query: "Col".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("workspace/symbol rpc")
        .map(flat_symbols)
        .expect("some hits");
    let shared_uri = Uri::from_file_path(&shared).unwrap();
    let color = hits
        .iter()
        .find(|s| s.name == "Color")
        .expect("Color found across the graph");
    assert_eq!(color.location.uri, shared_uri);
    assert_eq!(color.kind, tower_lsp_server::ls_types::SymbolKind::CLASS);

    // The empty query lists symbols from both files.
    let all = backend
        .symbol(tower_lsp_server::ls_types::WorkspaceSymbolParams {
            query: String::new(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("workspace/symbol rpc")
        .map(flat_symbols)
        .expect("some hits");
    assert!(all.iter().any(|s| s.name == "Local"));
    assert!(all.iter().any(|s| s.name == "Color"));

    // Subsequence matching: "clr" still finds Color.
    let fuzzy = backend
        .symbol(tower_lsp_server::ls_types::WorkspaceSymbolParams {
            query: "clr".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("workspace/symbol rpc")
        .map(flat_symbols)
        .expect("some hits");
    assert!(fuzzy.iter().any(|s| s.name == "Color"), "{fuzzy:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn workspace_symbols_answer_a_symlinked_workspace_in_its_spelling() {
    // No buffer is open, so the workspace folder is the only spelling
    // the client has given (macOS tempdirs live under /var, a symlink
    // to /private/var; Windows ones under an 8.3 short name).
    let dir = tempfile::tempdir().expect("tempdir");
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    std::fs::write(real.join("main.wcl"), "import \"./shared.wcl\"\n").unwrap();
    std::fs::write(
        real.join("shared.wcl"),
        "namespace shared\ntype Color { name: utf8 }\n",
    )
    .unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(&link))
        .await
        .expect("initialize");
    let hits = backend
        .symbol(tower_lsp_server::ls_types::WorkspaceSymbolParams {
            query: "Color".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("workspace/symbol rpc")
        .map(flat_symbols)
        .expect("some hits");
    let color = hits.iter().find(|s| s.name == "Color").expect("Color");
    assert_eq!(
        color.location.uri,
        Uri::from_file_path(link.join("shared.wcl")).unwrap()
    );
}

#[tokio::test]
async fn workspace_symbols_see_unsaved_overlay_buffer() {
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

    // The unsaved buffer adds `Color`; disk doesn't have it.
    let shared_uri = Uri::from_file_path(&shared).unwrap();
    open(
        backend,
        &shared_uri,
        "namespace shared\ntype Color { name: utf8 }\n",
    )
    .await;

    let hits = backend
        .symbol(tower_lsp_server::ls_types::WorkspaceSymbolParams {
            query: "Color".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("workspace/symbol rpc")
        .map(flat_symbols)
        .expect("some hits");
    assert!(
        hits.iter().any(|s| s.name == "Color"),
        "overlayed symbol searchable: {hits:?}"
    );
}

#[tokio::test]
async fn root_document_expands_contextual_blocks() {
    // The root parse must use the wdoc environment, not a bare one: a
    // `wdoc_repeater` is `@contextual`, so projecting the children it
    // generates is a hard error without wdoc's registered expander —
    // and cross-file resolution rests on this document.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("main.wcl"),
        concat!(
            "import <wdoc.wcl>\n\n",
            "@document(\"lsp_demo\") type LspDemo { @children(\"deck\") decks: list<Deck> }\n",
            "@block(\"deck\") type Deck { @inline(0) name: identifier  @children(\"card\") cards: list<Card> }\n",
            "@block(\"card\") type Card { @inline(0) id: identifier  title: utf8 }\n\n",
            "deck main {\n",
            "  wdoc_repeater { each = [\"one\"]  as = :m\n",
            "    card $\"g_${m}\" { title = $\"generated ${m}\" }\n",
            "  }\n",
            "}\n",
        ),
    )
    .unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    let doc = backend.root_document().expect("root document parses");
    let title = doc
        .get("decks.main.cards.g_one.title")
        .expect("generated card is addressable")
        .value()
        .expect("no missing-expander error");
    assert_eq!(title, wcl_lang::Value::Utf8("generated one".into()));
}

#[tokio::test]
async fn per_file_mode_resolves_relative_and_system_imports() {
    // No root: every request opens the buffer on its own, through the
    // same loader and wdoc environment the diagnostics use — so a
    // relative import and `import <wdoc.wcl>` both resolve.
    use tower_lsp_server::ls_types::{DocumentSymbolParams, DocumentSymbolResponse};
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    std::fs::write(&shared, "namespace shared\ntype Color { name: utf8 }\n").unwrap();
    let src = "import <wdoc.wcl>\nimport \"./shared.wcl\"\ntype Wrap { x: utf8 }\n";
    std::fs::write(&main, src).unwrap();

    let svc = service();
    let backend = svc.inner();
    let main_uri = Uri::from_file_path(&main).unwrap();
    open(backend, &main_uri, src).await;

    let outline = backend
        .document_symbol(DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: main_uri.clone(),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("document symbols");
    let Some(DocumentSymbolResponse::Nested(symbols)) = outline else {
        panic!("expected nested symbols, got {outline:?}");
    };
    assert!(symbols.iter().any(|s| s.name == "Wrap"), "{symbols:?}");

    let line = 2;
    let character = (src.lines().nth(2).unwrap().find("x: utf8").unwrap() + 3) as u32;
    let resp = backend
        .completion(CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: main_uri },
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
        "imported type in per-file mode: {labels:?}"
    );
}

/// `(line, character)` of the `nth` occurrence of `needle` in `text`,
/// plus `into` characters (ASCII text).
fn position_of(text: &str, needle: &str, nth: usize, into: u32) -> Position {
    let offset = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("{needle:?} #{nth} in {text:?}"))
        .0;
    let line = text[..offset].matches('\n').count() as u32;
    let line_start = text[..offset].rfind('\n').map_or(0, |p| p + 1);
    Position::new(line, (offset - line_start) as u32 + into)
}

#[tokio::test]
async fn goto_definition_reads_the_target_from_its_open_buffer() {
    // shared.wcl's unsaved buffer moves `Color` down three lines. The
    // root is built from that buffer, so the range must be computed
    // against it too — not against the stale file on disk.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    let main_src = "import \"./shared.wcl\"\ntype Wrap { c: shared.Color }\n";
    std::fs::write(&main, main_src).unwrap();
    std::fs::write(&shared, "namespace shared\ntype Color { name: utf8 }\n").unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");
    let main_uri = Uri::from_file_path(&main).unwrap();
    let shared_uri = Uri::from_file_path(&shared).unwrap();
    open(backend, &main_uri, main_src).await;
    open(
        backend,
        &shared_uri,
        "namespace shared\n\n\n\ntype Color { name: utf8 }\n",
    )
    .await;

    let resp = backend
        .goto_definition(tower_lsp_server::ls_types::GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: main_uri },
                position: position_of(main_src, "Color", 0, 1),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("goto");
    let tower_lsp_server::ls_types::GotoDefinitionResponse::Scalar(location) =
        resp.expect("definition found")
    else {
        panic!("expected a scalar location");
    };
    assert_eq!(location.uri, shared_uri);
    assert_eq!(location.range.start.line, 4, "{location:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn rename_through_a_symlinked_workspace_edits_each_file_once() {
    use tower_lsp_server::ls_types::RenameParams;
    // The root path is canonical; the editor opened main.wcl through a
    // symlink. Both spellings name one file, which must be edited once,
    // under the URI the editor knows.
    let dir = tempfile::tempdir().expect("tempdir");
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let main_src = "type Foo { x: utf8 }\ntype Wrap { f: Foo }\n";
    std::fs::write(real.join("main.wcl"), main_src).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(&link))
        .await
        .expect("initialize");
    let main_uri = Uri::from_file_path(link.join("main.wcl")).unwrap();
    open(backend, &main_uri, main_src).await;

    let edit = backend
        .rename(RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: main_uri.clone(),
                },
                position: position_of(main_src, "Foo", 0, 1),
            },
            new_name: "Bar".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("rename ok")
        .expect("workspace edit");
    let changes = edit.changes.expect("changes");
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[&main_uri].len(), 2, "{changes:?}");
}

#[tokio::test]
async fn root_document_is_built_once_per_change() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    std::fs::write(&main, "type Local {}\n").unwrap();
    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");

    let first = backend.root_document().expect("root opens");
    let again = backend.root_document().expect("root opens");
    assert!(
        std::sync::Arc::ptr_eq(&first, &again),
        "rebuilt without a change"
    );

    let main_uri = Uri::from_file_path(&main).unwrap();
    open(backend, &main_uri, "type Local {}\ntype Added {}\n").await;
    let changed = backend.root_document().expect("root opens");
    assert!(
        !std::sync::Arc::ptr_eq(&first, &changed),
        "stale after a change"
    );
    assert!(changed.find_symbol("Added").is_some());
}

#[tokio::test]
async fn a_watched_file_change_rebuilds_the_root() {
    // shared.wcl is not open, so only the client's file-watch event
    // says it changed on disk.
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
    assert!(
        backend
            .root_document()
            .expect("root opens")
            .find_symbol("shared.Color")
            .is_none()
    );

    std::fs::write(&shared, "namespace shared\ntype Color {}\n").unwrap();
    backend
        .did_change_watched_files(tower_lsp_server::ls_types::DidChangeWatchedFilesParams {
            changes: vec![tower_lsp_server::ls_types::FileEvent {
                uri: Uri::from_file_path(&shared).unwrap(),
                typ: tower_lsp_server::ls_types::FileChangeType::CHANGED,
            }],
        })
        .await;
    assert!(
        backend
            .root_document()
            .expect("root opens")
            .find_symbol("shared.Color")
            .is_some()
    );
}

#[tokio::test]
async fn references_span_the_import_graph() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shared = dir.path().join("shared.wcl");
    let main_src = "import \"./shared.wcl\"\ntype Wrap { a: shared.Color b: shared.Color }\n";
    std::fs::write(&main, main_src).unwrap();
    std::fs::write(&shared, "namespace shared\ntype Color { name: utf8 }\n").unwrap();

    let svc = service();
    let backend = svc.inner();
    backend
        .initialize(init_params_for(dir.path()))
        .await
        .expect("initialize");
    let main_uri = Uri::from_file_path(&main).unwrap();
    open(backend, &main_uri, main_src).await;

    let references = |include_declaration| {
        backend.references(tower_lsp_server::ls_types::ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: main_uri.clone(),
                },
                position: position_of(main_src, "Color", 1, 1),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: tower_lsp_server::ls_types::ReferenceContext {
                include_declaration,
            },
        })
    };
    let all = references(true)
        .await
        .expect("references")
        .expect("some references");
    let shared_uri = Uri::from_file_path(&shared).unwrap();
    assert_eq!(all.len(), 3, "{all:?}");
    let declaration = all
        .iter()
        .find(|l| l.uri == shared_uri)
        .expect("declaration in shared.wcl");
    assert_eq!(declaration.range.start, Position::new(1, 5));
    let uses = references(false)
        .await
        .expect("references")
        .expect("some references");
    assert_eq!(uses.len(), 2, "{uses:?}");
    assert!(uses.iter().all(|l| l.uri == main_uri));
}

#[tokio::test]
async fn rename_to_a_reserved_word_is_rejected() {
    use tower_lsp_server::ls_types::RenameParams;
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("reserved.wcl");
    let src = "@schemaless base = 2\n@schemaless doubled = base * 2\n";
    open(backend, &uri, src).await;
    for name in ["match", "none", "2x"] {
        let result = backend
            .rename(RenameParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: position_of(src, "base", 0, 1),
                },
                new_name: name.into(),
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .await;
        let error = result.expect_err(name);
        assert!(
            error.message.contains("not a valid WCL identifier"),
            "{error:?}"
        );
    }
}

#[tokio::test]
async fn semantic_tokens_classify_a_document() {
    use tower_lsp_server::ls_types::{SemanticTokensParams, SemanticTokensResult};
    let svc = service();
    let backend = svc.inner();
    let (_dir, uri) = scratch_uri("tokens.wcl");
    let src = "type Foo {}\n@schemaless x = try 1 catch e { 2 }\n";
    open(backend, &uri, src).await;
    let Some(SemanticTokensResult::Tokens(tokens)) = backend
        .semantic_tokens_full(SemanticTokensParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("semantic tokens")
    else {
        panic!("expected full tokens");
    };
    // Decode to (text, legend type) pairs.
    let legend = [
        "keyword",
        "string",
        "number",
        "operator",
        "decorator",
        "type",
        "variable",
        "enumMember",
    ];
    let (mut line, mut col) = (0u32, 0u32);
    let decoded: Vec<(String, &str)> = tokens
        .data
        .iter()
        .map(|t| {
            if t.delta_line > 0 {
                line += t.delta_line;
                col = 0;
            }
            col += t.delta_start;
            let text = src.lines().nth(line as usize).unwrap();
            let word = text[col as usize..(col + t.length) as usize].to_string();
            (word, legend[t.token_type as usize])
        })
        .collect();
    for expected in [
        ("type", "keyword"),
        ("Foo", "variable"),
        ("@", "decorator"),
        ("schemaless", "type"),
        ("try", "keyword"),
        ("catch", "keyword"),
        ("1", "number"),
    ] {
        assert!(
            decoded.contains(&(expected.0.to_string(), expected.1)),
            "{expected:?} in {decoded:?}"
        );
    }
}
