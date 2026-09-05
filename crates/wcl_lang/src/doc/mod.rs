//! The document model: opening a file, reading it, and checking it.
//!
//! This file holds the [`Document`] itself — the parsed sources, the
//! caches that make repeated reads cheap, and the read API a host calls:
//! [`get`](Document::get), [`field`](Document::field),
//! [`block`](Document::block) and the iterators beside them. Everything
//! a read has to *do* lives in a submodule:
//!
//! - **Opening** — [`open`] constructs one, [`validate`] resolves its
//!   namespace and `use` aliases, [`imports`] pulls in other files, and
//!   [`loader`] is how any of them reach the disk.
//! - **Reading** — [`views`] are the borrowed wrappers a consumer gets
//!   back, [`cells`] the caches behind them, [`lookup`] and [`scope`]
//!   the name resolution, [`provenance`] which file declared what.
//! - **Evaluating** — [`eval`] forces a field, [`eval_ops`] applies the
//!   operators, [`match_pat`] matches a pattern, [`connections`]
//!   resolves the two ends of a `->`.
//! - **Types** — [`types`] answers what a type name points at and what
//!   inhabits it.
//! - **Checking** — [`schema_lookup`] finds the schema governing a
//!   thing, [`schema_check`] checks one block against it, [`decorators`]
//!   checks a decorator, and [`strict`] runs the whole document through
//!   in one pass for `wcl check`.
//!
//! A `Document` is immutable once opened, so every one of those caches
//! is safe to fill lazily and never invalidate — which is why the
//! editing path ([`crate::edit`]) is a separate parse rather than a
//! mutable view of this one.

use std::path::Path;

use miette::{NamedSource, SourceSpan};

use std::collections::{HashMap, HashSet};

mod cells;
mod connections;
mod decorators;
mod eval;
mod eval_ops;
mod imports;
mod loader;
mod lookup;
mod match_pat;
mod open;
mod provenance;
mod schema_check;
mod schema_lookup;
mod scope;
mod strict;
mod types;
mod validate;
mod views;
pub use imports::{SYSTEM_IMPORT_ROOT, system_import_key};
pub use loader::{FileLoader, Registry, disk_loader, overlay_loader};
pub use types::{FieldShape, ResolvedType};
pub use views::{
    Block, ChildKind, Connection, ConnectionDecl, DataKind, DataRef, DeclName, DeclaresKind,
    Decorator, Field, InterfaceDecl, NamedArg, RowView, SymbolEntry, SymbolSetDecl, TableView,
    TypeDecl, TypeField, UnionDecl, UnionVariant, UseDeclView, UseFormView, UseItem,
    VariantBodyView,
};
pub(crate) use views::{BuiltinDecorator, LetView, UnionChildKind};

use crate::ast::{self, Span};
#[cfg(test)]
use crate::ast::{BuiltinType, TensorDim};
use crate::diagnostics::EvalError;
use crate::environment::Environment;
use crate::symbols::{SymbolIndex, SymbolKind, SymbolRecord};
use crate::value::Value;
use cells::{BlockCells, ItemCellKind, ItemCells, LoadedImport};
use connections::RESOLVING_CONN_OPERAND;
pub(crate) use connections::{ConnOperand, connection_type_matches};
use lookup::{find_block, find_field, find_let, iter_blocks, iter_fields};
use schema_check::has_schemaless;
use schema_lookup::{DeclLoc, DeclaredKind, block_first_label};
use scope::Scope;

/// An opened document: a parsed source, its imports, and the caches that
/// make repeated reads cheap.
///
/// A `Document` is immutable once opened. Field values are evaluated on
/// first read and memoised, so forcing the same field twice costs one
/// evaluation, and a field that fails reports the same error every time.
/// Everything a consumer reads — fields, blocks, type and union
/// declarations — comes back as a borrowed view, not a copy.
pub struct Document {
    /// The root source text and its name, for rendering diagnostics.
    src: NamedSource<String>,
    /// The parsed root source.
    ast: ast::Source,
    /// Evaluation caches, shaped to mirror `ast.items`.
    cells: BlockCells,
    /// Namespace declared by the root source, prefixed to the names it
    /// declares.
    file_ns: Vec<String>,
    /// `use` aliases that bind one item name to a full path.
    item_aliases: HashMap<String, Vec<String>>,
    /// `use` aliases that bind a namespace prefix to a full path.
    ns_aliases: HashMap<String, Vec<String>>,
    /// Namespaces pulled in wholesale, searched when a bare name
    /// resolves against neither alias map.
    wildcards: Vec<Vec<String>>,
    /// Type declarations supplied by the environment rather than by the
    /// source — schemas a host registers.
    synthetic_types: Vec<ast::TypeDecl>,
    /// Parallel cells for `synthetic_types` so view types can reuse the
    /// same caching paths without special-casing.
    synthetic_type_cells: Vec<ItemCells>,
    /// Closed symbol vocabularies supplied by the environment. Kept beside
    /// synthetic types because parser-built source indexes contain neither.
    synthetic_symbol_sets: Vec<ast::SymbolSetDecl>,
    /// Parallel evaluation cells for `synthetic_symbol_sets`.
    synthetic_symbol_set_cells: Vec<ItemCells>,
    /// Name-to-declaration index across the root source and its eager
    /// imports.
    symbols: SymbolIndex,
    /// Builtins and synthetic declarations the host supplied.
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
    profile: Option<std::sync::Mutex<crate::diagnostics::ProfileState>>,
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
    /// Lazily-built index of every kind an instance declares (see
    /// [`TypeDecl::declares_kind`]): declared kind name → the declarer's
    /// position in [`blocks`] order plus the `@block` schema derived
    /// from its params. Both halves are built together because they come
    /// from one scan and are demanded by the same lookups —
    /// [`kind_declarer`](Self::kind_declarer) (which a host's expander
    /// runs for every nested block) and the [`block_schema`] fallback.
    ///
    /// Unlike `synthetic_types`, these can't be built at construction
    /// time: deriving one reads the declarer's labels and param blocks,
    /// which is view-layer work. Same construction-time-only *inputs* as
    /// `ref_registry`, so the once-built result stays sound.
    declared_kinds: std::sync::OnceLock<HashMap<String, DeclaredKind>>,
    /// Threads currently inside `build_declared_kinds`. Deriving a
    /// schema evaluates the declarer's labels, and evaluation can
    /// resolve a name through a block's schema — landing back in the
    /// lookup that started the derivation. The derivation stands down
    /// for a re-entrant call on the same thread (the kinds it would
    /// answer for are exactly the ones still being built) rather than
    /// recursing or deadlocking on the `OnceLock`.
    deriving: std::sync::Mutex<HashSet<std::thread::ThreadId>>,
    /// Memo for `union_fqn_for_path`: source-written type path → the
    /// FQN of the union it resolves to (`None` = not a union). Argument
    /// coercion consults this **per function invocation per argument**;
    /// without the memo every closure call re-ran name resolution (and
    /// rebuilt list arguments) just to discover a type isn't a union.
    union_path_memo: std::sync::RwLock<HashMap<Vec<String>, Option<String>>>,
    /// Lazily-built locations of every `@document`-decorated type
    /// declaration, in `type_decls()` order (root-authored first).
    /// `doc_schemas_for_ns` consults the merged document schema on
    /// every unresolved-name fallthrough, so re-walking every
    /// declaration per call made each miss O(total declarations).
    /// Same construction-time-only inputs as `ref_registry`.
    document_schema_locs: std::sync::OnceLock<Vec<DeclLoc>>,
    /// Lazily-built index of top-level `let` name → (source, item)
    /// position. `root_let` runs on every scope-walk fallthrough, so
    /// scanning every source's items per lookup compounded with the
    /// reference count. First occurrence wins, matching the scan order
    /// it replaces.
    root_let_index: std::sync::OnceLock<HashMap<String, (usize, usize)>>,
    /// Memo for the root `@connections` projection in
    /// [`resolve_root_in`]: field name → projected edge list. The
    /// projection walks every source's connection statements and
    /// resolves each operand's label against the top-level blocks;
    /// it ran in full on every reference. Sound to cache: it always
    /// runs at `Scope::root()` over construction-time-fixed sources.
    root_conn_memo: std::sync::RwLock<HashMap<String, Value>>,
    /// Memo for the root union `@children`/`@child` projections in
    /// [`resolve_root_children`]: field name → dispatched value
    /// (`Value::List` for `@children`). Union dispatch reified every
    /// top-level block per reference. Kind-keyed (non-union) arms stay
    /// uncached — they return borrowed `Block` views from a cheap scan.
    root_children_memo: std::sync::RwLock<HashMap<String, Value>>,
    /// Deliberately over-approximate set of every name `scope_lookup`
    /// could resolve from static (eager) document content: let / field
    /// names and block kinds at any depth, table header names, type and
    /// interface field names (schema projections), alias keys, and the
    /// last segment of every declared FQN. `eval_call` dispatches a
    /// builtin callee without the O(document) scope walk when its name
    /// is absent here (and no scope frame carries renderer bindings or
    /// lazy in-block imports — the two dynamic binders this set cannot
    /// see). A stray extra entry only costs the slow path. Same
    /// construction-time-only inputs as `ref_registry`.
    shadow_names: std::sync::OnceLock<HashSet<String>>,
    /// Lazily-built index for root-scope connection-operand resolution:
    /// identifying label / `id` value → the matched block, in the exact
    /// DFS order `match_block_label_in_items` searched (first match
    /// wins). Without it every operand of every `@connections`
    /// projection re-walked every block of every source, re-evaluating
    /// each block's label expression through the evaluator — the
    /// dominant cost of projection-heavy documents.
    conn_operand_index: std::sync::OnceLock<HashMap<String, ConnOperand>>,
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

/// Schema errors paired with the source each should be rendered
/// against — `None` when the error carries no file provenance.
type CollectedSchemaErrors = Vec<(EvalError, Option<NamedSource<String>>)>;

/// A symbol lookup result that knows which source it came from.
/// Exposed so the LSP can build cross-file `Location`s for
/// go-to-definition without reaching into `SymbolIndex` directly.
#[derive(Debug, Clone, Copy)]
pub struct SymbolHit<'a> {
    /// The indexed declaration.
    pub record: &'a SymbolRecord,
    /// File path of the source this symbol was declared in. `None`
    /// when the symbol comes from the root document.
    pub source_path: Option<&'a Path>,
}

impl Document {
    /// Lazy dotted-path access into the document. Each segment is
    /// resolved on demand against the current node — only the cells
    /// actually visited are forced. Returns `None` for any unresolved
    /// path.
    ///
    /// Resolution order for the first segment matches the existing
    /// surface: top-level Field, then Block-by-kind, then TypeDecl,
    /// UnionDecl, SymbolSetDecl. Subsequent segments delegate to
    /// [`DataRef::child`].
    pub fn get(&self, path: &str) -> Option<DataRef<'_>> {
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
    ) -> Option<DataRef<'_>> {
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
    fn resolve_root_children(&self, name: &str) -> Option<DataRef<'_>> {
        use DataRef;
        let schemas = self.doc_schemas_for_ns(&self.file_ns);
        if schemas.is_empty() {
            return None;
        }
        let field = schemas.field(name)?;

        if let Some(ck) = field.children_kind_or_union() {
            match ck {
                ChildKind::Union(union) => {
                    if let Some(Value::List(items)) = self
                        .root_children_memo
                        .read()
                        .ok()
                        .and_then(|m| m.get(name).cloned())
                    {
                        return Some(DataRef::from_variant_value_list(items.to_vec()));
                    }
                    let mut out: Vec<Value> = Vec::new();
                    for b in self.blocks() {
                        if let Ok(v) = types::variant_dispatch::block_to_variant(self, &b, union) {
                            out.push(v);
                        }
                    }
                    // Computed-children splice (`name = <list expr>` at root):
                    // the declared `list<Union>` type already coerced each
                    // bare record to a variant by shape, so splice them in.
                    if let Some(f) = self.field(name)
                        && let Ok(Value::List(items)) = f.value()
                    {
                        for it in items.iter() {
                            if matches!(it, Value::Variant { .. }) {
                                out.push(it.clone());
                            }
                        }
                    }
                    if let Ok(mut m) = self.root_children_memo.write() {
                        m.insert(
                            name.to_string(),
                            Value::List(std::sync::Arc::new(out.clone())),
                        );
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
                    if let Some(hit) = self
                        .root_children_memo
                        .read()
                        .ok()
                        .and_then(|m| m.get(name).cloned())
                    {
                        return Some(DataRef::from_variant_value(hit));
                    }
                    let projected = self
                        .blocks()
                        .find_map(|b| {
                            types::variant_dispatch::block_to_variant(self, &b, union).ok()
                        })
                        .unwrap_or(Value::None);
                    if let Ok(mut m) = self.root_children_memo.write() {
                        m.insert(name.to_string(), projected.clone());
                    }
                    return Some(DataRef::from_variant_value(projected));
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

    /// Resolve a bare name against the document root — the last step
    /// of identifier lookup, after enclosing scopes have been tried.
    pub(crate) fn resolve_root(&self, name: &str) -> Option<DataRef<'_>> {
        self.resolve_root_in(name, &self.file_ns)
    }

    /// [`resolve_root`] with an explicit namespace context: declaration
    /// lookups (type / union / symbol set / interface) resolve as if the
    /// reference were written in a source whose namespace is
    /// `context_ns`, via the full name-resolution algorithm (context
    /// namespace first, then aliases / wildcards / absolute). Root-level
    /// projections, fields and blocks are root-document concerns and
    /// ignore `context_ns`.
    pub(crate) fn resolve_root_in(&self, name: &str, context_ns: &[String]) -> Option<DataRef<'_>> {
        use DataRef;
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
            if let Some(hit) = self
                .root_conn_memo
                .read()
                .ok()
                .and_then(|m| m.get(name).cloned())
            {
                return Some(DataRef::from_variant_value(hit));
            }
            let schemas = self.doc_schemas_for_ns(&self.file_ns);
            if let Some(field) = schemas.field(name)
                && let Some(conn_schema) = field.connection_schema()
            {
                let mut all: Vec<Value> = Vec::new();
                for src in self.all_sources() {
                    all.extend(self.project_connections(src.items, conn_schema, &Scope::root()));
                }
                let projected = Value::List(std::sync::Arc::new(all));
                if let Ok(mut m) = self.root_conn_memo.write() {
                    m.insert(name.to_string(), projected.clone());
                }
                return Some(DataRef::from_variant_value(projected));
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

    /// The top-level field with this name, searching the root source
    /// and its eager imports.
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
                            file_ns: src.file_ns,
                            scope: Scope::root(),
                        });
                    }
                }
            }
        }
        None
    }

    /// The first top-level block of this kind.
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
        let sources = self.all_sources();
        let index = self.root_let_index.get_or_init(|| {
            let mut map = HashMap::new();
            for (si, src) in sources.iter().enumerate() {
                for (ii, (item, cells)) in src.items.iter().zip(src.cells.iter()).enumerate() {
                    if let (ast::Item::Let(l), ItemCellKind::Let(_)) = (item, &cells.kind)
                        && !map.contains_key(&l.name)
                    {
                        map.insert(l.name.clone(), (si, ii));
                    }
                }
            }
            map
        });
        let &(si, ii) = index.get(name)?;
        let src = &sources[si];
        let (ast::Item::Let(l), ItemCellKind::Let(cell)) = (&src.items[ii], &src.cells[ii].kind)
        else {
            unreachable!("root_let_index points at a Let item")
        };
        Some(LetView {
            ast: l,
            cell,
            doc: self,
            scope: Scope::root(),
        })
    }

    /// Every top-level field, in source order, imports included.
    pub fn fields(&self) -> impl Iterator<Item = Field<'_>> + '_ {
        let doc = self;
        self.all_sources()
            .into_iter()
            .flat_map(move |src| iter_fields(src.items, src.cells, doc, src.file_ns, Scope::root()))
    }

    /// Every top-level block, in source order, imports included.
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

    /// The `use` declarations the root source writes.
    pub fn uses(&self) -> impl Iterator<Item = UseDeclView<'_>> {
        self.ast.items.iter().filter_map(|item| match item {
            ast::Item::UseDecl(u) => Some(UseDeclView { ast: u }),
            _ => None,
        })
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

    /// The over-approximate shadowable-name set — see the
    /// `shadow_names` field. Built once from construction-time-fixed
    /// inputs (root AST, eager imports, synthetic types, aliases,
    /// `ref_registry`).
    pub(crate) fn shadow_names(&self) -> &HashSet<String> {
        self.shadow_names.get_or_init(|| {
            fn walk_items(items: &[ast::Item], out: &mut HashSet<String>) {
                for item in items {
                    match item {
                        ast::Item::Let(l) => {
                            out.insert(l.name.clone());
                        }
                        ast::Item::Field(f) => {
                            out.insert(f.name.clone());
                        }
                        ast::Item::Block(b) => {
                            out.insert(b.kind.clone());
                            walk_items(&b.items, out);
                        }
                        ast::Item::Table(t) => {
                            out.insert(t.field_name.clone());
                        }
                        ast::Item::TypeDecl(t) => {
                            for f in &t.fields {
                                out.insert(f.name.clone());
                            }
                        }
                        ast::Item::InterfaceDecl(i) => {
                            for f in &i.fields {
                                out.insert(f.name.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
            let mut out = HashSet::new();
            for src in self.all_sources() {
                walk_items(src.items, &mut out);
            }
            for t in &self.synthetic_types {
                for f in &t.fields {
                    out.insert(f.name.clone());
                }
            }
            for k in self.item_aliases.keys() {
                out.insert(k.clone());
            }
            for k in self.ns_aliases.keys() {
                out.insert(k.clone());
            }
            // Declared type / union / symbol-set / interface /
            // connection names resolve bare via their last FQN segment.
            for fqn in self.ref_registry() {
                if let Some(last) = fqn.last() {
                    out.insert(last.clone());
                }
            }
            out
        })
    }

    /// The host's [`Expander`](crate::Expander), consulted when a
    /// `@contextual` block's generated children are demanded. `None`
    /// when the document was opened with an environment that registers
    /// none — see [`Block::expand_children`](crate::Block::expand_children).
    pub(crate) fn expander(&self) -> Option<&dyn crate::Expander> {
        self.env.expander()
    }
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

/// `true` when a block of kind `kind`, identified by `id` (its first
/// label, or an explicit `id = …` field), exists anywhere in the
/// document tree — top-level or nested. Drives the `@ref("kind")`
/// dangling-reference check.
impl Document {
    /// Whether any block of `kind` declares this id — the lookup
    /// behind reference resolution.
    pub(crate) fn has_block_with_id(&self, kind: &str, id: &str) -> bool {
        fn id_matches(b: &Block<'_>, id: &str) -> bool {
            if block_first_label(b).as_deref() == Some(id) {
                return true;
            }
            // Fall back to an explicit `id` field (blocks whose
            // identifier is a field, not an inline label).
            b.field("id")
                .and_then(|f| f.value().ok().cloned())
                .is_some_and(|v| {
                    matches!(&v, Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) if s == id)
                })
        }
        fn walk(b: &Block<'_>, kind: &str, id: &str) -> bool {
            (b.kind() == kind && id_matches(b, id)) || b.blocks().any(|c| walk(&c, kind, id))
        }
        self.blocks().any(|b| walk(&b, kind, id))
    }
}

/// The source span of any expression, whatever its variant.
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
        | E::Symbol(..)
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
        | E::InterpolatedString { span, .. }
        | E::UnitLiteral { span, .. } => *span,
        E::SelfKw(s) | E::ParentKw(s) => *s,
    }
}

/// Convert a byte-range span into the `miette` equivalent.
pub(crate) fn span_to_miette(span: Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.len().max(1))
}

#[cfg(test)]
#[cfg(test)]
mod tests;
