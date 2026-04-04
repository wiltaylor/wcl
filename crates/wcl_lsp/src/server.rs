use std::ops::ControlFlow;

use async_lsp::lsp_types::notification;
use async_lsp::lsp_types::*;
use async_lsp::router::Router;
use async_lsp::{ClientSocket, LanguageServer, ResponseError};
use futures::future::BoxFuture;
use ropey::Rope;

use crate::analysis::analyze;
use crate::convert::lsp_position_to_offset;
use crate::diagnostics::to_lsp_diagnostic;
use crate::state::{DocumentState, WorldState};

pub struct WclLanguageServer {
    pub client: ClientSocket,
    pub state: WorldState,
}

impl WclLanguageServer {
    pub fn new_router(client: ClientSocket) -> Router<Self> {
        Router::from_language_server(Self {
            client,
            state: WorldState::new(),
        })
    }

    fn analyze_and_publish(&mut self, uri: Url, source: String, version: i32) {
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
        let _ = self
            .client
            .notify::<notification::LogMessage>(LogMessageParams {
                typ: MessageType::INFO,
                message: format!(
                    "Analysis of {}: {} error(s), {} warning(s)",
                    uri, errors, warnings
                ),
            });

        let doc_state = DocumentState {
            uri: uri.clone(),
            version,
            source,
            rope,
            analysis: Some(analysis),
        };
        self.state.documents.insert(uri.clone(), doc_state);

        let _ = self
            .client
            .notify::<notification::PublishDiagnostics>(PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: Some(version),
            });
    }
}

impl LanguageServer for WclLanguageServer {
    type Error = ResponseError;
    type NotifyResult = ControlFlow<async_lsp::Result<()>>;

    fn initialize(
        &mut self,
        _: InitializeParams,
    ) -> BoxFuture<'static, Result<InitializeResult, Self::Error>> {
        Box::pin(async move {
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
                    rename_provider: Some(OneOf::Right(RenameOptions {
                        prepare_provider: Some(true),
                        work_done_progress_options: Default::default(),
                    })),
                    type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
                    document_formatting_provider: Some(OneOf::Left(true)),
                    ..Default::default()
                },
                server_info: Some(ServerInfo {
                    name: "wcl-lsp".to_string(),
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                }),
            })
        })
    }

    fn initialized(&mut self, _: InitializedParams) -> ControlFlow<async_lsp::Result<()>> {
        let _ = self
            .client
            .notify::<notification::LogMessage>(LogMessageParams {
                typ: MessageType::INFO,
                message: "WCL language server initialized".to_string(),
            });
        ControlFlow::Continue(())
    }

    fn did_open(
        &mut self,
        params: DidOpenTextDocumentParams,
    ) -> ControlFlow<async_lsp::Result<()>> {
        let uri = params.text_document.uri;
        let source = params.text_document.text;
        let version = params.text_document.version;
        let _ = self
            .client
            .notify::<notification::LogMessage>(LogMessageParams {
                typ: MessageType::INFO,
                message: format!("Document opened: {}", uri),
            });
        self.analyze_and_publish(uri, source, version);
        ControlFlow::Continue(())
    }

    fn did_change(
        &mut self,
        params: DidChangeTextDocumentParams,
    ) -> ControlFlow<async_lsp::Result<()>> {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let _ = self
            .client
            .notify::<notification::LogMessage>(LogMessageParams {
                typ: MessageType::INFO,
                message: format!("Document changed: {} (v{})", uri, version),
            });
        if let Some(change) = params.content_changes.into_iter().last() {
            self.analyze_and_publish(uri, change.text, version);
        }
        ControlFlow::Continue(())
    }

    fn did_close(
        &mut self,
        params: DidCloseTextDocumentParams,
    ) -> ControlFlow<async_lsp::Result<()>> {
        let uri = params.text_document.uri;
        let _ = self
            .client
            .notify::<notification::LogMessage>(LogMessageParams {
                typ: MessageType::INFO,
                message: format!("Document closed: {}", uri),
            });
        self.state.documents.remove(&uri);
        let _ = self
            .client
            .notify::<notification::PublishDiagnostics>(PublishDiagnosticsParams {
                uri,
                diagnostics: Vec::new(),
                version: None,
            });
        ControlFlow::Continue(())
    }

    fn did_save(
        &mut self,
        params: DidSaveTextDocumentParams,
    ) -> ControlFlow<async_lsp::Result<()>> {
        let uri = params.text_document.uri;
        let _ = self
            .client
            .notify::<notification::LogMessage>(LogMessageParams {
                typ: MessageType::INFO,
                message: format!("Document saved: {}", uri),
            });
        if let Some(text) = params.text {
            self.analyze_and_publish(uri, text, 0);
        }
        ControlFlow::Continue(())
    }

    fn hover(
        &mut self,
        params: HoverParams,
    ) -> BoxFuture<'static, Result<Option<Hover>, Self::Error>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let pos = params.text_document_position_params.position;

        let result = match self.state.documents.get(&uri) {
            Some(doc) => {
                let offset = lsp_position_to_offset(pos, &doc.rope);
                doc.analysis
                    .as_ref()
                    .and_then(|a| crate::hover::hover(a, offset, &doc.rope))
            }
            None => {
                let _ = self
                    .client
                    .notify::<notification::LogMessage>(LogMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("hover: document not found: {}", uri),
                    });
                None
            }
        };

        Box::pin(async move { Ok(result) })
    }

    fn definition(
        &mut self,
        params: GotoDefinitionParams,
    ) -> BoxFuture<'static, Result<Option<GotoDefinitionResponse>, Self::Error>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let pos = params.text_document_position_params.position;

        let result = match self.state.documents.get(&uri) {
            Some(doc) => {
                let offset = lsp_position_to_offset(pos, &doc.rope);
                doc.analysis
                    .as_ref()
                    .and_then(|a| crate::definition::goto_definition(a, offset, &doc.rope, &uri))
            }
            None => {
                let _ = self
                    .client
                    .notify::<notification::LogMessage>(LogMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("goto_definition: document not found: {}", uri),
                    });
                None
            }
        };

        Box::pin(async move { Ok(result) })
    }

    fn document_symbol(
        &mut self,
        params: DocumentSymbolParams,
    ) -> BoxFuture<'static, Result<Option<DocumentSymbolResponse>, Self::Error>> {
        let uri = params.text_document.uri.clone();

        let result = match self.state.documents.get(&uri) {
            Some(doc) => doc.analysis.as_ref().map(|a| {
                DocumentSymbolResponse::Nested(crate::symbols::document_symbols(&a.ast, &doc.rope))
            }),
            None => {
                let _ = self
                    .client
                    .notify::<notification::LogMessage>(LogMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("document_symbol: document not found: {}", uri),
                    });
                None
            }
        };

        Box::pin(async move { Ok(result) })
    }

    fn completion(
        &mut self,
        params: CompletionParams,
    ) -> BoxFuture<'static, Result<Option<CompletionResponse>, Self::Error>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;

        let result = match self.state.documents.get(&uri) {
            Some(doc) => {
                let offset = lsp_position_to_offset(pos, &doc.rope);
                doc.analysis.as_ref().map(|a| {
                    CompletionResponse::Array(crate::completion::completions(
                        a,
                        &doc.source,
                        offset,
                    ))
                })
            }
            None => {
                let _ = self
                    .client
                    .notify::<notification::LogMessage>(LogMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("completion: document not found: {}", uri),
                    });
                None
            }
        };

        Box::pin(async move { Ok(result) })
    }

    fn semantic_tokens_full(
        &mut self,
        params: SemanticTokensParams,
    ) -> BoxFuture<'static, Result<Option<SemanticTokensResult>, Self::Error>> {
        let uri = params.text_document.uri.clone();

        let result = match self.state.documents.get(&uri) {
            Some(doc) => doc.analysis.as_ref().map(|a| {
                let tokens = crate::semantic_tokens::compute_semantic_tokens(
                    &a.tokens,
                    &doc.rope,
                    Some(&a.ast),
                );
                SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: tokens,
                })
            }),
            None => {
                let _ = self
                    .client
                    .notify::<notification::LogMessage>(LogMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("semantic_tokens_full: document not found: {}", uri),
                    });
                None
            }
        };

        Box::pin(async move { Ok(result) })
    }

    fn signature_help(
        &mut self,
        params: SignatureHelpParams,
    ) -> BoxFuture<'static, Result<Option<SignatureHelp>, Self::Error>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let pos = params.text_document_position_params.position;

        let result = match self.state.documents.get(&uri) {
            Some(doc) => {
                let offset = lsp_position_to_offset(pos, &doc.rope);
                crate::signature_help::signature_help(&doc.source, offset, doc.analysis.as_ref())
            }
            None => {
                let _ = self
                    .client
                    .notify::<notification::LogMessage>(LogMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("signature_help: document not found: {}", uri),
                    });
                None
            }
        };

        Box::pin(async move { Ok(result) })
    }

    fn references(
        &mut self,
        params: ReferenceParams,
    ) -> BoxFuture<'static, Result<Option<Vec<Location>>, Self::Error>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let result = match self.state.documents.get(&uri) {
            Some(doc) => {
                let offset = lsp_position_to_offset(pos, &doc.rope);
                let refs = doc
                    .analysis
                    .as_ref()
                    .map(|a| {
                        crate::references::find_references(
                            a,
                            offset,
                            &doc.rope,
                            &uri,
                            include_declaration,
                        )
                    })
                    .unwrap_or_default();
                Some(refs)
            }
            None => {
                let _ = self
                    .client
                    .notify::<notification::LogMessage>(LogMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("references: document not found: {}", uri),
                    });
                None
            }
        };

        Box::pin(async move { Ok(result) })
    }

    fn prepare_rename(
        &mut self,
        params: TextDocumentPositionParams,
    ) -> BoxFuture<'static, Result<Option<PrepareRenameResponse>, Self::Error>> {
        let uri = params.text_document.uri.clone();
        let pos = params.position;

        let result = match self.state.documents.get(&uri) {
            Some(doc) => {
                let offset = lsp_position_to_offset(pos, &doc.rope);
                doc.analysis
                    .as_ref()
                    .and_then(|a| crate::rename::prepare_rename(a, offset, &doc.rope))
                    .map(PrepareRenameResponse::Range)
            }
            None => None,
        };

        Box::pin(async move { Ok(result) })
    }

    fn rename(
        &mut self,
        params: RenameParams,
    ) -> BoxFuture<'static, Result<Option<WorkspaceEdit>, Self::Error>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        let new_name = params.new_name;

        let result = match self.state.documents.get(&uri) {
            Some(doc) => {
                let offset = lsp_position_to_offset(pos, &doc.rope);
                doc.analysis
                    .as_ref()
                    .and_then(|a| crate::rename::rename(a, offset, &new_name, &doc.rope, &uri))
            }
            None => None,
        };

        Box::pin(async move { Ok(result) })
    }

    fn type_definition(
        &mut self,
        params: GotoDefinitionParams,
    ) -> BoxFuture<'static, Result<Option<GotoDefinitionResponse>, Self::Error>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let pos = params.text_document_position_params.position;

        let result = match self.state.documents.get(&uri) {
            Some(doc) => {
                let offset = lsp_position_to_offset(pos, &doc.rope);
                doc.analysis.as_ref().and_then(|a| {
                    crate::definition::goto_type_definition(a, offset, &doc.rope, &uri)
                })
            }
            None => None,
        };

        Box::pin(async move { Ok(result) })
    }

    fn formatting(
        &mut self,
        params: DocumentFormattingParams,
    ) -> BoxFuture<'static, Result<Option<Vec<TextEdit>>, Self::Error>> {
        let uri = params.text_document.uri.clone();

        let result = match self.state.documents.get(&uri) {
            Some(doc) => crate::formatting::format_document(&doc.source),
            None => {
                let _ = self
                    .client
                    .notify::<notification::LogMessage>(LogMessageParams {
                        typ: MessageType::WARNING,
                        message: format!("formatting: document not found: {}", uri),
                    });
                None
            }
        };

        Box::pin(async move { Ok(result) })
    }
}
