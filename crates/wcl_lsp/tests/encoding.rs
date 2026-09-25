//! Position-encoding negotiation and non-ASCII positions, driven through
//! the `LanguageServer` trait for both the UTF-16 default and UTF-8.

use tower_lsp_server::LanguageServer;
use tower_lsp_server::LspService;
use tower_lsp_server::ls_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, FormattingOptions, GeneralClientCapabilities, HoverParams,
    InitializeParams, PartialResultParams, Position, PositionEncodingKind, Range,
    SemanticTokensParams, SemanticTokensResult, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Uri,
    VersionedTextDocumentIdentifier, WorkDoneProgressParams,
};
use wcl_lsp::{Backend, Host};

/// The wdoc host `wcl lsp` runs with, so wdoc documents open as they
/// would in the editor.
fn wdoc_host() -> Host {
    Host::new(wcl_wdoc::wdoc_environment(), wcl_wdoc::schema_registry())
}

/// A backend initialised with a client offering `offered` position
/// encodings (`None` omits the capability, as VS Code does).
async fn initialized(offered: Option<Vec<PositionEncodingKind>>) -> LspService<Backend> {
    let (service, _socket) = LspService::new(|client| Backend::new(client, wdoc_host()));
    let response = service
        .inner()
        .initialize(InitializeParams {
            capabilities: ClientCapabilities {
                general: Some(GeneralClientCapabilities {
                    position_encodings: offered,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
        .await
        .expect("initialize");
    assert!(response.capabilities.position_encoding.is_some());
    service
}

async fn open(backend: &Backend, uri: &Uri, text: &str) {
    backend
        .did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "wcl".into(),
                version: 1,
                text: text.into(),
            },
        })
        .await;
}

async fn replace(backend: &Backend, uri: &Uri, range: Range, text: &str) {
    backend
        .did_change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(range),
                range_length: None,
                text: text.into(),
            }],
        })
        .await;
}

/// The server's view of the buffer, read back through a formatting edit
/// (the source used here is never already canonical).
async fn formatted(backend: &Backend, uri: &Uri) -> String {
    backend
        .formatting(DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            options: FormattingOptions::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("formatting")
        .and_then(|edits| edits.into_iter().next())
        .expect("an edit")
        .new_text
}

/// Column of `needle` on its line, counted in UTF-16 code units and in
/// UTF-8 bytes.
fn columns(line: &str, needle: &str) -> (u32, u32) {
    let byte = line.find(needle).expect("needle on line");
    (line[..byte].encode_utf16().count() as u32, byte as u32)
}

#[tokio::test]
async fn initialize_negotiates_the_position_encoding() {
    let cases = [
        (None, PositionEncodingKind::UTF16),
        (
            Some(vec![PositionEncodingKind::UTF16]),
            PositionEncodingKind::UTF16,
        ),
        (
            Some(vec![
                PositionEncodingKind::UTF16,
                PositionEncodingKind::UTF8,
            ]),
            PositionEncodingKind::UTF8,
        ),
    ];
    for (offered, expected) in cases {
        let (service, _socket) = LspService::new(|client| Backend::new(client, wdoc_host()));
        let response = service
            .inner()
            .initialize(InitializeParams {
                capabilities: ClientCapabilities {
                    general: Some(GeneralClientCapabilities {
                        position_encodings: offered.clone(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            })
            .await
            .expect("initialize");
        assert_eq!(
            response.capabilities.position_encoding,
            Some(expected),
            "offered {offered:?}"
        );
    }
}

#[tokio::test]
async fn incremental_edit_after_multibyte_characters() {
    let line = "@schemaless  s =  \"é😀—\"  + \"x\"";
    for (offered, utf8) in [
        (None, false),
        (Some(vec![PositionEncodingKind::UTF8]), true),
    ] {
        let service = initialized(offered).await;
        let backend = service.inner();
        let uri = "file:///enc.wcl".parse::<Uri>().unwrap();
        open(backend, &uri, &format!("{line}\n")).await;
        let (utf16_col, utf8_col) = columns(line, "x\"");
        let col = if utf8 { utf8_col } else { utf16_col };
        replace(
            backend,
            &uri,
            Range::new(Position::new(0, col), Position::new(0, col + 1)),
            "yz",
        )
        .await;
        let text = formatted(backend, &uri).await;
        assert!(text.contains("\"é😀—\" + \"yz\""), "utf8={utf8}: {text:?}");
    }
}

#[tokio::test]
async fn a_mid_character_edit_snaps_instead_of_panicking() {
    let service = initialized(None).await;
    let backend = service.inner();
    let uri = "file:///snap.wcl".parse::<Uri>().unwrap();
    open(backend, &uri, "@schemaless  s = \"😀\"\n").await;
    // UTF-16 column 19 is between the two halves of the surrogate pair.
    replace(
        backend,
        &uri,
        Range::new(Position::new(0, 19), Position::new(0, 19)),
        "a",
    )
    .await;
    assert!(formatted(backend, &uri).await.contains("\"a😀\""));
}

#[tokio::test]
async fn hover_range_counts_in_the_negotiated_unit() {
    let line = "s = \"é😀—\" config {";
    let src = format!(
        "@document\ntype Root {{\n  s: utf8\n}}\n@block(\"config\")\ntype Config {{\n  region: utf8\n}}\n{line}\n  region = \"x\"\n}}\n"
    );
    for (offered, utf8) in [
        (None, false),
        (Some(vec![PositionEncodingKind::UTF8]), true),
    ] {
        let service = initialized(offered).await;
        let backend = service.inner();
        let uri = "file:///hover.wcl".parse::<Uri>().unwrap();
        open(backend, &uri, &src).await;
        let (utf16_col, utf8_col) = columns(line, "config");
        let col = if utf8 { utf8_col } else { utf16_col };
        let hover = backend
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: Position::new(8, col + 2),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .await
            .expect("hover")
            .expect("hover on the block kind");
        let range = hover.range.expect("hover range");
        assert_eq!(range.start, Position::new(8, col), "utf8={utf8}");
        assert_eq!(range.end, Position::new(8, col + 6), "utf8={utf8}");
    }
}

#[tokio::test]
async fn semantic_tokens_count_in_the_negotiated_unit() {
    let line = "@schemaless s = \"é😀—\" + x";
    for (offered, utf8) in [
        (None, false),
        (Some(vec![PositionEncodingKind::UTF8]), true),
    ] {
        let service = initialized(offered).await;
        let backend = service.inner();
        let uri = "file:///sem.wcl".parse::<Uri>().unwrap();
        open(backend, &uri, &format!("{line}\n")).await;
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
        // Absolute (start, length) of each token on the line.
        let mut col = 0;
        let absolute: Vec<(u32, u32)> = tokens
            .data
            .iter()
            .map(|t| {
                col += t.delta_start;
                (col, t.length)
            })
            .collect();
        let string = line.find('"').unwrap();
        let string_text = "\"é😀—\"";
        let x = line.rfind('x').unwrap();
        let (string_col, string_len, x_col) = if utf8 {
            (string as u32, string_text.len() as u32, x as u32)
        } else {
            (
                line[..string].encode_utf16().count() as u32,
                string_text.encode_utf16().count() as u32,
                line[..x].encode_utf16().count() as u32,
            )
        };
        assert!(
            absolute.contains(&(string_col, string_len)),
            "utf8={utf8}: {absolute:?}"
        );
        assert!(absolute.contains(&(x_col, 1)), "utf8={utf8}: {absolute:?}");
    }
}
