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

#[tokio::test]
async fn folding_ranges_cover_blocks_and_type_decls() {
    use tower_lsp::lsp_types::FoldingRangeParams;
    let svc = service();
    let backend = svc.inner();
    let uri = Url::parse("file:///fold.wcl").unwrap();
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
    use tower_lsp::lsp_types::RenameParams;
    let svc = service();
    let backend = svc.inner();
    let uri = Url::parse("file:///rn.wcl").unwrap();
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
async fn rename_rejects_an_invalid_identifier() {
    use tower_lsp::lsp_types::RenameParams;
    let svc = service();
    let backend = svc.inner();
    let uri = Url::parse("file:///rn2.wcl").unwrap();
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
    use tower_lsp::lsp_types::RenameParams;
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

    let main_uri = Url::from_file_path(&main).unwrap();
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
    let shared_uri = Url::from_file_path(&shared).unwrap();
    let shared_edits = changes
        .get(&shared_uri)
        .unwrap_or_else(|| panic!("declaration file edited: {changes:?}"));
    assert!(shared_edits.iter().all(|e| e.new_text == "Hue"));
}

#[tokio::test]
async fn signature_help_for_builtin_after_open_paren() {
    let svc = service();
    let backend = svc.inner();
    let uri = Url::parse("file:///sig.wcl").unwrap();
    let src = "@schemaless x = len(";
    open(backend, &uri, src).await;
    let help = backend
        .signature_help(tower_lsp::lsp_types::SignatureHelpParams {
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
    let uri = Url::parse("file:///sig2.wcl").unwrap();
    let src = "fn add(a: i64, b: i64) -> i64 { a + b }\n@schemaless x = add(1, ";
    open(backend, &uri, src).await;
    let last_line = src.lines().count() as u32 - 1;
    let character = src.lines().last().unwrap().len() as u32;
    let help = backend
        .signature_help(tower_lsp::lsp_types::SignatureHelpParams {
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

    let main_uri = Url::from_file_path(&main).unwrap();
    let text = std::fs::read_to_string(&main).unwrap();
    open(backend, &main_uri, &text).await;
    let last_line = text.lines().count() as u32 - 1;
    let character = text.lines().last().unwrap().len() as u32;
    let help = backend
        .signature_help(tower_lsp::lsp_types::SignatureHelpParams {
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
        .symbol(tower_lsp::lsp_types::WorkspaceSymbolParams {
            query: "Col".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("workspace/symbol rpc")
        .expect("some hits");
    let shared_uri = Url::from_file_path(&shared).unwrap();
    let color = hits
        .iter()
        .find(|s| s.name == "Color")
        .expect("Color found across the graph");
    assert_eq!(color.location.uri, shared_uri);
    assert_eq!(color.kind, tower_lsp::lsp_types::SymbolKind::CLASS);

    // The empty query lists symbols from both files.
    let all = backend
        .symbol(tower_lsp::lsp_types::WorkspaceSymbolParams {
            query: String::new(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("workspace/symbol rpc")
        .expect("some hits");
    assert!(all.iter().any(|s| s.name == "Local"));
    assert!(all.iter().any(|s| s.name == "Color"));

    // Subsequence matching: "clr" still finds Color.
    let fuzzy = backend
        .symbol(tower_lsp::lsp_types::WorkspaceSymbolParams {
            query: "clr".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("workspace/symbol rpc")
        .expect("some hits");
    assert!(fuzzy.iter().any(|s| s.name == "Color"), "{fuzzy:?}");
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
    let shared_uri = Url::from_file_path(&shared).unwrap();
    open(
        backend,
        &shared_uri,
        "namespace shared\ntype Color { name: utf8 }\n",
    )
    .await;

    let hits = backend
        .symbol(tower_lsp::lsp_types::WorkspaceSymbolParams {
            query: "Color".into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await
        .expect("workspace/symbol rpc")
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
