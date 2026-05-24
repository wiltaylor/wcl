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
pub use loader::{FileLoader, disk_loader, overlay_loader};
pub use views::{
    Block, ChildKind, Connection, ConnectionDecl, DeclName, Decorator, Field, InterfaceDecl,
    NamedArg, ResolvedType, RowView, SymbolEntry, SymbolSetDecl, TableView, TypeDecl, TypeField,
    UnionDecl, UnionVariant, UseDeclView, UseFormView, UseItem, VariantBodyView,
};
pub(crate) use views::{BuiltinDecorator, UnionChildKind};

use crate::ast::{self, Span};
use crate::environment::Environment;
use crate::error::{EvalError, ParseError};
use crate::parser::Parser;
use crate::symbols::{SymbolIndex, SymbolKind, SymbolRecord};
#[cfg(test)]
use crate::value::{BuiltinType, TensorDim};
use crate::value::{TypeRef, Value};
use cells::{BlockCells, ItemCellKind, ItemCells, LoadedImport};
use imports::expand_top_level_imports;
use lookup::{find_block, find_field, iter_blocks, iter_fields};
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

impl Document {
    pub fn open(source: &str, name: &str) -> Result<Self, ParseError> {
        Self::open_with(source, name, &Environment::new())
    }

    pub fn open_with(source: &str, name: &str, env: &Environment) -> Result<Self, ParseError> {
        Self::open_at(source, name, None, env)
    }

    /// Variant of [`open_with`] that accepts a base directory for
    /// resolving relative `import` paths.
    pub(crate) fn open_at(
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
    pub(crate) fn open_at_with_loader(
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
        let mut loading: HashSet<PathBuf> = HashSet::new();
        let mut eager_imports: Vec<LoadedImport> = Vec::new();
        expand_top_level_imports(
            &ast.items,
            base_dir.as_deref(),
            &mut loading,
            &mut eager_imports,
            name,
            source,
            &loader,
        )?;

        let resolved = validate_document(&ast, &symbols, &synthetic, source, name)?;
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
        if segs.is_empty() {
            return None;
        }
        // Try the longest matching FQN prefix as the root, so a
        // dotted path can resolve directly to an imported item
        // (e.g. `doc.get("shared.brand")` for an imported file with
        // namespace `shared`). Falls through to single-segment root
        // (the existing block-then-child traversal).
        for k in (1..=segs.len()).rev() {
            let prefix = segs[..k].join(".");
            if let Some(root) = self.resolve_root(&prefix) {
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

    pub(crate) fn resolve_root(&self, name: &str) -> Option<crate::data::DataRef<'_>> {
        use crate::data::DataRef;
        // Document-schema-driven projections at the root: a field on
        // the `@document` type marked with `@connections(...)` is
        // synthesised from sibling Connection statements rather than
        // looked up as a literal Field item.
        if let Some(schema) = self.doc_schema()
            && let Some(field) = schema.field(name)
            && let Some(conn_schema) = field.connection_schema()
        {
            let mut all: Vec<Value> = Vec::new();
            for src in self.all_sources() {
                all.extend(self.project_connections(src.items, conn_schema, &Scope::root()));
            }
            return Some(DataRef::from_variant_value(Value::List(all)));
        }
        if let Some(f) = self.field(name) {
            return Some(DataRef::from_field(f));
        }
        if let Some(b) = self.block(name) {
            return Some(DataRef::from_block(b));
        }
        let qualified = self.qualified_name_public(name);
        if let Some(t) = self.type_decl(&qualified).or_else(|| self.type_decl(name)) {
            return Some(DataRef::from_type(t));
        }
        if let Some(u) = self
            .union_decl(&qualified)
            .or_else(|| self.union_decl(name))
        {
            return Some(DataRef::from_union(u));
        }
        if let Some(s) = self
            .symbol_set(&qualified)
            .or_else(|| self.symbol_set(name))
        {
            return Some(DataRef::from_symbol_set(s));
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
    ) -> Option<(Value, String)> {
        // Innermost scope frames first.
        for frame in scope.frames().iter().rev() {
            if let Some(found) = match_block_label_in_items(self, &frame.ast.items, name) {
                return Some(found);
            }
        }
        // Fall back to document root: walk every source's top-level items.
        for src in self.all_sources() {
            if let Some(found) = match_block_label_in_items(self, src.items, name) {
                return Some(found);
            }
        }
        None
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
        let source_fqn = self.resolve_type_fqn(schema.source_type());
        let dest_fqn = self.resolve_type_fqn(schema.destination_type());
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
        let (lhs_val, lhs_block_kind) = self.resolve_connection_operand(scope, &stmt.lhs)?;
        let (rhs_val, rhs_block_kind) = self.resolve_connection_operand(scope, &stmt.rhs)?;
        let lhs_ty = self
            .block_schema(&lhs_block_kind)
            .map(|t| t.name_segments().join("."))?;
        let rhs_ty = self
            .block_schema(&rhs_block_kind)
            .map(|t| t.name_segments().join("."))?;
        if Some(lhs_ty.as_str()) != source_fqn {
            return None;
        }
        if Some(rhs_ty.as_str()) != dest_fqn {
            return None;
        }
        let kind_name = match &stmt.kind {
            Some(s) => s.clone(),
            None => schema.default_kind()?,
        };
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

    /// Resolve a `TypeRef::Named` to its dotted FQN, if it points at a
    /// declared type/interface/union. Returns `None` for builtins,
    /// references, lists, etc.
    pub(crate) fn resolve_type_fqn(&self, t: &TypeRef) -> Option<String> {
        if let TypeRef::Named(path) = t {
            self.resolve_path(path).map(|p| p.join("."))
        } else {
            None
        }
    }

    /// Same composition rule as the evaluator's `qualified_name`, exposed
    /// here so `resolve_root` doesn't depend on a private helper.
    fn qualified_name_public(&self, name: &str) -> String {
        if self.file_ns.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", self.file_ns.join("."), name)
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
                        kind_override: None,
                        scope: Scope::root(),
                    });
                }
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
            .flat_map(move |src| iter_blocks(src.items, src.cells, doc, Scope::root()))
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
    pub fn type_decl(&self, fqn: &str) -> Option<TypeDecl<'_>> {
        for src in self.all_sources() {
            if let Some(rec) = src.symbols.lookup(fqn)
                && matches!(rec.kind, SymbolKind::TypeDecl)
                && let ast::Item::TypeDecl(t) = &src.items[rec.path.item_index]
            {
                return Some(TypeDecl {
                    ast: t,
                    file_ns: src.file_ns,
                    cells: &src.cells[rec.path.item_index],
                    doc: self,
                });
            }
        }
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
            });
        mine_and_imports.chain(syn)
    }

    /// Look up an interface declaration by fully-qualified name.
    /// Mirrors `type_decl` / `union_decl`.
    pub fn interface(&self, fqn: &str) -> Option<InterfaceDecl<'_>> {
        for src in self.all_sources() {
            if let Some(rec) = src.symbols.lookup(fqn)
                && matches!(rec.kind, SymbolKind::InterfaceDecl)
                && let ast::Item::InterfaceDecl(i) = &src.items[rec.path.item_index]
            {
                return Some(InterfaceDecl {
                    ast: i,
                    file_ns: src.file_ns,
                    cells: &src.cells[rec.path.item_index],
                    doc: self,
                });
            }
        }
        None
    }

    pub fn interfaces(&self) -> impl Iterator<Item = InterfaceDecl<'_>> + '_ {
        let doc = self;
        self.all_sources().into_iter().flat_map(move |src| {
            src.items
                .iter()
                .zip(src.cells.iter())
                .filter_map(move |(item, cells)| match item {
                    ast::Item::InterfaceDecl(i) => Some(InterfaceDecl {
                        ast: i,
                        file_ns: src.file_ns,
                        cells,
                        doc,
                    }),
                    _ => None,
                })
        })
    }

    /// Look up a union by fully-qualified name (dotted).
    pub fn union_decl(&self, fqn: &str) -> Option<UnionDecl<'_>> {
        for src in self.all_sources() {
            if let Some(rec) = src.symbols.lookup(fqn)
                && matches!(rec.kind, SymbolKind::UnionDecl)
                && let ast::Item::UnionDecl(u) = &src.items[rec.path.item_index]
            {
                return Some(UnionDecl {
                    ast: u,
                    file_ns: src.file_ns,
                    cells: &src.cells[rec.path.item_index],
                    doc: self,
                });
            }
        }
        None
    }

    pub fn union_decls(&self) -> impl Iterator<Item = UnionDecl<'_>> + '_ {
        let doc = self;
        self.all_sources().into_iter().flat_map(move |src| {
            src.items
                .iter()
                .zip(src.cells.iter())
                .filter_map(move |(item, cells)| match item {
                    ast::Item::UnionDecl(u) => Some(UnionDecl {
                        ast: u,
                        file_ns: src.file_ns,
                        cells,
                        doc,
                    }),
                    _ => None,
                })
        })
    }

    /// Look up a connection schema by fully-qualified name (dotted).
    pub fn connection_decl(&self, fqn: &str) -> Option<ConnectionDecl<'_>> {
        for src in self.all_sources() {
            if let Some(rec) = src.symbols.lookup(fqn)
                && matches!(rec.kind, SymbolKind::ConnectionDecl)
                && let ast::Item::ConnectionDecl(c) = &src.items[rec.path.item_index]
            {
                return Some(ConnectionDecl {
                    ast: c,
                    file_ns: src.file_ns,
                    doc: self,
                });
            }
        }
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
        for src in self.all_sources() {
            if let Some(rec) = src.symbols.lookup(fqn)
                && matches!(rec.kind, SymbolKind::SymbolSetDecl)
                && let ast::Item::SymbolSetDecl(s) = &src.items[rec.path.item_index]
            {
                return Some(SymbolSetDecl {
                    ast: s,
                    file_ns: src.file_ns,
                    cells: &src.cells[rec.path.item_index],
                    doc: self,
                });
            }
        }
        None
    }

    pub fn symbol_sets(&self) -> impl Iterator<Item = SymbolSetDecl<'_>> + '_ {
        let doc = self;
        self.all_sources().into_iter().flat_map(move |src| {
            src.items
                .iter()
                .zip(src.cells.iter())
                .filter_map(move |(item, cells)| match item {
                    ast::Item::SymbolSetDecl(s) => Some(SymbolSetDecl {
                        ast: s,
                        file_ns: src.file_ns,
                        cells,
                        doc,
                    }),
                    _ => None,
                })
        })
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

    /// Run the name-resolution algorithm on `path` against this document's
    /// file ns / aliases / wildcards / registry.
    fn resolve_path(&self, path: &[String]) -> Option<Vec<String>> {
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
        resolve_path(
            path,
            &self.file_ns,
            &self.item_aliases,
            &self.ns_aliases,
            &self.wildcards,
            &registry,
        )
    }

    /// Look up the type that schemas a block of the given kind, i.e. the
    /// first type carrying a `@block("kind")` decorator.
    pub fn block_schema(&self, kind: &str) -> Option<TypeDecl<'_>> {
        self.find_schema(BuiltinDecorator::Block, kind)
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

    /// Look up the type carrying the `@document` decorator, if any.
    /// At most one is expected; if multiple are declared, this
    /// returns the first and `Document::schema_errors` surfaces a
    /// `MultipleDocumentSchemas` violation.
    pub fn doc_schema(&self) -> Option<TypeDecl<'_>> {
        self.find_all_decorated(BuiltinDecorator::Document)
            .into_iter()
            .next()
    }

    /// Look up the `@document` type that governs `file_ns`.
    /// `@document` is scoped per-namespace: each namespace may declare
    /// at most one, and that schema only validates items declared in
    /// the same namespace. Imported files in a different namespace
    /// are validated by their own `@document` (or none if absent).
    pub(crate) fn doc_schema_for_ns(&self, file_ns: &[String]) -> Option<TypeDecl<'_>> {
        self.find_all_decorated(BuiltinDecorator::Document)
            .into_iter()
            .find(|t| t.file_ns() == file_ns)
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
        // is independent: a `@document` only governs items in its own
        // namespace, and at most one `@document` is allowed per ns.
        let doc_schemas = self.find_all_decorated(BuiltinDecorator::Document);
        let mut by_ns: BTreeMap<Vec<String>, Vec<TypeDecl<'_>>> = BTreeMap::new();
        for d in &doc_schemas {
            by_ns.entry(d.file_ns().to_vec()).or_default().push(*d);
        }
        for decls in by_ns.values() {
            for extra in decls.iter().skip(1) {
                EvalError::push_schema_violation(
                    &mut out,
                    Kind::MultipleDocumentSchemas,
                    format!("type '{}' declares an extra @document schema", extra.name()),
                    extra.span(),
                );
            }
        }

        // Walk every source (the main file + every eagerly-imported
        // file). Each source's items are validated against the
        // `@document` declared in that source's namespace, if any.
        for src in self.all_sources() {
            let root = by_ns.get(src.file_ns).and_then(|v| v.first()).copied();

            // Pre-compute the schema's union-typed children slots so
            // structurally-matched blocks bypass the kind check.
            let root_union_slots: Vec<UnionDecl<'_>> = root
                .map(|s| {
                    s.fields()
                        .filter_map(|f| {
                            f.children_kind_or_union()
                                .and_then(|k| k.as_union().copied())
                                .or_else(|| {
                                    f.child_kind_or_union().and_then(|k| k.as_union().copied())
                                })
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Walk the top-level fields in this source.
            for f in iter_fields(src.items, src.cells, self, Scope::root()) {
                if has_schemaless(&f.ast.decorators) {
                    continue;
                }
                match root {
                    Some(schema) => {
                        let Some(declared) = schema.field(f.name()) else {
                            EvalError::push_schema_violation(
                                &mut out,
                                Kind::UnknownField,
                                format!(
                                    "top-level field '{}' is not declared by @document schema '{}'",
                                    f.name(),
                                    schema.name()
                                ),
                                f.span(),
                            );
                            continue;
                        };
                        if let Ok(v) = f.value() {
                            if let TypeRef::Named(path) = declared.type_ref()
                                && let Some(union_decl) = self.union_decl(&path.join("."))
                            {
                                if let Value::Variant { union, variant, .. } = v
                                    && union != &union_decl.ast.name
                                {
                                    EvalError::push_schema_violation(
                                        &mut out,
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
                                }
                            } else if !value_matches_type_ref(v, declared.type_ref()) {
                                EvalError::push_schema_violation(
                                    &mut out,
                                    Kind::FieldTypeMismatch,
                                    format!(
                                        "field '{}' declared as {} but value is {}",
                                        f.name(),
                                        declared.type_ref(),
                                        v.type_name(),
                                    ),
                                    f.span(),
                                );
                            }
                        }
                    }
                    None => {
                        EvalError::push_schema_violation(
                            &mut out,
                            Kind::NoDocumentSchema,
                            format!("top-level field '{}' has no @document schema", f.name()),
                            f.span(),
                        );
                    }
                }
            }

            // Walk the top-level blocks in this source.
            for b in iter_blocks(src.items, src.cells, self, Scope::root()) {
                if has_schemaless(&b.ast.decorators) {
                    continue;
                }
                let dispatched_through_union = root_union_slots
                    .iter()
                    .any(|u| variant_dispatch::block_to_variant(self, &b, *u).is_ok());
                if dispatched_through_union {
                    continue;
                }
                if !self.is_registered_kind(b.kind()) {
                    let mut msg = format!(
                        "block kind '{}' has no @block or @table declaration",
                        b.kind()
                    );
                    if !root_union_slots.is_empty() {
                        let variants = format_union_variants_hint(self, &root_union_slots);
                        if !variants.is_empty() {
                            msg.push_str(&format!(" (nearby @children union accepts: {variants})"));
                        }
                    }
                    EvalError::push_schema_violation(
                        &mut out,
                        Kind::UnregisteredKind,
                        msg,
                        b.span(),
                    );
                    continue;
                }
                if let Some(schema) = root {
                    let allowed = schema.allowed_child_kinds();
                    if !allowed.iter().any(|k| k == b.kind()) {
                        EvalError::push_schema_violation(
                            &mut out,
                            Kind::DisallowedChild,
                            format!(
                                "block kind '{}' is not allowed at the document root by @document schema '{}'",
                                b.kind(),
                                schema.name()
                            ),
                            b.span(),
                        );
                    }
                } else {
                    EvalError::push_schema_violation(
                        &mut out,
                        Kind::NoDocumentSchema,
                        format!("top-level block '{}' has no @document schema", b.kind()),
                        b.span(),
                    );
                }
                for e in b.schema_errors() {
                    out.push(e.clone());
                }
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

    fn find_schema(&self, dec: BuiltinDecorator, value: &str) -> Option<TypeDecl<'_>> {
        let dec_name = dec.as_str();
        let want = Value::Utf8(value.to_string());
        self.type_decls().find(|t| {
            t.decorators().any(|d| {
                d.full_name() == dec_name
                    && d.positional()
                        .ok()
                        .and_then(|v| v.into_iter().next())
                        .as_ref()
                        == Some(&want)
            })
        })
    }
}

/// Take a navigator returned from path evaluation and reduce it to a
/// concrete `Value`. For `Field` targets, this evaluates the field
/// (auto-deref); for any other target, it errors `NotALeaf`.
/// Scan a flat slice of items for an `Item::Block` whose first label
/// (evaluated) matches the given name. Returns the label value plus
/// the block kind so callers can dispatch on the block's declared type.
fn match_block_label_in_items(
    doc: &Document,
    items: &[ast::Item],
    name: &str,
) -> Option<(Value, String)> {
    for item in items {
        let ast::Item::Block(b) = item else {
            continue;
        };
        let Some(first) = b.labels.first() else {
            continue;
        };
        let v = doc.eval_in_scope(first, &Scope::root()).ok()?;
        let s = match &v {
            Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s.as_str(),
            _ => continue,
        };
        if s == name {
            return Some((v, b.kind.clone()));
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
    match dr.inner() {
        DataKind::Field(f) => f.value().cloned().map_err(|e| e.clone()),
        DataKind::VariantValue(v) => Ok(v.clone()),
        DataKind::VariantValueList(vs) => Ok(Value::List(vs.clone())),
        other => Ok(Value::DataPath {
            kind: describe_datakind(other).to_string(),
            segments,
        }),
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

fn describe_datakind(k: &crate::data::DataKind<'_>) -> &'static str {
    use crate::data::DataKind;
    match k {
        DataKind::Document(_) => "document",
        DataKind::Field(_) => "field",
        DataKind::Block(_) => "block",
        DataKind::BlockList(_) => "block list",
        DataKind::Table(_) => "table",
        DataKind::Type(_) => "type",
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
        (Value::None, _) => false, // None doesn't satisfy any concrete type
        // Variant value against a named union type: compare FQN.
        (Value::Variant { union, .. }, TypeRef::Named(path)) => path_matches_suffix(path, union),
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
            let candidates: Vec<String> = if doc.file_ns.is_empty() {
                vec![parent_path.join(".")]
            } else {
                vec![
                    format!("{}.{}", doc.file_ns.join("."), parent_path.join(".")),
                    parent_path.join("."),
                ]
            };
            for fqn in &candidates {
                if let Some(p) = doc.union_decl(fqn) {
                    walk(doc, p.ast, seen, out, visiting);
                    break;
                }
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
        (VB::Record(af), VB::Record(bf)) => {
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
        | E::Variant { span, .. }
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
