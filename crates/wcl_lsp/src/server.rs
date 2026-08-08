//! `tower_lsp` server implementation: document store + request
//! handlers. Each handler is a thin shim over the helpers in
//! [`diagnostics`](crate::diagnostics), [`symbols`](crate::symbols),
//! and `wcl_lang::format`.
//!
//! ## Root document
//!
//! On `initialize`, the server looks for a *root document* that
//! anchors cross-file resolution. The candidates, in order, are:
//!
//!   1. `initializationOptions.root` (a path string), interpreted
//!      against the first workspace folder.
//!   2. `<workspace>/main.wcl` if it exists.
//!
//! When the root is found, every relevant handler parses *the root*
//! (with open editor buffers overlaid on disk) instead of the
//! per-URI snapshot, so imports, cross-file types and symbols are
//! all visible everywhere. When no root is found, the server falls
//! back to per-file parsing — every standalone `.wcl` file still
//! works.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CompletionOptions,
    CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, FoldingRange, FoldingRangeParams, FoldingRangeProviderCapability,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, InitializedParams, Location, MessageType, OneOf, Position,
    PositionEncodingKind, ReferenceParams, RenameParams, SaveOptions, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensResult, SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo,
    SignatureHelp, SignatureHelpOptions, SignatureHelpParams, SymbolInformation,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TextEdit, Url, WorkspaceEdit, WorkspaceSymbolParams,
};
use tower_lsp::{Client, LanguageServer};
use wcl_lang::{
    Document, Environment, FileLoader, format as wcl_format, overlay_loader, parse_for_edit,
};

use crate::code_actions;
use crate::completion;
use crate::convert::{full_document_range, position_to_offset};
use crate::diagnostics;
use crate::folding;
use crate::hover as hover_impl;
use crate::navigation;
use crate::semtokens;
use crate::signature;
use crate::symbols;
use crate::workspace;

/// Environment for a root-document parse: the wdoc one, so `@contextual`
/// block kinds (`wdoc_repeater`, component instances) expand through
/// wdoc's expander and its builtins (`page_metadata`, `__wdoc_slot`)
/// resolve. A bare `Environment::new()` would make every projection over a
/// repeater a hard error and paint valid documents red. Mirrors what
/// [`crate::diagnostics`] opens with.
fn root_environment() -> Environment {
    wcl_wdoc::wdoc_environment()
}

/// The LSP backend. Holds the open-document cache (a rope per URI,
/// kept in sync via incremental change events) and an optional root
/// document path resolved during `initialize`; everything else is
/// computed on demand from `wcl_lang`.
pub struct Backend {
    client: Client,
    docs: DashMap<Url, Rope>,
    /// Path to the root document, when one was discovered or
    /// configured. All open files are validated against this root
    /// (with their unsaved buffers overlaid) so cross-file imports
    /// resolve.
    root_path: RwLock<Option<PathBuf>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
            root_path: RwLock::new(None),
        }
    }

    /// Materialise the current text for a URI. Returns `None` when
    /// the document hasn't been opened by the client yet.
    pub(crate) fn document_text(&self, uri: &Url) -> Option<String> {
        self.docs.get(uri).map(|r| r.to_string())
    }

    /// The buffer text plus the byte offset of `pos` within it — the
    /// shared preamble for the position-bearing request handlers
    /// (definition / references / hover / completion). `None` when the
    /// document isn't open.
    fn source_and_offset(&self, uri: &Url, pos: Position) -> Option<(String, usize)> {
        let source = self.document_text(uri)?;
        let offset = position_to_offset(&source, pos);
        Some((source, offset))
    }

    /// Snapshot of every open buffer as `path → text`. Used to build
    /// an overlay [`FileLoader`] so root-document parses see unsaved
    /// edits. URIs that don't map to a filesystem path are silently
    /// skipped.
    pub(crate) fn overlay_snapshot(&self) -> HashMap<PathBuf, String> {
        let mut out = HashMap::new();
        for entry in self.docs.iter() {
            if let Ok(p) = entry.key().to_file_path() {
                out.insert(p, entry.value().to_string());
            }
        }
        out
    }

    /// Build a [`FileLoader`] that serves the embedded wdoc standard
    /// library for `import <wdoc.wcl>` (and the other `<wdoc/…>` system
    /// imports), falling through to an overlay of every open buffer on
    /// top of disk. Each call snapshots `docs`; long-running consumers
    /// should rebuild between operations. Rebuilding the registry per
    /// call is cheap — it registers `&'static` strings.
    pub(crate) fn loader(&self) -> FileLoader {
        wcl_wdoc::schema_registry().loader(overlay_loader(self.overlay_snapshot()))
    }

    /// Canonical path of the configured root document, if any. A
    /// poisoned lock is recovered (the guarded `Option<PathBuf>` can't
    /// be left torn) and logged — silently degrading to per-file mode
    /// would break cross-file resolution with no indication why.
    pub fn root_path(&self) -> Option<PathBuf> {
        match self.root_path.read() {
            Ok(g) => g.clone(),
            Err(e) => {
                tracing::error!("root_path lock poisoned; recovering: {e}");
                e.into_inner().clone()
            }
        }
    }

    /// Parse the root document (if configured) with the current
    /// overlay applied. Returns `None` when no root is configured or
    /// the root failed to parse — callers fall back to per-file
    /// parsing in that case.
    pub fn root_document(&self) -> Option<Document> {
        let path = self.root_path()?;
        Document::from_file_with_loader(&path, &root_environment(), self.loader()).ok()
    }

    /// Recompute diagnostics for `uri` from the cached rope and
    /// publish them. Diagnostics are currently per-file: cross-file
    /// errors surfaced by the root document are not attributed back
    /// to the originating file (that needs source-path tagging on
    /// `EvalError`, a separate change).
    ///
    /// When a root document is configured and `uri` is some *other*
    /// file, only syntax errors are published: schema validation of a
    /// fragment in isolation reports false positives for everything the
    /// root supplies (imported `@block` declarations, document schemas,
    /// referenced data), which would paint valid files red.
    async fn publish(&self, uri: Url, version: Option<i32>) {
        let Some(source) = self.document_text(&uri) else {
            return;
        };
        let is_non_root_fragment = match (self.root_path(), uri.to_file_path()) {
            (Some(root), Ok(path)) => std::fs::canonicalize(&path)
                .map(|c| c != root)
                .unwrap_or(true),
            (Some(_), Err(())) => true,
            (None, _) => false,
        };
        let base_dir = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
        let diags = if is_non_root_fragment {
            diagnostics::compute_syntax_only(
                &source,
                uri.as_str(),
                base_dir.as_deref(),
                self.loader(),
            )
        } else {
            diagnostics::compute(&source, uri.as_str(), base_dir.as_deref(), self.loader())
        };
        self.client.publish_diagnostics(uri, diags, version).await;
    }

    /// Resolve the root document path from `initialize` parameters.
    /// Falls back to `<first-workspace-folder>/main.wcl` when no
    /// `initializationOptions.root` is supplied. Returns `None` if
    /// neither path yields an existing file on disk.
    fn resolve_root(params: &InitializeParams) -> Option<PathBuf> {
        let workspace_dir = params
            .workspace_folders
            .as_ref()
            .and_then(|v| v.first())
            .and_then(|f| f.uri.to_file_path().ok())
            .or_else(|| {
                #[allow(deprecated)]
                params.root_uri.as_ref().and_then(|u| u.to_file_path().ok())
            });
        if let Some(opts) = params.initialization_options.as_ref()
            && let Some(root) = opts.get("root").and_then(|v| v.as_str())
        {
            let candidate = std::path::PathBuf::from(root);
            let resolved = if candidate.is_absolute() {
                candidate
            } else {
                workspace_dir
                    .as_ref()
                    .map(|d| d.join(&candidate))
                    .unwrap_or(candidate)
            };
            if resolved.is_file() {
                return std::fs::canonicalize(&resolved).ok();
            }
        }
        if let Some(dir) = workspace_dir {
            let main = dir.join("main.wcl");
            if main.is_file() {
                return std::fs::canonicalize(&main).ok();
            }
        }
        None
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> RpcResult<InitializeResult> {
        if let Some(p) = Backend::resolve_root(&params) {
            match self.root_path.write() {
                Ok(mut guard) => *guard = Some(p),
                Err(e) => {
                    tracing::error!("root_path lock poisoned; recovering: {e}");
                    *e.into_inner() = Some(p);
                }
            }
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(PositionEncodingKind::UTF8),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(false),
                        })),
                        ..Default::default()
                    },
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
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                rename_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    retrigger_characters: Some(vec![",".into()]),
                    ..Default::default()
                }),
                // Inlay hints are deliberately not advertised: type hints
                // would need expression-level inference wcl_lang doesn't
                // expose, and parameter-name hints are covered by
                // signature help + hover for a config language's short,
                // mostly-literal expressions.
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
        let msg = match self.root_path() {
            Some(p) => format!("wcl-lsp ready (root: {})", p.display()),
            None => "wcl-lsp ready (no root document; per-file mode)".to_string(),
        };
        self.client.log_message(MessageType::INFO, msg).await;
    }

    async fn shutdown(&self) -> RpcResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.docs
            .insert(uri.clone(), Rope::from_str(&params.text_document.text));
        self.publish(uri, Some(params.text_document.version)).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        // Apply every change event in order. With INCREMENTAL sync
        // the client sends one or more ranged edits per request;
        // when `range` is None it's a full-document replacement
        // (clients may still send those for large diffs).
        let mut rope = self.docs.entry(uri.clone()).or_insert_with(Rope::new);
        for change in params.content_changes {
            match change.range {
                Some(range) => {
                    let text = rope.to_string();
                    let start = crate::convert::position_to_offset(&text, range.start);
                    let end = crate::convert::position_to_offset(&text, range.end);
                    let start_char = rope.byte_to_char(start);
                    let end_char = rope.byte_to_char(end);
                    rope.remove(start_char..end_char);
                    rope.insert(start_char, &change.text);
                }
                None => {
                    *rope = Rope::from_str(&change.text);
                }
            }
        }
        drop(rope);
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
        let Some(source) = self.document_text(&uri) else {
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
        let Some(source) = self.document_text(&uri) else {
            return Ok(None);
        };
        let syms = symbols::compute(&source, uri.as_str());
        Ok(Some(DocumentSymbolResponse::Nested(syms)))
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> RpcResult<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;
        let Some(source) = self.document_text(&uri) else {
            return Ok(None);
        };
        Ok(Some(folding::compute(&source, uri.as_str())))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> RpcResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some((source, offset)) =
            self.source_and_offset(&uri, params.text_document_position_params.position)
        else {
            return Ok(None);
        };
        let root_doc = self.root_document();
        let root_path = self.root_path();
        Ok(navigation::goto_definition(
            uri,
            &source,
            offset,
            root_doc.as_ref(),
            root_path.as_deref(),
        ))
    }

    async fn references(&self, params: ReferenceParams) -> RpcResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let Some((source, offset)) =
            self.source_and_offset(&uri, params.text_document_position.position)
        else {
            return Ok(None);
        };
        let root_doc = self.root_document();
        let root_path = self.root_path();
        Ok(navigation::references(
            uri,
            &source,
            offset,
            params.context.include_declaration,
            root_doc.as_ref(),
            root_path.as_deref(),
        ))
    }

    async fn rename(&self, params: RenameParams) -> RpcResult<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let Some((source, offset)) =
            self.source_and_offset(&uri, params.text_document_position.position)
        else {
            return Ok(None);
        };
        let root_doc = self.root_document();
        let root_path = self.root_path();
        navigation::rename(
            uri,
            &source,
            offset,
            &params.new_name,
            root_doc.as_ref(),
            root_path.as_deref(),
        )
        .map_err(tower_lsp::jsonrpc::Error::invalid_params)
    }

    async fn hover(&self, params: HoverParams) -> RpcResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some((source, offset)) =
            self.source_and_offset(&uri, params.text_document_position_params.position)
        else {
            return Ok(None);
        };
        let root_doc = self.root_document();
        Ok(hover_impl::hover(
            &source,
            uri.as_str(),
            offset,
            root_doc.as_ref(),
        ))
    }

    async fn completion(&self, params: CompletionParams) -> RpcResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let Some((source, offset)) =
            self.source_and_offset(&uri, params.text_document_position.position)
        else {
            return Ok(None);
        };
        let root_doc = self.root_document();
        let items = completion::completions(&source, uri.as_str(), offset, root_doc.as_ref());
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> RpcResult<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some((source, offset)) =
            self.source_and_offset(&uri, params.text_document_position_params.position)
        else {
            return Ok(None);
        };
        // The buffer is usually mid-call (that's why help fired) and the
        // overlay carries that unparseable text, which would fail the
        // root parse and lose cross-file resolution. Retry the root with
        // the buffer's *repaired* form (open brackets closed) overlaid.
        let root_doc = self.root_document().or_else(|| {
            let root = self.root_path()?;
            let path = uri.to_file_path().ok()?;
            let mut overlay = self.overlay_snapshot();
            overlay.insert(path, signature::repair_source(&source, offset));
            let loader = wcl_wdoc::schema_registry().loader(overlay_loader(overlay));
            Document::from_file_with_loader(&root, &root_environment(), loader).ok()
        });
        Ok(signature::signature_help(
            &source,
            uri.as_str(),
            offset,
            root_doc.as_ref(),
            &self.overlay_snapshot(),
        ))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> RpcResult<Option<Vec<SymbolInformation>>> {
        let root_doc = self.root_document();
        let root_path = self.root_path();
        Ok(Some(workspace::workspace_symbols(
            &params.query,
            root_doc.as_ref(),
            root_path.as_deref(),
            &self.overlay_snapshot(),
        )))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> RpcResult<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let Some(source) = self.document_text(&uri) else {
            return Ok(None);
        };
        let data = semtokens::compute(&source);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn code_action(&self, params: CodeActionParams) -> RpcResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let Some(source) = self.document_text(&uri) else {
            return Ok(None);
        };
        Ok(code_actions::compute(
            &uri,
            &source,
            &params.context.diagnostics,
        ))
    }
}
