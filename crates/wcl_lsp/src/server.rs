//! `tower_lsp` server implementation: document store + request
//! handlers. Each handler is a thin shim over the helpers in
//! [`diagnostics`](crate::diagnostics), [`symbols`](crate::symbols),
//! and `wcl_lang::format`.

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    InitializedParams, Location, MessageType, OneOf, PositionEncodingKind, ReferenceParams,
    SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
    Url,
};
use tower_lsp::{Client, LanguageServer};
use wcl_lang::{format as wcl_format, parse_for_edit};

use crate::completion;
use crate::convert::{full_document_range, position_to_offset};
use crate::diagnostics;
use crate::hover as hover_impl;
use crate::navigation;
use crate::semtokens;
use crate::symbols;

/// The LSP backend. Holds the open-document cache; everything else is
/// computed on demand from `wcl_lang`.
pub struct Backend {
    client: Client,
    docs: DashMap<Url, String>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
        }
    }

    /// Recompute diagnostics for `uri` from the cached source and
    /// publish them.
    async fn publish(&self, uri: Url, version: Option<i32>) {
        let source = match self.docs.get(&uri) {
            Some(s) => s.clone(),
            None => return,
        };
        let diags = diagnostics::compute(&source, uri.as_str());
        self.client.publish_diagnostics(uri, diags, version).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> RpcResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(PositionEncodingKind::UTF8),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["@".into(), ":".into(), "&".into()]),
                    ..Default::default()
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: semtokens::LEGEND.to_vec(),
                                token_modifiers: Vec::new(),
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "wcl-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "wcl-lsp ready")
            .await;
    }

    async fn shutdown(&self) -> RpcResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.docs.insert(uri.clone(), params.text_document.text);
        self.publish(uri, Some(params.text_document.version)).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        // FULL sync: take the last (and only) change event's text.
        let Some(change) = params.content_changes.pop() else {
            return;
        };
        let uri = params.text_document.uri.clone();
        self.docs.insert(uri.clone(), change.text);
        self.publish(uri, Some(params.text_document.version)).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.remove(&params.text_document.uri);
        // Clear any stale diagnostics in the client.
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> RpcResult<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let Some(source) = self.docs.get(&uri).map(|s| s.clone()) else {
            return Ok(None);
        };
        let Ok(ast) = parse_for_edit(&source, uri.as_str()) else {
            // Parse failed — diagnostics already surface the error.
            return Ok(None);
        };
        let formatted = wcl_format::to_source(&ast);
        if formatted == source {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![TextEdit {
            range: full_document_range(&source),
            new_text: formatted,
        }]))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> RpcResult<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let Some(source) = self.docs.get(&uri).map(|s| s.clone()) else {
            return Ok(None);
        };
        let syms = symbols::compute(&source, uri.as_str());
        Ok(Some(DocumentSymbolResponse::Nested(syms)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> RpcResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(source) = self.docs.get(&uri).map(|s| s.clone()) else {
            return Ok(None);
        };
        let offset = position_to_offset(&source, params.text_document_position_params.position);
        Ok(navigation::goto_definition(uri, &source, offset))
    }

    async fn references(&self, params: ReferenceParams) -> RpcResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let Some(source) = self.docs.get(&uri).map(|s| s.clone()) else {
            return Ok(None);
        };
        let offset = position_to_offset(&source, params.text_document_position.position);
        Ok(navigation::references(
            uri,
            &source,
            offset,
            params.context.include_declaration,
        ))
    }

    async fn hover(&self, params: HoverParams) -> RpcResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(source) = self.docs.get(&uri).map(|s| s.clone()) else {
            return Ok(None);
        };
        let offset = position_to_offset(&source, params.text_document_position_params.position);
        Ok(hover_impl::hover(&source, uri.as_str(), offset))
    }

    async fn completion(&self, params: CompletionParams) -> RpcResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let Some(source) = self.docs.get(&uri).map(|s| s.clone()) else {
            return Ok(None);
        };
        let offset = position_to_offset(&source, params.text_document_position.position);
        let items = completion::completions(&source, uri.as_str(), offset);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> RpcResult<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let Some(source) = self.docs.get(&uri).map(|s| s.clone()) else {
            return Ok(None);
        };
        let data = semtokens::compute(&source);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }
}
