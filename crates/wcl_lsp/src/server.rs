//! `tower_lsp_server` server implementation: document store + request
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
//!
//! ## Work scheduling
//!
//! Every open, change or close of a buffer (and every change the client
//! reports to a watched file) starts a new *generation*. The snapshot of
//! open buffers and the evaluated root document are cached per
//! generation, so they are built at most once per change however many
//! requests follow it. Handlers run on the blocking pool, off the
//! protocol loop; a request the client cancels is dropped when its
//! result arrives. Diagnostics wait until edits pause for
//! [`DIAGNOSTICS_DEBOUNCE`], and a pass that a newer change overtakes is
//! abandoned rather than published.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};
use std::time::Duration;

use dashmap::DashMap;
use ropey::Rope;
use tokio::task::JoinHandle;
use tower_lsp_server::jsonrpc::{Error as RpcError, Result as RpcResult};
use tower_lsp_server::ls_types::{
    CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CompletionOptions,
    CompletionParams, CompletionResponse, Diagnostic, DidChangeTextDocumentParams,
    DidChangeWatchedFilesParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, FoldingRange,
    FoldingRangeParams, FoldingRangeProviderCapability, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, InitializedParams, Location, MessageType, OneOf, ReferenceParams,
    RenameParams, SaveOptions, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, SignatureHelp,
    SignatureHelpOptions, SignatureHelpParams, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, TextEdit, Uri, WorkspaceEdit,
    WorkspaceSymbolParams, WorkspaceSymbolResponse,
};
use tower_lsp_server::{Client, LanguageServer};
use wcl_lang::{Document, ParseError, format as wcl_format, parse_for_edit};

use crate::code_actions;
use crate::completion;
use crate::convert::{PositionEncoding, rope_char_index, uri_to_path};
use crate::ctx::Ctx;
use crate::diagnostics;
use crate::folding;
use crate::host::Host;
use crate::hover as hover_impl;
use crate::navigation;
use crate::semtokens;
use crate::signature;
use crate::symbols;
use crate::workspace;

/// How long edits must pause before diagnostics are recomputed. Typing
/// produces a change per keystroke; analysing each one would queue work
/// the next keystroke makes stale.
const DIAGNOSTICS_DEBOUNCE: Duration = Duration::from_millis(150);

/// Lock `mutex`, recovering (and logging) a poisoned one: every value
/// guarded here is a cache or a set that a panicking holder cannot leave
/// torn.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| {
        tracing::error!("lock poisoned; recovering: {e}");
        e.into_inner()
    })
}

/// One open editor buffer.
#[derive(Default)]
struct OpenDoc {
    /// The text, kept current from incremental change events.
    rope: Rope,
    /// The client's version of the text, echoed on published diagnostics.
    version: i32,
}

/// The root document as evaluated for one generation. A parse failure
/// is kept too: diagnostics report it, and requests fall back to
/// per-file parsing without reparsing the root.
type RootResult = Result<Arc<Document>, Arc<ParseError>>;

/// Every open buffer with a filesystem path, as `path → text`, and the
/// URI the client opened each one under, as `path → URI`.
type Buffers = (Arc<HashMap<PathBuf, String>>, Arc<HashMap<PathBuf, Uri>>);

/// Diagnostics to publish: each file with the version of its open
/// buffer (`None` for a file that is not open).
type Batch = Vec<(Uri, Option<i32>, Vec<Diagnostic>)>;

/// A value computed for one generation of the open buffers.
struct Cached<T> {
    /// The generation the value was computed for.
    generation: u64,
    /// The value.
    value: T,
}

/// A consistent view for one piece of work: an analysis context over
/// the open buffers of one generation.
struct Snapshot {
    /// The generation the buffers were taken at.
    generation: u64,
    /// Encoding plus the buffer snapshot and its loader.
    ctx: Ctx,
}

/// Everything the handlers share. Lives behind an `Arc` so work can move
/// to the blocking pool.
struct State {
    /// The environment and system imports every document opens with.
    host: Arc<Host>,
    /// Open buffers by URI.
    docs: DashMap<Uri, OpenDoc>,
    /// Path to the root document, when one was discovered or
    /// configured. All open files are validated against this root
    /// (with their unsaved buffers overlaid) so cross-file imports
    /// resolve.
    root_path: RwLock<Option<PathBuf>>,
    /// The workspace folder as the client named it at `initialize`.
    /// Paths under it go back to the client in that spelling, not the
    /// canonical one the import graph uses.
    workspace_dir: OnceLock<PathBuf>,
    /// Unit LSP `character` values count in, fixed by `initialize`.
    encoding: OnceLock<PositionEncoding>,
    /// Bumped after every change to the buffers or the files under them.
    generation: AtomicU64,
    /// Every buffer with a filesystem path, as `path → text`.
    buffers: Mutex<Option<Cached<Buffers>>>,
    /// The evaluated root document.
    root: Mutex<Option<Cached<RootResult>>>,
    /// Every URI the last publish sent diagnostics for, so a file whose
    /// errors are gone — or that left the import graph — is cleared.
    published: Mutex<HashSet<Uri>>,
}

/// The LSP backend. Holds the open-document cache (a rope per URI,
/// kept in sync via incremental change events) and an optional root
/// document path resolved during `initialize`; everything else is
/// computed on demand from `wcl_lang` and cached per generation.
pub struct Backend {
    /// Handle for sending notifications back to the editor.
    client: Client,
    /// State shared with work on the blocking pool.
    state: Arc<State>,
    /// The pending or running diagnostics pass, aborted when a newer
    /// change schedules another.
    diagnostics: Mutex<Option<JoinHandle<()>>>,
}

impl Backend {
    /// Build a backend with no open documents and no root discovered
    /// yet. Every document opens under `host`: [`Host::default`] serves
    /// plain WCL, and a host with a vocabulary of its own passes its
    /// environment and system imports.
    pub fn new(client: Client, host: Host) -> Self {
        Self {
            client,
            state: Arc::new(State {
                host: Arc::new(host),
                docs: DashMap::new(),
                root_path: RwLock::new(None),
                workspace_dir: OnceLock::new(),
                encoding: OnceLock::new(),
                generation: AtomicU64::new(0),
                buffers: Mutex::new(None),
                root: Mutex::new(None),
                published: Mutex::new(HashSet::new()),
            }),
            diagnostics: Mutex::new(None),
        }
    }

    /// Canonical path of the configured root document, if any.
    pub fn root_path(&self) -> Option<PathBuf> {
        self.state.root_path()
    }

    /// The root document (if configured) evaluated with the current
    /// buffers overlaid. `None` when no root is configured or the root
    /// failed to parse — callers fall back to per-file parsing then.
    /// Cached until the next change; call from a context that may block.
    pub fn root_document(&self) -> Option<Arc<Document>> {
        self.state.root_document(&self.state.snapshot())
    }

    /// Run `work` against the shared state on the blocking pool. A
    /// panic in it becomes an internal error for this request alone.
    async fn run<T, F>(&self, work: F) -> RpcResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&State) -> T + Send + 'static,
    {
        let state = Arc::clone(&self.state);
        tokio::task::spawn_blocking(move || work(&state))
            .await
            .map_err(|e| {
                tracing::error!("request failed: {e}");
                RpcError::internal_error()
            })
    }

    /// Record a change to the buffers (or the files under them) and
    /// schedule a diagnostics pass once edits pause.
    fn changed(&self) {
        self.state.generation.fetch_add(1, Ordering::SeqCst);
        let state = Arc::clone(&self.state);
        let client = self.client.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(DIAGNOSTICS_DEBOUNCE).await;
            let generation = state.generation();
            let worker = Arc::clone(&state);
            let batch = tokio::task::spawn_blocking(move || worker.collect_diagnostics(generation));
            let Ok(Some(batch)) = batch.await else {
                return;
            };
            if state.generation() != generation {
                return;
            }
            for (uri, version, diagnostics) in batch {
                client.publish_diagnostics(uri, diagnostics, version).await;
            }
        });
        if let Some(previous) = lock(&self.diagnostics).replace(task) {
            previous.abort();
        }
    }

    /// The first workspace folder, else the deprecated `rootUri`, as a
    /// path in the client's spelling.
    fn workspace_dir(params: &InitializeParams) -> Option<PathBuf> {
        params
            .workspace_folders
            .as_ref()
            .and_then(|v| v.first())
            .and_then(|f| uri_to_path(&f.uri))
            .or_else(|| {
                #[allow(deprecated)]
                params.root_uri.as_ref().and_then(uri_to_path)
            })
    }

    /// Resolve the root document path from `initialize` parameters.
    /// Falls back to `<first-workspace-folder>/main.wcl` when no
    /// `initializationOptions.root` is supplied. Returns `None` if
    /// neither path yields an existing file on disk.
    fn resolve_root(params: &InitializeParams) -> Option<PathBuf> {
        let workspace_dir = Backend::workspace_dir(params);
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
                return crate::ctx::canonical_existing(&resolved).ok();
            }
        }
        if let Some(dir) = workspace_dir {
            let main = dir.join("main.wcl");
            if main.is_file() {
                return crate::ctx::canonical_existing(&main).ok();
            }
        }
        None
    }
}

impl State {
    /// The current generation.
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// The negotiated position encoding — UTF-16, the protocol default,
    /// until `initialize` has run.
    fn encoding(&self) -> PositionEncoding {
        self.encoding.get().copied().unwrap_or_default()
    }

    /// Canonical path of the configured root document, if any. A
    /// poisoned lock is recovered (the guarded `Option<PathBuf>` can't
    /// be left torn) and logged — silently degrading to per-file mode
    /// would break cross-file resolution with no indication why.
    fn root_path(&self) -> Option<PathBuf> {
        match self.root_path.read() {
            Ok(g) => g.clone(),
            Err(e) => {
                tracing::error!("root_path lock poisoned; recovering: {e}");
                e.into_inner().clone()
            }
        }
    }

    /// Materialise the current text for a URI. Returns `None` when
    /// the document hasn't been opened by the client yet.
    fn document_text(&self, uri: &Uri) -> Option<String> {
        self.docs.get(uri).map(|doc| doc.rope.to_string())
    }

    /// An analysis context over the current buffers. The buffer map is
    /// built once per generation and shared by every request in it.
    fn snapshot(&self) -> Snapshot {
        // Read the generation before the buffers: a change landing in
        // between is then at worst cached under the older generation,
        // which the next request rebuilds.
        let generation = self.generation();
        let buffers = {
            let mut cache = lock(&self.buffers);
            match cache.as_ref() {
                Some(cached) if cached.generation == generation => cached.value.clone(),
                _ => {
                    let mut texts = HashMap::new();
                    let mut uris = HashMap::new();
                    for entry in self.docs.iter() {
                        if let Some(path) = uri_to_path(entry.key()) {
                            texts.insert(path.clone(), entry.rope.to_string());
                            uris.insert(path, entry.key().clone());
                        }
                    }
                    let buffers: Buffers = (Arc::new(texts), Arc::new(uris));
                    *cache = Some(Cached {
                        generation,
                        value: buffers.clone(),
                    });
                    buffers
                }
            }
        };
        let (texts, uris) = buffers;
        Snapshot {
            generation,
            ctx: Ctx::with_buffers(self.encoding(), texts, Arc::clone(&self.host))
                .with_client_uris(uris)
                .with_workspace(self.workspace_dir.get().map(PathBuf::as_path)),
        }
    }

    /// The root document evaluated over `snapshot`'s buffers, or `None`
    /// when no root is configured. Evaluated once per generation: the
    /// lock is held while building, so concurrent requests wait for the
    /// one build instead of repeating it.
    fn root(&self, snapshot: &Snapshot) -> Option<RootResult> {
        let path = self.root_path()?;
        let mut cache = lock(&self.root);
        if let Some(cached) = cache.as_ref()
            && cached.generation == snapshot.generation
        {
            return Some(cached.value.clone());
        }
        let value = Document::from_file_with_loader(
            &path,
            snapshot.ctx.environment(),
            snapshot.ctx.loader(),
        )
        .map(Arc::new)
        .map_err(Arc::new);
        if cache
            .as_ref()
            .is_none_or(|cached| cached.generation < snapshot.generation)
        {
            *cache = Some(Cached {
                generation: snapshot.generation,
                value: value.clone(),
            });
        }
        Some(value)
    }

    /// The root document when it parses.
    fn root_document(&self, snapshot: &Snapshot) -> Option<Arc<Document>> {
        self.root(snapshot)?.ok()
    }

    /// Diagnostics for every open file, every file an analysis placed a
    /// diagnostic in, and every file published last time (so stale
    /// errors clear), each with the version of its open buffer. `None`
    /// once a change newer than `generation` makes the pass stale.
    ///
    /// With a root document configured, the root is analysed once, its
    /// errors are placed in the files they were raised in, and every
    /// open buffer adds its own syntax errors. Schema-validating a
    /// fragment in isolation would report false positives for
    /// everything the root supplies (imported `@block` declarations,
    /// document schemas, referenced data), so files outside the root's
    /// import graph get syntax errors only. With no root, every open
    /// buffer is analysed as a document of its own.
    fn collect_diagnostics(&self, generation: u64) -> Option<Batch> {
        let snapshot = self.snapshot();
        if snapshot.generation != generation {
            return None;
        }
        let ctx = &snapshot.ctx;
        let open: Vec<(Uri, i32, String)> = self
            .docs
            .iter()
            .map(|entry| (entry.key().clone(), entry.version, entry.rope.to_string()))
            .collect();
        // A diagnostic placed by path lands on the URI the editor opened
        // the file with, even through a symlink.
        let uri_for = |path: &Path| ctx.uri_for(path);

        let mut by_uri: HashMap<Uri, Vec<Diagnostic>> = HashMap::new();
        let mut place = |uri: Uri, diagnostic: Diagnostic| {
            let slot = by_uri.entry(uri).or_default();
            if !slot.contains(&diagnostic) {
                slot.push(diagnostic);
            }
        };
        match (self.root_path(), self.root(&snapshot)) {
            (Some(root), Some(result)) => {
                let placed = match &result {
                    Ok(doc) => diagnostics::document(ctx, doc),
                    Err(e) => diagnostics::parse_failure(ctx, e, &root.display().to_string()),
                };
                for (origin, diagnostic) in placed {
                    let target = match origin {
                        diagnostics::Origin::Analysed => uri_for(&root),
                        diagnostics::Origin::File(path) => uri_for(&path),
                    };
                    if let Some(uri) = target {
                        place(uri, diagnostic);
                    }
                }
                for (uri, _, text) in &open {
                    for diagnostic in diagnostics::syntax_only(ctx, text, uri.as_str()) {
                        place(uri.clone(), diagnostic);
                    }
                }
            }
            _ => {
                for (uri, _, text) in &open {
                    if self.generation() != generation {
                        return None;
                    }
                    for (origin, diagnostic) in diagnostics::analyse(ctx, text, uri.as_str()) {
                        let target = match origin {
                            diagnostics::Origin::Analysed => Some(uri.clone()),
                            diagnostics::Origin::File(path) => uri_for(&path),
                        };
                        if let Some(target) = target {
                            place(target, diagnostic);
                        }
                    }
                }
            }
        }
        if self.generation() != generation {
            return None;
        }

        let mut published = lock(&self.published);
        let versions: HashMap<&Uri, i32> = open
            .iter()
            .map(|(uri, version, _)| (uri, *version))
            .collect();
        let targets: HashSet<Uri> = open
            .iter()
            .map(|(uri, _, _)| uri.clone())
            .chain(by_uri.keys().cloned())
            .chain(published.drain())
            .collect();
        let mut batch = Vec::with_capacity(targets.len());
        for uri in targets {
            let diagnostics = by_uri.remove(&uri).unwrap_or_default();
            if !diagnostics.is_empty() {
                published.insert(uri.clone());
            }
            let version = versions.get(&uri).copied();
            batch.push((uri, version, diagnostics));
        }
        Some(batch)
    }

    /// The buffer text plus a fresh snapshot and the byte offset of
    /// `pos` — the shared preamble for the position-bearing request
    /// handlers. `None` when the document isn't open.
    fn source_at(
        &self,
        uri: &Uri,
        pos: tower_lsp_server::ls_types::Position,
    ) -> Option<(String, Snapshot, usize)> {
        let source = self.document_text(uri)?;
        let snapshot = self.snapshot();
        let offset = snapshot.ctx.index(&source).offset(pos);
        Some((source, snapshot, offset))
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> RpcResult<InitializeResult> {
        let encoding = PositionEncoding::negotiate(&params.capabilities);
        if self.state.encoding.set(encoding).is_err() {
            tracing::warn!("initialize received twice; keeping the first position encoding");
        }
        if let Some(dir) = Backend::workspace_dir(&params)
            && self.state.workspace_dir.set(dir).is_err()
        {
            tracing::warn!("initialize received twice; keeping the first workspace folder");
        }
        if let Some(p) = Backend::resolve_root(&params) {
            match self.state.root_path.write() {
                Ok(mut guard) => *guard = Some(p),
                Err(e) => {
                    tracing::error!("root_path lock poisoned; recovering: {e}");
                    *e.into_inner() = Some(p);
                }
            }
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(self.state.encoding().kind()),
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
            offset_encoding: None,
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
        if let Some(pending) = lock(&self.diagnostics).take() {
            pending.abort();
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.state.docs.insert(
            params.text_document.uri,
            OpenDoc {
                rope: Rope::from_str(&params.text_document.text),
                version: params.text_document.version,
            },
        );
        self.changed();
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Apply every change event in order. With INCREMENTAL sync
        // the client sends one or more ranged edits per request;
        // when `range` is None it's a full-document replacement
        // (clients may still send those for large diffs).
        let encoding = self.state.encoding();
        let mut doc = self.state.docs.entry(params.text_document.uri).or_default();
        doc.version = params.text_document.version;
        let rope = &mut doc.rope;
        for change in params.content_changes {
            match change.range {
                Some(range) => {
                    let start_char = rope_char_index(rope, range.start, encoding);
                    let end_char = rope_char_index(rope, range.end, encoding).max(start_char);
                    rope.remove(start_char..end_char);
                    rope.insert(start_char, &change.text);
                }
                None => {
                    *rope = Rope::from_str(&change.text);
                }
            }
        }
        drop(doc);
        self.changed();
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Without its buffer the file reads from disk again, and its own
        // diagnostics clear unless an analysis still places some there.
        self.state.docs.remove(&params.text_document.uri);
        self.changed();
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        // A file under the open buffers changed on disk (a branch switch,
        // another tool): everything cached from it is stale.
        self.changed();
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> RpcResult<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        self.run(move |state| {
            let source = state.document_text(&uri)?;
            // A parse failure is already surfaced by the diagnostics.
            let ast = parse_for_edit(&source, uri.as_str()).ok()?;
            let formatted = wcl_format::to_source(&ast);
            if formatted == source {
                return Some(Vec::new());
            }
            let range = crate::convert::LineIndex::new(&source, state.encoding()).full_range();
            Some(vec![TextEdit {
                range,
                new_text: formatted,
            }])
        })
        .await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> RpcResult<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        self.run(move |state| {
            let source = state.document_text(&uri)?;
            let symbols = symbols::compute(&state.snapshot().ctx, &source, uri.as_str());
            Some(DocumentSymbolResponse::Nested(symbols))
        })
        .await
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> RpcResult<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;
        self.run(move |state| {
            let source = state.document_text(&uri)?;
            Some(folding::compute(&source, uri.as_str()))
        })
        .await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> RpcResult<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params;
        self.run(move |state| {
            let uri = position.text_document.uri;
            let (source, snapshot, offset) = state.source_at(&uri, position.position)?;
            let root_doc = state.root_document(&snapshot);
            let root_path = state.root_path();
            navigation::goto_definition(
                &snapshot.ctx,
                uri,
                &source,
                offset,
                root_doc.as_deref(),
                root_path.as_deref(),
            )
        })
        .await
    }

    async fn references(&self, params: ReferenceParams) -> RpcResult<Option<Vec<Location>>> {
        let position = params.text_document_position;
        let include_declaration = params.context.include_declaration;
        self.run(move |state| {
            let uri = position.text_document.uri;
            let (source, snapshot, offset) = state.source_at(&uri, position.position)?;
            let root_doc = state.root_document(&snapshot);
            let root_path = state.root_path();
            navigation::references(
                &snapshot.ctx,
                uri,
                &source,
                offset,
                include_declaration,
                root_doc.as_deref(),
                root_path.as_deref(),
            )
        })
        .await
    }

    async fn rename(&self, params: RenameParams) -> RpcResult<Option<WorkspaceEdit>> {
        let position = params.text_document_position;
        let new_name = params.new_name;
        self.run(move |state| {
            let uri = position.text_document.uri;
            let Some((source, snapshot, offset)) = state.source_at(&uri, position.position) else {
                return Ok(None);
            };
            // With no root the request file anchors the rename, opened
            // from its buffer like any other file.
            let (root_doc, root_path) = match state.root_path() {
                Some(root) => (state.root_document(&snapshot), Some(root)),
                None => {
                    let path = uri_to_path(&uri);
                    let doc = path.as_ref().and_then(|path| {
                        Document::from_file_with_loader(
                            path,
                            snapshot.ctx.environment(),
                            snapshot.ctx.loader(),
                        )
                        .ok()
                        .map(Arc::new)
                    });
                    (doc, path)
                }
            };
            navigation::rename(
                &snapshot.ctx,
                uri,
                &source,
                offset,
                &new_name,
                root_doc.as_deref(),
                root_path.as_deref(),
            )
        })
        .await?
        .map_err(RpcError::invalid_params)
    }

    async fn hover(&self, params: HoverParams) -> RpcResult<Option<Hover>> {
        let position = params.text_document_position_params;
        self.run(move |state| {
            let uri = position.text_document.uri;
            let (source, snapshot, offset) = state.source_at(&uri, position.position)?;
            let root_doc = state.root_document(&snapshot);
            hover_impl::hover(
                &snapshot.ctx,
                &source,
                uri.as_str(),
                offset,
                root_doc.as_deref(),
            )
        })
        .await
    }

    async fn completion(&self, params: CompletionParams) -> RpcResult<Option<CompletionResponse>> {
        let position = params.text_document_position;
        self.run(move |state| {
            let uri = position.text_document.uri;
            let (source, snapshot, offset) = state.source_at(&uri, position.position)?;
            let root_doc = state.root_document(&snapshot);
            let items = completion::completions(
                &snapshot.ctx,
                &source,
                uri.as_str(),
                offset,
                root_doc.as_deref(),
            );
            Some(CompletionResponse::Array(items))
        })
        .await
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> RpcResult<Option<SignatureHelp>> {
        let position = params.text_document_position_params;
        self.run(move |state| {
            let uri = position.text_document.uri;
            let (source, snapshot, offset) = state.source_at(&uri, position.position)?;
            let ctx = &snapshot.ctx;
            // The buffer is usually mid-call (that's why help fired) and
            // the overlay carries that unparseable text, which would fail
            // the root parse and lose cross-file resolution. Retry the
            // root with the buffer's *repaired* form (open brackets
            // closed) overlaid.
            let root_doc = state.root_document(&snapshot).or_else(|| {
                let root = state.root_path()?;
                let path = uri_to_path(&uri)?;
                let mut overlay = (*ctx.buffers).clone();
                overlay.insert(path, signature::repair_source(&source, offset));
                let repaired = Ctx::with_buffers(ctx.encoding, overlay, Arc::clone(ctx.host()));
                Document::from_file_with_loader(&root, ctx.environment(), repaired.loader())
                    .ok()
                    .map(Arc::new)
            });
            signature::signature_help(ctx, &source, uri.as_str(), offset, root_doc.as_deref())
        })
        .await
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> RpcResult<Option<WorkspaceSymbolResponse>> {
        self.run(move |state| {
            let snapshot = state.snapshot();
            let root_doc = state.root_document(&snapshot);
            let root_path = state.root_path();
            Some(WorkspaceSymbolResponse::Flat(workspace::workspace_symbols(
                &snapshot.ctx,
                &params.query,
                root_doc.as_deref(),
                root_path.as_deref(),
            )))
        })
        .await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> RpcResult<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        self.run(move |state| {
            let source = state.document_text(&uri)?;
            let data = semtokens::compute(&source, state.encoding());
            Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data,
            }))
        })
        .await
    }

    async fn code_action(&self, params: CodeActionParams) -> RpcResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let Some(source) = self.state.document_text(&uri) else {
            return Ok(None);
        };
        Ok(code_actions::compute(
            &uri,
            &source,
            &params.context.diagnostics,
        ))
    }
}
