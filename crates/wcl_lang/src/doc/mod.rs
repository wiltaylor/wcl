use std::path::Path;

use miette::{NamedSource, SourceSpan};

use std::collections::{HashMap, HashSet};

mod cells;
mod connections;
mod decorators;
mod effective_fields;
mod eval;
mod eval_ops;
mod imports;
mod interfaces;
mod loader;
mod lookup;
mod match_pat;
mod open;
mod provenance;
mod schema_check;
mod schema_lookup;
mod scope;
mod validate;
pub(super) mod variant_dispatch;
mod views;
pub use imports::{SYSTEM_IMPORT_ROOT, system_import_key};
pub use loader::{FileLoader, Registry, disk_loader, overlay_loader};
pub use views::{
    Block, ChildKind, Connection, ConnectionDecl, DeclName, DeclaresKind, Decorator, Field,
    FieldShape, InterfaceDecl, NamedArg, ResolvedType, RowView, SymbolEntry, SymbolSetDecl,
    TableView, TypeDecl, TypeField, UnionDecl, UnionVariant, UseDeclView, UseFormView, UseItem,
    VariantBodyView,
};
pub(crate) use views::{BuiltinDecorator, LetView, UnionChildKind};

use crate::ast::TypeRef;
use crate::ast::{self, Span};
#[cfg(test)]
use crate::ast::{BuiltinType, TensorDim};
use crate::environment::Environment;
use crate::error::EvalError;
use crate::symbols::{SymbolIndex, SymbolKind, SymbolRecord};
use crate::value::Value;
use cells::{BlockCells, ItemCellKind, ItemCells, LoadedImport};
use connections::RESOLVING_CONN_OPERAND;
pub(crate) use connections::{ConnOperand, connection_type_matches};
use decorators::decorator_name_span;
use lookup::{find_block, find_field, find_let, iter_blocks, iter_fields};
use schema_check::{has_annotation_exemption, has_schemaless};
use scope::Scope;
use validate::{decl_fqn_matches, resolve_path};

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

/// One block kind declared by an *instance* — a block whose type carries
/// `@declares_kind`. Owns the schema derived for it, which is what makes
/// an instance of the kind an ordinary block as far as every generic
/// check is concerned.
struct DeclaredKind {
    /// The declarer's position in `blocks()` order.
    /// `None` for a content-fill kind synthesised from a `slot` declaration:
    /// it has a permissive contextual schema but is not a component whose
    /// body the host expander should enter.
    declarer: Option<usize>,
    /// The derived `@block` type, plus its evaluation cells (a type
    /// declaration is viewed through both).
    ast: ast::TypeDecl,
    /// Evaluation caches for `ast`.
    cells: ItemCells,
}

/// Position of a type declaration found by a cached decorator scan:
/// either `all_sources()[source].items[item]` or an index into the
/// document's `synthetic_types`.
enum DeclLoc {
    /// An item of one of the document's sources.
    Source {
        /// Index into `all_sources()`.
        source: usize,
        /// Index into that source's items.
        item: usize,
    },
    /// An index into the document's `synthetic_types`.
    Synthetic(usize),
}

/// The first positional argument when it evaluates to a UTF-8 string.
/// Schema discovery uses this common shape for declarations such as
/// `@block("kind")` and `@decorator("name")`.
fn first_positional_utf8(decorator: &Decorator<'_>) -> Option<String> {
    match decorator.positional().ok()?.into_iter().next()? {
        Value::Utf8(value) => Some(value),
        _ => None,
    }
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
    /// Name index for this source.
    symbols: &'a SymbolIndex,
    /// The source's top-level items.
    items: &'a [ast::Item],
    /// Evaluation caches, index-aligned with `items`.
    cells: &'a [ItemCells],
    /// The raw text, for rendering diagnostics against this source.
    source: &'a str,
    /// Namespace this source declares.
    file_ns: &'a [String],
    /// Resolved path on disk. `None` for the root document (the host
    /// typically supplies that path itself, e.g. via the LSP request
    /// URI); `Some` for every eagerly-loaded import.
    path: Option<&'a Path>,
}

/// Schema errors paired with the source each should be rendered
/// against — `None` when the error carries no file provenance.
type CollectedSchemaErrors = Vec<(EvalError, Option<NamedSource<String>>)>;

/// The set of `@document` schemas governing one namespace, root-authored
/// first. Their *merge* forms the effective document schema: a top-level
/// field/block is legal if any member declares/allows it. Built by
/// [`Document::doc_schemas_for_ns`].
pub(crate) struct DocSchemas<'a> {
    /// The governing schemas, root-authored first.
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
    /// The indexed declaration.
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
/// borrow; the optional trailing field names are `TypeDecl`'s two origin
/// flags — `is_imported`, set from the source's import origin, and
/// `is_derived`, false for every declaration a source *writes*. Collapses
/// the five `type_decl`/`interface`/`union_decl`/`symbol_set`/
/// `connection_decl` lookups (M4).
macro_rules! find_decl {
    ($self:ident, $fqn:ident, $variant:ident, cells $(, $imp:ident, $der:ident)?) => {
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
                    $( $imp: src.path.is_some(), $der: false, )?
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
                        .find_map(|b| variant_dispatch::block_to_variant(self, &b, union).ok())
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
            TypeRef::Named { path, .. } => self.resolve_path_in(path, file_ns).map(|p| p.join(".")),
            TypeRef::Reference(inner) => self.resolve_type_fqn_in(inner, file_ns),
            _ => None,
        }
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

    /// Look up a type by fully-qualified name (dotted). Searches the
    /// importer, every eagerly-imported file, and registry-injected
    /// types in that order.
    /// Resolve type aliases (`type Port = u16`) inside `ty`, deeply:
    /// a `Named` ref whose declaration is an alias is replaced by its
    /// target (transitively, cycle-capped), and list / reference /
    /// tensor element types resolve recursively. Non-alias refs come
    /// back unchanged. Used by the schema value checks so a field
    /// declared with an alias validates against the target type.
    pub fn resolve_alias(&self, ty: &crate::ast::TypeRef) -> crate::ast::TypeRef {
        self.resolve_alias_in(ty, &self.file_ns)
    }

    /// Resolve a path through the `use` aliases visible in the given
    /// namespace, returning the fully-qualified form.
    pub(crate) fn resolve_alias_in(
        &self,
        ty: &crate::ast::TypeRef,
        context_ns: &[String],
    ) -> crate::ast::TypeRef {
        use crate::ast::TypeRef as T;
        fn go(doc: &Document, ty: &T, context_ns: &[String], depth: u8) -> T {
            if depth == 0 {
                return ty.clone(); // alias cycle — give up, stay permissive
            }
            match ty {
                T::Named { path, args } => {
                    let resolved_path = doc
                        .resolve_path_in(path, context_ns)
                        .unwrap_or_else(|| path.clone());
                    match doc.type_decl(&resolved_path.join(".")) {
                        Some(declaration) if declaration.ast.alias.is_some() => go(
                            doc,
                            declaration.ast.alias.as_ref().expect("alias checked"),
                            declaration.file_ns(),
                            depth - 1,
                        ),
                        _ => T::Named {
                            // Preserve the authored path for runtime value
                            // matching, which deliberately accepts a shorter
                            // suffix (`RelatedEdge` against a namespaced
                            // variant tag). Namespace resolution above is
                            // needed only to locate an alias declaration.
                            path: path.clone(),
                            args: args
                                .iter()
                                .map(|arg| go(doc, arg, context_ns, depth - 1))
                                .collect(),
                        },
                    }
                }
                T::List(inner) => T::List(Box::new(go(doc, inner, context_ns, depth - 1))),
                T::Reference(inner) => {
                    T::Reference(Box::new(go(doc, inner, context_ns, depth - 1)))
                }
                T::Tensor { element, dims } => T::Tensor {
                    element: Box::new(go(doc, element, context_ns, depth - 1)),
                    dims: dims.clone(),
                },
                T::Function { params, return_ty } => T::Function {
                    params: params
                        .iter()
                        .map(|param| go(doc, param, context_ns, depth - 1))
                        .collect(),
                    return_ty: Box::new(go(doc, return_ty, context_ns, depth - 1)),
                },
                other => other.clone(),
            }
        }
        go(self, ty, context_ns, views::ALIAS_DEPTH)
    }

    /// The chain of alias declarations behind `ty`, outermost first —
    /// empty for a non-alias type. Constraint decorators on each link
    /// apply to values of the aliased type.
    pub(crate) fn alias_chain(&self, ty: &crate::ast::TypeRef) -> Vec<TypeDecl<'_>> {
        self.alias_chain_in(ty, &self.file_ns)
    }

    /// Follow a chain of `use` aliases to its end, bounded so a cyclic
    /// alias cannot hang the walk.
    pub(crate) fn alias_chain_in(
        &self,
        ty: &crate::ast::TypeRef,
        context_ns: &[String],
    ) -> Vec<TypeDecl<'_>> {
        let mut out = Vec::new();
        let mut current = ty.clone();
        let mut current_ns = context_ns;
        for _ in 0..views::ALIAS_DEPTH {
            let crate::ast::TypeRef::Named { path, .. } = &current else {
                break;
            };
            let resolved = self
                .resolve_path_in(path, current_ns)
                .unwrap_or_else(|| path.clone());
            let Some(decl) = self.type_decl(&resolved.join(".")) else {
                break;
            };
            let Some(target) = decl.ast.alias.clone() else {
                break;
            };
            out.push(decl);
            current = target;
            current_ns = decl.file_ns();
        }
        out
    }

    /// The multiplier declared for `unit` on `ty`'s alias chain via a
    /// `@unit(name, factor)` decorator, or `None` if the type declares no
    /// such unit. Mirrors `schema_check::constraint_violation`'s
    /// alias-chain decorator walk; the factor expression evaluates through
    /// the document (so `@unit("MiB", 1024 * 1024)` works). Backs both
    /// literal-unit resolution and the `format_unit` builtin.
    pub(crate) fn unit_factor(&self, ty: &crate::ast::TypeRef, unit: &str) -> Option<Value> {
        for link in self.alias_chain(ty) {
            for d in link.ast.decorators.iter() {
                if d.name.join(".") != "unit" {
                    continue;
                }
                let Some(name_val) = d.positional.first().and_then(|e| self.eval(e).ok()) else {
                    continue;
                };
                let matches = matches!(&name_val,
                    Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s)
                        if s == unit);
                if !matches {
                    continue;
                }
                if let Some(factor) = d.positional.get(1).and_then(|e| self.eval(e).ok()) {
                    return Some(factor);
                }
            }
        }
        None
    }

    /// The `type` declaration with this fully-qualified name.
    pub fn type_decl(&self, fqn: &str) -> Option<TypeDecl<'_>> {
        find_decl!(self, fqn, TypeDecl, cells, is_imported, is_derived);
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
                is_derived: false,
            })
    }

    /// Every `type` declaration in scope, synthetic ones included.
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
                        is_derived: false,
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
                is_derived: false,
            });
        mine_and_imports.chain(syn)
    }

    /// Every declared decorator and the type that schemas it, in
    /// [`type_decls`](Self::type_decls) order. This includes declarations
    /// from eager imports and synthetic types supplied by the environment.
    pub fn declared_decorators(&self) -> impl Iterator<Item = (String, TypeDecl<'_>)> + '_ {
        self.type_decls().flat_map(|schema| {
            let names: Vec<_> = schema
                .decorators()
                .filter_map(|decorator| {
                    if decorator.is(BuiltinDecorator::Decorator) {
                        first_positional_utf8(&decorator)
                    } else {
                        None
                    }
                })
                .collect();
            names.into_iter().map(move |name| (name, schema))
        })
    }

    /// Look up an interface declaration by fully-qualified name.
    /// Mirrors `type_decl` / `union_decl`.
    pub fn interface(&self, fqn: &str) -> Option<InterfaceDecl<'_>> {
        find_decl!(self, fqn, InterfaceDecl, cells);
        None
    }

    /// Every `interface` declaration in scope.
    pub fn interfaces(&self) -> impl Iterator<Item = InterfaceDecl<'_>> + '_ {
        decl_iter_cells!(self, InterfaceDecl)
    }

    /// Look up a union by fully-qualified name (dotted).
    /// The union a source-written type path resolves to (root-namespace
    /// context), memoised per path — see the `union_path_memo` field.
    /// Returns the resolved FQN; look the decl up with [`union_decl`].
    pub(crate) fn union_fqn_for_path(&self, path: &[String]) -> Option<String> {
        if let Some(hit) = self.union_path_memo.read().ok()?.get(path) {
            return hit.clone();
        }
        let resolved = self
            .resolve_path_in(path, &self.file_ns)
            .map(|p| p.join("."))
            .unwrap_or_else(|| path.join("."));
        let fqn = if self.union_decl(&resolved).is_some() {
            Some(resolved)
        } else {
            let raw = path.join(".");
            if raw != resolved && self.union_decl(&raw).is_some() {
                Some(raw)
            } else {
                None
            }
        };
        if let Ok(mut memo) = self.union_path_memo.write() {
            memo.insert(path.to_vec(), fqn.clone());
        }
        fqn
    }

    /// The `union` declaration with this fully-qualified name.
    pub fn union_decl(&self, fqn: &str) -> Option<UnionDecl<'_>> {
        find_decl!(self, fqn, UnionDecl, cells);
        None
    }

    /// Every `union` declaration in scope.
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

    /// Every `connection` declaration in scope.
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

    /// The `symbol_set` with this fully-qualified name.
    pub fn symbol_set(&self, fqn: &str) -> Option<SymbolSetDecl<'_>> {
        find_decl!(self, fqn, SymbolSetDecl, cells);
        let target: Vec<&str> = fqn.split('.').collect();
        self.synthetic_symbol_sets
            .iter()
            .enumerate()
            .find(|(_, set)| decl_fqn_matches(&set.name, &target))
            .map(|(index, set)| SymbolSetDecl {
                ast: set,
                file_ns: &[],
                cells: &self.synthetic_symbol_set_cells[index],
                doc: self,
            })
    }

    /// Every `symbol_set` in scope, synthetic ones included.
    pub fn symbol_sets(&self) -> impl Iterator<Item = SymbolSetDecl<'_>> + '_ {
        let doc = self;
        let authored = decl_iter_cells!(self, SymbolSetDecl);
        let synthetic = self
            .synthetic_symbol_sets
            .iter()
            .zip(self.synthetic_symbol_set_cells.iter())
            .map(move |(set, cells)| SymbolSetDecl {
                ast: set,
                file_ns: &[],
                cells,
                doc,
            });
        authored.chain(synthetic)
    }

    /// Resolve a [`TypeRef`] to either its built-in tag or the user-declared
    /// [`TypeDecl`] / [`UnionDecl`] it points to. `Named` refs are validated
    /// at [`Document::open`], so the lookup never fails here.
    ///
    /// Names resolve from the document's ROOT namespace. A reference written
    /// inside a namespaced file must resolve from *that* namespace instead —
    /// see [`Document::resolve_in`] and [`TypeField::resolved_type`].
    pub fn resolve<'a>(&'a self, t: &'a TypeRef) -> ResolvedType<'a> {
        self.resolve_in(t, &self.file_ns)
    }

    /// [`Document::resolve`] for a reference written in a source whose
    /// namespace is `file_ns`: the name resolves *within its declaring
    /// namespace first*. This is what keeps two same-named types in
    /// different namespaces apart — a user schema's `acme.Container` and
    /// wdoc's diagram `wdoc.Container` are both named `Container`, and
    /// resolving an `acme` field's type from the root namespace can answer
    /// the wrong one.
    pub fn resolve_in<'a>(&'a self, t: &'a TypeRef, file_ns: &[String]) -> ResolvedType<'a> {
        match t {
            TypeRef::Builtin(b) => ResolvedType::Builtin(*b),
            TypeRef::Named { path, .. } => {
                let fqn = self
                    .resolve_path_in(path, file_ns)
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
            TypeRef::Reference(inner) => {
                ResolvedType::Reference(Box::new(self.resolve_in(inner, file_ns)))
            }
            TypeRef::List(inner) => ResolvedType::List(Box::new(self.resolve_in(inner, file_ns))),
            TypeRef::Tensor { element, dims } => ResolvedType::Tensor {
                element: Box::new(self.resolve_in(element, file_ns)),
                dims,
            },
            TypeRef::Function { params, return_ty } => ResolvedType::Function {
                params: params.iter().map(|p| self.resolve_in(p, file_ns)).collect(),
                return_ty: Box::new(self.resolve_in(return_ty, file_ns)),
            },
        }
    }

    /// Prefix a declared name with the root source's namespace.
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

    /// Collect every type FQN that some `&T` field references, so the
    /// reference-acceptance check can be answered by lookup.
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
        for set in &self.synthetic_symbol_sets {
            registry.insert(set.name.clone());
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

    /// Run the name-resolution algorithm on `path` against this document's
    /// root file ns / aliases / wildcards / registry.
    fn resolve_path(&self, path: &[String]) -> Option<Vec<String>> {
        self.resolve_path_in(path, &self.file_ns)
    }

    /// Resolve `path` as if it were written in a source whose namespace
    /// is `file_ns`. This makes a bare reference resolve **within its
    /// declaring file's namespace first** — e.g. a stdlib type's
    /// `extends ContentBlock` (written in `namespace wdoc`) resolves to
    /// `wdoc.ContentBlock`, not the root namespace. The document's
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

    /// Non-fatal schema diagnostics — advisory siblings of
    /// [`schema_errors`](Self::schema_errors) that hosts surface without
    /// failing (the CLI prints them, builds ignore them, the LSP maps
    /// them to `Warning` severity).
    ///
    /// Currently detects **gather-field shadowing** between `@document`
    /// schemas ([`SchemaViolationKind::DocumentFieldShadow`]): two
    /// schemas that co-govern a namespace declaring the same field name
    /// where at least one side is a `@child`/`@children` gather slot.
    /// The merge (`doc_schemas_for_ns`) resolves such a name first-wins,
    /// so the shadowed schema's gathered blocks silently vanish from any
    /// template iterating the field — the failure mode that once forced a
    /// real schema to rename its `component` gather to `sw_components`.
    /// Pairs are checked across *all* co-governing declarations (not
    /// just root-vs-imported): `is_imported` means "from an imported
    /// file", so two library schemas (e.g. wdoc's `Site` and a base
    /// schema imported from disk) can shadow each other too. Scalar
    /// fields colliding with scalar fields are deliberately not
    /// reported — only gather slots break silently.
    ///
    /// The lazy validation path (`Field::schema_membership_error`) is
    /// intentionally untouched: the strict/lazy agreement contract
    /// covers membership *errors* only.
    pub fn schema_warnings(&self) -> Vec<EvalError> {
        use crate::error::SchemaViolationKind as Kind;
        let mut out = Vec::new();

        let decls = self.document_schema_decls();
        // `document_schema_decls` just built the location cache; zip
        // each decl with its source path so messages can name files
        // (the `TypeDecl` view itself doesn't carry one).
        let sources = self.all_sources();
        let locs = self
            .document_schema_locs
            .get()
            .expect("document_schema_decls built the cache");
        let paths: Vec<Option<&Path>> = locs
            .iter()
            .map(|loc| match *loc {
                DeclLoc::Source { source, .. } => sources[source].path,
                DeclLoc::Synthetic(_) => None,
            })
            .collect();

        let is_gather = |f: &TypeField<'_>| {
            f.child_kind_or_union().is_some() || f.children_kind_or_union().is_some()
        };
        let file_of = |p: Option<&Path>| match p {
            Some(p) => format!(" ({})", p.display()),
            None => " (this document)".to_string(),
        };

        let mut seen: HashSet<(usize, String)> = HashSet::new();
        for (bi, b) in decls.iter().enumerate() {
            for (ai, a) in decls.iter().enumerate().take(bi) {
                // Co-governance mirrors `doc_schemas_for_ns`: an
                // imported `@document` governs every namespace, a
                // root-authored one only its own.
                let co_govern = a.is_imported() || b.is_imported() || a.file_ns() == b.file_ns();
                if !co_govern {
                    continue;
                }
                for fb in b.fields() {
                    let Some(fa) = a.field(fb.name()) else {
                        continue;
                    };
                    if !is_gather(&fa) && !is_gather(&fb) {
                        continue;
                    }
                    // Anchor at the root-authored side when exactly one
                    // side is root-authored (that span is valid against
                    // the root source, which is what the CLI snippet
                    // renderer and the LSP have open); otherwise at the
                    // later declaration (in practice the schema the
                    // user imported last).
                    let (anchor_idx, anchor_field, other, other_idx) =
                        if !a.is_imported() && b.is_imported() {
                            (ai, fa, b, bi)
                        } else {
                            (bi, fb, a, ai)
                        };
                    if !seen.insert((anchor_idx, anchor_field.name().to_string())) {
                        continue;
                    }
                    let anchored = decls[anchor_idx];
                    out.push(EvalError::schema_violation_named(
                        Kind::DocumentFieldShadow,
                        format!(
                            "gather field '{name}' of @document '{anchored_fqn}'{anchored_file} \
                             collides with '{name}' declared by @document '{other_fqn}'{other_file} \
                             — the merged document schema resolves '{name}' to only one \
                             declaration, so the other schema's gathered blocks silently \
                             vanish; rename one field",
                            name = anchor_field.name(),
                            anchored_fqn = anchored.full_name(),
                            anchored_file = file_of(paths[anchor_idx]),
                            other_fqn = other.full_name(),
                            other_file = file_of(paths[other_idx]),
                        ),
                        anchor_field.name(),
                        anchor_field.span(),
                    ));
                }
            }
        }
        out
    }

    /// The host's [`Expander`](crate::Expander), consulted when a
    /// `@contextual` block's generated children are demanded. `None`
    /// when the document was opened with an environment that registers
    /// none — see [`Block::expand_children`](crate::Block::expand_children).
    pub(crate) fn expander(&self) -> Option<&dyn crate::Expander> {
        self.env.expander()
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
        self.collect_schema_errors()
            .into_iter()
            .map(|(error, _)| error)
            .collect()
    }

    /// Strict-mode validation paired with a source when its provenance is
    /// known. Hosts should omit a snippet for `None` rather than attach the
    /// root document to a recursively-produced cross-file error.
    pub fn schema_diagnostics(&self) -> Vec<(EvalError, Option<NamedSource<String>>)> {
        self.collect_schema_errors()
    }

    /// Run the full schema validation pass, pairing each violation
    /// with the source it should be rendered against.
    fn collect_schema_errors(&self) -> CollectedSchemaErrors {
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
        let doc_schemas = self.document_schema_decls();
        let mut by_ns: BTreeMap<Vec<String>, Vec<TypeDecl<'_>>> = BTreeMap::new();
        for d in &doc_schemas {
            by_ns.entry(d.file_ns().to_vec()).or_default().push(*d);
        }
        for decls in by_ns.values() {
            for extra in decls.iter().filter(|d| !d.is_imported()).skip(1) {
                out.push((
                    EvalError::schema_violation(
                        Kind::MultipleDocumentSchemas,
                        format!(
                            "type '{}' declares a second root @document schema \
                         (only one root-authored @document is allowed per namespace; \
                         imported library schemas merge automatically)",
                            extra.name()
                        ),
                        extra.span(),
                    ),
                    Some(self.root_named_source()),
                ));
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
                    out.push((
                        EvalError::schema_violation(
                            Kind::DuplicateBlockKind,
                            format!(
                                "type '{}' redeclares @{dec_name}(\"{kind}\") already declared \
                             in this namespace (kinds must be unique per namespace; \
                             qualify across namespaces with `::`)",
                                extra.name()
                            ),
                            extra.span(),
                        ),
                        Some(self.root_named_source()),
                    ));
                }
            }
        }

        // An instance-declared kind (`@declares_kind`) whose name matches
        // a declared `@block`/`@table` kind is incoherent: expansion
        // dispatches the kind to the declarer while schema lookup
        // validates instances against the declared type (and a host may
        // special-case the kind in Rust, ignoring the declarer
        // entirely). The collision itself is the error, reported at the
        // declaration. Both sides are looked up *without* the
        // derivation, so a declared kind never collides with itself.
        let declared_kinds: Vec<String> = self
            .declared_kinds()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        for name in declared_kinds {
            let colliding = self
                .find_schema(BuiltinDecorator::Block, &name)
                .or_else(|| self.find_schema(BuiltinDecorator::Table, &name));
            let Some(t) = colliding else { continue };
            let Some(declarer) = self.kind_declarer(&name) else {
                continue;
            };
            out.push((
                EvalError::schema_violation(
                    Kind::DeclaredKindCollision,
                    format!(
                        "block '{}' declares the kind '{name}', which collides with the \
                     @block/@table kind \"{name}\" declared by type '{}' — rename it",
                        declarer.kind(),
                        t.full_name()
                    ),
                    declarer.span(),
                ),
                Some(declarer.named_source()),
            ));
        }

        // Walk every source (the main file + every eagerly-imported
        // file). Each source's items are validated against the merged
        // `@document` schema for that source's namespace, if any.
        for src in self.all_sources() {
            let mut errors = Vec::new();
            let schemas = self.doc_schemas_for_ns(src.file_ns);

            // Pre-compute the merged schema's union-typed children
            // slots so structurally-matched blocks bypass the kind
            // check.
            let root_union_slots = schemas.union_slots();

            // Walk the top-level fields in this source.
            for f in iter_fields(src.items, src.cells, self, src.file_ns, Scope::root()) {
                if has_schemaless(&f.ast.decorators) {
                    continue;
                }
                self.validate_root_field(&f, &schemas, &mut errors);
            }

            // Walk the top-level blocks in this source.
            for b in iter_blocks(src.items, src.cells, self, src.file_ns, Scope::root()) {
                if has_schemaless(&b.ast.decorators) {
                    continue;
                }
                self.validate_root_block(&b, &schemas, &root_union_slots, &mut errors);
            }
            let source = self.named_source_for_view(src);
            out.extend(errors.into_iter().map(|error| {
                let is_undeclared_decorator = matches!(
                    &error,
                    EvalError::SchemaViolation {
                        kind: Kind::UndeclaredDecorator,
                        ..
                    }
                );
                let error_source = is_undeclared_decorator.then(|| source.clone());
                (error, error_source)
            }));
        }

        // Duplicate identity labels among top-level blocks, grouped per
        // namespace — every source of one namespace feeds the same
        // gather lists, so two `component engine { }` blocks in
        // different data files silently coexist without this. Only
        // kinds whose schema declares an `@inline(0) identifier` label
        // participate (see `schema_check::identity_label`); nested
        // duplicates are caught per parent in `compute_schema_errors`.
        {
            let mut by_ns: BTreeMap<Vec<String>, Vec<Block<'_>>> = BTreeMap::new();
            for src in self.all_sources() {
                for b in iter_blocks(src.items, src.cells, self, src.file_ns, Scope::root()) {
                    if has_schemaless(&b.ast.decorators) {
                        continue;
                    }
                    by_ns.entry(src.file_ns.to_vec()).or_default().push(b);
                }
            }
            for blocks in by_ns.into_values() {
                out.extend(
                    crate::doc::schema_check::duplicate_id_errors(blocks.into_iter())
                        .into_iter()
                        .map(|(error, block)| (error, Some(block.named_source()))),
                );
            }
        }

        // Validate every union declaration: cycles in the extends
        // chain, duplicate variants across that chain, and structural
        // collisions between variant bodies.
        for u in self.union_decls() {
            let source = self.named_source_for_union(u.ast);
            out.extend(
                validate_union(self, u.ast)
                    .into_iter()
                    .map(|error| (error, Some(source.clone()))),
            );
        }

        // Top-level connection statements: dispatch and kind checks.
        for src in self.all_sources() {
            let errors = crate::doc::schema_check::validate_connection_stmts(
                self,
                src.items,
                &Scope::root(),
            );
            let source = self.named_source_for_view(src);
            out.extend(
                errors
                    .into_iter()
                    .map(|error| (error, Some(source.clone()))),
            );
        }

        // Applicability and cardinality share one grammar-shaped walk so
        // every decorator-bearing position is covered by the same rules.
        // Follow each authored import edge from the root exactly once. This
        // reaches eager and lazy block imports without re-walking eager
        // sources already exposed by `all_sources()`.
        let mut decorated_imports = HashSet::new();
        let root_source = self.root_named_source();
        self.validate_decorators_in_items(
            &self.ast.items,
            &self.cells.items,
            &self.file_ns,
            &mut decorated_imports,
            &root_source,
            &mut out,
        );
        for (declaration, cells) in self.synthetic_types.iter().zip(&self.synthetic_type_cells) {
            let mut errors = Vec::new();
            self.validate_decorator_group(
                &declaration.decorators,
                &cells.decorators,
                "type",
                None,
                &[],
                &mut errors,
            );
            let ItemCellKind::TypeDecl { field_decorators } = &cells.kind else {
                unreachable!("synthetic type cells mirror the declaration")
            };
            for (field, decorator_cells) in declaration.fields.iter().zip(field_decorators) {
                self.validate_decorator_group(
                    &field.decorators,
                    decorator_cells,
                    "type_field",
                    None,
                    &[],
                    &mut errors,
                );
            }
            out.extend(errors.into_iter().map(|error| (error, None)));
        }

        // Validate the applicability declarations themselves before using
        // them to check decorator occurrences.
        for declaration in self.type_decls() {
            let mut errors = Vec::new();
            let declares_decorator = declaration
                .decorators()
                .any(|decorator| decorator.full_name() == "decorator");
            for applies_to in declaration
                .decorators()
                .filter(|decorator| decorator.full_name() == "applies_to")
            {
                if !declares_decorator {
                    EvalError::push_schema_violation(
                        &mut errors,
                        Kind::InvalidDecoratorApplicability,
                        "@applies_to is attached to no decorator schema",
                        decorator_name_span(&applies_to),
                    );
                }
                let positions = applies_to
                    .named_arg("on")
                    .and_then(Result::ok)
                    .and_then(|value| match value {
                        Value::List(values) => Some(values),
                        _ => None,
                    });
                if let Some(positions) = &positions
                    && let Some(position_set) = self.symbol_set("DecoratorPosition")
                {
                    let on_span = applies_to
                        .named()
                        .find(|arg| arg.name() == "on")
                        .map(|arg| arg.span())
                        .unwrap_or_else(|| decorator_name_span(&applies_to));
                    for position in positions.iter().filter_map(|value| match value {
                        Value::Symbol(position) => Some(position.as_str()),
                        _ => None,
                    }) {
                        if !position_set.has(position) {
                            EvalError::push_schema_violation(
                                &mut errors,
                                Kind::InvalidDecoratorApplicability,
                                format!("unknown decorator position '{position}'"),
                                on_span,
                            );
                        }
                    }
                }
                let kinds_arg = applies_to.named().find(|arg| arg.name() == "kinds");
                if let (Some(positions), Some(kinds_arg)) = (positions, kinds_arg) {
                    let includes_block = positions.iter().any(
                        |value| matches!(value, Value::Symbol(position) if position == "block"),
                    );
                    if !includes_block {
                        EvalError::push_schema_violation(
                            &mut errors,
                            Kind::InvalidDecoratorApplicability,
                            "@applies_to 'kinds' requires the 'block' position in 'on'",
                            kinds_arg.span(),
                        );
                    }
                    if let Ok(Value::List(kinds)) = kinds_arg.value() {
                        for kind in kinds.iter().filter_map(|value| match value {
                            Value::Utf8(kind) => Some(kind.as_str()),
                            _ => None,
                        }) {
                            if self
                                .block_schema_in(&[], kind, declaration.file_ns())
                                .is_none()
                            {
                                EvalError::push_schema_violation(
                                    &mut errors,
                                    Kind::InvalidDecoratorApplicability,
                                    format!("@applies_to names unknown block kind '{kind}'"),
                                    kinds_arg.span(),
                                );
                            }
                        }
                    }
                }
            }
            let source = self.named_source_for_type(declaration.ast);
            out.extend(
                errors
                    .into_iter()
                    .map(|error| (error, Some(source.clone()))),
            );
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
        if !has_annotation_exemption(&f.ast.decorators) {
            for decorator in f.decorators() {
                out.extend(schema_check::decorator_argument_errors(self, &decorator));
            }
        }
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
        // An optional field written out as `none` is absent, not
        // ill-typed: there is no value left to check against the
        // declared type, a symbol set, or a `@min`/`@non_empty` bound.
        if declared.optional() && matches!(v, Value::None) {
            return;
        }
        if let TypeRef::Named { path, .. } = declared.type_ref()
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
        } else if !value_matches_type_ref(
            v,
            &self.resolve_alias_in(declared.type_ref(), declared.file_ns),
        ) {
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
        } else if let Some(err) = symbol_set_membership_error_in(
            self,
            &self.resolve_alias_in(declared.type_ref(), declared.file_ns),
            v,
            f.name(),
            f.span(),
            declared.file_ns,
        ) {
            out.push(err);
        } else if let Some(msg) = schema_check::constraint_violation(
            self,
            &declared.ast.decorators,
            declared.type_ref(),
            declared.file_ns,
            v,
        ) {
            EvalError::push_schema_violation(
                out,
                Kind::ConstraintViolation,
                format!("field '{}': {msg}", f.name()),
                f.span(),
            );
        } else if let Some(msg) = schema_check::ref_violation(self, &declared, v) {
            EvalError::push_schema_violation(
                out,
                Kind::DanglingReference,
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
            if self.is_possible_block_slot_fill(b.kind()) {
                return;
            }
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
                let Some(v) = first_positional_utf8(&d) else {
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
}

/// Match `name` against a block's first label, which is how a block
/// with an `@inline(0)` identifier field is addressed.
fn match_block_first_label(doc: &Document, b: &ast::Block, name: &str) -> Option<Value> {
    let first = b.labels.first()?;
    // A block label is an opaque identity name, not a reference: `eval_literal`
    // short-circuits a bare identifier to `Value::Identifier(s)` in O(1) instead
    // of resolving it across the whole document scope (which made building the
    // root operand index quadratic over a large doc). Non-identifier labels
    // (string literals, interpolations) still evaluate at root, as before.
    let v = doc.eval_literal(first).ok()?;
    let matches = match &v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s == name,
        _ => false,
    };
    if matches { Some(v) } else { None }
}

/// Match `name` against a block's declared id field, for blocks whose
/// id is written as an ordinary field rather than a label.
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

/// Turn a resolved `DataRef` into a `Value`: a leaf yields its value,
/// and anything else becomes a [`Value::DataPath`] handle.
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
        DataKind::VariantValueList(vs) => Ok(Value::List(std::sync::Arc::new(vs.clone()))),
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
/// a repetition block's `each`. Used by the bare-identifier / member-access
/// evaluation path. The `&T`-reference deref path keeps the plain
/// `materialise_dataref_or_path` behaviour (a `Value::DataPath` handle) so
/// reflective builtins can keep walking the source.
fn materialise_dataref_value(
    dr: crate::data::DataRef<'_>,
    segments: Vec<String>,
    span: Span,
) -> Result<Value, EvalError> {
    // Thread the source-written path as the reification base, so a `@by_ref`
    // child slot (e.g. a wdoc `body`) reachable through this value reifies to
    // a root-resolvable `Value::DataPath` reference rather than inlined
    // content.
    if let Some(v) = views::dataref_to_value_at(&dr, &segments) {
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

/// Name a `DataKind` as diagnostics spell it (`block`, `type_field`,
/// …).
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
        DataKind::Error(_) => "error",
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
fn block_first_label(b: &Block<'_>) -> Option<String> {
    block_label_at(b, 0)
}

/// Label at slot `n` of a block, as a string. Used to read a declared
/// kind's name out of its declarer's `@inline(N)` label.
fn block_label_at(b: &Block<'_>, n: usize) -> Option<String> {
    match b.labels().ok()?.into_iter().nth(n)? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Some(s),
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

/// Whether `value` satisfies a *declared field*: [`value_matches_type_ref`]
/// plus the optional rule — a `T?` field accepts the `none` literal, so
/// writing absence out (`note = none`) is as legal as omitting the field.
///
/// The `?` lives on the declaration (`ast::TypeField::optional`), not in
/// `TypeRef`, so `value_matches_type_ref` cannot see it: on its own it
/// answers `false` for `none` against every concrete type. Every
/// value-vs-declared-type check goes through here so the two stay in step.
pub(crate) fn value_matches_declared(value: &Value, ty: &TypeRef, optional: bool) -> bool {
    (optional && matches!(value, Value::None)) || value_matches_type_ref(value, ty)
}

/// Conservative check that `value` could inhabit `ty`.
///
/// Deliberately one-sided: it returns `true` when it cannot decide, so
/// the schema check never rejects a value it merely failed to
/// understand.
pub(crate) fn value_matches_type_ref(value: &Value, ty: &TypeRef) -> bool {
    use crate::ast::BuiltinType as B;
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
        (Value::Symbol(_), TypeRef::Named { .. }) => true,
        (Value::None, _) => false, // None doesn't satisfy any concrete type
        // Numeric values satisfy any numeric builtin type: the evaluator
        // promotes numerics (an `f64` field authored as `520` holds an
        // i64 literal), so an exact-variant check here would flag values
        // the eval path accepts.
        (v, TypeRef::Builtin(b)) if v.is_numeric() && b.is_numeric() => true,
        // Variant value against a named union type: compare FQN.
        (Value::Variant { union, .. }, TypeRef::Named { path, .. }) => {
            path_matches_suffix(path, union)
        }
        // Record value against a named (non-union) type. Builtin-produced
        // records (e.g. `@connections` projections) carry the producing
        // declaration's FQN in `ty` — compare it. Bare record literals
        // (`ty` empty) stay permissive: matching them by shape would need
        // the declaration, which `Value` doesn't carry (mirrors the
        // tensor / function pass-through below).
        (Value::Record { ty, .. }, TypeRef::Named { path, .. }) => {
            ty.is_empty() || path_matches_suffix(path, ty)
        }
        // Lists check element type recursively — except that a `none`
        // *element* is always legal. An else-less `if` in a list literal
        // (`["base", if e.current { "current" }]`) contributes one, and
        // consumers drop it; the rule that a `none` never satisfies a
        // concrete type bounds a field's own value, not what a list may
        // hold on the way to being filtered.
        (Value::List(items), TypeRef::List(inner)) => items
            .iter()
            .all(|el| matches!(el, Value::None) || value_matches_type_ref(el, inner)),
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

/// When `ty` (already alias-resolved) names a `symbol_set` and `value`
/// is a symbol that isn't one of its members, return the membership
/// violation; otherwise `None`. Mirrors the connection-kind membership
/// check (`schema_check::connection_errors`) so a `status: SomeSet`
/// field rejects an out-of-set symbol identically whether the block is a
/// document child or nested — `value_matches_type_ref` stays permissive
/// for `(Symbol, Named)` because it lacks the declaration to check.
pub(crate) fn symbol_set_membership_error_in(
    doc: &Document,
    ty: &TypeRef,
    value: &Value,
    field_name: &str,
    span: crate::ast::Span,
    context_ns: &[String],
) -> Option<EvalError> {
    let TypeRef::Named { path, .. } = ty else {
        return None;
    };
    let resolved = doc
        .resolve_path_in(path, context_ns)
        .unwrap_or_else(|| path.clone());
    let ss = doc.symbol_set(&resolved.join("."))?;
    let Value::Symbol(sym) = value else {
        return None;
    };
    if ss.has(sym) {
        return None;
    }
    Some(EvalError::schema_violation(
        crate::error::SchemaViolationKind::SymbolNotInSet,
        format!(
            "field '{field_name}' declared as symbol_set '{}' but ':{sym}' is not one of its members",
            path.join(".")
        ),
        span,
    ))
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
///
/// Type arguments don't distinguish anything — dispatch resolves a
/// named type by path — so `A(S<X>)` and `B(S<Y>)` collide just as
/// `A(S)` and `B(S)` do.
fn variant_bodies_collide(a: &ast::VariantBody, b: &ast::VariantBody) -> bool {
    use ast::VariantBody as VB;
    match (a, b) {
        (VB::Unit, VB::Unit) => true,
        (VB::TypeRef { ty: a, .. }, VB::TypeRef { ty: b, .. }) => a.same_ignoring_type_args(b),
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
            a_sorted
                .iter()
                .zip(b_sorted.iter())
                .all(|((an, at), (bn, bt))| an == bn && at.same_ignoring_type_args(bt))
        }
        _ => false,
    }
}

/// Whether a pattern's (possibly unqualified) union path matches a
/// fully-qualified union name, comparing from the right.
fn path_matches_suffix(pat_path: &[String], union_fqn: &[String]) -> bool {
    if pat_path.len() > union_fqn.len() {
        return false;
    }
    let offset = union_fqn.len() - pat_path.len();
    union_fqn[offset..] == *pat_path
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
