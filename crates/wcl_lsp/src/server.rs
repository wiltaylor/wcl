use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::LanguageServer;

use ropey::Rope;

use crate::analysis::analyze;
use crate::convert::lsp_position_to_offset;
use crate::diagnostics::to_lsp_diagnostic;
use crate::state::{DocumentState, WorldState};

pub struct WclLanguageServer {
    pub client: tower_lsp::Client,
    pub state: WorldState,
}

impl WclLanguageServer {
    pub fn new(client: tower_lsp::Client) -> Self {
        WclLanguageServer {
            client,
            state: WorldState::new(),
        }
    }

    async fn analyze_and_publish(&self, uri: Url, source: String, version: i32) {
        let rope = Rope::from_str(&source);
        let analysis = analyze(&source, &self.state.default_options);

        let diagnostics: Vec<Diagnostic> = analysis
            .diagnostics
            .iter()
            .filter_map(|d| to_lsp_diagnostic(d, &rope, &uri))
            .collect();

        let errors = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .count();
        let warnings = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
            .count();
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Analysis of {}: {} error(s), {} warning(s)",
                    uri, errors, warnings
                ),
            )
            .await;

        let doc_state = DocumentState {
            uri: uri.clone(),
            version,
            source,
            rope,
            analysis: Some(analysis),
        };
        self.state.documents.insert(uri.clone(), doc_state);

        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for WclLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..Default::default()
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "@".to_string(),
                        ".".to_string(),
                        "\"".to_string(),
                    ]),
                    ..Default::default()
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: crate::semantic_tokens::legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            ..Default::default()
                        },
                    ),
                ),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                references_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "wcl-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "WCL language server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let source = params.text_document.text;
        let version = params.text_document.version;
        self.client
            .log_message(MessageType::INFO, format!("Document opened: {}", uri))
            .await;
        self.analyze_and_publish(uri, source, version).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        self.client
            .log_message(
                MessageType::INFO,
                format!("Document changed: {} (v{})", uri, version),
            )
            .await;
        if let Some(change) = params.content_changes.into_iter().last() {
            self.analyze_and_publish(uri, change.text, version).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.client
            .log_message(MessageType::INFO, format!("Document closed: {}", uri))
            .await;
        self.state.documents.remove(&uri);
        self.client
            .publish_diagnostics(uri, Vec::new(), None)
            .await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        self.client
            .log_message(MessageType::INFO, format!("Document saved: {}", uri))
            .await;
        // Re-analyze if the editor sent the full text on save.
        if let Some(text) = params.text {
            // Use version 0 since DidSave does not carry a version.
            self.analyze_and_publish(uri, text, 0).await;
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let Some(doc) = self.state.documents.get(uri) else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("hover: document not found: {}", uri),
                )
                .await;
            return Ok(None);
        };

        let offset = lsp_position_to_offset(pos, &doc.rope);
        let result = doc
            .analysis
            .as_ref()
            .and_then(|a| crate::hover::hover(a, offset, &doc.rope));

        Ok(result)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let Some(doc) = self.state.documents.get(uri) else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("goto_definition: document not found: {}", uri),
                )
                .await;
            return Ok(None);
        };

        let offset = lsp_position_to_offset(pos, &doc.rope);
        let result = doc
            .analysis
            .as_ref()
            .and_then(|a| crate::definition::goto_definition(a, offset, &doc.rope, uri));

        Ok(result)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;

        let Some(doc) = self.state.documents.get(uri) else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("document_symbol: document not found: {}", uri),
                )
                .await;
            return Ok(None);
        };

        let result = doc.analysis.as_ref().map(|a| {
            DocumentSymbolResponse::Nested(crate::symbols::document_symbols(&a.ast, &doc.rope))
        });

        Ok(result)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let Some(doc) = self.state.documents.get(uri) else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("completion: document not found: {}", uri),
                )
                .await;
            return Ok(None);
        };

        let offset = lsp_position_to_offset(pos, &doc.rope);
        let result = doc.analysis.as_ref().map(|a| {
            CompletionResponse::Array(crate::completion::completions(a, &doc.source, offset))
        });

        Ok(result)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;

        let Some(doc) = self.state.documents.get(uri) else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("semantic_tokens_full: document not found: {}", uri),
                )
                .await;
            return Ok(None);
        };

        let result = doc.analysis.as_ref().map(|a| {
            let tokens = crate::semantic_tokens::compute_semantic_tokens(
                &a.tokens,
                &doc.rope,
                Some(&a.ast),
            );
            SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            })
        });

        Ok(result)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let Some(doc) = self.state.documents.get(uri) else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("signature_help: document not found: {}", uri),
                )
                .await;
            return Ok(None);
        };

        let offset = lsp_position_to_offset(pos, &doc.rope);
        let result =
            crate::signature_help::signature_help(&doc.source, offset, doc.analysis.as_ref());

        Ok(result)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let Some(doc) = self.state.documents.get(uri) else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("references: document not found: {}", uri),
                )
                .await;
            return Ok(None);
        };

        let offset = lsp_position_to_offset(pos, &doc.rope);
        let result = doc
            .analysis
            .as_ref()
            .map(|a| {
                crate::references::find_references(a, offset, &doc.rope, uri, include_declaration)
            })
            .unwrap_or_default();

        Ok(Some(result))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;

        let Some(doc) = self.state.documents.get(uri) else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("formatting: document not found: {}", uri),
                )
                .await;
            return Ok(None);
        };

        let result = crate::formatting::format_document(&doc.source);

        Ok(result)
    }
}
