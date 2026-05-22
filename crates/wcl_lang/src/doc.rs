use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use miette::{NamedSource, SourceSpan};

use std::collections::{HashMap, HashSet};

mod cells;
mod effective_fields;
mod eval_ops;
mod imports;
mod interfaces;
mod lookup;
mod match_pat;
mod schema_check;
mod scope;
mod validate;
pub(super) mod variant_dispatch;

use crate::ast::{self, Span};
use crate::environment::Environment;
use crate::error::{EvalError, ParseError};
use crate::parser::Parser;
use crate::symbols::{SymbolIndex, SymbolKind};
use crate::value::{BuiltinType, FnParam, FnValue, TensorDim, TypeRef, Value};
use cells::{BlockCells, DecoratorCell, FieldCell, ItemCellKind, ItemCells, LoadedImport};
use effective_fields::{build_effective_fields, is_descendant_of_walk, lookup_effective_field};
use eval_ops::{apply_binary, apply_unary, as_bool, describe_expr, format_member_path};
use imports::{BlockSlice, expand_top_level_imports, load_import_lazily, push_loaded_imports};
use interfaces::{check_interface_conformance, dataref_concrete_type, same_type_decl};
use lookup::{find_block, find_field, iter_blocks, iter_fields, iter_tables};
use schema_check::{compute_schema_errors, has_schemaless};
use scope::{Scope, ScopeFrame};
use validate::{decl_fqn_matches, resolve_path, validate_document};

#[derive(Debug)]
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
    /// Optional profile collector. Populated only when the document is
    /// opened through one of the `*_profiled` constructors; otherwise
    /// every profile hook is a no-op `Option::is_some` check.
    profile: Option<std::sync::Mutex<crate::profile::ProfileState>>,
}

/// A homogeneous view over one source of top-level items — either the
/// importer's own source or an eagerly-loaded import.
#[derive(Clone, Copy)]
struct SourceView<'a> {
    symbols: &'a SymbolIndex,
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    file_ns: &'a [String],
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
        let source = std::fs::read_to_string(path)?;
        let base_dir = path.parent().map(Path::to_path_buf);
        let mut doc = Self::open_at(
            &source,
            &path.display().to_string(),
            base_dir,
            &Environment::new(),
        )?;
        doc.profile = Some(crate::profile::ProfileState::new_root());
        Ok(doc)
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
    fn all_sources(&self) -> Vec<SourceView<'_>> {
        let mut out = vec![SourceView {
            symbols: &self.symbols,
            items: &self.ast.items,
            cells: &self.cells.items,
            file_ns: &self.file_ns,
        }];
        fn push_imports<'a>(imports: &'a [LoadedImport], out: &mut Vec<SourceView<'a>>) {
            for imp in imports {
                out.push(SourceView {
                    symbols: &imp.symbols,
                    items: &imp.items,
                    cells: &imp.cells,
                    file_ns: &imp.file_ns,
                });
                push_imports(&imp.eager_imports, out);
            }
        }
        push_imports(&self.eager_imports, &mut out);
        out
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
        let source = std::fs::read_to_string(path)?;
        let base_dir = path.parent().map(Path::to_path_buf);
        Self::open_at(
            &source,
            &path.display().to_string(),
            base_dir,
            &Environment::new(),
        )
    }

    /// Like [`from_file`] but also accepts a custom `Environment`. Use
    /// this when the host registers built-ins or schema types.
    pub fn from_file_with(path: &Path, env: &Environment) -> Result<Self, ParseError> {
        let source = std::fs::read_to_string(path)?;
        let base_dir = path.parent().map(Path::to_path_buf);
        Self::open_at(&source, &path.display().to_string(), base_dir, env)
    }

    pub fn source(&self) -> &NamedSource<String> {
        &self.src
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
        let mut out = Vec::new();

        // Detect multiple @document declarations (the first one
        // wins for `doc_schema()` but the duplicates are surfaced
        // as a violation).
        let doc_schemas = self.find_all_decorated(BuiltinDecorator::Document);
        for extra in doc_schemas.iter().skip(1) {
            out.push(EvalError::schema_violation(
                Kind::MultipleDocumentSchemas,
                format!("type '{}' declares an extra @document schema", extra.name()),
                extra.span(),
            ));
        }
        let root = doc_schemas.first().copied();

        // Walk the top-level fields.
        for f in self.fields() {
            if has_schemaless(&f.ast.decorators) {
                continue;
            }
            match root {
                Some(schema) => {
                    let Some(declared) = schema.field(f.name()) else {
                        out.push(EvalError::schema_violation(
                            Kind::UnknownField,
                            format!(
                                "top-level field '{}' is not declared by @document schema '{}'",
                                f.name(),
                                schema.name()
                            ),
                            f.span(),
                        ));
                        continue;
                    };
                    // Value-vs-declared-type check.
                    if let Ok(v) = f.value() {
                        // Union path: variant FQN must match.
                        if let TypeRef::Named(path) = declared.type_ref()
                            && let Some(union_decl) = self.union_decl(&path.join("."))
                        {
                            if let Value::Variant { union, variant, .. } = v
                                && union != &union_decl.ast.name
                            {
                                out.push(EvalError::schema_violation(
                                    Kind::VariantUnionMismatch,
                                    format!(
                                        "field '{}' declared as union '{}' but value is {}::{}",
                                        f.name(),
                                        union_decl.ast.name.join("."),
                                        union.join("."),
                                        variant,
                                    ),
                                    f.span(),
                                ));
                            }
                        } else if !value_matches_type_ref(v, declared.type_ref()) {
                            out.push(EvalError::schema_violation(
                                Kind::FieldTypeMismatch,
                                format!(
                                    "field '{}' declared as {} but value is {}",
                                    f.name(),
                                    declared.type_ref(),
                                    v.type_name(),
                                ),
                                f.span(),
                            ));
                        }
                    }
                }
                None => {
                    out.push(EvalError::schema_violation(
                        Kind::NoDocumentSchema,
                        format!("top-level field '{}' has no @document schema", f.name()),
                        f.span(),
                    ));
                }
            }
        }

        // Pre-compute the @document schema's union-typed children
        // slots: a block matching one of these unions by structural
        // shape is accepted regardless of its kind name.
        let root_union_slots: Vec<UnionDecl<'_>> = root
            .map(|s| {
                s.fields()
                    .filter_map(|f| {
                        f.children_kind_or_union()
                            .and_then(|k| k.as_union().copied())
                            .or_else(|| f.child_kind_or_union().and_then(|k| k.as_union().copied()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Walk the top-level blocks.
        for b in self.blocks() {
            if has_schemaless(&b.ast.decorators) {
                continue;
            }
            // A block dispatched through a union @children slot is
            // exempt from kind registration + the disallowed-child
            // check + recursion into its own schema errors (the
            // dispatcher applies its own structural rules).
            let dispatched_through_union = root_union_slots
                .iter()
                .any(|u| variant_dispatch::block_to_variant(self, &b, *u).is_ok());
            if dispatched_through_union {
                continue;
            }
            // First: the kind must be registered.
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
                out.push(EvalError::schema_violation(
                    Kind::UnregisteredKind,
                    msg,
                    b.span(),
                ));
                // Skip nested validation — the block has no schema
                // to validate against.
                continue;
            }
            // Second: the doc schema (if any) must accept this kind.
            if let Some(schema) = root {
                let allowed = schema.allowed_child_kinds();
                if !allowed.iter().any(|k| k == b.kind()) {
                    out.push(EvalError::schema_violation(
                        Kind::DisallowedChild,
                        format!(
                            "block kind '{}' is not allowed at the document root by @document schema '{}'",
                            b.kind(),
                            schema.name()
                        ),
                        b.span(),
                    ));
                }
            } else {
                out.push(EvalError::schema_violation(
                    Kind::NoDocumentSchema,
                    format!("top-level block '{}' has no @document schema", b.kind()),
                    b.span(),
                ));
            }
            // Third: recurse into the block's own schema errors.
            for e in b.schema_errors() {
                out.push(e.clone());
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

/// Closed set of decorator names the document layer special-cases:
/// schema dispatch (`@block`, `@table`, `@document`, `@decorator`),
/// field shape (`@inline`, `@default`, `@child`, `@children`),
/// connection decomposition (`@connections`), and per-block schema
/// opt-out (`@schemaless`). User-defined decorators are matched by
/// their declared name and don't go through this enum.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum BuiltinDecorator {
    Block,
    Table,
    Document,
    Decorator,
    Schemaless,
    Inline,
    Default,
    Child,
    Children,
    Connections,
}

impl BuiltinDecorator {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            BuiltinDecorator::Block => "block",
            BuiltinDecorator::Table => "table",
            BuiltinDecorator::Document => "document",
            BuiltinDecorator::Decorator => "decorator",
            BuiltinDecorator::Schemaless => "schemaless",
            BuiltinDecorator::Inline => "inline",
            BuiltinDecorator::Default => "default",
            BuiltinDecorator::Child => "child",
            BuiltinDecorator::Children => "children",
            BuiltinDecorator::Connections => "connections",
        }
    }
}

/// Extract a `u64`-valued named argument from the first decorator in
/// `decs` whose `full_name()` matches `dec_name`. Returns `None` if the
/// decorator isn't present, the named arg isn't present, the eval
/// failed, or the value isn't a non-negative integer.
fn decorator_u64_named(
    decs: &[Decorator<'_>],
    dec: BuiltinDecorator,
    arg_name: &str,
) -> Option<u64> {
    let found = find_builtin_dec(decs, dec)?;
    found.named_arg(arg_name)?.ok()?.as_u64()
}

/// Borrow the first decorator on `decs` whose `full_name()` matches the
/// canonical name of `dec`. Used by view methods that special-case one
/// of the builtin decorators (e.g. `Field::default_value`, `Field::child`).
fn find_builtin_dec<'a, 'b>(
    decs: &'b [Decorator<'a>],
    dec: BuiltinDecorator,
) -> Option<&'b Decorator<'a>> {
    let name = dec.as_str();
    decs.iter().find(|d| d.full_name() == name)
}

#[derive(Debug)]
pub enum ResolvedType<'a> {
    Builtin(BuiltinType),
    Named(TypeDecl<'a>),
    Interface(InterfaceDecl<'a>),
    Union(UnionDecl<'a>),
    SymbolSet(SymbolSetDecl<'a>),
    Connection(ConnectionDecl<'a>),
    Reference(Box<ResolvedType<'a>>),
    List(Box<ResolvedType<'a>>),
    Tensor {
        element: Box<ResolvedType<'a>>,
        dims: &'a [TensorDim],
    },
    Function {
        params: Vec<ResolvedType<'a>>,
        return_ty: Box<ResolvedType<'a>>,
    },
}

/// Shared accessors for the four top-level declaration views (`TypeDecl`,
/// `InterfaceDecl`, `UnionDecl`, `SymbolSetDecl`). All four hold a
/// segment-path name plus the surrounding file namespace; the rendering of
/// `name`, `name_segments`, `full_name`, and `namespace` is identical.
pub trait DeclName<'a> {
    /// Path written in source (without the file namespace).
    fn name_segments(&self) -> &'a [String];

    /// The file namespace this declaration was parsed under.
    fn file_ns(&self) -> &'a [String];

    /// Last segment of the declared name.
    fn name(&self) -> &'a str {
        self.name_segments()
            .last()
            .map(String::as_str)
            .expect("name has at least one segment")
    }

    /// Fully-qualified name as a dotted string: `file_ns + name_segments`.
    fn full_name(&self) -> String {
        self.file_ns()
            .iter()
            .chain(self.name_segments().iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(".")
    }

    /// Namespace path containing this declaration: `file_ns +
    /// name_segments[..-1]`. Empty when the declaration sits directly in
    /// the file namespace with a single-segment name.
    fn namespace(&self) -> Vec<String> {
        let segs = self.name_segments();
        let mut v: Vec<String> = self.file_ns().to_vec();
        if segs.len() > 1 {
            v.extend(segs[..segs.len() - 1].iter().cloned());
        }
        v
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UnionDecl<'a> {
    pub(super) ast: &'a ast::UnionDecl,
    pub(super) file_ns: &'a [String],
    pub(super) cells: &'a ItemCells,
    pub(super) doc: &'a Document,
}

impl<'a> DeclName<'a> for UnionDecl<'a> {
    fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }
    fn file_ns(&self) -> &'a [String] {
        self.file_ns
    }
}

impl<'a> UnionDecl<'a> {
    fn variant_decorator_cells(&self) -> &'a [Vec<DecoratorCell>] {
        let ItemCellKind::UnionDecl {
            variant_decorators, ..
        } = &self.cells.kind
        else {
            unreachable!("UnionDecl view wraps a UnionDecl cell")
        };
        variant_decorators
    }

    fn variant_field_cells(&self) -> &'a [Vec<Vec<DecoratorCell>>] {
        let ItemCellKind::UnionDecl {
            variant_field_decorators,
            ..
        } = &self.cells.kind
        else {
            unreachable!("UnionDecl view wraps a UnionDecl cell")
        };
        variant_field_decorators
    }

    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        let doc = self.doc;
        self.ast
            .decorators
            .iter()
            .zip(self.cells.decorators.iter())
            .map(move |(ast, cell)| Decorator { ast, cell, doc })
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn variants(&self) -> impl Iterator<Item = UnionVariant<'a>> + 'a {
        let doc = self.doc;
        let variant_cells = self.variant_decorator_cells();
        let field_cells = self.variant_field_cells();
        self.ast
            .variants
            .iter()
            .enumerate()
            .map(move |(i, v)| UnionVariant {
                ast: v,
                decorator_cells: &variant_cells[i],
                field_decorator_cells: &field_cells[i],
                doc,
            })
    }

    pub fn variant(&self, name: &str) -> Option<UnionVariant<'a>> {
        let variant_cells = self.variant_decorator_cells();
        let field_cells = self.variant_field_cells();
        self.ast
            .variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == name)
            .map(|(i, v)| UnionVariant {
                ast: v,
                decorator_cells: &variant_cells[i],
                field_decorator_cells: &field_cells[i],
                doc: self.doc,
            })
    }
}

#[derive(Clone, Copy)]
pub struct UnionVariant<'a> {
    ast: &'a ast::UnionVariant,
    decorator_cells: &'a [DecoratorCell],
    field_decorator_cells: &'a [Vec<DecoratorCell>],
    doc: &'a Document,
}

impl<'a> UnionVariant<'a> {
    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        let doc = self.doc;
        self.ast
            .decorators
            .iter()
            .zip(self.decorator_cells.iter())
            .map(move |(ast, cell)| Decorator { ast, cell, doc })
    }

    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn body(&self) -> VariantBodyView<'a> {
        match &self.ast.body {
            ast::VariantBody::Record(_) => VariantBodyView::Record,
            ast::VariantBody::TypeRef { ty, .. } => VariantBodyView::TypeRef(ty),
            ast::VariantBody::InterfaceRef { iface, .. } => VariantBodyView::InterfaceRef(iface),
            ast::VariantBody::Unit => VariantBodyView::Unit,
        }
    }

    pub fn fields(&self) -> Box<dyn Iterator<Item = TypeField<'a>> + 'a> {
        let doc = self.doc;
        let field_cells = self.field_decorator_cells;
        match &self.ast.body {
            ast::VariantBody::Record(fields) => {
                Box::new(fields.iter().enumerate().map(move |(i, f)| TypeField {
                    ast: f,
                    decorator_cells: &field_cells[i],
                    doc,
                }))
            }
            _ => Box::new(std::iter::empty()),
        }
    }

    pub fn field(&self, name: &str) -> Option<TypeField<'a>> {
        match &self.ast.body {
            ast::VariantBody::Record(fields) => fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.name == name)
                .map(|(i, f)| TypeField {
                    ast: f,
                    decorator_cells: &self.field_decorator_cells[i],
                    doc: self.doc,
                }),
            _ => None,
        }
    }
}

/// Source kind for a union-typed `@children(SomeUnion)` element —
/// nested block in source form, or a synthesised row from an
/// `Item::Table`. Decides which dispatcher we hand the block off to.
#[derive(Clone, Copy)]
pub(crate) enum UnionChildKind {
    Nested,
    TableRow,
}

/// Resolves the positional argument of an `@child` / `@children`
/// decorator into one of two acceptable shapes: a string kind name
/// (the legacy form) or a reference to a `UnionDecl` (structural
/// dispatch). Mirrors the namespace-resolution dance used elsewhere
/// for path lookups.
pub enum ChildKind<'a> {
    /// `@child("button")` — match nested blocks by their `kind`.
    Kind(String),
    /// `@child(Component)` — match nested blocks by structural shape
    /// against the union's variants.
    Union(UnionDecl<'a>),
}

impl<'a> ChildKind<'a> {
    pub fn as_kind(&self) -> Option<&str> {
        match self {
            ChildKind::Kind(s) => Some(s.as_str()),
            ChildKind::Union(_) => None,
        }
    }

    pub fn as_union(&self) -> Option<&UnionDecl<'a>> {
        match self {
            ChildKind::Kind(_) => None,
            ChildKind::Union(u) => Some(u),
        }
    }
}

fn resolve_child_kind_arg<'a>(doc: &'a Document, positional: &[Value]) -> Option<ChildKind<'a>> {
    let first = positional.first()?;
    match first {
        Value::Utf8(s) | Value::Ascii(s) => Some(ChildKind::Kind(s.clone())),
        Value::Identifier(name) => {
            let candidates: Vec<String> = if doc.file_ns.is_empty() {
                vec![name.clone()]
            } else {
                vec![format!("{}.{}", doc.file_ns.join("."), name), name.clone()]
            };
            for fqn in &candidates {
                if let Some(u) = doc.union_decl(fqn) {
                    return Some(ChildKind::Union(u));
                }
            }
            None
        }
        _ => None,
    }
}

pub enum VariantBodyView<'a> {
    Record,
    TypeRef(&'a TypeRef),
    /// Variant body of the form `&InterfaceName`: payload is any value
    /// implementing the interface. The slice borrows the path segments
    /// declared in source.
    InterfaceRef(&'a [String]),
    Unit,
}

#[derive(Debug)]
pub struct Decorator<'a> {
    ast: &'a ast::Decorator,
    cell: &'a DecoratorCell,
    doc: &'a Document,
}

impl<'a> Decorator<'a> {
    pub fn name(&self) -> &'a str {
        self.ast
            .name
            .last()
            .expect("decorator name has at least one segment")
    }

    pub fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }

    pub fn full_name(&self) -> String {
        self.ast.name.join(".")
    }

    /// `true` if this decorator's single-segment name matches the
    /// canonical name of `dec`. Cheap (no allocation, unlike
    /// `full_name()`), so prefer this for filtering against builtin
    /// decorator names.
    pub(crate) fn is(&self, dec: BuiltinDecorator) -> bool {
        self.ast.name.len() == 1 && self.ast.name[0] == dec.as_str()
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Evaluate every positional argument. The result is cached so
    /// repeated calls return the same eval outcome without re-running.
    pub fn positional(&self) -> Result<Vec<Value>, EvalError> {
        let result = self.cell.positional.get_or_init(|| {
            self.ast
                .positional
                .iter()
                .map(|e| self.doc.eval(e))
                .collect()
        });
        match result {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(e.clone()),
        }
    }

    pub fn named(&self) -> impl Iterator<Item = NamedArg<'a>> + 'a {
        let parent_ast = self.ast;
        let cell = self.cell;
        let doc = self.doc;
        self.ast.named.iter().map(move |n| NamedArg {
            ast: n,
            parent_ast,
            parent: cell,
            doc,
        })
    }

    pub fn named_arg(&self, name: &str) -> Option<Result<Value, EvalError>> {
        let map = self.cell.named.get_or_init(|| {
            self.ast
                .named
                .iter()
                .map(|n| (n.name.clone(), self.doc.eval(&n.value)))
                .collect()
        });
        map.get(name).map(|r| match r {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(e.clone()),
        })
    }

    /// Resolve the value of one declared slot on this decorator's
    /// schema. If the slot's declared type is a union, the
    /// decorator's positional + named args are dispatched into a
    /// `Value::Variant` by structural shape. Otherwise, the named
    /// arg is consulted first, then the positional arg at the slot's
    /// declaration index — so `@block("books")` resolves the `name`
    /// slot from positional[0] when no `name = ...` was written.
    ///
    /// Returns `None` when the decorator has no registered schema, the
    /// schema doesn't declare a slot of this name, or neither a named
    /// arg nor a positional arg fills it.
    pub fn resolved_arg_value(&self, slot_name: &str) -> Option<Result<Value, EvalError>> {
        let schema_name = self.ast.name.last()?;
        let schema = self.doc.decorator_schema(schema_name)?;
        let slot = schema.field(slot_name)?;
        // If the slot is union-typed, dispatch the decorator's args.
        if let TypeRef::Named(path) = slot.type_ref()
            && let Some(union) = self.doc.union_decl(&path.join("."))
        {
            return Some(self.dispatch_into_union(union));
        }
        if let Some(v) = self.named_arg(slot_name) {
            return Some(v);
        }
        let slot_idx = schema.fields().position(|f| f.name() == slot_name)?;
        let positional = match self.positional() {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        positional.into_iter().nth(slot_idx).map(Ok)
    }

    /// Dispatch the decorator's positional + named args into a
    /// `Value::Variant` for the given union, by structural shape.
    /// Returns `VariantNoMatch` if the args don't fit any variant,
    /// `VariantAmbiguous` defensively if multiple variants match.
    pub fn dispatch_into_union(&self, union: UnionDecl<'a>) -> Result<Value, EvalError> {
        let positional = self.positional()?;
        let mut named_map: std::collections::BTreeMap<String, Value> =
            std::collections::BTreeMap::new();
        for n in self.named() {
            let v = n.value()?;
            named_map.insert(n.name().to_string(), v);
        }
        variant_dispatch::decorator_to_variant(
            self.doc,
            &positional,
            &named_map,
            union,
            self.ast.span,
        )
    }
}

pub struct NamedArg<'a> {
    ast: &'a ast::NamedArg,
    /// The parent decorator's full AST, used to seed the shared named-arg
    /// cache on first access from any sibling.
    parent_ast: &'a ast::Decorator,
    parent: &'a DecoratorCell,
    doc: &'a Document,
}

impl<'a> NamedArg<'a> {
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    /// Cached via the parent [`DecoratorCell`]'s named-arg map.
    pub fn value(&self) -> Result<Value, EvalError> {
        let map = self.parent.named.get_or_init(|| {
            self.parent_ast
                .named
                .iter()
                .map(|n| (n.name.clone(), self.doc.eval(&n.value)))
                .collect()
        });
        match map.get(&self.ast.name) {
            Some(Ok(v)) => Ok(v.clone()),
            Some(Err(e)) => Err(e.clone()),
            None => self.doc.eval(&self.ast.value),
        }
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }
}

/// Public view of an `lhs -> rhs [:sym]` connection statement.
#[derive(Debug, Clone, Copy)]
pub struct Connection<'a> {
    pub(super) ast: &'a ast::ConnectionStmt,
}

impl<'a> Connection<'a> {
    pub fn source(&self) -> &'a str {
        &self.ast.lhs
    }

    pub fn destination(&self) -> &'a str {
        &self.ast.rhs
    }

    /// Explicit `:kind` symbol if present, or `None` when the writer
    /// relied on the connection schema's default symbol.
    pub fn kind(&self) -> Option<&'a str> {
        self.ast.kind.as_deref()
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConnectionDecl<'a> {
    pub(super) ast: &'a ast::ConnectionDecl,
    pub(super) file_ns: &'a [String],
    pub(super) doc: &'a Document,
}

impl<'a> DeclName<'a> for ConnectionDecl<'a> {
    fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }
    fn file_ns(&self) -> &'a [String] {
        self.file_ns
    }
}

impl<'a> ConnectionDecl<'a> {
    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn source_type(&self) -> &'a TypeRef {
        &self.ast.source
    }

    pub fn destination_type(&self) -> &'a TypeRef {
        &self.ast.destination
    }

    /// FQN segments of the symbol_set that the connection's `kind`
    /// is drawn from.
    pub fn kind_set_path(&self) -> &'a [String] {
        &self.ast.kind_set
    }

    /// Resolve the kind symbol_set to its declaration.
    pub fn kind_set(&self) -> Option<SymbolSetDecl<'a>> {
        let fqn = self.doc.resolve_path(&self.ast.kind_set)?;
        self.doc.symbol_set(&fqn.join("."))
    }

    /// First symbol in the kind set; used as the default when a
    /// connection statement omits an explicit symbol.
    pub fn default_kind(&self) -> Option<String> {
        self.kind_set()?
            .symbols()
            .next()
            .map(|s| s.name().to_string())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SymbolSetDecl<'a> {
    pub(super) ast: &'a ast::SymbolSetDecl,
    pub(super) file_ns: &'a [String],
    pub(super) cells: &'a ItemCells,
    pub(super) doc: &'a Document,
}

impl<'a> DeclName<'a> for SymbolSetDecl<'a> {
    fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }
    fn file_ns(&self) -> &'a [String] {
        self.file_ns
    }
}

impl<'a> SymbolSetDecl<'a> {
    fn symbol_decorator_cells(&self) -> &'a [Vec<DecoratorCell>] {
        let ItemCellKind::SymbolSetDecl { symbol_decorators } = &self.cells.kind else {
            unreachable!("SymbolSetDecl view wraps a SymbolSetDecl cell")
        };
        symbol_decorators
    }

    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        let doc = self.doc;
        self.ast
            .decorators
            .iter()
            .zip(self.cells.decorators.iter())
            .map(move |(ast, cell)| Decorator { ast, cell, doc })
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn symbols(&self) -> impl Iterator<Item = SymbolEntry<'a>> + 'a {
        let doc = self.doc;
        let cells = self.symbol_decorator_cells();
        self.ast
            .symbols
            .iter()
            .enumerate()
            .map(move |(i, s)| SymbolEntry {
                ast: s,
                decorator_cells: &cells[i],
                doc,
            })
    }

    pub fn has(&self, name: &str) -> bool {
        self.ast.symbols.iter().any(|s| s.name == name)
    }
}

#[derive(Clone, Copy)]
pub struct SymbolEntry<'a> {
    ast: &'a ast::SymbolEntry,
    decorator_cells: &'a [DecoratorCell],
    doc: &'a Document,
}

impl<'a> SymbolEntry<'a> {
    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        let doc = self.doc;
        self.ast
            .decorators
            .iter()
            .zip(self.decorator_cells.iter())
            .map(move |(ast, cell)| Decorator { ast, cell, doc })
    }

    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TypeDecl<'a> {
    pub(super) ast: &'a ast::TypeDecl,
    pub(super) file_ns: &'a [String],
    pub(super) cells: &'a ItemCells,
    pub(super) doc: &'a Document,
}

impl<'a> DeclName<'a> for TypeDecl<'a> {
    fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }
    fn file_ns(&self) -> &'a [String] {
        self.file_ns
    }
}

impl<'a> TypeDecl<'a> {
    fn field_decorator_cells(&self) -> &'a [Vec<DecoratorCell>] {
        let ItemCellKind::TypeDecl { field_decorators } = &self.cells.kind else {
            unreachable!("TypeDecl view wraps a TypeDecl cell")
        };
        field_decorators
    }

    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        let doc = self.doc;
        self.ast
            .decorators
            .iter()
            .zip(self.cells.decorators.iter())
            .map(move |(ast, cell)| Decorator { ast, cell, doc })
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// `max_children = N` named arg on the type's `@block(...)`. Caps
    /// the total number of nested blocks inside an instance of this
    /// type.
    pub fn max_children(&self) -> Option<u64> {
        let decs: Vec<_> = self.decorators().collect();
        decorator_u64_named(&decs, BuiltinDecorator::Block, "max_children")
    }

    /// `required_children = ["kind", ...]` named arg on the type's
    /// `@block(...)` decorator. Each listed kind must appear at least
    /// once in any instance of this type. Non-string entries in the
    /// list are silently dropped.
    pub fn required_children(&self) -> Vec<String> {
        let decs: Vec<_> = self.decorators().collect();
        let dec = match find_builtin_dec(&decs, BuiltinDecorator::Block) {
            Some(d) => d,
            None => return Vec::new(),
        };
        let arg = match dec.named_arg("required_children") {
            Some(Ok(v)) => v,
            _ => return Vec::new(),
        };
        match arg {
            Value::List(items) => items
                .into_iter()
                .filter_map(|v| match v {
                    Value::Utf8(s) | Value::Ascii(s) => Some(s),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Implicit set of allowed child block kinds: the union of all
    /// `@child(K)` and `@children(K)` decorators across this type's
    /// fields. Any nested block whose kind isn't in this set is a
    /// schema violation.
    pub fn allowed_child_kinds(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for f in self.fields() {
            if let Some(k) = f.child_block_kind()
                && !out.contains(&k)
            {
                out.push(k);
            }
            if let Some(k) = f.children_block_kind()
                && !out.contains(&k)
            {
                out.push(k);
            }
        }
        out
    }

    pub fn fields(&self) -> impl Iterator<Item = TypeField<'a>> + 'a {
        let doc = self.doc;
        let cells = self.field_decorator_cells();
        self.ast
            .fields
            .iter()
            .enumerate()
            .map(move |(i, f)| TypeField {
                ast: f,
                decorator_cells: &cells[i],
                doc,
            })
    }

    pub fn field(&self, name: &str) -> Option<TypeField<'a>> {
        let cells = self.field_decorator_cells();
        self.ast
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
            .map(|(i, f)| TypeField {
                ast: f,
                decorator_cells: &cells[i],
                doc: self.doc,
            })
    }

    /// Names of parent types/interfaces this type extends, in
    /// source order. Each entry is a path (dotted name segments).
    pub fn extends(&self) -> &'a [Vec<String>] {
        &self.ast.extends
    }

    /// Iterate this type's fields plus those inherited from its
    /// `extends` chain (transitively). Ancestor fields are emitted
    /// before the type's own, in extends-list order. Duplicate
    /// names (identical-type child redeclarations) are emitted
    /// once: the *latest* (child-most) definition wins.
    pub fn effective_fields(&self) -> Vec<TypeField<'a>> {
        build_effective_fields(self.doc, &self.ast.extends, self.fields())
    }

    /// Like `effective_fields()` but optimised for a one-shot
    /// lookup. Returns the resolved `TypeField` for the named field
    /// considering the full extends chain.
    pub fn effective_field(&self, name: &str) -> Option<TypeField<'a>> {
        lookup_effective_field(self.doc, &self.ast.extends, |n| self.field(n), name)
    }

    /// `true` if `other` appears anywhere in `self`'s transitive
    /// `extends` chain. Used by the reference-acceptance check.
    pub fn is_descendant_of(&self, other_fqn: &str) -> bool {
        let mut seen: HashSet<String> = HashSet::new();
        is_descendant_of_walk(self.doc, &self.ast.extends, other_fqn, &mut seen)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InterfaceDecl<'a> {
    pub(super) ast: &'a ast::InterfaceDecl,
    pub(super) file_ns: &'a [String],
    pub(super) cells: &'a ItemCells,
    pub(super) doc: &'a Document,
}

impl<'a> DeclName<'a> for InterfaceDecl<'a> {
    fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }
    fn file_ns(&self) -> &'a [String] {
        self.file_ns
    }
}

impl<'a> InterfaceDecl<'a> {
    fn field_decorator_cells(&self) -> &'a [Vec<DecoratorCell>] {
        let ItemCellKind::InterfaceDecl { field_decorators } = &self.cells.kind else {
            unreachable!("InterfaceDecl view wraps an InterfaceDecl cell")
        };
        field_decorators
    }

    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        let doc = self.doc;
        self.ast
            .decorators
            .iter()
            .zip(self.cells.decorators.iter())
            .map(move |(ast, cell)| Decorator { ast, cell, doc })
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn fields(&self) -> impl Iterator<Item = TypeField<'a>> + 'a {
        let doc = self.doc;
        let cells = self.field_decorator_cells();
        self.ast
            .fields
            .iter()
            .enumerate()
            .map(move |(i, f)| TypeField {
                ast: f,
                decorator_cells: &cells[i],
                doc,
            })
    }

    pub fn field(&self, name: &str) -> Option<TypeField<'a>> {
        let cells = self.field_decorator_cells();
        self.ast
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
            .map(|(i, f)| TypeField {
                ast: f,
                decorator_cells: &cells[i],
                doc: self.doc,
            })
    }

    /// Names of parent types/interfaces this interface extends.
    pub fn extends(&self) -> &'a [Vec<String>] {
        &self.ast.extends
    }

    pub fn effective_fields(&self) -> Vec<TypeField<'a>> {
        build_effective_fields(self.doc, &self.ast.extends, self.fields())
    }

    pub fn effective_field(&self, name: &str) -> Option<TypeField<'a>> {
        lookup_effective_field(self.doc, &self.ast.extends, |n| self.field(n), name)
    }
}

pub struct UseDeclView<'a> {
    ast: &'a ast::UseDecl,
}

impl<'a> UseDeclView<'a> {
    pub fn path(&self) -> &'a [String] {
        &self.ast.path
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn form(&self) -> UseFormView<'a> {
        match &self.ast.form {
            ast::UseForm::Bare(alias) => UseFormView::Bare(alias.as_deref()),
            ast::UseForm::List(_) => UseFormView::List,
        }
    }

    /// If this `use` is a brace-list form, iterate its items.
    pub fn items(&self) -> Box<dyn Iterator<Item = UseItem<'a>> + 'a> {
        match &self.ast.form {
            ast::UseForm::List(items) => Box::new(items.iter().map(|i| UseItem { ast: i })),
            ast::UseForm::Bare(_) => Box::new(std::iter::empty()),
        }
    }
}

pub enum UseFormView<'a> {
    Bare(Option<&'a str>),
    List,
}

pub struct UseItem<'a> {
    ast: &'a ast::UseItem,
}

impl<'a> UseItem<'a> {
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn alias(&self) -> Option<&'a str> {
        self.ast.alias.as_deref()
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }
}

#[derive(Clone, Copy)]
pub struct TypeField<'a> {
    pub(super) ast: &'a ast::TypeField,
    pub(super) decorator_cells: &'a [DecoratorCell],
    pub(super) doc: &'a Document,
}

impl<'a> TypeField<'a> {
    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        let doc = self.doc;
        self.ast
            .decorators
            .iter()
            .zip(self.decorator_cells.iter())
            .map(move |(ast, cell)| Decorator { ast, cell, doc })
    }

    /// If this field carries an `@inline(N)` decorator, returns N.
    /// Used by schemas to map block label slots to typed fields.
    pub fn inline_slot(&self) -> Option<u64> {
        let dec = self.decorators().find(|d| d.is(BuiltinDecorator::Inline))?;
        dec.positional().ok()?.first()?.as_u64()
    }

    /// If this field carries an `@default(v)` decorator, returns v.
    pub fn default_value(&self) -> Option<Value> {
        let dec = self
            .decorators()
            .find(|d| d.is(BuiltinDecorator::Default))?;
        dec.positional().ok()?.into_iter().next()
    }

    /// If this field carries an `@child("kind")` decorator, returns the
    /// nested block kind it binds. Returns `None` when the decorator
    /// is absent OR when its positional arg names a union type rather
    /// than a string kind (use [`child_kind_or_union`] for the union
    /// case).
    pub fn child_block_kind(&self) -> Option<String> {
        match self.child_kind_or_union()? {
            ChildKind::Kind(s) => Some(s),
            ChildKind::Union(_) => None,
        }
    }

    /// If this field carries an `@children("kind", min?, max?)`
    /// decorator, returns the nested block kind it binds. Returns
    /// `None` for the union form — use [`children_kind_or_union`].
    pub fn children_block_kind(&self) -> Option<String> {
        match self.children_kind_or_union()? {
            ChildKind::Kind(s) => Some(s),
            ChildKind::Union(_) => None,
        }
    }

    /// Resolves the positional arg of `@child(...)` into either a
    /// string kind or a union declaration. `None` when the decorator
    /// is absent or the arg is neither.
    pub fn child_kind_or_union(&self) -> Option<ChildKind<'a>> {
        let dec = self.decorators().find(|d| d.is(BuiltinDecorator::Child))?;
        resolve_child_kind_arg(self.doc, &dec.positional().ok()?)
    }

    /// Resolves the positional arg of `@children(...)` into either a
    /// string kind or a union declaration. `None` when the decorator
    /// is absent or the arg is neither.
    pub fn children_kind_or_union(&self) -> Option<ChildKind<'a>> {
        let dec = self
            .decorators()
            .find(|d| d.is(BuiltinDecorator::Children))?;
        resolve_child_kind_arg(self.doc, &dec.positional().ok()?)
    }

    /// Resolves the positional arg of `@connections(...)` into a
    /// connection schema. `None` if the decorator is absent or the
    /// positional arg doesn't name a declared connection.
    pub fn connection_schema(&self) -> Option<ConnectionDecl<'a>> {
        let dec = self
            .decorators()
            .find(|d| d.is(BuiltinDecorator::Connections))?;
        let positional = dec.positional().ok()?;
        let first = positional.first()?;
        let name = match first {
            Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s,
            _ => return None,
        };
        let candidates: Vec<String> = if self.doc.file_ns.is_empty() {
            vec![name.clone()]
        } else {
            vec![
                format!("{}.{}", self.doc.file_ns.join("."), name),
                name.clone(),
            ]
        };
        for fqn in &candidates {
            if let Some(c) = self.doc.connection_decl(fqn) {
                return Some(c);
            }
        }
        None
    }

    /// Like [`children_block_kind`] but borrows directly from the AST
    /// — useful when callers need a `&'a str` (e.g. to plug into a
    /// `Block::kind_override`). `None` if the decorator isn't present
    /// or the positional arg isn't a string literal.
    pub fn children_block_kind_str(&self) -> Option<&'a str> {
        let dec = self
            .ast
            .decorators
            .iter()
            .find(|d| d.name.last().map(|s| s == "children").unwrap_or(false))?;
        let first = dec.positional.first()?;
        match first {
            crate::ast::Expr::Utf8(s) | crate::ast::Expr::Ascii(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Optional `min` cardinality on `@children(...)`.
    pub fn children_min(&self) -> Option<u64> {
        decorator_u64_named(
            &self.decorators().collect::<Vec<_>>(),
            BuiltinDecorator::Children,
            "min",
        )
    }

    /// Optional `max` cardinality on `@children(...)`.
    pub fn children_max(&self) -> Option<u64> {
        decorator_u64_named(
            &self.decorators().collect::<Vec<_>>(),
            BuiltinDecorator::Children,
            "max",
        )
    }

    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn optional(&self) -> bool {
        self.ast.optional
    }

    pub fn type_ref(&self) -> &'a TypeRef {
        &self.ast.ty
    }
}

#[derive(Clone)]
pub struct Field<'a> {
    pub(super) ast: &'a ast::Field,
    pub(super) cells: &'a ItemCells,
    pub(super) doc: &'a Document,
    pub(super) scope: Scope<'a>,
}

impl<'a> Field<'a> {
    fn field_cell(&self) -> &'a FieldCell {
        let ItemCellKind::Field(c) = &self.cells.kind else {
            unreachable!("Field view wraps a Field cell")
        };
        c
    }

    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        let doc = self.doc;
        self.ast
            .decorators
            .iter()
            .zip(self.cells.decorators.iter())
            .map(move |(ast, cell)| Decorator { ast, cell, doc })
    }

    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn value(&self) -> Result<&'a Value, &'a EvalError> {
        let cell = self.field_cell();
        if let Some(cached) = cell.value.get() {
            return cached.as_ref();
        }
        let _profile_guard = self.doc.profile_enter(crate::profile::ProfileKey::Field {
            path: self.ast.name.clone(),
        });
        // Strict membership check (skipped when the field is
        // `@schemaless`). The field must be named by either the
        // enclosing block's schema or, for top-level fields, the
        // document's `@document` schema.
        if !has_schemaless(&self.ast.decorators)
            && let Some(err) = self.schema_membership_error()
        {
            let _ = cell.value.set(Err(err));
            return cell
                .value
                .get()
                .expect("just-set membership error")
                .as_ref();
        }
        if cell.evaluating.swap(true, Ordering::Acquire) {
            let _ = cell.value.set(Err(EvalError::Cycle {
                field: self.ast.name.clone(),
                span: span_to_miette(self.ast.span),
            }));
            return cell
                .value
                .get()
                .expect("cycle cell was just initialised")
                .as_ref();
        }
        // For `&T`-typed fields, evaluate the RHS as a path producing
        // a `DataRef`. If the target is a leaf `Field`, auto-deref to
        // its value. Otherwise (type / union / variant / block / …),
        // produce a `Value::DataPath` so reflective builtins can keep
        // walking. Non-reference fields evaluate normally through
        // `eval_in_scope`.
        let result = if matches!(self.declared_type_ref(), Some(TypeRef::Reference(_))) {
            let ctx = EvalCtx::new(self.scope.clone());
            self.doc
                .eval_to_dataref(&self.ast.expr, &ctx)
                .and_then(|dr| {
                    let segments = expr_to_path_segments(&self.ast.expr).unwrap_or_default();
                    materialise_dataref_or_path(dr, segments, self.ast.span)
                })
        } else {
            self.doc.eval_in_scope(&self.ast.expr, &self.scope)
        };
        cell.evaluating.store(false, Ordering::Release);
        cell.value.get_or_init(|| result).as_ref()
    }

    /// `Some(err)` if this field's name isn't accepted by the
    /// applicable schema (parent block, or the document if top-level).
    /// `None` means the membership check passes.
    fn schema_membership_error(&self) -> Option<EvalError> {
        use crate::error::SchemaViolationKind as Kind;
        let parent_schema = match self.scope.frames().last().copied() {
            Some(frame) => {
                // Whole-block opt-out shadows individual fields too.
                if has_schemaless(&frame.ast.decorators) {
                    return None;
                }
                let block = Block {
                    ast: frame.ast,
                    cells: frame.cells,
                    doc: self.doc,
                    kind_override: frame.kind_override,
                    scope: Scope::root(),
                };
                block.schema()
            }
            None => self.doc.doc_schema(),
        };
        match parent_schema {
            Some(schema) => {
                if schema.field(self.name()).is_some() {
                    None
                } else {
                    Some(EvalError::schema_violation(
                        Kind::UnknownField,
                        format!(
                            "field '{}' is not declared by schema '{}'",
                            self.name(),
                            schema.name()
                        ),
                        self.ast.span,
                    ))
                }
            }
            None => {
                // Top-level field with no @document schema is fine
                // when inside a schema'd block — we already short-
                // circuited above. Otherwise it's NoDocumentSchema.
                if self.scope.frames().is_empty() {
                    Some(EvalError::schema_violation(
                        Kind::NoDocumentSchema,
                        format!("top-level field '{}' has no @document schema", self.name()),
                        self.ast.span,
                    ))
                } else {
                    // Inside an un-schema'd block — the enclosing
                    // block's UnregisteredKind covers it.
                    None
                }
            }
        }
    }

    /// Returns the schema-declared `TypeRef` for this field, if the
    /// field lives inside a schema'd block and that schema declares
    /// it. Top-level fields and fields inside un-schema'd blocks
    /// return `None`.
    fn declared_type_ref(&self) -> Option<&'a TypeRef> {
        if let Some(frame) = self.scope.frames().last().copied() {
            let block = Block {
                ast: frame.ast,
                cells: frame.cells,
                doc: self.doc,
                kind_override: frame.kind_override,
                scope: Scope::root(),
            };
            let schema = block.schema()?;
            let schema_field = schema.field(self.name())?;
            return Some(schema_field.type_ref());
        }
        // Top-level field: consult the @document schema if present.
        let doc_schema = self.doc.doc_schema()?;
        let schema_field = doc_schema.field(self.name())?;
        Some(schema_field.type_ref())
    }

    /// For a `&T`-typed field, return the lazy navigator pointing at
    /// the referenced target.
    ///
    /// - `None` — the field is not declared as a reference.
    /// - `Some(Ok(dr))` — the reference resolves; `dr` walks the
    ///   target the same way `Document::get` would.
    /// - `Some(Err(e))` — the field is `&T` but the target can't be
    ///   resolved through the field's scope chain.
    pub fn reference(&self) -> Option<Result<crate::data::DataRef<'a>, EvalError>> {
        let declared = self.declared_type_ref()?;
        let TypeRef::Reference(inner) = declared else {
            return None;
        };
        let ctx = EvalCtx::new(self.scope.clone());
        let target_dr = match self.doc.eval_to_dataref(&self.ast.expr, &ctx) {
            Ok(d) => d,
            Err(e) => return Some(Err(e)),
        };

        // Apply interface conformance / ancestor-acceptance checks
        // only when the target has a statically-known concrete type
        // and the declared inner is a named path. For anything
        // unresolvable (raw blocks, lists, etc.) we trust the
        // navigator and skip both checks.
        if let TypeRef::Named(path) = inner.as_ref()
            && let Some(target_decl) = dataref_concrete_type(&target_dr, self.doc)
        {
            let key = path.join(".");
            // Case A: interface conformance.
            if let Some(iface) = self.doc.interface(&key) {
                if let Err(e) =
                    check_interface_conformance(self.doc, &iface, &target_decl, self.ast.span)
                {
                    return Some(Err(e));
                }
            } else if let Some(expected) = self.doc.type_decl(&key)
                && !same_type_decl(&expected, &target_decl)
                && !target_decl.is_descendant_of(&expected.full_name())
            {
                // Case B: ancestor acceptance for regular types.
                return Some(Err(EvalError::schema_violation(
                    crate::error::SchemaViolationKind::InterfaceNotImplemented,
                    format!(
                        "target type '{}' is not '{}' and does not extend it",
                        target_decl.full_name(),
                        expected.full_name(),
                    ),
                    self.ast.span,
                )));
            }
        }
        Some(Ok(target_dr))
    }
}

/// When a `DataRef` has a statically-known concrete type (because
/// it's a `Block` whose kind has a `@block`/`@table` schema, or a
/// `Field` whose declared type is a named type), return that
/// `TypeDecl`. Otherwise `None`.

#[derive(Clone)]
pub struct Block<'a> {
    pub(super) ast: &'a ast::Block,
    pub(super) cells: &'a ItemCells,
    pub(super) doc: &'a Document,
    /// When `Some`, overrides `ast.kind` for views derived from a
    /// synthesised row-Block (its stored `kind` is blank). Real
    /// blocks always have `None`.
    pub(super) kind_override: Option<&'a str>,
    /// Lexical scope chain — outermost first, **excluding** this
    /// block. To get the scope a child expression sees from inside
    /// this block, push this block's frame: `self.scope.push(self_frame)`.
    pub(super) scope: Scope<'a>,
}

impl<'a> Block<'a> {
    fn block_inner(&self) -> (&'a OnceLock<Result<Vec<Value>, EvalError>>, &'a [ItemCells]) {
        let ItemCellKind::Block { labels, items, .. } = &self.cells.kind else {
            unreachable!("Block view wraps a Block cell")
        };
        (labels, items)
    }

    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        let doc = self.doc;
        self.ast
            .decorators
            .iter()
            .zip(self.cells.decorators.iter())
            .map(move |(ast, cell)| Decorator { ast, cell, doc })
    }

    pub fn kind(&self) -> &'a str {
        self.kind_override.unwrap_or(&self.ast.kind)
    }

    /// Scope that child expressions inside this block see — the
    /// block's own `scope` extended with one frame for itself.
    pub(crate) fn child_scope(&self) -> Scope<'a> {
        self.scope.push(ScopeFrame {
            ast: self.ast,
            cells: self.cells,
            kind_override: self.kind_override,
        })
    }

    /// Evaluated values for each label slot. Cached on first call; later
    /// calls return a clone of the cached `Vec`.
    pub fn labels(&self) -> Result<Vec<Value>, EvalError> {
        let (cell, _) = self.block_inner();
        let result = cell.get_or_init(|| {
            self.ast
                .labels
                .iter()
                .map(|e| self.doc.eval_literal(e))
                .collect()
        });
        match result {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(e.clone()),
        }
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Realise any pending block-level imports, then return one
    /// `BlockSlice` for the block's own items plus one for each
    /// successfully-loaded import (transitively).
    fn realize_and_sources(&self) -> Vec<BlockSlice<'a>> {
        let (_, items_cells) = self.block_inner();
        // Force any unloaded Import cells.
        for cell in items_cells {
            if let ItemCellKind::Import {
                path,
                base_dir,
                path_span,
                loaded,
            } = &cell.kind
            {
                let _ = loaded
                    .get_or_init(|| load_import_lazily(path, base_dir.as_deref(), *path_span));
            }
        }
        let mut out = vec![BlockSlice {
            items: &self.ast.items,
            cells: items_cells,
        }];
        push_loaded_imports(items_cells, &mut out);
        out
    }

    pub fn field(&self, name: &str) -> Option<Field<'a>> {
        let child_scope = self.child_scope();
        for src in self.realize_and_sources() {
            if let Some(f) = find_field(src.items, src.cells, name, self.doc, &child_scope) {
                return Some(f);
            }
        }
        None
    }

    pub fn block(&self, kind: &str) -> Option<Block<'a>> {
        let child_scope = self.child_scope();
        for src in self.realize_and_sources() {
            if let Some(b) = find_block(src.items, src.cells, kind, self.doc, &child_scope) {
                return Some(b);
            }
        }
        None
    }

    pub fn fields(&self) -> impl Iterator<Item = Field<'a>> + 'a {
        let doc = self.doc;
        let scope = self.child_scope();
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_fields(src.items, src.cells, doc, scope.clone()))
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block<'a>> + 'a {
        let doc = self.doc;
        let scope = self.child_scope();
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_blocks(src.items, src.cells, doc, scope.clone()))
    }

    /// Source-order iterator over `Item::Table` entries in this block.
    /// Each [`TableView`] carries the parent field name and the row
    /// values as written. Hosts that want the schema-projected view
    /// should use `typed_field`/`doc.get` instead.
    pub fn tables(&self) -> impl Iterator<Item = TableView<'a>> + 'a {
        let doc = self.doc;
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_tables(src.items, doc))
    }

    /// Return the most recently surfaced lazy-import error for this
    /// block, if any. Useful for callers that want to surface load
    /// failures explicitly rather than only seeing `None` from
    /// `field`/`block`.
    pub fn import_errors(&self) -> Vec<EvalError> {
        let (_, items_cells) = self.block_inner();
        let mut out = Vec::new();
        for cell in items_cells {
            if let ItemCellKind::Import { loaded, .. } = &cell.kind
                && let Some(Err(e)) = loaded.get()
            {
                out.push(e.clone());
            }
        }
        out
    }

    /// The schema (`TypeDecl`) for this block's `kind`, if any.
    pub fn schema(&self) -> Option<TypeDecl<'a>> {
        let k = self.kind();
        self.doc
            .block_schema(k)
            .or_else(|| self.doc.table_schema(k))
    }

    /// Schema-aware field lookup. Projects the block through its
    /// declared type:
    ///
    /// - `@inline(N)` → returns a synthetic `Field` over the label slot
    /// - `@child(K)`  → returns a `DataRef::Block` of the matching
    ///   nested block (or `None` if absent)
    /// - `@children(K)` → returns a `DataRef::BlockList` of all matching
    ///   nested blocks
    /// - any other named field on the schema → tries a literal child
    ///   field by name
    ///
    /// Returns `None` if the block has no schema, or if the name
    /// doesn't match any schema field or literal item.
    pub fn typed_field(&self, name: &str) -> Option<crate::data::DataRef<'a>> {
        let schema = self.schema()?;
        let f = schema.field(name)?;

        // `@connections(SchemaName)`: project sibling Item::Connection
        // statements through the named connection schema.
        if let Some(conn_schema) = f.connection_schema() {
            let scope = self.child_scope();
            let values = self
                .doc
                .project_connections(&self.ast.items, conn_schema, &scope);
            return Some(crate::data::DataRef::from_variant_value(Value::List(
                values,
            )));
        }

        // Union-typed @children: dispatch every nested block / table
        // row to a Value::Variant via structural-shape matching.
        if let Some(crate::doc::ChildKind::Union(union)) = f.children_kind_or_union() {
            return Some(self.dispatch_union_children(name, union));
        }
        // Union-typed @child: dispatch the single matching nested block.
        if let Some(crate::doc::ChildKind::Union(union)) = f.child_kind_or_union() {
            return Some(self.dispatch_union_child(union));
        }

        if let Some(kind) = f.children_block_kind_str() {
            // Use the projection: combines literal nested blocks of
            // this kind with synthesised blocks from `Item::Table`
            // rows under the matching field name.
            let blocks = self.children_projection(name, kind);
            let is_table = self.doc.table_schema(kind).is_some();
            return Some(if is_table {
                crate::data::DataRef::from_table(blocks)
            } else {
                crate::data::DataRef::from_block_list(blocks)
            });
        }
        if let Some(kind) = f.child_block_kind() {
            let block = self.blocks().find(|b| b.kind() == kind)?;
            return Some(crate::data::DataRef::from_block(block));
        }
        if f.inline_slot().is_some() {
            // Inline labels become a synthetic field — we don't have
            // a `Field` view for a label, so return the typed-field
            // view. Hosts wanting the label value should access
            // `block.labels()` directly.
            return Some(crate::data::DataRef::new(crate::data::DataKind::TypeField(
                f,
            )));
        }
        // Plain schema field → look it up in literal block items.
        self.field(name).map(crate::data::DataRef::from_field)
    }

    /// Dispatch all of a `@children(SomeUnion)` field's nested blocks
    /// and table rows through structural-shape matching to produce a
    /// list of `Value::Variant`. Failures from individual blocks or
    /// rows are silently skipped here; the schema check pipeline
    /// emits them via `Document::schema_errors()`.
    fn dispatch_union_children(
        &self,
        field_name: &str,
        union: UnionDecl<'a>,
    ) -> crate::data::DataRef<'a> {
        let mut out: Vec<Value> = Vec::new();
        for (kind, blk) in self.union_children_blocks(field_name) {
            let v = match kind {
                UnionChildKind::Nested => variant_dispatch::block_to_variant(self.doc, &blk, union),
                UnionChildKind::TableRow => {
                    variant_dispatch::table_row_to_variant(self.doc, &blk, union)
                }
            };
            if let Ok(v) = v {
                out.push(v);
            }
        }
        crate::data::DataRef::from_variant_value_list(out)
    }

    /// Iterate the nested-block + synth-row sources for a union-typed
    /// `@children(SomeUnion)` field. Each entry comes back with a
    /// tag identifying which dispatcher should consume it.
    pub(crate) fn union_children_blocks(
        &self,
        field_name: &str,
    ) -> Vec<(UnionChildKind, Block<'a>)> {
        let (items_cells, synth_rows) = match &self.cells.kind {
            ItemCellKind::Block {
                items, synth_rows, ..
            } => (items, synth_rows),
            _ => unreachable!("Block view wraps a Block cell"),
        };
        let mut out: Vec<(UnionChildKind, Block<'a>)> = Vec::new();
        let child_scope = self.child_scope();
        for (item, cells) in self.ast.items.iter().zip(items_cells.iter()) {
            match item {
                ast::Item::Block(b) => {
                    out.push((
                        UnionChildKind::Nested,
                        Block {
                            ast: b,
                            cells,
                            doc: self.doc,
                            kind_override: None,
                            scope: child_scope.clone(),
                        },
                    ));
                }
                ast::Item::Table(t) if t.field_name == field_name => {
                    let mut synth_iter = synth_rows.iter().filter(|r| r.field_name == field_name);
                    for _ in &t.rows {
                        if let Some(sr) = synth_iter.next() {
                            out.push((
                                UnionChildKind::TableRow,
                                Block {
                                    ast: &sr.block,
                                    cells: &sr.cells,
                                    doc: self.doc,
                                    kind_override: None,
                                    scope: child_scope.clone(),
                                },
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Dispatch a single nested block to a variant for a
    /// `@child(SomeUnion)` field.
    fn dispatch_union_child(&self, union: UnionDecl<'a>) -> crate::data::DataRef<'a> {
        let (items_cells, _) = match &self.cells.kind {
            ItemCellKind::Block {
                items, synth_rows, ..
            } => (items, synth_rows),
            _ => unreachable!("Block view wraps a Block cell"),
        };
        let child_scope = self.child_scope();
        for (item, cells) in self.ast.items.iter().zip(items_cells.iter()) {
            if let ast::Item::Block(b) = item {
                let blk = Block {
                    ast: b,
                    cells,
                    doc: self.doc,
                    kind_override: None,
                    scope: child_scope.clone(),
                };
                if let Ok(v) = variant_dispatch::block_to_variant(self.doc, &blk, union) {
                    return crate::data::DataRef::from_variant_value(v);
                }
            }
        }
        crate::data::DataRef::from_variant_value(Value::None)
    }

    /// Build the list of `Block`s for one `@children(kind)` field —
    /// combining literal nested `Block`s of the matching kind with
    /// the parent's pre-built synthesised row-Blocks whose
    /// `field_name` matches. The synthesised blocks store an empty
    /// kind in the AST; we set `kind_override` here so views see the
    /// correct kind.
    fn children_projection(&self, field_name: &str, kind: &'a str) -> Vec<Block<'a>> {
        let (items_cells, synth_rows) = match &self.cells.kind {
            ItemCellKind::Block {
                items, synth_rows, ..
            } => (items, synth_rows),
            _ => unreachable!("Block view wraps a Block cell"),
        };
        let mut out: Vec<Block<'a>> = Vec::new();
        // Walk items + cells in source order. Real Item::Block entries
        // contribute their own Block view; Item::Table entries are
        // replaced (in-order) by their corresponding synthesised rows
        // from `synth_rows`.
        let child_scope = self.child_scope();
        let mut synth_iter = synth_rows.iter().filter(|r| r.field_name == field_name);
        for (item, cells) in self.ast.items.iter().zip(items_cells.iter()) {
            match item {
                ast::Item::Block(b) if b.kind == kind => {
                    out.push(Block {
                        ast: b,
                        cells,
                        doc: self.doc,
                        kind_override: None,
                        scope: child_scope.clone(),
                    });
                }
                ast::Item::Table(t) if t.field_name == field_name => {
                    // Pull one synthesised row per source row.
                    for _ in &t.rows {
                        if let Some(sr) = synth_iter.next() {
                            out.push(Block {
                                ast: &sr.block,
                                cells: &sr.cells,
                                doc: self.doc,
                                kind_override: Some(kind),
                                scope: child_scope.clone(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Iterate schema-projected fields in declared order. Empty for
    /// un-schema'd blocks.
    pub fn typed_fields(
        &self,
    ) -> Box<dyn Iterator<Item = (&'a str, crate::data::DataRef<'a>)> + 'a> {
        let Some(schema) = self.schema() else {
            return Box::new(std::iter::empty());
        };
        let this = self.clone();
        Box::new(schema.fields().filter_map(move |f| {
            let name = f.name();
            this.typed_field(name).map(|dr| (name, dr))
        }))
    }

    /// Schema-content validation errors for this block. Computed and
    /// cached on first access; subsequent calls return the cached slice.
    pub fn schema_errors(&self) -> &'a [EvalError] {
        let ItemCellKind::Block {
            schema_validation, ..
        } = &self.cells.kind
        else {
            unreachable!("Block view wraps a Block cell")
        };
        let result = schema_validation.get_or_init(|| compute_schema_errors(self));
        result.as_slice()
    }
}

/// Source-level view of an `Item::Table` (a `FIELD:` header followed
/// by one or more `| ... |` rows) within a parent block.
#[derive(Clone, Copy)]
pub struct TableView<'a> {
    pub(super) ast: &'a ast::TableItem,
    pub(super) doc: &'a Document,
}

impl<'a> TableView<'a> {
    /// Name of the parent-block field that this table binds to.
    pub fn field_name(&self) -> &'a str {
        &self.ast.field_name
    }

    /// Iterator over the rows in source order.
    pub fn rows(&self) -> impl Iterator<Item = RowView<'a>> + 'a {
        let doc = self.doc;
        self.ast.rows.iter().map(move |r| RowView { ast: r, doc })
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }
}

/// Source-level view of a single `| ... |` row inside a [`TableView`].
#[derive(Clone, Copy)]
pub struct RowView<'a> {
    ast: &'a ast::Row,
    doc: &'a Document,
}

impl<'a> RowView<'a> {
    /// Evaluate each cell expression and return the row as values.
    /// Cells are treated as literals: bare identifiers materialise as
    /// `Value::Identifier`, not resolved through the enclosing scope.
    pub fn values(&self) -> Result<Vec<Value>, EvalError> {
        self.ast
            .values
            .iter()
            .map(|e| self.doc.eval_literal(e))
            .collect()
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }
}

/// Hard cap on nested user-`fn` invocations during a single evaluation.
/// Prevents accidental recursion in a `Value::Function` body from blowing
/// the Rust stack; surfaces as [`EvalError::CallDepthExceeded`].
const MAX_CALL_DEPTH: usize = 256;

pub(crate) struct EvalCtx<'a> {
    /// Stack of name → value bindings introduced by `Block` let-bindings.
    /// Searched right-to-left so the most recent binding shadows older ones.
    locals: Vec<(String, Value)>,
    /// Lexical scope of the expression's evaluation site. Used to
    /// resolve bare identifiers and `self`/`parent`.
    scope: Scope<'a>,
    /// Current nested `Value::Function` invocation depth.
    call_depth: usize,
}

impl<'a> EvalCtx<'a> {
    fn new(scope: Scope<'a>) -> Self {
        Self {
            locals: Vec::new(),
            scope,
            call_depth: 0,
        }
    }

    fn lookup(&self, name: &str) -> Option<&Value> {
        self.locals
            .iter()
            .rev()
            .find_map(|(n, v)| if n == name { Some(v) } else { None })
    }
}

/// Resolve `name` to a `Value::Function`, if it has one bound in the
/// caller's locals or document scope. Returns `None` for anything else
/// (non-function values, missing names) so the caller can decide what
/// fallback to take (e.g. dispatch to a builtin by the same name).
fn lookup_function(doc: &Document, ctx: &EvalCtx<'_>, name: &str) -> Option<FnValue> {
    if let Some(Value::Function(fv)) = ctx.lookup(name) {
        return Some(fv.clone());
    }
    let dr = doc.scope_lookup(&ctx.scope, name)?;
    let span = Span::new(0, 0);
    match materialise_dataref(dr, span) {
        Ok(Value::Function(fv)) => Some(fv),
        _ => None,
    }
}

/// Attach the call site name to a generic call-arity error, so error
/// reporting for `myFn(1, 2)` mentions `myFn`. Other variants pass
/// through unchanged.
fn call_err_at(err: EvalError, name: String, span: Span) -> EvalError {
    match err {
        EvalError::CallArity { expected, got, .. } => {
            EvalError::builtin_arity(name, expected, got, span)
        }
        other => other,
    }
}

/// `Caller` impl used by the evaluator to invoke `Value::Function`
/// callbacks from inside HOF builtins. Holds a back-reference to the
/// document and the live `EvalCtx`, so the call observes (and reuses)
/// the surrounding evaluation's locals/scope/call_depth.
struct EvalCaller<'a, 'c> {
    doc: &'a Document,
    ctx: &'c mut EvalCtx<'a>,
    span: Span,
    /// If a user-function invocation surfaces an `EvalError`, we stash
    /// it here and return a string from `call_fn` so the builtin can
    /// short-circuit. The dispatch site re-raises the structured error.
    err: Option<EvalError>,
}

impl<'a> crate::builtins::Caller for EvalCaller<'a, '_> {
    fn call_fn(&mut self, f: &FnValue, args: &[Value]) -> Result<Value, String> {
        let _profile_guard = self.doc.profile_enter(crate::profile::ProfileKey::UserFn {
            name: String::new(),
        });
        match self.doc.invoke_fn_value(f, args, self.ctx, self.span) {
            Ok(v) => Ok(v),
            Err(e) => {
                let msg = e.to_string();
                self.err = Some(e);
                Err(msg)
            }
        }
    }

    fn resolve<'r>(&'r self, path: &[String]) -> Option<crate::data::DataRef<'r>> {
        let (first, rest) = path.split_first()?;
        let mut cur = self.doc.resolve_root(first)?;
        for seg in rest {
            cur = cur.child(seg)?;
        }
        Some(cur)
    }
}

impl Document {
    /// Scope-aware expression evaluator. Bare identifiers and path
    /// expressions resolve via the supplied [`Scope`] chain, falling
    /// through to the document root. Unresolved names error with
    /// [`EvalError::UnresolvedReference`].
    pub(crate) fn eval_in_scope(
        &self,
        expr: &ast::Expr,
        scope: &Scope<'_>,
    ) -> Result<Value, EvalError> {
        let mut ctx = EvalCtx::new(scope.clone());
        self.eval_in(expr, &mut ctx)
    }

    /// Literal-mode evaluator for contexts that intentionally treat
    /// bare identifiers as opaque names (block labels). Any
    /// non-identifier expression is evaluated through the root scope.
    pub(crate) fn eval_literal(&self, expr: &ast::Expr) -> Result<Value, EvalError> {
        if let ast::Expr::Identifier(s) = expr {
            return Ok(Value::Identifier(s.clone()));
        }
        self.eval_in_scope(expr, &Scope::root())
    }

    /// Back-compat shim — same as `eval_literal`. Used by call sites
    /// that pre-date the scope distinction (decorator args). Bare
    /// identifiers fall through; everything else evaluates at the
    /// document root.
    pub(crate) fn eval(&self, expr: &ast::Expr) -> Result<Value, EvalError> {
        self.eval_literal(expr)
    }

    /// Trivial value-literal expressions: scalar number/bool/string
    /// variants, symbols, `none`. Pulled out of `eval_in` to keep the
    /// big match focused on expressions that actually involve scope or
    /// recursion.
    fn eval_value_literal(expr: &ast::Expr) -> Option<Value> {
        use ast::Expr as E;
        Some(match expr {
            E::Bool(b) => Value::Bool(*b),
            E::I8(v) => Value::I8(*v),
            E::I16(v) => Value::I16(*v),
            E::I32(v) => Value::I32(*v),
            E::I64(v) => Value::I64(*v),
            E::I128(v) => Value::I128(*v),
            E::Isize(v) => Value::Isize(*v),
            E::U8(v) => Value::U8(*v),
            E::U16(v) => Value::U16(*v),
            E::U32(v) => Value::U32(*v),
            E::U64(v) => Value::U64(*v),
            E::U128(v) => Value::U128(*v),
            E::Usize(v) => Value::Usize(*v),
            E::F32(v) => Value::F32(*v),
            E::F64(v) => Value::F64(*v),
            E::Utf8(s) => Value::Utf8(s.clone()),
            E::Ascii(s) => Value::Ascii(s.clone()),
            E::Utf16(v) => Value::Utf16(v.clone()),
            E::Utf32(v) => Value::Utf32(v.clone()),
            E::Symbol(s) => Value::Symbol(s.clone()),
            E::None => Value::None,
            _ => return None,
        })
    }

    fn eval_in<'a>(&'a self, expr: &ast::Expr, ctx: &mut EvalCtx<'a>) -> Result<Value, EvalError> {
        use ast::Expr as E;
        if let Some(v) = Self::eval_value_literal(expr) {
            return Ok(v);
        }
        Ok(match expr {
            E::InterpolatedString {
                encoding,
                parts,
                span,
            } => {
                use crate::lexer::StringEncoding as Enc;
                let mut joined = String::new();
                for part in parts {
                    match part {
                        ast::TemplatePart::Literal(s) => joined.push_str(s),
                        ast::TemplatePart::Expr(e) => {
                            let v = self.eval_in(e, ctx)?;
                            joined.push_str(&crate::collections::format_value(&v));
                        }
                    }
                }
                return match encoding {
                    Enc::Utf8 => Ok(Value::Utf8(joined)),
                    Enc::Ascii => {
                        if joined.chars().any(|c| (c as u32) >= 0x80) {
                            Err(EvalError::schema_violation(
                                crate::error::SchemaViolationKind::FieldTypeMismatch,
                                "interpolated ascii string contains a non-ASCII character",
                                *span,
                            ))
                        } else {
                            Ok(Value::Ascii(joined))
                        }
                    }
                    Enc::Utf16 => Ok(Value::Utf16(joined.encode_utf16().collect())),
                    Enc::Utf32 => Ok(Value::Utf32(joined.chars().collect())),
                };
            }
            E::Function(f) => {
                let params: Vec<FnParam> = f
                    .params
                    .iter()
                    .map(|p| FnParam::new(p.name.clone(), p.ty.clone()))
                    .collect();
                // Snapshot surrounding locals as the function value's
                // lexical capture. Document-scope identifiers (fields,
                // blocks, …) resolve at call time, so they don't need
                // snapshotting.
                let captured = ctx.locals.clone();
                Value::Function(
                    FnValue::new(params, f.return_ty.clone(), f.body.clone())
                        .with_captures(captured),
                )
            }
            E::Identifier(name) => {
                // Locals (let-binding scope) shadow scope-walked names.
                if let Some(v) = ctx.lookup(name) {
                    return Ok(v.clone());
                }
                let dr = self
                    .scope_lookup(&ctx.scope, name)
                    .ok_or_else(|| EvalError::unresolved_reference(name, span_of(expr)))?;
                return materialise_dataref_or_path(dr, vec![name.clone()], span_of(expr));
            }
            E::SelfKw(span) => {
                let dr = self.self_dataref(&ctx.scope);
                return materialise_dataref(dr, *span);
            }
            E::ParentKw(span) => {
                let dr = self.parent_dataref(&ctx.scope).ok_or_else(|| {
                    EvalError::unresolved_reference("parent at document root", *span)
                })?;
                return materialise_dataref(dr, *span);
            }
            E::Member {
                recv: _,
                name: _,
                span,
            } => {
                let dr = self.eval_to_dataref(expr, ctx)?;
                let segments = expr_to_path_segments(expr).unwrap_or_default();
                return materialise_dataref_or_path(dr, segments, *span);
            }
            E::Paren { inner, .. } => return self.eval_in(inner, ctx),
            E::Unary { op, operand, span } => {
                let v = self.eval_in(operand, ctx)?;
                return apply_unary(*op, v, *span);
            }
            E::Binary { op, lhs, rhs, span } => {
                // Short-circuit logical ops.
                if matches!(op, ast::BinOp::And | ast::BinOp::Or) {
                    let l = self.eval_in(lhs, ctx)?;
                    let lb = as_bool(&l, *op, *span)?;
                    let short =
                        matches!(op, ast::BinOp::And) && !lb || matches!(op, ast::BinOp::Or) && lb;
                    if short {
                        return Ok(Value::Bool(lb));
                    }
                    let r = self.eval_in(rhs, ctx)?;
                    let rb = as_bool(&r, *op, *span)?;
                    return Ok(Value::Bool(rb));
                }
                let l = self.eval_in(lhs, ctx)?;
                let r = self.eval_in(rhs, ctx)?;
                return apply_binary(*op, l, r, *span);
            }
            E::Call { callee, args, span } => {
                // Resolution order: when callee is a bare identifier, try
                // to find a `Value::Function` in locals/scope first; fall
                // back to the builtin registry by name. For any other
                // callee expression, evaluate it and require a function
                // value.
                if let E::Identifier(name) = callee.as_ref() {
                    if let Some(fv) = lookup_function(self, ctx, name) {
                        let mut evald = Vec::with_capacity(args.len());
                        for arg in args {
                            evald.push(self.eval_in(arg, ctx)?);
                        }
                        let _profile_guard =
                            self.profile_enter(crate::profile::ProfileKey::UserFn {
                                name: name.clone(),
                            });
                        return self
                            .invoke_fn_value(&fv, &evald, ctx, *span)
                            .map_err(|e| call_err_at(e, name.clone(), *span));
                    }
                    let Some(builtin) = self.env.builtin(name).cloned() else {
                        return Err(EvalError::unknown_builtin(name.clone(), *span));
                    };
                    // `format(template, ...args)` is variadic — its
                    // registered arity (0) is a sentinel that just
                    // means "skip the standard arity check".
                    if name != "format" && args.len() != builtin.arity {
                        return Err(EvalError::builtin_arity(
                            name.clone(),
                            builtin.arity,
                            args.len(),
                            *span,
                        ));
                    }
                    let mut evald = Vec::with_capacity(args.len());
                    for arg in args {
                        evald.push(self.eval_in(arg, ctx)?);
                    }
                    let _profile_guard = self
                        .profile_enter(crate::profile::ProfileKey::Builtin { name: name.clone() });
                    // Special-case `error(msg)`: raise a structured UserError
                    // rather than the generic BuiltinTypeMismatch path that
                    // every other fallible builtin uses. Keeps `error` a
                    // first-class control-flow primitive without bending the
                    // builtin trait machinery.
                    if (name == "error" || name == "panic") && evald.len() == 1 {
                        let msg = match &evald[0] {
                            Value::Utf8(s) | Value::Ascii(s) => s.clone(),
                            other => {
                                return Err(EvalError::builtin_type(
                                    name.clone(),
                                    format!(
                                        "{name}: expected utf8 string, got {}",
                                        other.type_name()
                                    ),
                                    *span,
                                ));
                            }
                        };
                        return Err(EvalError::user_error(msg, *span));
                    }
                    // `assert(cond, msg)` — when cond is false, raise a
                    // structured UserError; when true, return None.
                    if name == "assert" && evald.len() == 2 {
                        let cond = matches!(&evald[0], Value::Bool(true));
                        if cond {
                            return Ok(Value::None);
                        }
                        let msg = match &evald[1] {
                            Value::Utf8(s) | Value::Ascii(s) => s.clone(),
                            other => {
                                return Err(EvalError::builtin_type(
                                    name.clone(),
                                    format!(
                                        "assert: message must be utf8, got {}",
                                        other.type_name()
                                    ),
                                    *span,
                                ));
                            }
                        };
                        return Err(EvalError::user_error(msg, *span));
                    }
                    return match &builtin.kind {
                        crate::builtins::BuiltinKind::Pure(body) => (body)(&evald)
                            .map_err(|msg| EvalError::builtin_type(name.clone(), msg, *span)),
                        crate::builtins::BuiltinKind::Hof(body) => {
                            let mut caller = EvalCaller {
                                doc: self,
                                ctx,
                                span: *span,
                                err: None,
                            };
                            let res = (body)(&mut caller, &evald);
                            if let Some(e) = caller.err.take() {
                                return Err(e);
                            }
                            res.map_err(|msg| EvalError::builtin_type(name.clone(), msg, *span))
                        }
                    };
                }
                let callee_val = self.eval_in(callee, ctx)?;
                let Value::Function(fv) = callee_val else {
                    return Err(EvalError::non_callable(*span));
                };
                let mut evald = Vec::with_capacity(args.len());
                for arg in args {
                    evald.push(self.eval_in(arg, ctx)?);
                }
                let _profile_guard = self.profile_enter(crate::profile::ProfileKey::UserFn {
                    name: String::new(),
                });
                return self.invoke_fn_value(&fv, &evald, ctx, *span);
            }
            E::Block { lets, tail, .. } => {
                let frame_base = ctx.locals.len();
                for binding in lets {
                    let v = self.eval_in(&binding.value, ctx)?;
                    ctx.locals.push((binding.name.clone(), v));
                }
                let result = self.eval_in(tail, ctx);
                ctx.locals.truncate(frame_base);
                return result;
            }
            E::ListLit { elements, .. } => {
                let mut out = Vec::with_capacity(elements.len());
                for e in elements {
                    out.push(self.eval_in(e, ctx)?);
                }
                Value::List(out)
            }
            E::If {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let c = self.eval_in(cond, ctx)?;
                let b = as_bool(&c, ast::BinOp::And, *span)?;
                return self.eval_in(if b { then_block } else { else_block }, ctx);
            }
            E::IfLet {
                pattern,
                scrut,
                then_block,
                else_block,
                ..
            } => {
                let v = self.eval_in(scrut, ctx)?;
                if let Some(bindings) = match_pat::match_pattern(pattern, &v) {
                    let frame_base = ctx.locals.len();
                    for (name, val) in bindings {
                        ctx.locals.push((name, val));
                    }
                    let result = self.eval_in(then_block, ctx);
                    ctx.locals.truncate(frame_base);
                    return result;
                }
                return self.eval_in(else_block, ctx);
            }
            E::Match { scrut, arms, span } => {
                let v = self.eval_in(scrut, ctx)?;
                'arms: for arm in arms {
                    for pat in &arm.patterns {
                        let Some(bindings) = match_pat::match_pattern(pat, &v) else {
                            continue;
                        };
                        let frame_base = ctx.locals.len();
                        for (name, val) in bindings {
                            ctx.locals.push((name, val));
                        }
                        if let Some(guard) = &arm.guard {
                            match self.eval_in(guard, ctx) {
                                Ok(Value::Bool(true)) => {}
                                Ok(Value::Bool(false)) => {
                                    ctx.locals.truncate(frame_base);
                                    continue 'arms;
                                }
                                Ok(other) => {
                                    let kind = other.type_name();
                                    ctx.locals.truncate(frame_base);
                                    return Err(EvalError::guard_not_bool(kind, span_of(guard)));
                                }
                                Err(e) => {
                                    ctx.locals.truncate(frame_base);
                                    return Err(e);
                                }
                            }
                        }
                        let result = self.eval_in(&arm.body, ctx);
                        ctx.locals.truncate(frame_base);
                        return result;
                    }
                }
                return Err(EvalError::match_no_arm(*span));
            }
            E::Variant {
                type_path,
                variant,
                args,
                span,
            } => {
                return self.build_variant(type_path, variant, args, *span, ctx);
            }
            // Trivial value literals were handled by `eval_value_literal`
            // at the top of this function.
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
            | E::None => unreachable!("handled by eval_value_literal"),
        })
    }

    /// Apply a `Value::Function` to the supplied argument values. Pushes
    /// the function's parameters onto `ctx.locals`, evaluates the body in
    /// the caller's context, and pops the frame regardless of outcome.
    ///
    /// Closure semantics: the body sees its own parameters plus whatever
    /// the caller's `ctx` has on its `locals` stack and `scope` chain.
    /// There is no capture of the *definition-site* scope in this pass,
    /// so a function value passed across blocks observes the *call*
    /// site's lexical environment, not its origin.
    pub(crate) fn invoke_fn_value<'a>(
        &'a self,
        f: &FnValue,
        args: &[Value],
        ctx: &mut EvalCtx<'a>,
        span: Span,
    ) -> Result<Value, EvalError> {
        if args.len() != f.params().len() {
            return Err(EvalError::call_arity(f.params().len(), args.len(), span));
        }
        if ctx.call_depth >= MAX_CALL_DEPTH {
            return Err(EvalError::call_depth_exceeded(MAX_CALL_DEPTH, span));
        }
        let frame_base = ctx.locals.len();
        // Lexical captures first — later pushes (params, nested let
        // bindings) shadow them on right-to-left lookup.
        for (name, value) in &f.captured {
            ctx.locals.push((name.clone(), value.clone()));
        }
        for (param, value) in f.params().iter().zip(args.iter()) {
            ctx.locals.push((param.name().to_string(), value.clone()));
        }
        ctx.call_depth += 1;
        let result = self.eval_in(&f.body, ctx);
        ctx.call_depth -= 1;
        ctx.locals.truncate(frame_base);
        result
    }

    /// Construct a [`Value::Variant`] from a parsed `Type::Variant`
    /// expression. Resolves the union by `type_path` (with the
    /// document's `file_ns` as a candidate prefix), validates the args
    /// shape against the declared variant body, evaluates each arg,
    /// and stashes them into the appropriate `VariantPayload`. Field
    /// *type* checking is left to schema validation.
    pub(crate) fn build_variant<'a>(
        &'a self,
        type_path: &[String],
        variant: &str,
        args: &ast::VariantArgs,
        span: Span,
        ctx: &mut EvalCtx<'a>,
    ) -> Result<Value, EvalError> {
        // Resolve the union — try the path as-is, then with the
        // document's namespace prefixed (same dance as `field`/`union_decl`).
        let candidates: Vec<String> = if self.file_ns.is_empty() {
            vec![type_path.join(".")]
        } else {
            let bare = type_path.join(".");
            let qualified = format!("{}.{}", self.file_ns.join("."), bare);
            vec![qualified, bare]
        };
        let mut found_union: Option<&ast::UnionDecl> = None;
        for fqn in &candidates {
            if let Some(u) = self.union_decl(fqn) {
                found_union = Some(u.ast);
                break;
            }
        }
        let Some(union_ast) = found_union else {
            return Err(EvalError::unknown_union(type_path.join("."), span));
        };
        let union_fqn = union_ast.name.clone();
        let effective = self.effective_variants_of(union_ast)?;
        let Some(variant_decl) = effective.iter().copied().find(|v| v.name == variant) else {
            return Err(EvalError::unknown_variant(
                union_fqn.join("."),
                variant.to_string(),
                span,
            ));
        };
        let payload = match (&variant_decl.body, args) {
            (ast::VariantBody::Unit, ast::VariantArgs::Unit) => crate::value::VariantPayload::Unit,
            (ast::VariantBody::TypeRef { .. }, ast::VariantArgs::Positional(e)) => {
                let v = self.eval_in(e, ctx)?;
                crate::value::VariantPayload::Positional(Box::new(v))
            }
            (ast::VariantBody::InterfaceRef { iface, .. }, ast::VariantArgs::Positional(e)) => {
                let v = self.eval_in(e, ctx)?;
                self.check_value_implements_iface(&v, iface, span)?;
                crate::value::VariantPayload::Positional(Box::new(v))
            }
            (ast::VariantBody::Record(decl_fields), ast::VariantArgs::Record(named_args)) => {
                let mut map = std::collections::BTreeMap::new();
                // Each declared field must be supplied exactly once.
                for decl_field in decl_fields {
                    let Some(arg) = named_args.iter().find(|na| na.name == decl_field.name) else {
                        return Err(EvalError::variant_shape_mismatch(
                            format!("field '{}'", decl_field.name),
                            "missing",
                            span,
                        ));
                    };
                    let v = self.eval_in(&arg.value, ctx)?;
                    map.insert(decl_field.name.clone(), v);
                }
                // Reject extras — keeps the runtime value strictly
                // shaped to the declared variant body.
                for arg in named_args {
                    if !decl_fields.iter().any(|f| f.name == arg.name) {
                        return Err(EvalError::variant_shape_mismatch(
                            format!("declared fields of {}::{}", union_fqn.join("."), variant),
                            format!("unexpected field '{}'", arg.name),
                            span,
                        ));
                    }
                }
                crate::value::VariantPayload::Record(map)
            }
            (expected_body, given) => {
                let expected = match expected_body {
                    ast::VariantBody::Unit => "no arguments",
                    ast::VariantBody::TypeRef { .. } => "positional argument",
                    ast::VariantBody::InterfaceRef { .. } => "positional argument (interface ref)",
                    ast::VariantBody::Record(_) => "record arguments",
                };
                let got = match given {
                    ast::VariantArgs::Unit => "no arguments",
                    ast::VariantArgs::Positional(_) => "positional argument",
                    ast::VariantArgs::Record(_) => "record arguments",
                };
                return Err(EvalError::variant_shape_mismatch(expected, got, span));
            }
        };
        Ok(Value::Variant {
            union: union_fqn,
            variant: variant.to_string(),
            payload,
        })
    }

    /// Effective variants of a union: parent unions' variants first
    /// (depth-first across the `extends` chain), then the union's own
    /// variants, deduplicating by name (parent first wins; collisions
    /// are caught separately by validation). Detects cycles and
    /// returns `EvalError::UnionCycle`.
    pub(crate) fn effective_variants_of<'a>(
        &'a self,
        union_ast: &'a ast::UnionDecl,
    ) -> Result<Vec<&'a ast::UnionVariant>, EvalError> {
        let mut out: Vec<&ast::UnionVariant> = Vec::new();
        let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut visiting: std::collections::HashSet<String> = std::collections::HashSet::new();
        self.collect_effective_variants(union_ast, &mut out, &mut seen_names, &mut visiting)?;
        Ok(out)
    }

    /// Resolve a `union extends` parent reference. Prefers the
    /// file-namespace-qualified form (matching how the parent was
    /// indexed) and falls back to the bare path. Returns `None` if
    /// neither form is a registered union.
    fn resolve_parent_union(&self, parent_path: &[String]) -> Option<&ast::UnionDecl> {
        let bare = parent_path.join(".");
        if !self.file_ns.is_empty() {
            let qualified = format!("{}.{bare}", self.file_ns.join("."));
            if let Some(p) = self.union_decl(&qualified) {
                return Some(p.ast);
            }
        }
        self.union_decl(&bare).map(|p| p.ast)
    }

    fn collect_effective_variants<'a>(
        &'a self,
        u: &'a ast::UnionDecl,
        out: &mut Vec<&'a ast::UnionVariant>,
        seen: &mut std::collections::HashSet<String>,
        visiting: &mut std::collections::HashSet<String>,
    ) -> Result<(), EvalError> {
        let key = u.name.join(".");
        if visiting.contains(&key) {
            return Err(EvalError::union_cycle(key, u.span));
        }
        visiting.insert(key.clone());
        // Parents first (depth-first), then own variants.
        for parent_path in &u.extends {
            let p = self
                .resolve_parent_union(parent_path)
                .ok_or_else(|| EvalError::unknown_union(parent_path.join("."), u.span))?;
            self.collect_effective_variants(p, out, seen, visiting)?;
        }
        for v in &u.variants {
            if seen.insert(v.name.clone()) {
                out.push(v);
            }
            // Collisions silently drop the later definition — declaration
            // validation reports them as DuplicateVariant errors.
        }
        visiting.remove(&key);
        Ok(())
    }

    /// Check that `value`'s effective fields cover the interface's
    /// declared fields with matching types — for `VariantBody::InterfaceRef`.
    ///
    /// Scope (this pass): only `Value::Variant` payloads whose body is
    /// a `Record` can be structurally introspected. Other value shapes
    /// pass through permissively — value→type introspection across
    /// closures, lists, and tensors would require runtime type tags
    /// that the language doesn't currently carry.
    pub(crate) fn check_value_implements_iface(
        &self,
        value: &Value,
        iface_path: &[String],
        span: Span,
    ) -> Result<(), EvalError> {
        // Resolve the interface declaration. Try with namespace prefix
        // first (matching `union_decl`/`field` lookup conventions).
        let candidates: Vec<String> = if self.file_ns.is_empty() {
            vec![iface_path.join(".")]
        } else {
            vec![
                format!("{}.{}", self.file_ns.join("."), iface_path.join(".")),
                iface_path.join("."),
            ]
        };
        let mut iface_decl: Option<&ast::InterfaceDecl> = None;
        for fqn in &candidates {
            if let Some(i) = self.interface(fqn) {
                iface_decl = Some(i.ast);
                break;
            }
        }
        let Some(iface) = iface_decl else {
            return Err(EvalError::unknown_union(iface_path.join("."), span));
        };
        // For now we only structurally introspect variant values with
        // record payloads. Anything else gets a pass-through; richer
        // checking lands when value-type introspection exists.
        let Value::Variant { payload, .. } = value else {
            return Ok(());
        };
        let crate::value::VariantPayload::Record(map) = payload else {
            return Ok(());
        };
        for f in &iface.fields {
            let Some(v) = map.get(&f.name) else {
                return Err(EvalError::variant_shape_mismatch(
                    format!("interface field '{}'", f.name),
                    "missing on variant payload",
                    span,
                ));
            };
            let expected = &f.ty;
            if !value_matches_type_ref(v, expected) {
                return Err(EvalError::variant_shape_mismatch(
                    format!("interface field '{}': {expected:?}", f.name),
                    format!("payload field is {}", v.type_name()),
                    span,
                ));
            }
        }
        Ok(())
    }

    /// Walk a path expression and return the navigator for its
    /// resolved target.
    pub(crate) fn eval_to_dataref<'a>(
        &'a self,
        expr: &ast::Expr,
        ctx: &EvalCtx<'a>,
    ) -> Result<crate::data::DataRef<'a>, EvalError> {
        use ast::Expr as E;
        match expr {
            E::Identifier(name) => self
                .scope_lookup(&ctx.scope, name)
                .ok_or_else(|| EvalError::unresolved_reference(name, span_of(expr))),
            E::SelfKw(_) => Ok(self.self_dataref(&ctx.scope)),
            E::ParentKw(span) => self
                .parent_dataref(&ctx.scope)
                .ok_or_else(|| EvalError::unresolved_reference("parent at document root", *span)),
            E::Member { recv, name, span } => {
                let recv_dr = self.eval_to_dataref(recv, ctx)?;
                recv_dr.child(name).ok_or_else(|| {
                    let full_path = format_member_path(expr);
                    EvalError::unresolved_reference(full_path, *span)
                })
            }
            E::Paren { inner, .. } => self.eval_to_dataref(inner, ctx),
            other => Err(EvalError::not_a_reference(
                describe_expr(other),
                span_of(other),
            )),
        }
    }

    /// Rebuild a [`Block`] view for the frame at index `i` of `scope`'s
    /// frame chain. The new block's own scope is the slice of frames
    /// strictly *before* `i`, which is what every scope-walking caller
    /// needs (looking up a name from frame `i` should see ancestors but
    /// not siblings).
    fn frame_as_block<'a>(&'a self, scope: &Scope<'a>, i: usize) -> Block<'a> {
        let frames = scope.frames();
        Block {
            ast: frames[i].ast,
            cells: frames[i].cells,
            doc: self,
            kind_override: frames[i].kind_override,
            scope: Scope::from_frames(&frames[..i]),
        }
    }

    /// Resolve a single name against the scope chain (innermost
    /// frame first) and fall through to the document root.
    pub(crate) fn scope_lookup<'a>(
        &'a self,
        scope: &Scope<'a>,
        name: &str,
    ) -> Option<crate::data::DataRef<'a>> {
        for i in (0..scope.frames().len()).rev() {
            let dr = crate::data::DataRef::from_block(self.frame_as_block(scope, i));
            if let Some(child) = dr.child(name) {
                return Some(child);
            }
        }
        self.resolve_root(name)
    }

    fn self_dataref<'a>(&'a self, scope: &Scope<'a>) -> crate::data::DataRef<'a> {
        match scope.frames().len().checked_sub(1) {
            Some(last_idx) => {
                crate::data::DataRef::from_block(self.frame_as_block(scope, last_idx))
            }
            None => crate::data::DataRef::from_document(self),
        }
    }

    fn parent_dataref<'a>(&'a self, scope: &Scope<'a>) -> Option<crate::data::DataRef<'a>> {
        match scope.frames().len() {
            0 => None,
            1 => Some(crate::data::DataRef::from_document(self)),
            n => Some(crate::data::DataRef::from_block(
                self.frame_as_block(scope, n - 2),
            )),
        }
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
        E::Identifier(s) => Some(vec![s.clone()]),
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
                out.push(EvalError::schema_violation(
                    Kind::DuplicateVariant,
                    format!(
                        "variant '{}' is declared more than once in union '{}' (first at offset {})",
                        v.name,
                        u.name.join("."),
                        prev.start,
                    ),
                    v.span,
                ));
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
                out.push(EvalError::schema_violation(
                    Kind::VariantShapeCollision,
                    format!(
                        "variants '{}' and '{}' in union '{}' have identical bodies",
                        effective[i].name,
                        effective[j].name,
                        u.name.join("."),
                    ),
                    effective[j].span,
                ));
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
        | E::Identifier(_)
        | E::Symbol(_)
        | E::None => Span::new(0, 0),
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
