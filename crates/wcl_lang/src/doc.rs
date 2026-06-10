use std::path::{Path, PathBuf};

use miette::{NamedSource, SourceSpan};

use std::collections::{HashMap, HashSet};

mod cells;
mod effective_fields;
mod eval;
mod eval_ops;
mod imports;
mod interfaces;
mod loader;
mod lookup;
mod match_pat;
mod schema_check;
mod scope;
mod validate;
pub(super) mod variant_dispatch;
mod views;
pub use imports::SYSTEM_IMPORT_ROOT;
pub use loader::{FileLoader, Registry, disk_loader, overlay_loader};
pub use views::{
    Block, ChildKind, Connection, ConnectionDecl, DeclName, Decorator, Field, InterfaceDecl,
    NamedArg, ResolvedType, RowView, SymbolEntry, SymbolSetDecl, TableView, TypeDecl, TypeField,
    UnionDecl, UnionVariant, UseDeclView, UseFormView, UseItem, VariantBodyView,
};
pub(crate) use views::{BuiltinDecorator, LetView, UnionChildKind};

use crate::ast::{self, Span};
use crate::environment::Environment;
use crate::error::{EvalError, ParseError};
use crate::parser::Parser;
use crate::symbols::{SymbolIndex, SymbolKind, SymbolRecord};
#[cfg(test)]
use crate::value::{BuiltinType, TensorDim};
use crate::value::{TypeRef, Value};
use cells::{BlockCells, ItemCellKind, ItemCells, LoadedImport};
use imports::{expand_top_level_imports, load_import_lazily};
use lookup::{find_block, find_field, find_let, iter_blocks, iter_fields};
use schema_check::has_schemaless;
use scope::Scope;
use validate::{decl_fqn_matches, resolve_path, validate_document};

pub struct Document {
    src: NamedSource<String>,
    ast: ast::Source,
    cells: BlockCells,
    file_ns: Vec<String>,
    item_aliases: HashMap<String, Vec<String>>,
    ns_aliases: HashMap<String, Vec<String>>,
    wildcards: Vec<Vec<String>>,
    synthetic_types: Vec<ast::TypeDecl>,
    /// Parallel cells for `synthetic_types` so view types can reuse the
    /// same caching paths without special-casing.
    synthetic_type_cells: Vec<ItemCells>,
    symbols: SymbolIndex,
    env: Environment,
    /// Top-level imports expanded eagerly at `open_with` time. Their
    /// items participate in the unified `symbols` index and in
    /// `Document::fields/blocks/...` iteration.
    eager_imports: Vec<LoadedImport>,
    /// File reader used for every import this document follows
    /// (eager top-level + lazy in-block). Defaults to
    /// [`disk_loader`]; the LSP supplies an [`overlay_loader`]
    /// pre-loaded with open buffers.
    loader: FileLoader,
    /// Optional profile collector. Populated only when the document is
    /// opened through one of the `*_profiled` constructors; otherwise
    /// every profile hook is a no-op `Option::is_some` check.
    profile: Option<std::sync::Mutex<crate::profile::ProfileState>>,
    /// Lazily-built cache of every declared FQN (see [`ref_registry`]).
    /// Sound to compute once: its inputs — the root AST,
    /// `synthetic_types`, and the eager imports' symbol indexes — are
    /// all fixed at construction time (lazy in-block imports never
    /// contribute; `all_sources` walks eager imports only). Name
    /// resolution consults this on every lookup, so rebuilding it per
    /// call made resolution O(total declarations) each time.
    ref_registry: std::sync::OnceLock<HashSet<Vec<String>>>,
    /// Lazily-built index of `(decorator name, first positional string
    /// arg)` → positions in [`type_decls`] order, replacing the
    /// full-scan-with-decorator-eval that [`schema_candidates`] ran on
    /// every block-kind lookup. Same construction-time-only inputs as
    /// `ref_registry`.
    schema_index: std::sync::OnceLock<HashMap<(String, String), Vec<usize>>>,
    /// Lazily-built index of `wdoc_component` name → position in
    /// [`blocks`] order, replacing the per-call label-evaluating scan in
    /// [`component_def`] (which wdoc expansion runs for every nested
    /// block). Same construction-time-only inputs as `ref_registry`.
    component_index: std::sync::OnceLock<HashMap<String, usize>>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("file_ns", &self.file_ns)
            .field("item_aliases", &self.item_aliases)
            .field("ns_aliases", &self.ns_aliases)
            .field("wildcards", &self.wildcards)
            .field("eager_imports", &self.eager_imports)
            .finish_non_exhaustive()
    }
}

/// A homogeneous view over one source of top-level items — either the
/// importer's own source or an eagerly-loaded import.
#[derive(Clone, Copy)]
struct SourceView<'a> {
    symbols: &'a SymbolIndex,
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    file_ns: &'a [String],
    /// Resolved path on disk. `None` for the root document (the host
    /// typically supplies that path itself, e.g. via the LSP request
    /// URI); `Some` for every eagerly-loaded import.
    path: Option<&'a Path>,
}

/// The set of `@document` schemas governing one namespace, root-authored
/// first. Their *merge* forms the effective document schema: a top-level
/// field/block is legal if any member declares/allows it. Built by
/// [`Document::doc_schemas_for_ns`].
pub(crate) struct DocSchemas<'a> {
    schemas: Vec<TypeDecl<'a>>,
}

impl<'a> DocSchemas<'a> {
    /// No `@document` governs this namespace.
    pub(crate) fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }

    /// The field declared by name, preferring the root-authored schema
    /// (members are ordered root-first).
    pub(crate) fn field(&self, name: &str) -> Option<TypeField<'a>> {
        self.schemas.iter().find_map(|s| s.field(name))
    }

    /// `true` if any member schema declares a field of this name.
    pub(crate) fn declares_field(&self, name: &str) -> bool {
        self.schemas.iter().any(|s| s.field(name).is_some())
    }

    /// The union of every member's allowed child block kinds.
    pub(crate) fn allowed_child_kinds(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.schemas {
            for k in s.allowed_child_kinds() {
                if !out.contains(&k) {
                    out.push(k);
                }
            }
        }
        out
    }

    /// The union of every member's union-typed `@children`/`@child`
    /// slots — a structurally-matched top-level block bypasses the
    /// kind check against any of these.
    pub(crate) fn union_slots(&self) -> Vec<UnionDecl<'a>> {
        let mut out: Vec<UnionDecl<'a>> = Vec::new();
        for s in &self.schemas {
            for f in s.fields() {
                if let Some(u) = f
                    .children_kind_or_union()
                    .and_then(|k| k.as_union().copied())
                    .or_else(|| f.child_kind_or_union().and_then(|k| k.as_union().copied()))
                {
                    out.push(u);
                }
            }
        }
        out
    }

    /// Names of the member schemas, joined for diagnostics.
    pub(crate) fn names(&self) -> String {
        self.schemas
            .iter()
            .map(|s| s.name())
            .collect::<Vec<_>>()
            .join("', '")
    }
}

/// A symbol lookup result that knows which source it came from.
/// Exposed so the LSP can build cross-file `Location`s for
/// go-to-definition without reaching into `SymbolIndex` directly.
#[derive(Debug, Clone, Copy)]
pub struct SymbolHit<'a> {
    pub record: &'a SymbolRecord,
    /// File path of the source this symbol was declared in. `None`
    /// when the symbol comes from the root document.
    pub source_path: Option<&'a Path>,
}

/// Scan every source for the first declaration matching `$fqn` whose symbol
/// kind, AST item, and view struct all share `$variant`'s name, returning the
/// constructed view from the enclosing fn on a hit. The macro emits only the
/// search loop — the caller supplies the miss tail (`None`, or a synthetic
/// fallback). `cells`/`nocells` selects whether the view carries a `cells`
/// borrow; an optional trailing field name (`is_imported`) is set from the
/// source's import origin. Collapses the five `type_decl`/`interface`/
/// `union_decl`/`symbol_set`/`connection_decl` lookups (M4).
macro_rules! find_decl {
    ($self:ident, $fqn:ident, $variant:ident, cells $(, $imp:ident)?) => {
        for src in $self.all_sources() {
            if let Some(rec) = src.symbols.lookup($fqn)
                && matches!(rec.kind, SymbolKind::$variant)
                && let ast::Item::$variant(node) = &src.items[rec.path.item_index]
            {
                return Some($variant {
                    ast: node,
                    file_ns: src.file_ns,
                    cells: &src.cells[rec.path.item_index],
                    doc: $self,
                    $( $imp: src.path.is_some(), )?
                });
            }
        }
    };
    ($self:ident, $fqn:ident, $variant:ident, nocells) => {
        for src in $self.all_sources() {
            if let Some(rec) = src.symbols.lookup($fqn)
                && matches!(rec.kind, SymbolKind::$variant)
                && let ast::Item::$variant(node) = &src.items[rec.path.item_index]
            {
                return Some($variant {
                    ast: node,
                    file_ns: src.file_ns,
                    doc: $self,
                });
            }
        }
    };
}

/// Iterate every `$variant` declaration across the document and its eager
/// imports, in source order, yielding the matching `cells`-carrying view.
/// Collapses the `interfaces`/`union_decls`/`symbol_sets` iterators (M4).
macro_rules! decl_iter_cells {
    ($self:ident, $variant:ident) => {{
        let doc = $self;
        doc.all_sources().into_iter().flat_map(move |src| {
            src.items
                .iter()
                .zip(src.cells.iter())
                .filter_map(move |(item, cells)| match item {
                    ast::Item::$variant(node) => Some($variant {
                        ast: node,
                        file_ns: src.file_ns,
                        cells,
                        doc,
                    }),
                    _ => None,
                })
        })
    }};
}

impl Document {
    pub fn open(source: &str, name: &str) -> Result<Self, ParseError> {
        Self::open_with(source, name, &Environment::new())
    }

    pub fn open_with(source: &str, name: &str, env: &Environment) -> Result<Self, ParseError> {
        Self::open_at(source, name, None, env)
    }

    /// Variant of [`open_with`] that accepts a base directory for
    /// resolving relative `import` paths. Hosts that synthesise
    /// source in memory (e.g. wdoc prepending a schema) call this
    /// directly so the source's own imports still resolve relative
    /// to its on-disk location.
    pub fn open_at(
        source: &str,
        name: &str,
        base_dir: Option<PathBuf>,
        env: &Environment,
    ) -> Result<Self, ParseError> {
        Self::open_at_with_loader(source, name, base_dir, env, loader::disk_loader())
    }

    /// Like [`open_at`] but uses a caller-supplied [`FileLoader`] for
    /// every imported file. Hosts that maintain in-memory buffers
    /// (e.g. the LSP) pass an [`overlay_loader`] so unsaved edits
    /// participate in import resolution.
    pub fn open_at_with_loader(
        source: &str,
        name: &str,
        base_dir: Option<PathBuf>,
        env: &Environment,
        loader: FileLoader,
    ) -> Result<Self, ParseError> {
        let (ast, symbols) = Parser::new(source, name).parse_source()?;
        let synthetic = env.types().to_vec();

        // Resolve top-level imports eagerly. Each LoadedImport carries
        // its own (items, cells, symbols).
        let mut state = imports::ImportState::default();
        let mut eager_imports: Vec<LoadedImport> = Vec::new();
        expand_top_level_imports(
            &ast.items,
            base_dir.as_deref(),
            &mut state,
            &mut eager_imports,
            name,
            source,
            &loader,
        )?;

        let mut import_syms: Vec<&SymbolIndex> = Vec::new();
        imports::collect_import_symbols(&eager_imports, &mut import_syms);
        let mut import_nss: Vec<Vec<String>> = Vec::new();
        imports::collect_import_namespaces(&eager_imports, &mut import_nss);
        let resolved = validate_document(
            &ast,
            &symbols,
            &synthetic,
            &import_syms,
            &import_nss,
            source,
            name,
        )?;
        let cells = BlockCells::build(&ast.items, base_dir.as_deref());
        let synthetic_type_cells = synthetic
            .iter()
            .map(|t| ItemCells::build(&ast::Item::TypeDecl(t.clone()), None))
            .collect();
        Ok(Self {
            src: NamedSource::new(name, source.to_string()),
            ast,
            cells,
            file_ns: resolved.file_ns,
            item_aliases: resolved.item_aliases,
            ns_aliases: resolved.ns_aliases,
            wildcards: resolved.wildcards,
            synthetic_types: synthetic,
            synthetic_type_cells,
            symbols,
            env: env.clone(),
            eager_imports,
            loader,
            profile: None,
            ref_registry: std::sync::OnceLock::new(),
            schema_index: std::sync::OnceLock::new(),
            component_index: std::sync::OnceLock::new(),
        })
    }

    /// [`open`](Self::open) with profiling enabled. The resulting
    /// document records timings into a tree visible via
    /// [`profile`](Self::profile).
    pub fn open_profiled(source: &str, name: &str) -> Result<Self, ParseError> {
        Self::open_with_profiled(source, name, &Environment::new())
    }

    /// [`open_with`](Self::open_with) with profiling enabled.
    pub fn open_with_profiled(
        source: &str,
        name: &str,
        env: &Environment,
    ) -> Result<Self, ParseError> {
        let mut doc = Self::open_at(source, name, None, env)?;
        doc.profile = Some(crate::profile::ProfileState::new_root());
        Ok(doc)
    }

    /// Switch on profiling for an already-opened document; subsequent
    /// evaluation records timings visible via [`profile`](Self::profile).
    /// For hosts (e.g. `wcl wdoc build --profile`) whose constructor has
    /// no `*_profiled` twin.
    pub fn enable_profiling(&mut self) {
        self.profile = Some(crate::profile::ProfileState::new_root());
    }

    /// [`from_file`](Self::from_file) with profiling enabled.
    pub fn from_file_profiled(path: &Path) -> Result<Self, ParseError> {
        let mut doc = Self::from_file(path)?;
        doc.profile = Some(crate::profile::ProfileState::new_root());
        Ok(doc)
    }

    /// The file loader this document was opened with. Lazy in-block
    /// imports go through it so the same overlay (if any) applies.
    pub(crate) fn loader(&self) -> &FileLoader {
        &self.loader
    }

    /// Snapshot of the profile tree, when profiling is enabled.
    /// Returns `None` if the document was not opened through one of
    /// the `*_profiled` constructors.
    pub fn profile(&self) -> Option<crate::profile::Profile> {
        self.profile
            .as_ref()
            .map(|m| m.lock().unwrap_or_else(|p| p.into_inner()).snapshot())
    }

    /// Internal helper used by hook sites. Returns a no-op guard when
    /// profiling is disabled. Wrap the work to be measured by binding
    /// the return value to `let _guard = …;`.
    pub(crate) fn profile_enter(
        &self,
        key: crate::profile::ProfileKey,
    ) -> crate::profile::ProfileGuard<'_> {
        crate::profile::ProfileGuard::enter(self.profile.as_ref(), key)
    }

    /// The identifier index built incrementally during parsing.
    /// See [`SymbolIndex`] for what it covers and what is excluded.
    pub fn symbols(&self) -> &SymbolIndex {
        &self.symbols
    }

    /// Borrow the importer's items + cells + symbols followed by every
    /// eagerly-imported file's items + cells + symbols (recursively).
    /// Used by all of `field` / `block` / `type_decl` etc. so imports
    /// are searched after the importer.
    /// Locate the file that owns `target_field` by pointer-identity.
    /// Returns `None` when the field lives in the document's main
    /// source (the file the host opened directly); returns the
    /// originating import's `path` when the field came in through an
    /// eager top-level import or an already-forced in-block lazy
    /// import.
    ///
    /// Always returning `None` for the main source keeps `Document`
    /// out of the business of tracking its own filesystem path — the
    /// CLI (or other host) already has that string from
    /// `Document::from_file(path)` and doesn't need it round-tripped.
    pub(crate) fn find_field_source_path(&self, target: *const ast::Field) -> Option<&Path> {
        // Main file first — if we find it there, the answer is None.
        if field_in_items(&self.ast.items, target, &self.cells.items) {
            return None;
        }
        // Eager imports (and their transitive eagers) carry their
        // own path; descend until we find a match.
        for imp in &self.eager_imports {
            if let Some(p) = find_in_import(imp, target) {
                return Some(p);
            }
        }
        // Lazy in-block imports inside the main file. Their
        // `LoadedImport` is populated on first access — if a CLI
        // caller drove `Document::get` over a path that crossed
        // them, the cell is filled and we can recover the path.
        find_lazy_in_blocks(&self.ast.items, &self.cells.items, target)
    }

    /// Like [`find_field_source_path`] but returns the source's
    /// `file_ns` (the namespace declared at the top of that file).
    /// Falls back to the document's own `file_ns` when the field
    /// isn't located in any known source — callers treat the main
    /// document as the default.
    pub(crate) fn find_field_source_ns(&self, target: *const ast::Field) -> &[String] {
        if field_in_items(&self.ast.items, target, &self.cells.items) {
            return &self.file_ns;
        }
        for imp in &self.eager_imports {
            if let Some(ns) = find_field_ns_in_import(imp, target) {
                return ns;
            }
        }
        find_lazy_field_ns_in_blocks(&self.ast.items, &self.cells.items, target)
            .unwrap_or(&self.file_ns)
    }

    /// miette source (name + text) for the root document.
    fn root_named_source(&self) -> NamedSource<String> {
        NamedSource::new(self.src.name(), self.src.inner().clone())
    }

    /// The miette source (name + text) of the file that declares the
    /// block `target` points into — the root document, or the imported
    /// file that carries it. Hosts (e.g. the wdoc renderer) use this to
    /// render an eval diagnostic against the correct file's snippet rather
    /// than always against the root source (whose offsets won't match a
    /// cross-file span — the cause of the `OutOfBounds` misrender). Falls
    /// back to the root source when the block can't be located (e.g. a
    /// synthesised block that isn't backed by on-disk AST).
    pub fn named_source_for_block(&self, target: *const ast::Block) -> NamedSource<String> {
        if block_in_items(&self.ast.items, target) {
            return self.root_named_source();
        }
        for imp in &self.eager_imports {
            if let Some(src) = named_source_in_import(imp, target) {
                return src;
            }
        }
        self.root_named_source()
    }

    fn all_sources(&self) -> Vec<SourceView<'_>> {
        let mut out = vec![SourceView {
            symbols: &self.symbols,
            items: &self.ast.items,
            cells: &self.cells.items,
            file_ns: &self.file_ns,
            path: None,
        }];
        fn push_imports<'a>(imports: &'a [LoadedImport], out: &mut Vec<SourceView<'a>>) {
            for imp in imports {
                out.push(SourceView {
                    symbols: &imp.symbols,
                    items: &imp.items,
                    cells: &imp.cells,
                    file_ns: &imp.file_ns,
                    path: Some(imp.path.as_path()),
                });
                push_imports(&imp.eager_imports, out);
            }
        }
        push_imports(&self.eager_imports, &mut out);
        out
    }

    /// Paths of every eagerly-loaded import reachable from this
    /// document, deduplicated. Used by tooling (e.g. the LSP) that
    /// needs to scan imported source files — for instance, to find
    /// cross-file references to a symbol.
    pub fn imported_paths(&self) -> Vec<&Path> {
        let mut out = Vec::new();
        fn walk<'a>(imports: &'a [LoadedImport], out: &mut Vec<&'a Path>) {
            for imp in imports {
                let p = imp.path.as_path();
                if !out.contains(&p) {
                    out.push(p);
                }
                walk(&imp.eager_imports, out);
            }
        }
        walk(&self.eager_imports, &mut out);
        out
    }

    /// Lookup a fully-qualified symbol across this document and every
    /// eagerly-loaded import. Returns the matching `SymbolRecord`
    /// together with the file path of the source it lives in (`None`
    /// for the root document). Hosts use this for cross-file
    /// go-to-definition.
    pub fn find_symbol(&self, fqn: &str) -> Option<SymbolHit<'_>> {
        for src in self.all_sources() {
            if let Some(record) = src.symbols.lookup(fqn) {
                return Some(SymbolHit {
                    record,
                    source_path: src.path,
                });
            }
        }
        None
    }

    /// Lazy dotted-path access into the document. Each segment is
    /// resolved on demand against the current node — only the cells
    /// actually visited are forced. Returns `None` for any unresolved
    /// path.
    ///
    /// Resolution order for the first segment matches the existing
    /// surface: top-level Field, then Block-by-kind, then TypeDecl,
    /// UnionDecl, SymbolSetDecl. Subsequent segments delegate to
    /// [`DataRef::child`].
    pub fn get(&self, path: &str) -> Option<crate::data::DataRef<'_>> {
        let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        self.resolve_segments_in(&segs, &self.file_ns)
    }

    /// Dotted-path resolution shared by [`get`] and the reflection
    /// builtins' `Caller::resolve`, with an explicit namespace context
    /// for the root lookup (see [`resolve_root_in`]).
    ///
    /// Tries the longest matching FQN prefix as the root, so a dotted
    /// path can resolve directly to an imported item (e.g.
    /// `doc.get("shared.brand")` for an imported file with namespace
    /// `shared`). Falls through to single-segment root (the existing
    /// block-then-child traversal).
    pub(crate) fn resolve_segments_in(
        &self,
        segs: &[&str],
        context_ns: &[String],
    ) -> Option<crate::data::DataRef<'_>> {
        if segs.is_empty() {
            return None;
        }
        for k in (1..=segs.len()).rev() {
            let prefix = segs[..k].join(".");
            if let Some(root) = self.resolve_root_in(&prefix, context_ns) {
                let mut cur = root;
                for seg in &segs[k..] {
                    cur = cur.child(seg)?;
                }
                return Some(cur);
            }
        }
        None
    }

    /// Invoke a [`FnValue`] with the supplied arguments. Uses a fresh
    /// evaluation context rooted at the document — bodies see
    /// document-level symbols but no caller locals.
    ///
    /// Errors propagate as [`EvalError`]: arity mismatch ⇒
    /// `CallArity`, depth blown ⇒ `CallDepthExceeded`, anything the
    /// function body raises bubbles up unchanged.
    pub fn call_value(
        &self,
        f: &crate::value::FnValue,
        args: &[Value],
    ) -> Result<Value, EvalError> {
        use crate::doc::eval::EvalCtx;
        let mut ctx = EvalCtx::new(Scope::root());
        let span = ast::Span::new(0, 0);
        self.invoke_fn_value(f, args, &mut ctx, span)
    }

    /// Look up a top-level binding named `name`, expect a function
    /// value there, and invoke it with `args`. Convenience over
    /// [`Self::call_value`] when the host doesn't already hold the
    /// [`FnValue`].
    ///
    /// Returns a `UserError`-shaped diagnostic when the name doesn't
    /// resolve or resolves to a non-function value; otherwise
    /// behaves like [`Self::call_value`].
    pub fn call_function(&self, name: &str, args: &[Value]) -> Result<Value, EvalError> {
        let span = ast::Span::new(0, 0);
        let dr = self.get(name).ok_or_else(|| {
            EvalError::user_error(format!("no top-level binding named '{name}'"), span)
        })?;
        match dr.value()? {
            Value::Function(fv) => self.call_value(&fv, args),
            other => Err(EvalError::user_error(
                format!("'{name}' is not a function (got {})", other.type_name()),
                span,
            )),
        }
    }

    /// Project a top-level `@children`/`@child` field declared on the
    /// *merged* `@document` schema for the root namespace into a
    /// [`DataRef`], collecting the matching top-level blocks across every
    /// source. Mirrors the precedence of [`Block::typed_field`] but at the
    /// document root. Returns `None` when `name` is not a children/child
    /// field on the merged schema.
    ///
    /// Deferred (mirroring the block-level limits / rarity): top-level
    /// `Item::Table` rows feeding a root `@children` (no synth rows are
    /// built for top-level tables — see `cells.rs`), computed-children
    /// splices for the *string* `@children`/`@child` forms (only the union
    /// form folds in a literal-field splice), and interface-typed children.
    fn resolve_root_children(&self, name: &str) -> Option<crate::data::DataRef<'_>> {
        use crate::data::DataRef;
        let schemas = self.doc_schemas_for_ns(&self.file_ns);
        if schemas.is_empty() {
            return None;
        }
        let field = schemas.field(name)?;

        if let Some(ck) = field.children_kind_or_union() {
            match ck {
                ChildKind::Union(union) => {
                    let mut out: Vec<Value> = Vec::new();
                    for b in self.blocks() {
                        if let Ok(v) = variant_dispatch::block_to_variant(self, &b, union) {
                            out.push(v);
                        }
                    }
                    // Computed-children splice (`name = <list expr>` at root):
                    // the declared `list<Union>` type already coerced each
                    // bare record to a variant by shape, so splice them in.
                    if let Some(f) = self.field(name)
                        && let Ok(Value::List(items)) = f.value()
                    {
                        for it in items {
                            if matches!(it, Value::Variant { .. }) {
                                out.push(it.clone());
                            }
                        }
                    }
                    return Some(DataRef::from_variant_value_list(out));
                }
                ChildKind::Kind(kind) => {
                    let blocks: Vec<Block<'_>> =
                        self.blocks().filter(|b| b.kind() == kind).collect();
                    let is_table = self.table_schema(&kind).is_some();
                    return Some(if is_table {
                        DataRef::from_table(blocks)
                    } else {
                        DataRef::from_block_list(blocks)
                    });
                }
                ChildKind::Interface(_) => return None,
            }
        }

        if let Some(ck) = field.child_kind_or_union() {
            match ck {
                ChildKind::Union(union) => {
                    for b in self.blocks() {
                        if let Ok(v) = variant_dispatch::block_to_variant(self, &b, union) {
                            return Some(DataRef::from_variant_value(v));
                        }
                    }
                    return Some(DataRef::from_variant_value(Value::None));
                }
                ChildKind::Kind(kind) => {
                    return self
                        .blocks()
                        .find(|b| b.kind() == kind)
                        .map(DataRef::from_block);
                }
                ChildKind::Interface(_) => return None,
            }
        }

        None
    }

    pub(crate) fn resolve_root(&self, name: &str) -> Option<crate::data::DataRef<'_>> {
        self.resolve_root_in(name, &self.file_ns)
    }

    /// [`resolve_root`] with an explicit namespace context: declaration
    /// lookups (type / union / symbol set / interface) resolve as if the
    /// reference were written in a source whose namespace is
    /// `context_ns`, via the full name-resolution algorithm (context
    /// namespace first, then aliases / wildcards / absolute). Root-level
    /// projections, fields and blocks are root-document concerns and
    /// ignore `context_ns`.
    pub(crate) fn resolve_root_in(
        &self,
        name: &str,
        context_ns: &[String],
    ) -> Option<crate::data::DataRef<'_>> {
        use crate::data::DataRef;
        // Document-schema-driven projections at the root: a field on
        // the `@document` type marked with `@connections(...)` is
        // synthesised from sibling Connection statements rather than
        // looked up as a literal Field item. Resolve the field against
        // the *merged* `@document` schema for the namespace (root-authored
        // + imported), exactly as `resolve_root_children` does — using the
        // first `@document` globally (`doc_schema()`) would miss a
        // connections field declared in an imported schema when another
        // `@document` (e.g. the stdlib `Site`) sorts ahead of it.
        //
        // Skip projection while we're resolving a connection operand's
        // identifying label (see `RESOLVING_CONN_OPERAND`): a block label
        // can't depend on the connection set, and re-entering projection
        // here would recurse infinitely.
        if !RESOLVING_CONN_OPERAND.with(std::cell::Cell::get) {
            let schemas = self.doc_schemas_for_ns(&self.file_ns);
            if let Some(field) = schemas.field(name)
                && let Some(conn_schema) = field.connection_schema()
            {
                let mut all: Vec<Value> = Vec::new();
                for src in self.all_sources() {
                    all.extend(self.project_connections(src.items, conn_schema, &Scope::root()));
                }
                return Some(DataRef::from_variant_value(Value::List(all)));
            }
        }
        // Document-root `@children`/`@child` projection: a field declared
        // on the merged `@document` schema collects top-level blocks by
        // kind / structural shape, exactly as `Block::typed_field` does
        // for nested blocks. Runs before the literal field/block lookups
        // so a `@children("concept") concepts` slot isn't shadowed by a
        // top-level block of kind `concepts`.
        if let Some(dr) = self.resolve_root_children(name) {
            return Some(dr);
        }
        if let Some(f) = self.field(name) {
            return Some(DataRef::from_field(f));
        }
        if let Some(b) = self.block(name) {
            return Some(DataRef::from_block(b));
        }
        // Declarations resolve through the real name-resolution
        // algorithm so a type declared in an imported file's namespace
        // (registered as e.g. `lib.Gizmo`) is found from a bare `Gizmo`
        // (wildcard), an aliased reference, or a dotted `lib.Gizmo`.
        let segs: Vec<String> = name.split('.').map(str::to_string).collect();
        let fqn = self.resolve_path_in(&segs, context_ns).map(|p| p.join("."));
        let qualified: &str = fqn.as_deref().unwrap_or(name);
        if let Some(t) = self.type_decl(qualified).or_else(|| self.type_decl(name)) {
            return Some(DataRef::from_type(t));
        }
        if let Some(u) = self.union_decl(qualified).or_else(|| self.union_decl(name)) {
            return Some(DataRef::from_union(u));
        }
        if let Some(s) = self.symbol_set(qualified).or_else(|| self.symbol_set(name)) {
            return Some(DataRef::from_symbol_set(s));
        }
        if let Some(i) = self.interface(qualified).or_else(|| self.interface(name)) {
            return Some(DataRef::from_interface(i));
        }
        None
    }

    /// Resolve a connection-statement operand (a bare identifier) by
    /// walking the scope chain looking for a block whose first label
    /// equals `name`. Falls back to the document root. Returns the
    /// label value plus the block kind so callers can dispatch.
    pub(crate) fn resolve_connection_operand(
        &self,
        scope: &Scope<'_>,
        name: &str,
    ) -> Option<ConnOperand> {
        // Identifying a block here means evaluating its first label / `id`
        // field. That evaluation must not re-enter `@connections`
        // projection (a block's identity can't depend on the connection
        // set) — see `RESOLVING_CONN_OPERAND` in `resolve_root`. Without
        // this guard, projection → operand resolution → label eval →
        // projection recurses until the stack overflows.
        let _guard = ConnOperandGuard::enter();
        // Innermost scope frames first.
        for frame in scope.frames().iter().rev() {
            if let ItemCellKind::Block { items: fcells, .. } = &frame.cells.kind
                && let Some(found) =
                    match_block_label_in_items(self, &frame.ast.items, fcells, frame.file_ns, name)
            {
                return Some(found);
            }
        }
        // Fall back to document root: walk every source's top-level items.
        for src in self.all_sources() {
            if let Some(found) =
                match_block_label_in_items(self, src.items, src.cells, src.file_ns, name)
            {
                return Some(found);
            }
        }
        None
    }

    /// The schema of the block a connection operand resolved to,
    /// looked up with the operand's own namespace context (the `::`
    /// qualifier at the instance site, and the namespace of the file
    /// the block lives in) — mirroring [`Block::schema`].
    pub(crate) fn operand_schema(&self, op: &ConnOperand) -> Option<TypeDecl<'_>> {
        self.block_schema_in(&op.kind_ns, &op.kind, &op.file_ns)
            .or_else(|| self.table_schema_in(&op.kind_ns, &op.kind, &op.file_ns))
    }

    /// Project sibling `Item::Connection` statements through a
    /// `@connections(SchemaName)` decorator: gather every statement
    /// whose `(lhs_type, rhs_type)` matches the schema and produce a
    /// `Value::Record` per match.
    pub(crate) fn project_connections(
        &self,
        items: &[ast::Item],
        schema: ConnectionDecl<'_>,
        scope: &Scope<'_>,
    ) -> Vec<Value> {
        let mut out: Vec<Value> = Vec::new();
        // Endpoint types resolve relative to the file that declared the
        // connection, so a namespaced library's bare `Adr` means its own
        // `lib.Adr`.
        let source_fqn = self.resolve_type_fqn_in(schema.source_type(), schema.file_ns());
        let dest_fqn = self.resolve_type_fqn_in(schema.destination_type(), schema.file_ns());
        for item in items {
            let ast::Item::Connection(stmt) = item else {
                continue;
            };
            let Some(record) = self.build_connection_record(
                stmt,
                schema,
                scope,
                source_fqn.as_deref(),
                dest_fqn.as_deref(),
            ) else {
                continue;
            };
            out.push(record);
        }
        out
    }

    fn build_connection_record(
        &self,
        stmt: &ast::ConnectionStmt,
        schema: ConnectionDecl<'_>,
        scope: &Scope<'_>,
        source_fqn: Option<&str>,
        dest_fqn: Option<&str>,
    ) -> Option<Value> {
        let lhs = self.resolve_connection_operand(scope, &stmt.lhs);
        let rhs = self.resolve_connection_operand(scope, &stmt.rhs);

        // Does each *resolved* operand's block type satisfy this schema's
        // source / destination role? (An unresolved operand has no AST
        // block, so it can't be type-checked — see the dynamic path below.)
        let lhs_type_ok = matches!(&lhs, Some(op)
            if self.operand_schema(op).is_some_and(|d| connection_type_matches(&d, source_fqn)));
        let rhs_type_ok = matches!(&rhs, Some(op)
            if self.operand_schema(op).is_some_and(|d| connection_type_matches(&d, dest_fqn)));

        if lhs.is_some() && rhs.is_some() {
            // Both operands name a literal block — strict path, unchanged:
            // both must type-match for this schema to claim the statement.
            if !(lhs_type_ok && rhs_type_ok) {
                return None;
            }
        } else {
            // At least one operand didn't resolve to a literal block — e.g.
            // an id GENERATED by a `wdoc_repeater` / `wdoc_component`. Only a
            // `@dynamic` connection accepts that; it emits the raw operand
            // string so a downstream consumer can match it against the
            // generated shape ids. The resolved side(s) must still type-match
            // so we don't claim a statement meant for a different schema.
            if !schema.is_dynamic() {
                return None;
            }
            if (lhs.is_some() && !lhs_type_ok) || (rhs.is_some() && !rhs_type_ok) {
                return None;
            }
        }

        let kind_name = match &stmt.kind {
            Some(s) => s.clone(),
            None => schema.default_kind()?,
        };
        let lhs_val = lhs
            .map(|op| op.value)
            .unwrap_or_else(|| Value::Identifier(stmt.lhs.clone()));
        let rhs_val = rhs
            .map(|op| op.value)
            .unwrap_or_else(|| Value::Identifier(stmt.rhs.clone()));
        let mut fields: std::collections::BTreeMap<String, Value> =
            std::collections::BTreeMap::new();
        fields.insert("source".to_string(), lhs_val);
        fields.insert("destination".to_string(), rhs_val);
        fields.insert("kind".to_string(), Value::Symbol(kind_name));
        Some(Value::Record {
            ty: schema.ast.name.clone(),
            fields,
        })
    }

    /// Resolve a `TypeRef::Named` (or `TypeRef::Reference(Named ...)`,
    /// which is how interface-typed connection endpoints must be
    /// written) to its dotted FQN. Returns `None` for builtins,
    /// lists, tensors, etc.
    ///
    /// The reference resolves as if written in a source whose namespace
    /// is `file_ns` — e.g. a `connection X : Adr -> Adr` declared under
    /// `namespace lib` resolves its endpoints to `lib.Adr`.
    pub(crate) fn resolve_type_fqn_in(&self, t: &TypeRef, file_ns: &[String]) -> Option<String> {
        match t {
            TypeRef::Named(path) => self.resolve_path_in(path, file_ns).map(|p| p.join(".")),
            TypeRef::Reference(inner) => self.resolve_type_fqn_in(inner, file_ns),
            _ => None,
        }
    }

    pub fn from_file(path: &Path) -> Result<Self, ParseError> {
        Self::from_file_with_loader(path, &Environment::new(), loader::disk_loader())
    }

    /// Like [`from_file`] but also accepts a custom `Environment`. Use
    /// this when the host registers built-ins or schema types.
    pub fn from_file_with(path: &Path, env: &Environment) -> Result<Self, ParseError> {
        Self::from_file_with_loader(path, env, loader::disk_loader())
    }

    /// [`from_file_with`] plus a caller-supplied [`FileLoader`]. The
    /// loader is consulted for the root file *and* every transitive
    /// import (eager + lazy in-block). Use this with
    /// [`overlay_loader`] to make a long-running host's open buffers
    /// shadow disk contents.
    pub fn from_file_with_loader(
        path: &Path,
        env: &Environment,
        loader: FileLoader,
    ) -> Result<Self, ParseError> {
        let source = loader(path)?;
        let base_dir = path.parent().map(Path::to_path_buf);
        Self::open_at_with_loader(&source, &path.display().to_string(), base_dir, env, loader)
    }

    pub fn source(&self) -> &NamedSource<String> {
        &self.src
    }

    /// The host environment (synthetic types + builtins) that this
    /// document was opened with. Exposed so tooling (e.g. the LSP)
    /// can enumerate registered builtins for completion.
    pub fn environment(&self) -> &Environment {
        &self.env
    }

    pub fn field(&self, name: &str) -> Option<Field<'_>> {
        // Try the importer's file_ns first; fall back to the bare name
        // (which lets `doc.field("port")` find an imported field whose
        // imported file declared no namespace).
        let candidates = if self.file_ns.is_empty() {
            vec![name.to_string()]
        } else {
            vec![
                format!("{}.{}", self.file_ns.join("."), name),
                name.to_string(),
            ]
        };
        for fqn in candidates {
            for src in self.all_sources() {
                if let Some(rec) = src.symbols.lookup(&fqn)
                    && matches!(rec.kind, SymbolKind::Field)
                {
                    let idx = rec.path.item_index;
                    if let (ast::Item::Field(f), ItemCellKind::Field(_)) =
                        (&src.items[idx], &src.cells[idx].kind)
                    {
                        return Some(Field {
                            ast: f,
                            cells: &src.cells[idx],
                            doc: self,
                            scope: Scope::root(),
                        });
                    }
                }
            }
        }
        None
    }

    pub fn block(&self, kind: &str) -> Option<Block<'_>> {
        for src in self.all_sources() {
            let paths = src.symbols.blocks_with_kind(kind);
            if let Some(path) = paths.first() {
                let idx = path.item_index;
                if let (ast::Item::Block(b), ItemCellKind::Block { .. }) =
                    (&src.items[idx], &src.cells[idx].kind)
                {
                    return Some(Block {
                        ast: b,
                        cells: &src.cells[idx],
                        doc: self,
                        file_ns: src.file_ns,
                        kind_override: None,
                        scope: Scope::root(),
                    });
                }
            }
        }
        None
    }

    /// Resolve a top-level `let name = expr` binding. Lets are not in
    /// the symbol index (they are composition helpers, not document
    /// data), so this scans source items directly. The value evaluates
    /// at document-root scope, so it can reference other top-level
    /// lets / fields.
    pub(crate) fn root_let(&self, name: &str) -> Option<LetView<'_>> {
        for src in self.all_sources() {
            if let Some(l) = find_let(src.items, src.cells, name, self, &Scope::root()) {
                return Some(l);
            }
        }
        None
    }

    pub fn fields(&self) -> impl Iterator<Item = Field<'_>> + '_ {
        let doc = self;
        self.all_sources()
            .into_iter()
            .flat_map(move |src| iter_fields(src.items, src.cells, doc, Scope::root()))
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block<'_>> + '_ {
        let doc = self;
        self.all_sources()
            .into_iter()
            .flat_map(move |src| iter_blocks(src.items, src.cells, doc, src.file_ns, Scope::root()))
    }

    /// Like [`blocks`](Self::blocks) but pairs each top-level block with
    /// the path of the file it was declared in: `None` for the root
    /// document, `Some(path)` for an eagerly-imported file. Useful when
    /// emission order must place imported (library) definitions before
    /// the root's — e.g. CSS, where later rules win and so the user's
    /// root-level rules should override the library defaults.
    pub fn blocks_with_source(&self) -> impl Iterator<Item = (Option<&Path>, Block<'_>)> + '_ {
        let doc = self;
        self.all_sources().into_iter().flat_map(move |src| {
            let path = src.path;
            iter_blocks(src.items, src.cells, doc, src.file_ns, Scope::root())
                .map(move |b| (path, b))
        })
    }

    /// Returns the file-level namespace path. Empty when the file declared none.
    pub fn namespace(&self) -> &[String] {
        &self.file_ns
    }

    pub fn uses(&self) -> impl Iterator<Item = UseDeclView<'_>> {
        self.ast.items.iter().filter_map(|item| match item {
            ast::Item::UseDecl(u) => Some(UseDeclView { ast: u }),
            _ => None,
        })
    }

    /// Look up a type by fully-qualified name (dotted). Searches the
    /// importer, every eagerly-imported file, and registry-injected
    /// types in that order.
    /// Resolve type aliases (`type Port = u16`) inside `ty`, deeply:
    /// a `Named` ref whose declaration is an alias is replaced by its
    /// target (transitively, cycle-capped), and list / reference /
    /// tensor element types resolve recursively. Non-alias refs come
    /// back unchanged. Used by the schema value checks so a field
    /// declared with an alias validates against the target type.
    pub fn resolve_alias(&self, ty: &crate::value::TypeRef) -> crate::value::TypeRef {
        use crate::value::TypeRef as T;
        fn go(doc: &Document, ty: &T, depth: u8) -> T {
            if depth == 0 {
                return ty.clone(); // alias cycle — give up, stay permissive
            }
            match ty {
                T::Named(path) => {
                    let fqn = path.join(".");
                    match doc.type_decl(&fqn).and_then(|t| t.ast.alias.clone()) {
                        Some(target) => go(doc, &target, depth - 1),
                        None => ty.clone(),
                    }
                }
                T::List(inner) => T::List(Box::new(go(doc, inner, depth - 1))),
                T::Reference(inner) => T::Reference(Box::new(go(doc, inner, depth - 1))),
                other => other.clone(),
            }
        }
        go(self, ty, 8)
    }

    /// The chain of alias declarations behind `ty`, outermost first —
    /// empty for a non-alias type. Constraint decorators on each link
    /// apply to values of the aliased type.
    pub(crate) fn alias_chain(&self, ty: &crate::value::TypeRef) -> Vec<TypeDecl<'_>> {
        let mut out = Vec::new();
        let mut current = ty.clone();
        for _ in 0..8 {
            let crate::value::TypeRef::Named(path) = &current else {
                break;
            };
            let Some(decl) = self.type_decl(&path.join(".")) else {
                break;
            };
            let Some(target) = decl.ast.alias.clone() else {
                break;
            };
            out.push(decl);
            current = target;
        }
        out
    }

    pub fn type_decl(&self, fqn: &str) -> Option<TypeDecl<'_>> {
        find_decl!(self, fqn, TypeDecl, cells, is_imported);
        // Synthetic types live in the root namespace (no file ns prefix)
        // and are not registered in the parser-built index.
        let target: Vec<&str> = fqn.split('.').collect();
        self.synthetic_types
            .iter()
            .enumerate()
            .find(|(_, t)| decl_fqn_matches(&t.name, &target))
            .map(|(i, t)| TypeDecl {
                ast: t,
                file_ns: &[],
                cells: &self.synthetic_type_cells[i],
                doc: self,
                is_imported: false,
            })
    }

    pub fn type_decls(&self) -> impl Iterator<Item = TypeDecl<'_>> + '_ {
        let doc = self;
        let mine_and_imports = self.all_sources().into_iter().flat_map(move |src| {
            src.items
                .iter()
                .zip(src.cells.iter())
                .filter_map(move |(item, cells)| match item {
                    ast::Item::TypeDecl(t) => Some(TypeDecl {
                        ast: t,
                        file_ns: src.file_ns,
                        cells,
                        doc,
                        is_imported: src.path.is_some(),
                    }),
                    _ => None,
                })
        });
        let syn = self
            .synthetic_types
            .iter()
            .zip(self.synthetic_type_cells.iter())
            .map(move |(t, cells)| TypeDecl {
                ast: t,
                file_ns: &[],
                cells,
                doc,
                is_imported: false,
            });
        mine_and_imports.chain(syn)
    }

    /// Look up an interface declaration by fully-qualified name.
    /// Mirrors `type_decl` / `union_decl`.
    pub fn interface(&self, fqn: &str) -> Option<InterfaceDecl<'_>> {
        find_decl!(self, fqn, InterfaceDecl, cells);
        None
    }

    pub fn interfaces(&self) -> impl Iterator<Item = InterfaceDecl<'_>> + '_ {
        decl_iter_cells!(self, InterfaceDecl)
    }

    /// Look up a union by fully-qualified name (dotted).
    pub fn union_decl(&self, fqn: &str) -> Option<UnionDecl<'_>> {
        find_decl!(self, fqn, UnionDecl, cells);
        None
    }

    pub fn union_decls(&self) -> impl Iterator<Item = UnionDecl<'_>> + '_ {
        decl_iter_cells!(self, UnionDecl)
    }

    /// Look up a connection schema by fully-qualified name (dotted).
    pub fn connection_decl(&self, fqn: &str) -> Option<ConnectionDecl<'_>> {
        find_decl!(self, fqn, ConnectionDecl, nocells);
        None
    }

    /// Iterate every top-level connection statement in this document
    /// and its eager imports, in source order.
    pub fn connection_stmts(&self) -> impl Iterator<Item = Connection<'_>> + '_ {
        self.all_sources().into_iter().flat_map(|src| {
            src.items.iter().filter_map(|item| match item {
                ast::Item::Connection(c) => Some(Connection { ast: c }),
                _ => None,
            })
        })
    }

    pub fn connection_decls(&self) -> impl Iterator<Item = ConnectionDecl<'_>> + '_ {
        let doc = self;
        self.all_sources().into_iter().flat_map(move |src| {
            src.items.iter().filter_map(move |item| match item {
                ast::Item::ConnectionDecl(c) => Some(ConnectionDecl {
                    ast: c,
                    file_ns: src.file_ns,
                    doc,
                }),
                _ => None,
            })
        })
    }

    pub fn symbol_set(&self, fqn: &str) -> Option<SymbolSetDecl<'_>> {
        find_decl!(self, fqn, SymbolSetDecl, cells);
        None
    }

    pub fn symbol_sets(&self) -> impl Iterator<Item = SymbolSetDecl<'_>> + '_ {
        decl_iter_cells!(self, SymbolSetDecl)
    }

    /// Resolve a [`TypeRef`] to either its built-in tag or the user-declared
    /// [`TypeDecl`] / [`UnionDecl`] it points to. `Named` refs are validated
    /// at [`Document::open`], so the lookup never fails here.
    pub fn resolve<'a>(&'a self, t: &'a TypeRef) -> ResolvedType<'a> {
        match t {
            TypeRef::Builtin(b) => ResolvedType::Builtin(*b),
            TypeRef::Named(path) => {
                let fqn = self
                    .resolve_path(path)
                    .expect("named ref validated at Document::open");
                let fqn_dotted = fqn.join(".");
                if let Some(decl) = self.type_decl(&fqn_dotted) {
                    ResolvedType::Named(decl)
                } else if let Some(iface) = self.interface(&fqn_dotted) {
                    ResolvedType::Interface(iface)
                } else if let Some(union) = self.union_decl(&fqn_dotted) {
                    ResolvedType::Union(union)
                } else if let Some(ss) = self.symbol_set(&fqn_dotted) {
                    ResolvedType::SymbolSet(ss)
                } else {
                    ResolvedType::Connection(
                        self.connection_decl(&fqn_dotted)
                            .expect("named ref validated at Document::open"),
                    )
                }
            }
            TypeRef::Reference(inner) => ResolvedType::Reference(Box::new(self.resolve(inner))),
            TypeRef::List(inner) => ResolvedType::List(Box::new(self.resolve(inner))),
            TypeRef::Tensor { element, dims } => ResolvedType::Tensor {
                element: Box::new(self.resolve(element)),
                dims,
            },
            TypeRef::Function { params, return_ty } => ResolvedType::Function {
                params: params.iter().map(|p| self.resolve(p)).collect(),
                return_ty: Box::new(self.resolve(return_ty)),
            },
        }
    }

    fn compose_fqn(&self, name: &[String]) -> Vec<String> {
        let mut v = self.file_ns.clone();
        v.extend(name.iter().cloned());
        v
    }

    /// The set of fully-qualified names declared anywhere in the
    /// document (root + every eagerly-imported file), used as the
    /// resolution registry for type references. Built once per document
    /// (see the `ref_registry` field for why that's sound).
    fn ref_registry(&self) -> &HashSet<Vec<String>> {
        self.ref_registry.get_or_init(|| self.build_ref_registry())
    }

    fn build_ref_registry(&self) -> HashSet<Vec<String>> {
        let mut registry: HashSet<Vec<String>> = self
            .ast
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::TypeDecl(t) => Some(self.compose_fqn(&t.name)),
                ast::Item::InterfaceDecl(i) => Some(self.compose_fqn(&i.name)),
                ast::Item::UnionDecl(u) => Some(self.compose_fqn(&u.name)),
                ast::Item::SymbolSetDecl(s) => Some(self.compose_fqn(&s.name)),
                ast::Item::ConnectionDecl(c) => Some(self.compose_fqn(&c.name)),
                _ => None,
            })
            .collect();
        // Synthetic types live at the root namespace.
        for t in &self.synthetic_types {
            registry.insert(t.name.clone());
        }
        // Declarations from eagerly-imported files resolve too — e.g. a
        // connection whose endpoint type `&SvgBlock` is defined in an
        // imported schema file. Each source's symbol index already holds
        // FQNs with that file's namespace composed in.
        for src in self.all_sources() {
            for rec in src.symbols.iter() {
                if matches!(
                    rec.kind,
                    SymbolKind::TypeDecl
                        | SymbolKind::InterfaceDecl
                        | SymbolKind::UnionDecl
                        | SymbolKind::SymbolSetDecl
                        | SymbolKind::ConnectionDecl
                ) {
                    registry.insert(rec.fqn.split('.').map(str::to_string).collect());
                }
            }
        }
        registry
    }

    /// Run the name-resolution algorithm on `path` against this document's
    /// root file ns / aliases / wildcards / registry.
    fn resolve_path(&self, path: &[String]) -> Option<Vec<String>> {
        self.resolve_path_in(path, &self.file_ns)
    }

    /// Resolve `path` as if it were written in a source whose namespace
    /// is `file_ns`. This makes a bare reference resolve **within its
    /// declaring file's namespace first** — e.g. a stdlib type's
    /// `extends WdocBlock` (written in `namespace wdoc`) resolves to
    /// `wdoc.WdocBlock`, not the root namespace. The document's
    /// `use`-aliases/wildcards still apply (they only come from the root
    /// today).
    pub(crate) fn resolve_path_in(
        &self,
        path: &[String],
        file_ns: &[String],
    ) -> Option<Vec<String>> {
        let registry = self.ref_registry();
        // `self.wildcards` already includes every imported library's
        // namespace (added in `validate_document`), so a bare reference
        // to a stdlib type resolves through it.
        resolve_path(
            path,
            file_ns,
            &self.item_aliases,
            &self.ns_aliases,
            &self.wildcards,
            registry,
        )
    }

    /// Look up the type that schemas a block of the given kind. Bare
    /// kinds resolve from the document's own namespace (local
    /// declarations preferred), so a user `@block("process")` shadows an
    /// imported one of the same kind.
    pub fn block_schema(&self, kind: &str) -> Option<TypeDecl<'_>> {
        self.find_schema(BuiltinDecorator::Block, kind)
    }

    /// Namespace-aware block lookup: `qualifier` is the `::` namespace
    /// (empty for bare), `context_ns` the referencing site's namespace.
    pub(crate) fn block_schema_in(
        &self,
        qualifier: &[String],
        kind: &str,
        context_ns: &[String],
    ) -> Option<TypeDecl<'_>> {
        self.find_schema_ns(BuiltinDecorator::Block, qualifier, kind, context_ns)
    }

    /// Look up the type that schemas a decorator of the given name.
    pub fn decorator_schema(&self, name: &str) -> Option<TypeDecl<'_>> {
        self.find_schema(BuiltinDecorator::Decorator, name)
    }

    /// Look up the type that schemas a table of the given name, i.e.
    /// the first type carrying an `@table("name")` decorator.
    pub fn table_schema(&self, name: &str) -> Option<TypeDecl<'_>> {
        self.find_schema(BuiltinDecorator::Table, name)
    }

    /// Namespace-aware table lookup (see [`Document::block_schema_in`]).
    pub(crate) fn table_schema_in(
        &self,
        qualifier: &[String],
        name: &str,
        context_ns: &[String],
    ) -> Option<TypeDecl<'_>> {
        self.find_schema_ns(BuiltinDecorator::Table, qualifier, name, context_ns)
    }

    /// Look up the type carrying the `@document` decorator, if any.
    /// At most one is expected; if multiple are declared, this
    /// returns the first and `Document::schema_errors` surfaces a
    /// `MultipleDocumentSchemas` violation.
    pub fn doc_schema(&self) -> Option<TypeDecl<'_>> {
        self.find_all_decorated(BuiltinDecorator::Document)
            .into_iter()
            .next()
    }

    /// Every `@document` schema that governs `file_ns`, root-authored
    /// first. The effective document schema for a namespace is the
    /// *merge* of these: a top-level field/block is legal if any of
    /// them declares it. This lets a user declare their own
    /// `@document` at the root that composes with library `@document`
    /// schemas pulled in by imports (from any number of modules),
    /// instead of an imported schema "taking over" the root.
    pub(crate) fn doc_schemas_for_ns(&self, file_ns: &[String]) -> DocSchemas<'_> {
        // `type_decls()` walks the root source before imports, so
        // root-authored declarations already sort ahead of imported
        // ones — preserve that order so `field()`/`primary()` prefer
        // the root-authored schema.
        // A source is governed by the `@document` schemas declared in
        // its own namespace, plus any pulled in from an imported library
        // (`is_imported`) — importing a schema library is opting its
        // `@document` into your document. This is what lets an
        // un-namespaced (or differently-namespaced) user document compose
        // against the stdlib `Site` schema, which lives in `namespace
        // wdoc`. Root-authored declarations stay namespace-scoped.
        let schemas = self
            .find_all_decorated(BuiltinDecorator::Document)
            .into_iter()
            .filter(|t| t.file_ns() == file_ns || t.is_imported())
            .collect();
        DocSchemas { schemas }
    }

    /// Every type declaration carrying the named decorator. Used by
    /// the document-level validator to detect duplicate `@document`
    /// declarations.
    pub(crate) fn find_all_decorated(&self, dec: BuiltinDecorator) -> Vec<TypeDecl<'_>> {
        let dec_name = dec.as_str();
        self.type_decls()
            .filter(|t| t.decorators().any(|d| d.full_name() == dec_name))
            .collect()
    }

    /// `true` if any declared type carries `@block(kind)` or
    /// `@table(kind)`. Used to spot un-registered block kinds in the
    /// strict validator.
    pub(crate) fn is_registered_kind(&self, kind: &str) -> bool {
        self.block_schema(kind).is_some() || self.table_schema(kind).is_some()
    }

    /// The `wdoc_component` definition whose name (`@inline(0)` label)
    /// equals `name`, if any. A component is instantiated by its own
    /// name as a bare block; this resolves that instance kind back to its
    /// declarative definition (slots + body). Served from a once-built
    /// name → position index — expansion consults this for every nested
    /// block, so the previous per-call label-evaluating scan over all
    /// top-level blocks was O(blocks²) across a build.
    pub fn component_def(&self, name: &str) -> Option<Block<'_>> {
        if name.is_empty() {
            return None;
        }
        let index = self.component_index.get_or_init(|| {
            let mut map: HashMap<String, usize> = HashMap::new();
            for (i, b) in self.blocks().enumerate() {
                if b.kind() == "wdoc_component"
                    && let Some(label) = block_first_label(&b)
                {
                    // First declaration wins, matching `find` semantics.
                    map.entry(label).or_insert(i);
                }
            }
            map
        });
        let pos = *index.get(name)?;
        self.blocks().nth(pos)
    }

    /// `true` if `name` is the name of a declared `wdoc_component` — i.e.
    /// a legal bare-block instance kind.
    pub fn is_component_kind(&self, name: &str) -> bool {
        self.component_def(name).is_some()
    }

    /// Strict-mode validation: returns every schema violation across
    /// the document.
    ///
    /// At the top level: detects multiple `@document` declarations,
    /// top-level fields/blocks without a matching schema, and
    /// top-level block kinds that aren't registered via
    /// `@block`/`@table`. Each top-level block then has its
    /// `Block::schema_errors()` collected recursively.
    pub fn schema_errors(&self) -> Vec<EvalError> {
        use crate::error::SchemaViolationKind as Kind;
        use std::collections::BTreeMap;
        let mut out = Vec::new();

        // Group every `@document` type by its file_ns. Each namespace
        // is independent. Several `@document` schemas may govern one
        // namespace and they *compose* (a root-authored one plus any
        // library-provided ones from imports) — see
        // `doc_schemas_for_ns`. Only a second *root-authored*
        // `@document` in a namespace is an error: imported library
        // schemas merge silently, so importing several modules that
        // each ship a base document is fine.
        let doc_schemas = self.find_all_decorated(BuiltinDecorator::Document);
        let mut by_ns: BTreeMap<Vec<String>, Vec<TypeDecl<'_>>> = BTreeMap::new();
        for d in &doc_schemas {
            by_ns.entry(d.file_ns().to_vec()).or_default().push(*d);
        }
        for decls in by_ns.values() {
            for extra in decls.iter().filter(|d| !d.is_imported()).skip(1) {
                EvalError::push_schema_violation(
                    &mut out,
                    Kind::MultipleDocumentSchemas,
                    format!(
                        "type '{}' declares a second root @document schema \
                         (only one root-authored @document is allowed per namespace; \
                         imported library schemas merge automatically)",
                        extra.name()
                    ),
                    extra.span(),
                );
            }
        }

        // Duplicate kind detection for `@block`/`@table`/`@decorator`.
        // Two declarations sharing a kind string *within one namespace*
        // are ambiguous; across namespaces a `::` qualifier disambiguates,
        // so those are fine. Mirroring the `@document` rule, only a second
        // *root-authored* declaration in a (namespace, kind) group is an
        // error — imported library duplicates resolve first-wins silently.
        for dec in [
            BuiltinDecorator::Block,
            BuiltinDecorator::Table,
            BuiltinDecorator::Decorator,
        ] {
            let dec_name = dec.as_str();
            let mut by_kind: BTreeMap<(Vec<String>, String), Vec<TypeDecl<'_>>> = BTreeMap::new();
            for t in self.find_all_decorated(dec) {
                let Some(kind) = t.decorators().find_map(|d| {
                    if d.full_name() != dec_name {
                        return None;
                    }
                    match d.positional().ok()?.into_iter().next() {
                        Some(Value::Utf8(s)) => Some(s),
                        _ => None,
                    }
                }) else {
                    continue;
                };
                by_kind
                    .entry((t.file_ns().to_vec(), kind))
                    .or_default()
                    .push(t);
            }
            for ((_, kind), decls) in &by_kind {
                for extra in decls.iter().filter(|d| !d.is_imported()).skip(1) {
                    EvalError::push_schema_violation(
                        &mut out,
                        Kind::DuplicateBlockKind,
                        format!(
                            "type '{}' redeclares @{dec_name}(\"{kind}\") already declared \
                             in this namespace (kinds must be unique per namespace; \
                             qualify across namespaces with `::`)",
                            extra.name()
                        ),
                        extra.span(),
                    );
                }
            }
        }

        // Walk every source (the main file + every eagerly-imported
        // file). Each source's items are validated against the merged
        // `@document` schema for that source's namespace, if any.
        for src in self.all_sources() {
            let schemas = self.doc_schemas_for_ns(src.file_ns);

            // Pre-compute the merged schema's union-typed children
            // slots so structurally-matched blocks bypass the kind
            // check.
            let root_union_slots = schemas.union_slots();

            // Walk the top-level fields in this source.
            for f in iter_fields(src.items, src.cells, self, Scope::root()) {
                if has_schemaless(&f.ast.decorators) {
                    continue;
                }
                self.validate_root_field(&f, &schemas, &mut out);
            }

            // Walk the top-level blocks in this source.
            for b in iter_blocks(src.items, src.cells, self, src.file_ns, Scope::root()) {
                if has_schemaless(&b.ast.decorators) {
                    continue;
                }
                self.validate_root_block(&b, &schemas, &root_union_slots, &mut out);
            }
        }

        // Validate every union declaration: cycles in the extends
        // chain, duplicate variants across that chain, and structural
        // collisions between variant bodies.
        for u in self.union_decls() {
            out.extend(validate_union(self, u.ast));
        }

        // Top-level connection statements: dispatch and kind checks.
        for src in self.all_sources() {
            out.extend(crate::doc::schema_check::validate_connection_stmts(
                self,
                src.items,
                &Scope::root(),
            ));
        }

        out
    }

    /// Validate one top-level field against the merged `@document`
    /// schema(s) for its namespace, pushing any violation into `out`.
    /// The field is legal if any member schema declares it; the
    /// declaring schema (root-authored preferred) drives the type
    /// check. Split out of [`schema_errors`] for readability.
    fn validate_root_field(
        &self,
        f: &Field<'_>,
        schemas: &DocSchemas<'_>,
        out: &mut Vec<EvalError>,
    ) {
        use crate::error::SchemaViolationKind as Kind;
        if schemas.is_empty() {
            EvalError::push_schema_violation(
                out,
                Kind::NoDocumentSchema,
                format!("top-level field '{}' has no @document schema", f.name()),
                f.span(),
            );
            return;
        }
        let Some(declared) = schemas.field(f.name()) else {
            EvalError::push_schema_violation(
                out,
                Kind::UnknownField,
                format!(
                    "top-level field '{}' is not declared by @document schema '{}'",
                    f.name(),
                    schemas.names()
                ),
                f.span(),
            );
            return;
        };
        let Ok(v) = f.value() else {
            return;
        };
        if let TypeRef::Named(path) = declared.type_ref()
            && let Some(union_decl) = self.union_decl(&path.join("."))
        {
            if let Value::Variant { union, variant, .. } = v
                && union != &union_decl.ast.name
            {
                EvalError::push_schema_violation(
                    out,
                    Kind::VariantUnionMismatch,
                    format!(
                        "field '{}' declared as union '{}' but value is {}::{}",
                        f.name(),
                        union_decl.ast.name.join("."),
                        union.join("."),
                        variant,
                    ),
                    f.span(),
                );
            } else if let Value::Record { .. } = v {
                EvalError::push_schema_violation(
                    out,
                    Kind::VariantNoMatch,
                    format!(
                        "field '{}' declared as union '{}' but value is an \
                         un-inferred record (no variant matches its shape)",
                        f.name(),
                        union_decl.ast.name.join("."),
                    ),
                    f.span(),
                );
            }
        } else if !value_matches_type_ref(v, &self.resolve_alias(declared.type_ref())) {
            EvalError::push_schema_violation(
                out,
                Kind::FieldTypeMismatch,
                format!(
                    "field '{}' declared as {} but value is {}",
                    f.name(),
                    declared.type_ref(),
                    v.type_name(),
                ),
                f.span(),
            );
        } else if let Some(msg) = schema_check::constraint_violation(
            self,
            &declared.ast.decorators,
            declared.type_ref(),
            v,
        ) {
            EvalError::push_schema_violation(
                out,
                Kind::ConstraintViolation,
                format!("field '{}': {msg}", f.name()),
                f.span(),
            );
        }
    }

    /// Validate one top-level block: union dispatch, kind registration, and
    /// allowed-child placement under the merged `@document` schema(s) for the
    /// namespace. `root_union_slots` are those schemas' union-typed
    /// `@children` slots (a structurally-matched block bypasses the kind
    /// check). The block is legal if any member schema allows its kind.
    /// Split out of [`schema_errors`].
    fn validate_root_block(
        &self,
        b: &Block<'_>,
        schemas: &DocSchemas<'_>,
        root_union_slots: &[UnionDecl<'_>],
        out: &mut Vec<EvalError>,
    ) {
        use crate::error::SchemaViolationKind as Kind;
        let dispatched_through_union = root_union_slots
            .iter()
            .any(|u| variant_dispatch::block_to_variant(self, b, *u).is_ok());
        if dispatched_through_union {
            return;
        }
        if !self.is_registered_kind(b.kind()) {
            let mut msg = format!(
                "block kind '{}' has no @block or @table declaration",
                b.kind()
            );
            if !root_union_slots.is_empty() {
                let variants = format_union_variants_hint(self, root_union_slots);
                if !variants.is_empty() {
                    msg.push_str(&format!(" (nearby @children union accepts: {variants})"));
                }
            }
            EvalError::push_schema_violation(out, Kind::UnregisteredKind, msg, b.span());
            return;
        }
        if !schemas.is_empty() {
            let allowed = schemas.allowed_child_kinds();
            if !allowed.iter().any(|k| k == b.kind()) {
                EvalError::push_schema_violation(
                    out,
                    Kind::DisallowedChild,
                    format!(
                        "block kind '{}' is not allowed at the document root by @document schema '{}'",
                        b.kind(),
                        schemas.names()
                    ),
                    b.span(),
                );
            }
        } else {
            EvalError::push_schema_violation(
                out,
                Kind::NoDocumentSchema,
                format!("top-level block '{}' has no @document schema", b.kind()),
                b.span(),
            );
        }
        for e in b.schema_errors() {
            out.push(e.clone());
        }
    }

    /// Every type declaration carrying `@dec(value)`, in `type_decls()`
    /// order (root source before imports). Served from a once-built
    /// positional index — block-kind lookups run this for every schema
    /// resolution, so the previous full scan (evaluating each decl's
    /// decorator args per call) dominated large builds.
    fn schema_candidates(&self, dec: BuiltinDecorator, value: &str) -> Vec<TypeDecl<'_>> {
        let index = self.schema_index.get_or_init(|| self.build_schema_index());
        let key = (dec.as_str().to_string(), value.to_string());
        let Some(positions) = index.get(&key) else {
            return Vec::new();
        };
        // `positions` is sorted (built in iteration order): walk the
        // decl iterator once, picking off matches until the last one.
        let mut want = positions.iter().copied().peekable();
        let mut out = Vec::with_capacity(positions.len());
        for (i, t) in self.type_decls().enumerate() {
            match want.peek() {
                Some(&p) if p == i => {
                    out.push(t);
                    want.next();
                }
                Some(_) => {}
                None => break,
            }
        }
        out
    }

    /// `(decorator name, first positional string arg)` → positions in
    /// `type_decls()` order, for every decorator whose first positional
    /// evaluates to a string — exactly the pairs `schema_candidates`
    /// matches on.
    fn build_schema_index(&self) -> HashMap<(String, String), Vec<usize>> {
        let mut map: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (i, t) in self.type_decls().enumerate() {
            for d in t.decorators() {
                let Ok(args) = d.positional() else { continue };
                let Some(Value::Utf8(v)) = args.into_iter().next() else {
                    continue;
                };
                map.entry((d.full_name(), v)).or_default().push(i);
            }
        }
        map
    }

    /// Resolve a `::` namespace qualifier to the set of concrete
    /// namespaces it may name: the qualifier itself, plus any
    /// single-segment `use … as` namespace-alias expansion.
    fn resolve_ns_qualifier(&self, qualifier: &[String]) -> Vec<Vec<String>> {
        let mut out = vec![qualifier.to_vec()];
        if qualifier.len() == 1
            && let Some(expanded) = self.ns_aliases.get(&qualifier[0])
            && !out.contains(expanded)
        {
            out.push(expanded.clone());
        }
        out
    }

    /// Resolve the type that schemas a decorated kind, honouring
    /// namespaces. `qualifier` is the namespace written before the kind
    /// with `::` (empty for a bare kind); `context_ns` is the namespace
    /// of the referencing site.
    ///
    /// - **Qualified `N::kind`** selects the candidate whose owning
    ///   namespace is `N` (after alias expansion).
    /// - **Bare `kind`** prefers a declaration in `context_ns`, then a
    ///   root-authored one, then the first match — preserving the
    ///   historical behaviour when there is no collision.
    fn find_schema_ns(
        &self,
        dec: BuiltinDecorator,
        qualifier: &[String],
        value: &str,
        context_ns: &[String],
    ) -> Option<TypeDecl<'_>> {
        let candidates = self.schema_candidates(dec, value);
        if candidates.is_empty() {
            return None;
        }
        if !qualifier.is_empty() {
            let targets = self.resolve_ns_qualifier(qualifier);
            return candidates
                .into_iter()
                .find(|t| targets.iter().any(|ns| ns.as_slice() == t.file_ns()));
        }
        let idx = candidates
            .iter()
            .position(|t| t.file_ns() == context_ns)
            .or_else(|| candidates.iter().position(|t| !t.is_imported()))
            .unwrap_or(0);
        candidates.into_iter().nth(idx)
    }

    /// Bare-kind, root-namespace-context lookup. Retained for the many
    /// callers that resolve a kind from the document's own perspective.
    fn find_schema(&self, dec: BuiltinDecorator, value: &str) -> Option<TypeDecl<'_>> {
        self.find_schema_ns(dec, &[], value, &self.file_ns)
    }

    /// The namespace this document's root source declares (empty for the
    /// global namespace).
    pub(crate) fn file_ns(&self) -> &[String] {
        &self.file_ns
    }
}

/// A connection-statement operand resolved to a literal block: its
/// identifying value plus enough namespace context to look up the
/// block's schema the way [`Block::schema`] would.
pub(crate) struct ConnOperand {
    /// The identifying label / `id` value.
    pub(crate) value: Value,
    /// The block's kind as written (`ast::Block.kind`).
    pub(crate) kind: String,
    /// The `::` namespace qualifier at the instance site (empty for a
    /// bare kind).
    pub(crate) kind_ns: Vec<String>,
    /// Namespace of the file the matched block lives in.
    pub(crate) file_ns: Vec<String>,
}

/// Recursively search a slice of items for an `Item::Block` that
/// identifies as `name`. `file_ns` is the namespace of the file the
/// items live in. Identity sources, in priority order:
///
///   1. the block's first label (evaluated as a literal)
///   2. a field named `id` whose value evaluates to the name
///   3. nested blocks reachable through the block's own `items`
///
/// Returns the identifying value plus the block's kind and namespace
/// context so callers can dispatch on the block's declared type.
fn match_block_label_in_items(
    doc: &Document,
    items: &[ast::Item],
    cells: &[ItemCells],
    file_ns: &[String],
    name: &str,
) -> Option<ConnOperand> {
    let operand = |value: Value, b: &ast::Block| ConnOperand {
        value,
        kind: b.kind.clone(),
        kind_ns: b.kind_ns.clone(),
        file_ns: file_ns.to_vec(),
    };
    for (item, cell) in items.iter().zip(cells) {
        match (item, &cell.kind) {
            (ast::Item::Block(b), ItemCellKind::Block { items: bcells, .. }) => {
                if let Some(v) = match_block_first_label(doc, b, name) {
                    return Some(operand(v, b));
                }
                if let Some(v) = match_block_id_field(doc, b, name) {
                    return Some(operand(v, b));
                }
                // Nested blocks live in the same file as their parent.
                if let Some(found) =
                    match_block_label_in_items(doc, &b.items, bcells, file_ns, name)
                {
                    return Some(found);
                }
            }
            // An in-block `import` splices its top-level blocks into this
            // scope, so a connection endpoint can name a block that lives
            // in the imported fragment. Force the lazy load and recurse
            // with the imported file's own namespace.
            (
                ast::Item::Import(_),
                ItemCellKind::Import {
                    path,
                    system,
                    base_dir,
                    path_span,
                    loaded,
                },
            ) => {
                let li = loaded.get_or_init(|| {
                    load_import_lazily(path, base_dir.as_deref(), *system, *path_span, doc.loader())
                });
                if let Ok(li) = li
                    && let Some(found) =
                        match_block_label_in_items(doc, &li.items, &li.cells, &li.file_ns, name)
                {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// `true` when a concrete block's [`TypeDecl`] satisfies a connection
/// schema's declared source or destination FQN. Direct FQN equality
/// wins; otherwise we walk the type's `extends` chain so connections
/// declared against an interface or supertype admit any conforming
/// concrete block.
pub(crate) fn connection_type_matches(decl: &TypeDecl<'_>, target_fqn: Option<&str>) -> bool {
    let Some(target) = target_fqn else {
        return false;
    };
    if decl.full_name() == target {
        return true;
    }
    decl.is_descendant_of(target)
}

thread_local! {
    /// Set while a connection operand's identifying block label is being
    /// evaluated. `Document::resolve_root` consults it to suppress
    /// `@connections` projection during that window, breaking what would
    /// otherwise be unbounded recursion (operand → label eval → projection
    /// → operand → …).
    static RESOLVING_CONN_OPERAND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// RAII guard that sets [`RESOLVING_CONN_OPERAND`] for its lifetime and
/// restores the previous value on drop (so nested operand resolution
/// stays correct).
struct ConnOperandGuard(bool);

impl ConnOperandGuard {
    fn enter() -> Self {
        let prev = RESOLVING_CONN_OPERAND.with(|f| f.replace(true));
        Self(prev)
    }
}

impl Drop for ConnOperandGuard {
    fn drop(&mut self) {
        RESOLVING_CONN_OPERAND.with(|f| f.set(self.0));
    }
}

fn match_block_first_label(doc: &Document, b: &ast::Block, name: &str) -> Option<Value> {
    let first = b.labels.first()?;
    let v = doc.eval_in_scope(first, &Scope::root()).ok()?;
    let matches = match &v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s == name,
        _ => false,
    };
    if matches { Some(v) } else { None }
}

fn match_block_id_field(doc: &Document, b: &ast::Block, name: &str) -> Option<Value> {
    for it in &b.items {
        let ast::Item::Field(f) = it else { continue };
        if f.name != "id" {
            continue;
        }
        let v = doc.eval_literal(&f.expr).ok()?;
        let matches = match &v {
            Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s == name,
            _ => false,
        };
        if matches {
            return Some(v);
        }
    }
    None
}

fn materialise_dataref(dr: crate::data::DataRef<'_>, span: Span) -> Result<Value, EvalError> {
    use crate::data::DataKind;
    match dr.inner() {
        DataKind::Field(f) => f.value().cloned().map_err(|e| e.clone()),
        other => Err(EvalError::not_a_leaf(describe_datakind(other), span)),
    }
}

/// Like [`materialise_dataref`] but, for non-leaf targets, returns a
/// `Value::DataPath` carrying the source-level segments instead of
/// erroring with `NotALeaf`. Used at every site that resolves an
/// identifier / member chain so reflective builtins can keep walking
/// the document tree from inside WCL code.
fn materialise_dataref_or_path(
    dr: crate::data::DataRef<'_>,
    segments: Vec<String>,
    _span: Span,
) -> Result<Value, EvalError> {
    use crate::data::DataKind;
    use crate::doc::views::DeclName;
    match dr.inner() {
        DataKind::Field(f) => f.value().cloned().map_err(|e| e.clone()),
        DataKind::VariantValue(v) => Ok(v.clone()),
        DataKind::VariantValueList(vs) => Ok(Value::List(vs.clone())),
        other => {
            // A handle to a *declaration* carries the declaration's FQN
            // segments, not the source-written ones, so the path stays
            // resolvable when the value crosses into another namespace
            // (e.g. a `type = LibModel` slot binding consumed inside the
            // stdlib's `namespace wdoc` component bodies). Child kinds
            // (type fields, variants, symbols) keep the source segments;
            // they resolve through the namespace-aware lookup instead.
            let segments = match other {
                DataKind::Type(t) => t.fqn_segments(),
                DataKind::Interface(i) => i.fqn_segments(),
                DataKind::Union(u) => u.fqn_segments(),
                DataKind::Symbols(s) => s.fqn_segments(),
                _ => segments,
            };
            Ok(Value::DataPath {
                kind: describe_datakind(other).to_string(),
                segments,
            })
        }
    }
}

/// Like [`materialise_dataref_or_path`] but additionally reifies a
/// `@children`/`@child`/`@table` projection (block / block list / table)
/// into ordinary record / list values, so a bare reference to such a slot
/// is consumable by builtins (`len`, `map`, …), arithmetic, and
/// `wdoc_repeater`'s `each`. Used by the bare-identifier / member-access
/// evaluation path. The `&T`-reference deref path keeps the plain
/// `materialise_dataref_or_path` behaviour (a `Value::DataPath` handle) so
/// reflective builtins can keep walking the source.
fn materialise_dataref_value(
    dr: crate::data::DataRef<'_>,
    segments: Vec<String>,
    span: Span,
) -> Result<Value, EvalError> {
    if let Some(v) = views::dataref_to_value(&dr) {
        return v;
    }
    materialise_dataref_or_path(dr, segments, span)
}

/// Best-effort projection of an expression into the dotted name it
/// addresses. Returns `None` for anything more complex than identifier
/// / member / paren chains so the call site falls back to the strict
/// `NotALeaf` path. `self`/`parent`/calls/operators have no static
/// name and intentionally return `None`.
fn expr_to_path_segments(expr: &ast::Expr) -> Option<Vec<String>> {
    use ast::Expr as E;
    match expr {
        E::Identifier(s, _) => Some(vec![s.clone()]),
        E::Member { recv, name, .. } => {
            let mut segs = expr_to_path_segments(recv)?;
            segs.push(name.clone());
            Some(segs)
        }
        E::Paren { inner, .. } => expr_to_path_segments(inner),
        _ => None,
    }
}

fn describe_datakind(k: &crate::data::DataKind<'_>) -> &'static str {
    use crate::data::DataKind;
    match k {
        DataKind::Document(_) => "document",
        DataKind::Field(_) => "field",
        DataKind::Block(_) => "block",
        DataKind::BlockList(_) => "block list",
        DataKind::Table(_) => "table",
        DataKind::Type(_) => "type",
        DataKind::Interface(_) => "interface",
        DataKind::TypeField(_) => "type field",
        DataKind::Union(_) => "union",
        DataKind::Variant(_) => "variant",
        DataKind::Symbols(_) => "symbol set",
        DataKind::Symbol(_) => "symbol",
        DataKind::VariantValue(_) => "variant value",
        DataKind::VariantValueList(_) => "variant value list",
    }
}

/// Light value-vs-declared-type check used by structural variant
/// validation. Conservative: returns `true` only for the cases we know
/// how to compare. Named/Reference/Function/Tensor/List types are
/// accepted permissively — a full value-vs-type framework would need
/// runtime type tags on every Value, which the language deliberately
/// avoids; structural shape checks plus declaration-time validation
/// are intended to cover the rest.
/// First label of a block, as a string (identifier / utf8 / ascii).
/// `None` if the block has no labels or the first isn't string-like.
/// Used to resolve a `wdoc_component`'s `@inline(0)` name.
fn block_first_label(b: &Block<'_>) -> Option<String> {
    match b.labels().ok()?.into_iter().next()? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Some(s),
        _ => None,
    }
}

pub(crate) fn value_matches_type_ref(value: &Value, ty: &TypeRef) -> bool {
    use crate::value::BuiltinType as B;
    match (value, ty) {
        (Value::Bool(_), TypeRef::Builtin(B::Bool)) => true,
        (Value::I8(_), TypeRef::Builtin(B::I8)) => true,
        (Value::I16(_), TypeRef::Builtin(B::I16)) => true,
        (Value::I32(_), TypeRef::Builtin(B::I32)) => true,
        (Value::I64(_), TypeRef::Builtin(B::I64)) => true,
        (Value::I128(_), TypeRef::Builtin(B::I128)) => true,
        (Value::Isize(_), TypeRef::Builtin(B::Isize)) => true,
        (Value::U8(_), TypeRef::Builtin(B::U8)) => true,
        (Value::U16(_), TypeRef::Builtin(B::U16)) => true,
        (Value::U32(_), TypeRef::Builtin(B::U32)) => true,
        (Value::U64(_), TypeRef::Builtin(B::U64)) => true,
        (Value::U128(_), TypeRef::Builtin(B::U128)) => true,
        (Value::Usize(_), TypeRef::Builtin(B::Usize)) => true,
        (Value::F32(_), TypeRef::Builtin(B::F32)) => true,
        (Value::F64(_), TypeRef::Builtin(B::F64)) => true,
        (Value::Utf8(_), TypeRef::Builtin(B::Utf8)) => true,
        (Value::Ascii(_), TypeRef::Builtin(B::Ascii)) => true,
        (Value::Utf16(_), TypeRef::Builtin(B::Utf16)) => true,
        (Value::Utf32(_), TypeRef::Builtin(B::Utf32)) => true,
        (Value::Symbol(_), TypeRef::Builtin(B::Symbol)) => true,
        (Value::Identifier(_), TypeRef::Builtin(B::Identifier)) => true,
        // A string for an `identifier` field is a tolerated authoring
        // form (`set = "platformer"`): consumers read Identifier and
        // Utf8/Ascii interchangeably for id-typed fields.
        (Value::Utf8(_) | Value::Ascii(_), TypeRef::Builtin(B::Identifier)) => true,
        // A symbol against a named type is (typically) a `symbol_set`
        // member — checking membership would need the declaration, which
        // `Value` doesn't carry, so stay permissive.
        (Value::Symbol(_), TypeRef::Named(_)) => true,
        (Value::None, _) => false, // None doesn't satisfy any concrete type
        // Numeric values satisfy any numeric builtin type: the evaluator
        // promotes numerics (an `f64` field authored as `520` holds an
        // i64 literal), so an exact-variant check here would flag values
        // the eval path accepts.
        (v, TypeRef::Builtin(b)) if v.is_numeric() && b.is_numeric() => true,
        // Variant value against a named union type: compare FQN.
        (Value::Variant { union, .. }, TypeRef::Named(path)) => path_matches_suffix(path, union),
        // Record value against a named (non-union) type. Builtin-produced
        // records (e.g. `@connections` projections) carry the producing
        // declaration's FQN in `ty` — compare it. Bare record literals
        // (`ty` empty) stay permissive: matching them by shape would need
        // the declaration, which `Value` doesn't carry (mirrors the
        // tensor / function pass-through below).
        (Value::Record { ty, .. }, TypeRef::Named(path)) => {
            ty.is_empty() || path_matches_suffix(path, ty)
        }
        // Lists check element type recursively.
        (Value::List(items), TypeRef::List(inner)) => {
            items.iter().all(|el| value_matches_type_ref(el, inner))
        }
        // Tensors / functions / references stay permissive — strict
        // checks here would need richer type information than we
        // currently carry on `Value`.
        (Value::Tensor { .. }, TypeRef::Tensor { .. }) => true,
        (Value::Function(_), TypeRef::Function { .. }) => true,
        // `&T` fields evaluate to a `Value::DataPath` (lazy navigator).
        (Value::DataPath { .. }, TypeRef::Reference(_)) => true,
        _ => false,
    }
}

/// Render a comma-separated list of all variant names across the
/// given union slots — used to enrich `UnregisteredKind` errors with
/// a "did you mean" hint when a nearby `@children(SomeUnion)` field
/// exists.
fn format_union_variants_hint(doc: &Document, slots: &[UnionDecl<'_>]) -> String {
    let mut names: Vec<String> = Vec::new();
    for u in slots {
        if let Ok(effective) = doc.effective_variants_of(u.ast) {
            for v in effective {
                names.push(format!("{}::{}", u.ast.name.join("."), v.name));
            }
        }
    }
    names.join(", ")
}

/// Declaration-time validation for a single union: cycles, duplicate
/// variant names across the `extends` chain, and structural-shape
/// collisions between variant bodies that would make dispatch
/// ambiguous.
fn validate_union(doc: &Document, u: &ast::UnionDecl) -> Vec<EvalError> {
    use crate::error::SchemaViolationKind as Kind;
    let mut out = Vec::new();
    let effective = match doc.effective_variants_of(u) {
        Ok(v) => v,
        Err(e) => {
            out.push(e);
            return out;
        }
    };
    // Duplicate variant names across the chain: walk own + all parents
    // and report any name appearing more than once. effective_variants
    // dedups silently — we re-walk the raw lists here to catch.
    let mut seen: std::collections::HashMap<String, ast::Span> = Default::default();
    fn walk(
        doc: &Document,
        u: &ast::UnionDecl,
        seen: &mut std::collections::HashMap<String, ast::Span>,
        out: &mut Vec<EvalError>,
        visiting: &mut std::collections::HashSet<String>,
    ) {
        use crate::error::SchemaViolationKind as Kind;
        let key = u.name.join(".");
        if visiting.contains(&key) {
            return;
        }
        visiting.insert(key);
        for parent_path in &u.extends {
            let resolved = doc
                .resolve_path_in(parent_path, &doc.file_ns)
                .map(|p| p.join("."))
                .unwrap_or_else(|| parent_path.join("."));
            if let Some(p) = doc
                .union_decl(&resolved)
                .or_else(|| doc.union_decl(&parent_path.join(".")))
            {
                walk(doc, p.ast, seen, out, visiting);
            }
        }
        for v in &u.variants {
            if let Some(prev) = seen.get(&v.name) {
                EvalError::push_schema_violation(
                    out,
                    Kind::DuplicateVariant,
                    format!(
                        "variant '{}' is declared more than once in union '{}' (first at offset {})",
                        v.name,
                        u.name.join("."),
                        prev.start,
                    ),
                    v.span,
                );
            } else {
                seen.insert(v.name.clone(), v.span);
            }
        }
    }
    walk(
        doc,
        u,
        &mut seen,
        &mut out,
        &mut std::collections::HashSet::new(),
    );
    // Structural-shape collisions among effective variants. Each pair
    // is checked once; collisions are flagged on the second offender.
    for i in 0..effective.len() {
        for j in (i + 1)..effective.len() {
            if variant_bodies_collide(&effective[i].body, &effective[j].body) {
                EvalError::push_schema_violation(
                    &mut out,
                    Kind::VariantShapeCollision,
                    format!(
                        "variants '{}' and '{}' in union '{}' have identical bodies",
                        effective[i].name,
                        effective[j].name,
                        u.name.join("."),
                    ),
                    effective[j].span,
                );
            }
        }
    }
    out
}

/// Bodies "collide" when they're indistinguishable for dispatch:
/// same set of record-field (name, type) pairs, or identical Unit /
/// TypeRef / InterfaceRef references.
fn variant_bodies_collide(a: &ast::VariantBody, b: &ast::VariantBody) -> bool {
    use ast::VariantBody as VB;
    match (a, b) {
        (VB::Unit, VB::Unit) => true,
        (VB::TypeRef { ty: a, .. }, VB::TypeRef { ty: b, .. }) => a == b,
        (VB::InterfaceRef { iface: a, .. }, VB::InterfaceRef { iface: b, .. }) => a == b,
        (VB::Record { fields: af, .. }, VB::Record { fields: bf, .. }) => {
            if af.len() != bf.len() {
                return false;
            }
            let mut a_sorted: Vec<(&String, &TypeRef)> =
                af.iter().map(|f| (&f.name, &f.ty)).collect();
            let mut b_sorted: Vec<(&String, &TypeRef)> =
                bf.iter().map(|f| (&f.name, &f.ty)).collect();
            a_sorted.sort_by_key(|(n, _)| (*n).clone());
            b_sorted.sort_by_key(|(n, _)| (*n).clone());
            a_sorted == b_sorted
        }
        _ => false,
    }
}

fn path_matches_suffix(pat_path: &[String], union_fqn: &[String]) -> bool {
    if pat_path.len() > union_fqn.len() {
        return false;
    }
    let offset = union_fqn.len() - pat_path.len();
    union_fqn[offset..] == *pat_path
}

fn span_of(expr: &ast::Expr) -> Span {
    use ast::Expr as E;
    match expr {
        E::Bool(_)
        | E::I8(_)
        | E::I16(_)
        | E::I32(_)
        | E::I64(_)
        | E::I128(_)
        | E::Isize(_)
        | E::U8(_)
        | E::U16(_)
        | E::U32(_)
        | E::U64(_)
        | E::U128(_)
        | E::Usize(_)
        | E::F32(_)
        | E::F64(_)
        | E::Utf8(_)
        | E::Ascii(_)
        | E::Utf16(_)
        | E::Utf32(_)
        | E::Symbol(_)
        | E::None => Span::new(0, 0),
        E::Identifier(_, span) => *span,
        E::Function(f) => f.span,
        E::Call { span, .. }
        | E::Binary { span, .. }
        | E::Unary { span, .. }
        | E::Block { span, .. }
        | E::Paren { span, .. }
        | E::ListLit { span, .. }
        | E::Member { span, .. }
        | E::If { span, .. }
        | E::IfLet { span, .. }
        | E::Match { span, .. }
        | E::Try { span, .. }
        | E::Variant { span, .. }
        | E::Record { span, .. }
        | E::InterpolatedString { span, .. } => *span,
        E::SelfKw(s) | E::ParentKw(s) => *s,
    }
}

pub(crate) fn span_to_miette(span: Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.len().max(1))
}

/// Pointer-identity walk used by `Field::source_path`. Returns
/// `true` if `target_field` lives directly in any `Item::Field` of
/// `items`, or inside an `Item::Block`'s nested items. Lazy in-block
/// imports are searched separately by [`find_lazy_in_blocks`] so the
/// path-bearing variant can return the import's `PathBuf`.
/// `true` when `target` points at a [`ast::Block`] reachable from `items`
/// (recursing through nested blocks). Block identity is by pointer, so
/// this only matches blocks backed by on-disk AST — synthesised blocks
/// (table rows, computed children, component expansions) aren't found.
fn block_in_items(items: &[ast::Item], target: *const ast::Block) -> bool {
    for item in items {
        if let ast::Item::Block(b) = item {
            if std::ptr::eq(b, target) {
                return true;
            }
            if block_in_items(&b.items, target) {
                return true;
            }
        }
    }
    false
}

/// The miette source (name + text) of `imp` (or a transitive eager
/// import) when it declares the block `target` points into.
fn named_source_in_import(
    imp: &cells::LoadedImport,
    target: *const ast::Block,
) -> Option<NamedSource<String>> {
    if block_in_items(&imp.items, target) {
        return Some(NamedSource::new(
            imp.path.display().to_string(),
            imp.source.clone(),
        ));
    }
    for child in &imp.eager_imports {
        if let Some(src) = named_source_in_import(child, target) {
            return Some(src);
        }
    }
    None
}

fn field_in_items(items: &[ast::Item], target: *const ast::Field, cells: &[ItemCells]) -> bool {
    for (i, item) in items.iter().enumerate() {
        match item {
            ast::Item::Field(f) => {
                if std::ptr::eq(f, target) {
                    return true;
                }
            }
            ast::Item::Block(b) => {
                let block_cells = match cells.get(i).map(|c| &c.kind) {
                    Some(ItemCellKind::Block { items: inner, .. }) => inner.as_slice(),
                    _ => &[],
                };
                if field_in_items(&b.items, target, block_cells) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Recursively search a [`LoadedImport`] (and any in-block lazy
/// imports it owns) for `target`. The match's enclosing import's
/// `path` is returned via the first enclosing scope that owns the
/// item — never the deepest, so a field in shared.wcl reports
/// shared.wcl even if it's inside a block.
fn find_in_import(imp: &cells::LoadedImport, target: *const ast::Field) -> Option<&Path> {
    if field_in_items(&imp.items, target, &imp.cells) {
        return Some(&imp.path);
    }
    if let Some(p) = find_lazy_in_blocks(&imp.items, &imp.cells, target) {
        return Some(p);
    }
    for child in &imp.eager_imports {
        if let Some(p) = find_in_import(child, target) {
            return Some(p);
        }
    }
    None
}

/// Like [`find_in_import`] but returns the import's `file_ns`.
fn find_field_ns_in_import(
    imp: &cells::LoadedImport,
    target: *const ast::Field,
) -> Option<&[String]> {
    if field_in_items(&imp.items, target, &imp.cells) {
        return Some(&imp.file_ns);
    }
    if let Some(ns) = find_lazy_field_ns_in_blocks(&imp.items, &imp.cells, target) {
        return Some(ns);
    }
    for child in &imp.eager_imports {
        if let Some(ns) = find_field_ns_in_import(child, target) {
            return Some(ns);
        }
    }
    None
}

/// Like [`find_lazy_in_blocks`] but returns the originating import's
/// `file_ns` rather than its path.
fn find_lazy_field_ns_in_blocks<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    target: *const ast::Field,
) -> Option<&'a [String]> {
    for (i, item) in items.iter().enumerate() {
        let Some(cell) = cells.get(i) else { continue };
        if let ast::Item::Block(b) = item {
            let block_cells = match &cell.kind {
                ItemCellKind::Block { items: inner, .. } => inner.as_slice(),
                _ => continue,
            };
            for (j, inner_item) in b.items.iter().enumerate() {
                let Some(inner_cell) = block_cells.get(j) else {
                    continue;
                };
                if let ast::Item::Import(_) = inner_item
                    && let ItemCellKind::Import { loaded, .. } = &inner_cell.kind
                    && let Some(Ok(li)) = loaded.get()
                    && let Some(ns) = find_field_ns_in_import(li, target)
                {
                    return Some(ns);
                }
            }
            if let Some(ns) = find_lazy_field_ns_in_blocks(&b.items, block_cells, target) {
                return Some(ns);
            }
        }
    }
    None
}

/// Walk `items`+`cells` looking for `ItemCellKind::Import` cells
/// whose lazy `loaded` slot has been forced. Each forced
/// `LoadedImport` is searched via [`find_in_import`].
fn find_lazy_in_blocks<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    target: *const ast::Field,
) -> Option<&'a Path> {
    for (i, item) in items.iter().enumerate() {
        let Some(cell) = cells.get(i) else { continue };
        if let ast::Item::Block(b) = item {
            let block_cells = match &cell.kind {
                ItemCellKind::Block { items: inner, .. } => inner.as_slice(),
                _ => continue,
            };
            // Lazy `import` statements live in this block's cells:
            // check those first, then recurse into nested blocks.
            for (j, inner_item) in b.items.iter().enumerate() {
                let Some(inner_cell) = block_cells.get(j) else {
                    continue;
                };
                if let ast::Item::Import(_) = inner_item
                    && let ItemCellKind::Import { loaded, .. } = &inner_cell.kind
                    && let Some(Ok(li)) = loaded.get()
                    && let Some(p) = find_in_import(li, target)
                {
                    return Some(p);
                }
            }
            if let Some(p) = find_lazy_in_blocks(&b.items, block_cells, target) {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
fn laxify_for_tests(src: &str) -> String {
    // Parse the source once with an empty environment to identify
    // the offsets of top-level `Item::Field` and `Item::Block`
    // name tokens, then insert `@schemaless ` before each. The
    // resulting source parses identically but exempts every
    // top-level value from strict-validation.
    let Ok(d) = Document::open_with(src, "tmp-laxify", &Environment::empty()) else {
        return src.to_string();
    };
    // Only laxify top-level fields. Tests that use un-schema'd
    // top-level blocks are expected to write `@schemaless` (or a
    // real `@block` declaration) explicitly — there are far fewer
    // of them, and most snippets that use blocks already declare
    // matching `@block` schemas inline.
    let mut insertions: Vec<usize> = Vec::new();
    for item in &d.ast.items {
        if let ast::Item::Field(f) = item
            && !has_schemaless(&f.decorators)
        {
            insertions.push(f.span.start);
        }
    }
    if insertions.is_empty() {
        return src.to_string();
    }
    insertions.sort_unstable();
    let mut out = String::with_capacity(src.len() + insertions.len() * 12);
    let mut cursor = 0;
    for pos in insertions {
        out.push_str(&src[cursor..pos]);
        out.push_str("@schemaless ");
        cursor = pos;
    }
    out.push_str(&src[cursor..]);
    out
}

#[cfg(test)]
mod tests;
