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
    /// Directory the document was loaded from, when known. Used as the
    /// base for resolving relative `import` paths.
    #[allow(dead_code)] // captured in cells; here for introspection
    base_dir: Option<PathBuf>,
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
            base_dir,
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
                } else {
                    ResolvedType::SymbolSet(
                        self.symbol_set(&fqn_dotted)
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
        self.find_schema("block", kind)
    }

    /// Look up the type that schemas a decorator of the given name.
    pub fn decorator_schema(&self, name: &str) -> Option<TypeDecl<'_>> {
        self.find_schema("decorator", name)
    }

    /// Look up the type that schemas a table of the given name, i.e.
    /// the first type carrying an `@table("name")` decorator.
    pub fn table_schema(&self, name: &str) -> Option<TypeDecl<'_>> {
        self.find_schema("table", name)
    }

    /// Look up the type carrying the `@document` decorator, if any.
    /// At most one is expected; if multiple are declared, this
    /// returns the first and `Document::schema_errors` surfaces a
    /// `MultipleDocumentSchemas` violation.
    pub fn doc_schema(&self) -> Option<TypeDecl<'_>> {
        self.find_all_decorated("document").into_iter().next()
    }

    /// Every type declaration carrying the named decorator. Used by
    /// the document-level validator to detect duplicate `@document`
    /// declarations.
    pub(crate) fn find_all_decorated(&self, dec_name: &str) -> Vec<TypeDecl<'_>> {
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
        let doc_schemas = self.find_all_decorated("document");
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

        out
    }

    fn find_schema(&self, dec_name: &str, value: &str) -> Option<TypeDecl<'_>> {
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

/// Extract a `u64`-valued named argument from the first decorator in
/// `decs` whose `full_name()` matches `dec_name`. Returns `None` if the
/// decorator isn't present, the named arg isn't present, the eval
/// failed, or the value isn't a non-negative integer.
fn decorator_u64_named(decs: &[Decorator<'_>], dec_name: &str, arg_name: &str) -> Option<u64> {
    let dec = decs.iter().find(|d| d.full_name() == dec_name)?;
    dec.named_arg(arg_name)?.ok()?.as_u64()
}

#[derive(Debug)]
pub enum ResolvedType<'a> {
    Builtin(BuiltinType),
    Named(TypeDecl<'a>),
    Interface(InterfaceDecl<'a>),
    Union(UnionDecl<'a>),
    SymbolSet(SymbolSetDecl<'a>),
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
    /// arg is looked up directly (matching the legacy
    /// [`named_arg`](Self::named_arg) accessor).
    ///
    /// Returns `None` when the decorator has no registered schema or
    /// the schema doesn't declare a slot of this name.
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
        // Fall back: read the named arg.
        self.named_arg(slot_name)
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
        decorator_u64_named(&decs, "block", "max_children")
    }

    /// `required_children = ["kind", ...]` named arg on the type's
    /// `@block(...)` decorator. Each listed kind must appear at least
    /// once in any instance of this type. Non-string entries in the
    /// list are silently dropped.
    pub fn required_children(&self) -> Vec<String> {
        let decs: Vec<_> = self.decorators().collect();
        let dec = match decs.iter().find(|d| d.full_name() == "block") {
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
        let dec = self.decorators().find(|d| d.full_name() == "inline")?;
        dec.positional().ok()?.first()?.as_u64()
    }

    /// If this field carries an `@default(v)` decorator, returns v.
    pub fn default_value(&self) -> Option<Value> {
        let dec = self.decorators().find(|d| d.full_name() == "default")?;
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
        let dec = self.decorators().find(|d| d.full_name() == "child")?;
        resolve_child_kind_arg(self.doc, &dec.positional().ok()?)
    }

    /// Resolves the positional arg of `@children(...)` into either a
    /// string kind or a union declaration. `None` when the decorator
    /// is absent or the arg is neither.
    pub fn children_kind_or_union(&self) -> Option<ChildKind<'a>> {
        let dec = self.decorators().find(|d| d.full_name() == "children")?;
        resolve_child_kind_arg(self.doc, &dec.positional().ok()?)
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
        decorator_u64_named(&self.decorators().collect::<Vec<_>>(), "children", "min")
    }

    /// Optional `max` cardinality on `@children(...)`.
    pub fn children_max(&self) -> Option<u64> {
        decorator_u64_named(&self.decorators().collect::<Vec<_>>(), "children", "max")
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
        // a `DataRef`, then auto-deref by reading that target's leaf
        // value. Non-reference fields evaluate normally through
        // `eval_in_scope`.
        let result = if matches!(self.declared_type_ref(), Some(TypeRef::Reference(_))) {
            let ctx = EvalCtx::new(self.scope.clone());
            self.doc
                .eval_to_dataref(&self.ast.expr, &ctx)
                .and_then(|dr| materialise_dataref(dr, self.ast.span))
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

impl crate::builtins::Caller for EvalCaller<'_, '_> {
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

    fn eval_in<'a>(&'a self, expr: &ast::Expr, ctx: &mut EvalCtx<'a>) -> Result<Value, EvalError> {
        use ast::Expr as E;
        Ok(match expr {
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
                return materialise_dataref(dr, span_of(expr));
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
                return materialise_dataref(dr, *span);
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
            let candidates: Vec<String> = if self.file_ns.is_empty() {
                vec![parent_path.join(".")]
            } else {
                vec![
                    format!("{}.{}", self.file_ns.join("."), parent_path.join(".")),
                    parent_path.join("."),
                ]
            };
            let mut parent_ast: Option<&ast::UnionDecl> = None;
            for fqn in &candidates {
                if let Some(p) = self.union_decl(fqn) {
                    parent_ast = Some(p.ast);
                    break;
                }
            }
            let Some(p) = parent_ast else {
                return Err(EvalError::unknown_union(parent_path.join("."), u.span));
            };
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
    /// either error or get a runtime pass-through with a TODO note.
    /// A future pass with `Value` → effective-type introspection can
    /// tighten the check.
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

    /// Resolve a single name against the scope chain (innermost
    /// frame first) and fall through to the document root.
    pub(crate) fn scope_lookup<'a>(
        &'a self,
        scope: &Scope<'a>,
        name: &str,
    ) -> Option<crate::data::DataRef<'a>> {
        let frames = scope.frames();
        for i in (0..frames.len()).rev() {
            let block = Block {
                ast: frames[i].ast,
                cells: frames[i].cells,
                doc: self,
                kind_override: frames[i].kind_override,
                scope: Scope::from_frames(&frames[..i]),
            };
            let dr = crate::data::DataRef::from_block(block);
            if let Some(child) = dr.child(name) {
                return Some(child);
            }
        }
        self.resolve_root(name)
    }

    fn self_dataref<'a>(&'a self, scope: &Scope<'a>) -> crate::data::DataRef<'a> {
        let frames = scope.frames();
        if let Some(last_idx) = frames.len().checked_sub(1) {
            let block = Block {
                ast: frames[last_idx].ast,
                cells: frames[last_idx].cells,
                doc: self,
                kind_override: frames[last_idx].kind_override,
                scope: Scope::from_frames(&frames[..last_idx]),
            };
            crate::data::DataRef::from_block(block)
        } else {
            crate::data::DataRef::from_document(self)
        }
    }

    fn parent_dataref<'a>(&'a self, scope: &Scope<'a>) -> Option<crate::data::DataRef<'a>> {
        let frames = scope.frames();
        match frames.len() {
            0 => None,
            1 => Some(crate::data::DataRef::from_document(self)),
            n => {
                let target_idx = n - 2;
                let block = Block {
                    ast: frames[target_idx].ast,
                    cells: frames[target_idx].cells,
                    doc: self,
                    kind_override: frames[target_idx].kind_override,
                    scope: Scope::from_frames(&frames[..target_idx]),
                };
                Some(crate::data::DataRef::from_block(block))
            }
        }
    }
}

/// Take a navigator returned from path evaluation and reduce it to a
/// concrete `Value`. For `Field` targets, this evaluates the field
/// (auto-deref); for any other target, it errors `NotALeaf`.
fn materialise_dataref(dr: crate::data::DataRef<'_>, span: Span) -> Result<Value, EvalError> {
    use crate::data::DataKind;
    match dr.inner() {
        DataKind::Field(f) => f.value().cloned().map_err(|e| e.clone()),
        other => Err(EvalError::not_a_leaf(describe_datakind(other), span)),
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
/// accepted permissively for now (the alternative would be a much
/// bigger value-vs-type framework).
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
        | E::Variant { span, .. } => *span,
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
mod tests {
    use super::*;

    fn open(src: &str) -> Document {
        // The strict-validation default rejects any top-level field
        // or block without a `@document` schema. Most tests in this
        // module assert parser/eval behaviour, not validation, so
        // we wrap each top-level field/block with `@schemaless`
        // here. Tests that exercise validation use
        // `Document::open(_with)` directly with explicit schemas.
        let lax = laxify_for_tests(src);
        // Use an empty registry so existing tests aren't polluted with the
        // four built-in decorator schemas. Explicit `Document::open` /
        // `open_with` behaviour is tested separately.
        Document::open_with(&lax, "test", &Environment::empty()).expect("open")
    }

    #[test]
    fn document_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Document>();
    }

    #[test]
    fn default_scalars_resolve_to_default_types() {
        let doc = open(
            r#"
            s = "alpha"
            i = 3
            f = 2.5
            b = true
            "#,
        );
        assert_eq!(
            doc.field("s").unwrap().value().unwrap(),
            &Value::Utf8("alpha".into())
        );
        assert_eq!(doc.field("i").unwrap().value().unwrap(), &Value::I64(3));
        assert_eq!(doc.field("f").unwrap().value().unwrap(), &Value::F64(2.5));
        assert_eq!(doc.field("b").unwrap().value().unwrap(), &Value::Bool(true));
    }

    #[test]
    fn every_typed_literal_evaluates() {
        let doc = open(
            r#"
            a = 1i8
            b = 2i16
            c = 3i32
            d = 4i64
            e = 5i128
            f = 6isize
            g = 7u8
            h = 8u16
            i = 9u32
            j = 10u64
            k = 11u128
            l = 12usize
            m = 1.5f32
            n = 2.5f64
            s_utf8  = utf8"alpha"
            s_ascii = ascii"beta"
            s_utf16 = utf16"gamma"
            s_utf32 = utf32"delta"
            "#,
        );
        assert_eq!(doc.field("a").unwrap().value().unwrap(), &Value::I8(1));
        assert_eq!(doc.field("b").unwrap().value().unwrap(), &Value::I16(2));
        assert_eq!(doc.field("c").unwrap().value().unwrap(), &Value::I32(3));
        assert_eq!(doc.field("d").unwrap().value().unwrap(), &Value::I64(4));
        assert_eq!(doc.field("e").unwrap().value().unwrap(), &Value::I128(5));
        assert_eq!(doc.field("f").unwrap().value().unwrap(), &Value::Isize(6));
        assert_eq!(doc.field("g").unwrap().value().unwrap(), &Value::U8(7));
        assert_eq!(doc.field("h").unwrap().value().unwrap(), &Value::U16(8));
        assert_eq!(doc.field("i").unwrap().value().unwrap(), &Value::U32(9));
        assert_eq!(doc.field("j").unwrap().value().unwrap(), &Value::U64(10));
        assert_eq!(doc.field("k").unwrap().value().unwrap(), &Value::U128(11));
        assert_eq!(doc.field("l").unwrap().value().unwrap(), &Value::Usize(12));
        assert_eq!(doc.field("m").unwrap().value().unwrap(), &Value::F32(1.5));
        assert_eq!(doc.field("n").unwrap().value().unwrap(), &Value::F64(2.5));
        assert_eq!(
            doc.field("s_utf8").unwrap().value().unwrap(),
            &Value::Utf8("alpha".into())
        );
        assert_eq!(
            doc.field("s_ascii").unwrap().value().unwrap(),
            &Value::Ascii("beta".into())
        );
        assert_eq!(
            doc.field("s_utf16").unwrap().value().unwrap(),
            &Value::Utf16("gamma".encode_utf16().collect())
        );
        assert_eq!(
            doc.field("s_utf32").unwrap().value().unwrap(),
            &Value::Utf32("delta".chars().collect())
        );
    }

    #[test]
    fn strict_typing_distinguishes_widths() {
        let doc = open("a = 42i32\nb = 42i64\n");
        let a = doc.field("a").unwrap().value().unwrap();
        let b = doc.field("b").unwrap().value().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn field_value_caches_on_first_access() {
        let doc = open(r#"name = "alpha""#);
        let f = doc.field("name").unwrap();
        let first = f.value().unwrap() as *const Value;
        let second = f.value().unwrap() as *const Value;
        assert_eq!(first, second);
    }

    #[test]
    fn span_is_available_without_forcing_eval() {
        let doc = open(r#"name = "alpha""#);
        let f = doc.field("name").unwrap();
        assert!(!f.span().is_empty());
        // value() not called; the OnceLock should still be empty.
        assert!(f.field_cell().value.get().is_none());
    }

    #[test]
    fn block_field_resolves() {
        let doc = open(r#"service "web" { port = 8080 }"#);
        let b = doc.block("service").unwrap();
        assert_eq!(b.labels().unwrap(), vec![Value::Utf8("web".into())]);
        assert_eq!(b.field("port").unwrap().value().unwrap(), &Value::I64(8080));
    }

    #[test]
    fn nested_block_field_resolves() {
        let doc = open(
            r#"
            service "web" {
              metadata {
                region = "us-east-1"
              }
            }
            "#,
        );
        let svc = doc.block("service").unwrap();
        let meta = svc.block("metadata").unwrap();
        assert_eq!(
            meta.field("region").unwrap().value().unwrap(),
            &Value::Utf8("us-east-1".into())
        );
    }

    #[test]
    fn unknown_field_returns_none() {
        let doc = open(r#"name = "alpha""#);
        assert!(doc.field("missing").is_none());
    }

    #[test]
    fn unknown_block_returns_none() {
        let doc = open(r#"name = "alpha""#);
        assert!(doc.block("missing").is_none());
    }

    #[test]
    fn fields_iter_yields_only_root_fields() {
        let doc = open(
            r#"a = 1
                          b "label" { c = 2 }
                          d = 3"#,
        );
        let names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(names, vec!["a".to_string(), "d".to_string()]);
    }

    #[test]
    fn blocks_iter_yields_only_root_blocks() {
        let doc = open(
            r#"a = 1
                          b "label" { c = 2 }
                          d {}"#,
        );
        let kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
        assert_eq!(kinds, vec!["b".to_string(), "d".to_string()]);
    }

    #[test]
    fn unresolved_bare_identifier_in_field_rhs_errors() {
        // Bare identifiers in expression position must resolve via
        // the scope chain or the document root, otherwise we surface
        // an UnresolvedReference. This replaces the old
        // Value::Identifier pass-through behaviour.
        let doc = open("owner = wil_taylor");
        let err = doc.field("owner").unwrap().value().unwrap_err();
        assert!(
            matches!(err, EvalError::UnresolvedReference { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn none_value_resolves() {
        let doc = open("maybe = none");
        assert_eq!(doc.field("maybe").unwrap().value().unwrap(), &Value::None);
    }

    #[test]
    fn type_decls_are_queryable() {
        use crate::value::BuiltinType;
        let doc = open(
            r#"
            type User {
              name: utf8
              bio:  utf8?
              link: &User?
            }
            type Empty {}
            "#,
        );
        assert_eq!(doc.type_decls().count(), 2);
        let user = doc.type_decl("User").expect("User type");
        let fields: Vec<_> = user.fields().collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].name(), "name");
        assert_eq!(fields[0].type_ref(), &TypeRef::Builtin(BuiltinType::Utf8));
        assert!(!fields[0].optional());
        assert_eq!(fields[1].name(), "bio");
        assert!(fields[1].optional());
        assert_eq!(fields[2].name(), "link");
        assert_eq!(
            fields[2].type_ref(),
            &TypeRef::Reference(Box::new(TypeRef::Named(vec!["User".into()])))
        );
        assert!(fields[2].optional());
        assert!(doc.type_decl("Empty").unwrap().fields().count() == 0);
    }

    #[test]
    fn type_decl_named_field_lookup() {
        let doc = open("type Point { x: i32 y: i32 }");
        let t = doc.type_decl("Point").unwrap();
        assert!(t.field("x").is_some());
        assert!(t.field("y").is_some());
        assert!(t.field("z").is_none());
    }

    #[test]
    fn type_decls_dont_appear_in_field_or_block_iteration() {
        let doc = open(
            r#"
            type User { name: utf8 }
            count = 1
            svc {}
            "#,
        );
        let field_names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(field_names, vec!["count".to_string()]);
        let block_kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
        assert_eq!(block_kinds, vec!["svc".to_string()]);
    }

    #[test]
    fn forward_ref_resolves_at_open() {
        let doc = open("type A { b: B }\ntype B { x: i32 }");
        let a = doc.type_decl("A").unwrap();
        let b_field = a.field("b").unwrap();
        match doc.resolve(b_field.type_ref()) {
            ResolvedType::Named(decl) => assert_eq!(decl.name(), "B"),
            _ => panic!("expected named ResolvedType::Named"),
        }
    }

    #[test]
    fn self_ref_resolves_at_open() {
        let doc = open("type Tree { parent: Tree? }");
        let t = doc.type_decl("Tree").unwrap();
        let parent = t.field("parent").unwrap();
        match doc.resolve(parent.type_ref()) {
            ResolvedType::Named(decl) => assert_eq!(decl.name(), "Tree"),
            _ => panic!("expected ResolvedType::Named"),
        }
    }

    #[test]
    fn unknown_type_ref_errors_at_open() {
        let err = Document::open("type X { y: NotDecl }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("unknown type 'NotDecl'"),
                    "message: {}",
                    e.message
                );
                // Span covers exactly the type ident `NotDecl`.
                assert_eq!(e.span.offset(), "type X { y: ".len());
                assert_eq!(e.span.len(), "NotDecl".len());
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn duplicate_type_decl_errors_at_open() {
        let err = Document::open("type Foo { x: i32 }\ntype Foo { y: i32 }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("duplicate declaration 'Foo'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn type_and_union_with_same_name_errors() {
        let err = Document::open("type Foo {}\nunion Foo {}", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("duplicate declaration 'Foo'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn duplicate_variant_name_errors() {
        let err = Document::open("union X { A none A none }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("duplicate variant 'A' in union 'X'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn union_decls_are_queryable() {
        let doc = open(
            r#"
            type Point { x: f64 y: f64 }
            union Shape {
              Circle { radius: f64 }
              Polygon Point
              Empty none
            }
            union Maybe { Some { v: i32 } Nothing none }
            "#,
        );
        assert_eq!(doc.union_decls().count(), 2);
        let shape = doc.union_decl("Shape").expect("Shape union");
        assert_eq!(shape.variants().count(), 3);
        assert!(shape.variant("Circle").is_some());
        assert!(shape.variant("missing").is_none());
    }

    #[test]
    fn variant_record_fields_iterate() {
        let doc = open(
            "type Point { x: f64 y: f64 }\nunion Shape { Circle { radius: f64 center: Point } }",
        );
        let shape = doc.union_decl("Shape").unwrap();
        let circle = shape.variant("Circle").unwrap();
        assert!(matches!(circle.body(), VariantBodyView::Record));
        let names: Vec<_> = circle.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(names, vec!["radius".to_string(), "center".to_string()]);
    }

    #[test]
    fn variant_type_ref_body_resolves() {
        let doc = open("type Point { x: f64 y: f64 }\nunion Shape { Polygon Point }");
        let shape = doc.union_decl("Shape").unwrap();
        let v = shape.variant("Polygon").unwrap();
        match v.body() {
            VariantBodyView::TypeRef(t) => assert_eq!(*t, TypeRef::Named(vec!["Point".into()])),
            _ => panic!("expected TypeRef body"),
        }
    }

    #[test]
    fn variant_unit_body() {
        let doc = open("union Maybe { Nothing none }");
        let m = doc.union_decl("Maybe").unwrap();
        let n = m.variant("Nothing").unwrap();
        assert!(matches!(n.body(), VariantBodyView::Unit));
        assert_eq!(n.fields().count(), 0);
    }

    #[test]
    fn resolve_union_returns_union() {
        let doc = open("union Shape { Empty none }\ntype Box { contents: Shape }");
        let b = doc.type_decl("Box").unwrap();
        let contents = b.field("contents").unwrap();
        match doc.resolve(contents.type_ref()) {
            ResolvedType::Union(u) => assert_eq!(u.name(), "Shape"),
            _ => panic!("expected union"),
        }
    }

    #[test]
    fn unknown_variant_body_type_errors() {
        let err = Document::open("union X { V NotDecl }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("unknown type 'NotDecl'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn namespace_sets_file_ns_and_qualifies_decls() {
        let doc = open("namespace foo.bar\ntype X {}");
        assert_eq!(doc.namespace(), &["foo".to_string(), "bar".to_string()]);
        let x = doc.type_decl("foo.bar.X").expect("found");
        assert_eq!(x.full_name(), "foo.bar.X");
        assert_eq!(x.name(), "X");
    }

    #[test]
    fn dotted_decl_name_extends_file_ns() {
        let doc = open("namespace foo\ntype a.b.X {}");
        assert!(doc.type_decl("foo.a.b.X").is_some());
        let x = doc.type_decl("foo.a.b.X").unwrap();
        assert_eq!(x.name(), "X");
        assert_eq!(x.full_name(), "foo.a.b.X");
        assert_eq!(
            x.namespace(),
            vec!["foo".to_string(), "a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn local_name_resolves_within_file_ns() {
        let doc = open(
            r#"
            namespace app
            type X { name: utf8 }
            type Y { f: X }
            "#,
        );
        let y = doc.type_decl("app.Y").unwrap();
        match doc.resolve(y.field("f").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "app.X"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn item_alias_with_as_resolves() {
        let doc = open(
            r#"
            type a.b.Baz {}
            use a.b.Baz as MB
            type Q { f: MB }
            "#,
        );
        let q = doc.type_decl("Q").unwrap();
        match doc.resolve(q.field("f").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Baz"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn wildcard_import_resolves_bare_name() {
        let doc = open(
            r#"
            type a.b.Baz {}
            use a.b
            type Q { f: Baz }
            "#,
        );
        let q = doc.type_decl("Q").unwrap();
        match doc.resolve(q.field("f").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Baz"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn namespace_alias_with_as_resolves() {
        let doc = open(
            r#"
            type a.b.Baz {}
            use a.b as B
            type Q { f: B.Baz }
            "#,
        );
        let q = doc.type_decl("Q").unwrap();
        match doc.resolve(q.field("f").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Baz"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn brace_list_imports_each_item() {
        let doc = open(
            r#"
            type a.b.X {}
            type a.b.Y {}
            use a.b.{X, Y as Z}
            type Q { f1: X f2: Z }
            "#,
        );
        let q = doc.type_decl("Q").unwrap();
        match doc.resolve(q.field("f1").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.X"),
            _ => panic!("f1"),
        }
        match doc.resolve(q.field("f2").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Y"),
            _ => panic!("f2"),
        }
    }

    #[test]
    fn namespace_must_be_first_item() {
        let err = Document::open("type X {}\nnamespace foo", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("must be the first item"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn duplicate_namespace_errors() {
        let err = Document::open("namespace foo\nnamespace bar", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("duplicate namespace"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn unknown_use_target_errors() {
        let err = Document::open("use foo.bar.Nope", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("unknown use target"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn brace_list_unknown_item_errors() {
        let err = Document::open("type a.b.X {}\nuse a.b.{Nope}", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("unknown use target"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn brace_list_on_leaf_errors() {
        // foo.bar.X is a leaf; can't do use foo.bar.X.{Y}
        let err = Document::open("type foo.bar.X {}\nuse foo.bar.X.{Y}", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("expected namespace"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn duplicate_alias_errors() {
        let err = Document::open(
            r#"
            type a.X {}
            type b.X {}
            use a.X
            use b.X
            "#,
            "test",
        )
        .unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("duplicate use alias"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn unions_dont_appear_in_field_or_block_iteration() {
        let doc = open(
            r#"
            union Shape { Empty none }
            count = 1
            svc {}
            "#,
        );
        let field_names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(field_names, vec!["count".to_string()]);
        let block_kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
        assert_eq!(block_kinds, vec!["svc".to_string()]);
    }

    #[test]
    fn resolve_reference_unwraps_to_named() {
        let doc = open("type User { name: utf8 }\ntype Post { author: &User }");
        let post = doc.type_decl("Post").unwrap();
        let author = post.field("author").unwrap();
        let resolved = doc.resolve(author.type_ref());
        let ResolvedType::Reference(inner) = resolved else {
            panic!("expected reference");
        };
        let ResolvedType::Named(decl) = *inner else {
            panic!("expected inner Named");
        };
        assert_eq!(decl.name(), "User");
    }

    #[test]
    fn resolve_reference_to_builtin() {
        let doc = open("type Score { value: &i32 }");
        let s = doc.type_decl("Score").unwrap();
        let v = s.field("value").unwrap();
        let resolved = doc.resolve(v.type_ref());
        let ResolvedType::Reference(inner) = resolved else {
            panic!("expected reference");
        };
        let ResolvedType::Builtin(b) = *inner else {
            panic!("expected inner builtin");
        };
        assert_eq!(b, BuiltinType::I32);
    }

    #[test]
    fn unknown_reference_target_errors() {
        let err = Document::open("type X { y: &NotDecl }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("unknown type 'NotDecl'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn resolve_builtin_returns_builtin() {
        let doc = open("");
        match doc.resolve(&TypeRef::Builtin(BuiltinType::I32)) {
            ResolvedType::Builtin(b) => assert_eq!(b, BuiltinType::I32),
            _ => panic!("expected builtin"),
        }
    }

    #[test]
    fn resolve_lets_caller_walk_named_decl_fields() {
        let doc = open(
            r#"
            type Inner { a: i32 b: utf8 }
            type Outer { inner: Inner }
            "#,
        );
        let outer = doc.type_decl("Outer").unwrap();
        let inner_field = outer.field("inner").unwrap();
        let ResolvedType::Named(inner_decl) = doc.resolve(inner_field.type_ref()) else {
            panic!("expected named");
        };
        let sub_names: Vec<_> = inner_decl.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(sub_names, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn resolve_list_of_named() {
        let doc = open("type User { name: utf8 }\ntype Q { items: list<User> }");
        let q = doc.type_decl("Q").unwrap();
        let items = q.field("items").unwrap();
        let resolved = doc.resolve(items.type_ref());
        let ResolvedType::List(inner) = resolved else {
            panic!("expected List");
        };
        let ResolvedType::Named(decl) = *inner else {
            panic!("expected Named inner");
        };
        assert_eq!(decl.name(), "User");
    }

    #[test]
    fn resolve_tensor_keeps_dims() {
        let doc = open("type Q { w: tensor<f32, [3, N, 5]> }");
        let q = doc.type_decl("Q").unwrap();
        let w = q.field("w").unwrap();
        let resolved = doc.resolve(w.type_ref());
        let ResolvedType::Tensor { element, dims } = resolved else {
            panic!("expected Tensor");
        };
        let ResolvedType::Builtin(b) = *element else {
            panic!("expected Builtin element");
        };
        assert_eq!(b, BuiltinType::F32);
        assert_eq!(
            dims,
            &[
                TensorDim::Fixed(3),
                TensorDim::Symbolic("N".into()),
                TensorDim::Fixed(5),
            ]
        );
    }

    #[test]
    fn unknown_list_element_type_errors() {
        let err = Document::open("type Q { items: list<NotDecl> }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("unknown type 'NotDecl'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn unknown_tensor_element_type_errors() {
        let err = Document::open("type Q { w: tensor<NotDecl, [4]> }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("unknown type 'NotDecl'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn list_of_reference_resolves() {
        let doc = open("type T { x: i32 }\ntype Q { items: list<&T> }");
        let q = doc.type_decl("Q").unwrap();
        let resolved = doc.resolve(q.field("items").unwrap().type_ref());
        let ResolvedType::List(inner) = resolved else {
            panic!("expected List");
        };
        let ResolvedType::Reference(inner2) = *inner else {
            panic!("expected Reference inner");
        };
        let ResolvedType::Named(d) = *inner2 else {
            panic!("expected Named");
        };
        assert_eq!(d.name(), "T");
    }

    #[test]
    fn symbol_sets_are_queryable() {
        let doc = open(
            r#"
            symbol_set Color { red green blue }
            symbol_set Mood { warm cool }
            "#,
        );
        assert_eq!(doc.symbol_sets().count(), 2);
        let color = doc.symbol_set("Color").expect("Color");
        assert_eq!(color.symbols().count(), 3);
        assert!(color.has("red"));
        assert!(!color.has("missing"));
    }

    #[test]
    fn resolve_named_symbol_set() {
        let doc = open("symbol_set Color { red green }\ntype Q { f: Color }");
        let q = doc.type_decl("Q").unwrap();
        let f = q.field("f").unwrap();
        match doc.resolve(f.type_ref()) {
            ResolvedType::SymbolSet(s) => assert_eq!(s.name(), "Color"),
            _ => panic!("expected SymbolSet"),
        }
    }

    #[test]
    fn duplicate_symbol_in_set_errors() {
        let err = Document::open("symbol_set X { a a }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("duplicate symbol 'a' in symbol_set 'X'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn symbol_set_collides_with_type_name() {
        let err = Document::open("type Foo {}\nsymbol_set Foo {}", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("duplicate declaration 'Foo'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn symbol_value_evaluates() {
        let doc = open("tag = :wide");
        assert_eq!(
            doc.field("tag").unwrap().value().unwrap(),
            &Value::Symbol("wide".into())
        );
    }

    #[test]
    fn symbol_sets_dont_appear_in_field_or_block_iteration() {
        let doc = open(
            r#"
            symbol_set Color { red green }
            count = 1
            svc {}
            "#,
        );
        let field_names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(field_names, vec!["count".to_string()]);
        let block_kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
        assert_eq!(block_kinds, vec!["svc".to_string()]);
    }

    #[test]
    fn decorator_iterator_on_type() {
        let doc = open(r#"@deprecated("X") type Foo {}"#);
        let t = doc.type_decl("Foo").unwrap();
        let decs: Vec<_> = t.decorators().collect();
        assert_eq!(decs.len(), 1);
        assert_eq!(decs[0].name(), "deprecated");
        assert_eq!(decs[0].positional().unwrap(), vec![Value::Utf8("X".into())]);
    }

    #[test]
    fn decorator_iterator_on_field() {
        let doc = open("type T { @max(64) name: utf8 }");
        let t = doc.type_decl("T").unwrap();
        let f = t.field("name").unwrap();
        let decs: Vec<_> = f.decorators().collect();
        assert_eq!(decs.len(), 1);
        assert_eq!(decs[0].name(), "max");
        assert_eq!(decs[0].positional().unwrap(), vec![Value::I64(64)]);
    }

    #[test]
    fn decorator_iterator_on_variant() {
        let doc = open("union U { @hidden Circle { radius: f64 } }");
        let u = doc.union_decl("U").unwrap();
        let v = u.variant("Circle").unwrap();
        let decs: Vec<_> = v.decorators().collect();
        assert_eq!(decs.len(), 1);
        assert_eq!(decs[0].name(), "hidden");
    }

    #[test]
    fn decorator_iterator_on_symbol_entry() {
        let doc = open("symbol_set C { @default red green }");
        let s = doc.symbol_set("C").unwrap();
        let entries: Vec<_> = s.symbols().collect();
        assert_eq!(entries[0].decorators().count(), 1);
        assert_eq!(entries[1].decorators().count(), 0);
    }

    #[test]
    fn decorator_named_args_via_helper() {
        let doc = open("@v(min = 1, max = 10) type X {}");
        let x = doc.type_decl("X").unwrap();
        let d = x.decorators().next().unwrap();
        assert_eq!(d.named_arg("min").unwrap().unwrap(), Value::I64(1));
        assert_eq!(d.named_arg("max").unwrap().unwrap(), Value::I64(10));
        assert!(d.named_arg("missing").is_none());
    }

    #[test]
    fn decorator_with_symbol_arg() {
        let doc = open("@tagged(:enabled) type X {}");
        let x = doc.type_decl("X").unwrap();
        let d = x.decorators().next().unwrap();
        assert_eq!(
            d.positional().unwrap(),
            vec![Value::Symbol("enabled".into())]
        );
    }

    #[test]
    fn decorator_with_none_arg() {
        let doc = open("@default(none) type X {}");
        let x = doc.type_decl("X").unwrap();
        let d = x.decorators().next().unwrap();
        assert_eq!(d.positional().unwrap(), vec![Value::None]);
    }

    #[test]
    fn decorator_dotted_name_full_name() {
        let doc = open("@a.b.c type X {}");
        let x = doc.type_decl("X").unwrap();
        let d = x.decorators().next().unwrap();
        assert_eq!(d.full_name(), "a.b.c");
        assert_eq!(d.name(), "c");
    }

    #[test]
    fn block_schema_lookup() {
        let doc = open(r#"@block("service") type Service {}"#);
        let s = doc.block_schema("service").expect("Service schema");
        assert_eq!(s.name(), "Service");
        assert!(doc.block_schema("nope").is_none());
    }

    #[test]
    fn decorator_schema_lookup() {
        let doc = open(r#"@decorator("max") type MaxDec { value: i64 }"#);
        let s = doc.decorator_schema("max").expect("max schema");
        assert_eq!(s.name(), "MaxDec");
    }

    #[test]
    fn inline_slot_helper() {
        let doc = open("type Q { @inline(2) f: utf8 }");
        let q = doc.type_decl("Q").unwrap();
        assert_eq!(q.field("f").unwrap().inline_slot(), Some(2));
    }

    #[test]
    fn inline_slot_returns_none_when_decorator_absent() {
        let doc = open("type Q { f: utf8 }");
        let q = doc.type_decl("Q").unwrap();
        assert_eq!(q.field("f").unwrap().inline_slot(), None);
    }

    #[test]
    fn default_value_helper() {
        let doc = open("type Q { @default(8080) port: u32? }");
        let q = doc.type_decl("Q").unwrap();
        assert_eq!(
            q.field("port").unwrap().default_value(),
            Some(Value::I64(8080))
        );
    }

    #[test]
    fn default_value_with_symbol_arg() {
        let doc = open("type Q { @default(:enabled) mode: symbol }");
        let q = doc.type_decl("Q").unwrap();
        assert_eq!(
            q.field("mode").unwrap().default_value(),
            Some(Value::Symbol("enabled".into()))
        );
    }

    #[test]
    fn mixed_block_labels_round_trip() {
        let doc = open(r#"service web "prod" { port = 1 }"#);
        let b = doc.block("service").unwrap();
        let labels = b.labels().unwrap();
        assert_eq!(
            labels,
            vec![Value::Identifier("web".into()), Value::Utf8("prod".into())]
        );
    }

    #[test]
    fn block_label_can_be_any_value() {
        let doc = open(r#"slot 0 :enabled 1.5 { x = 1 }"#);
        let b = doc.block("slot").unwrap();
        let labels = b.labels().unwrap();
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0], Value::I64(0));
        assert_eq!(labels[1], Value::Symbol("enabled".into()));
        assert_eq!(labels[2], Value::F64(1.5));
    }

    #[test]
    fn cycle_error_renders() {
        let err = EvalError::Cycle {
            field: "x".into(),
            span: SourceSpan::new(0.into(), 1),
        };
        let s = format!("{}", err);
        assert!(s.contains("cycle"));
    }

    // ─── Evaluator (builtins + operators + identifier resolution) ────

    fn env_with_test_builtins() -> Environment {
        use crate::builtins::from_fn;
        let mut env = Environment::empty();
        env.add_builtin("upper", from_fn(|s: String| s.to_uppercase()));
        env.add_builtin("len", from_fn(|s: String| s.len() as i64));
        env.add_builtin("add", from_fn(|a: i64, b: i64| a + b));
        env.add_builtin(
            "die",
            from_fn(|s: String| -> Result<i64, String> { Err(s) }),
        );
        env
    }

    fn open_with_builtins(src: &str) -> Document {
        // Same lax wrap as `open()` so the strict-validation default
        // doesn't fail every eval test on `NoDocumentSchema`.
        let lax = laxify_for_tests(src);
        Document::open_with(&lax, "test", &env_with_test_builtins()).expect("open")
    }

    #[test]
    fn eval_call_literal_arg() {
        let doc = open_with_builtins(r#"out = upper("hi")"#);
        assert_eq!(
            doc.field("out").unwrap().value().unwrap(),
            &Value::Utf8("HI".into())
        );
    }

    #[test]
    fn eval_call_identifier_arg_resolves_via_field_lookup() {
        let doc = open_with_builtins(
            r#"
            name = "alpha"
            out  = upper(name)
            "#,
        );
        assert_eq!(
            doc.field("out").unwrap().value().unwrap(),
            &Value::Utf8("ALPHA".into())
        );
    }

    #[test]
    fn eval_call_nested() {
        let doc = open_with_builtins(r#"n = add(len("ab"), 1)"#);
        assert_eq!(doc.field("n").unwrap().value().unwrap(), &Value::I64(3));
    }

    #[test]
    fn eval_unknown_builtin_errors() {
        let doc = open_with_builtins("x = nope()");
        let err = doc.field("x").unwrap().value().unwrap_err();
        assert!(matches!(err, EvalError::UnknownBuiltin { .. }));
    }

    #[test]
    fn eval_arity_mismatch_errors() {
        let doc = open_with_builtins(r#"x = upper("a", "b")"#);
        let err = doc.field("x").unwrap().value().unwrap_err();
        assert!(matches!(err, EvalError::BuiltinArity { .. }));
    }

    #[test]
    fn eval_type_mismatch_errors_at_builtin() {
        let doc = open_with_builtins("x = upper(42)");
        let err = doc.field("x").unwrap().value().unwrap_err();
        assert!(matches!(err, EvalError::BuiltinTypeMismatch { .. }));
    }

    #[test]
    fn eval_fallible_builtin_propagates_error() {
        let doc = open_with_builtins(r#"x = die("boom")"#);
        let err = doc.field("x").unwrap().value().unwrap_err();
        let EvalError::BuiltinTypeMismatch { message, .. } = err else {
            panic!("expected BuiltinTypeMismatch, got {err:?}");
        };
        assert!(message.contains("boom"), "{message}");
    }

    #[test]
    fn eval_arithmetic_precedence() {
        let doc = open_with_builtins("x = 1 + 2 * 3");
        assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::I64(7));
    }

    #[test]
    fn eval_unary_neg_and_paren() {
        let doc = open_with_builtins("x = -(1 + 2)");
        assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::I64(-3));
    }

    #[test]
    fn eval_comparison_returns_bool() {
        let doc = open_with_builtins("a = 2 > 1\nb = 2 == 1\nc = 2 != 1");
        assert_eq!(doc.field("a").unwrap().value().unwrap(), &Value::Bool(true));
        assert_eq!(
            doc.field("b").unwrap().value().unwrap(),
            &Value::Bool(false)
        );
        assert_eq!(doc.field("c").unwrap().value().unwrap(), &Value::Bool(true));
    }

    #[test]
    fn eval_short_circuits_logical_and() {
        // `false && nope()` must not invoke the unknown builtin.
        let doc = open_with_builtins("x = false && nope()");
        assert_eq!(
            doc.field("x").unwrap().value().unwrap(),
            &Value::Bool(false)
        );
    }

    #[test]
    fn eval_short_circuits_logical_or() {
        let doc = open_with_builtins("x = true || nope()");
        assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::Bool(true));
    }

    #[test]
    fn eval_block_with_let_bindings() {
        let doc = open_with_builtins("x = { let a = 2; let b = 3; a + b }");
        assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::I64(5));
    }

    #[test]
    fn eval_block_inner_let_shadows_field() {
        let doc = open_with_builtins(
            r#"
            n = 100
            x = { let n = 1; n + 2 }
            "#,
        );
        assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::I64(3));
    }

    #[test]
    fn eval_decorator_positional_arg_evaluates() {
        let doc = open_with_builtins("@logged(add(1, 2)) type X {}");
        let t = doc.type_decl("X").unwrap();
        let d = t.decorators().next().unwrap();
        let pos = d.positional().unwrap();
        assert_eq!(pos, vec![Value::I64(3)]);
    }

    #[test]
    fn eval_user_function_call_returns_body_value() {
        // Function literals are first-class: bind one to a field and call
        // it by name; the body sees the parameter as a local.
        let doc = open_with_builtins(
            r#"
            f = fn(x: i32) -> i32 x
            y = f(3)
            "#,
        );
        assert_eq!(*doc.field("y").unwrap().value().unwrap(), Value::I64(3));
    }

    #[test]
    fn eval_user_function_arity_mismatch() {
        let doc = open_with_builtins(
            r#"
            f = fn(x: i32) -> i32 x
            y = f(1, 2)
            "#,
        );
        let err = doc.field("y").unwrap().value().unwrap_err();
        assert!(matches!(err, EvalError::BuiltinArity { .. }));
    }

    // ─── Lazy data access (DataRef / Document::get) ───────────────────

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    #[test]
    fn get_resolves_top_level_field() {
        let doc = open("port = 8080");
        let r = doc.get("port").expect("port should resolve");
        assert_eq!(r.value().unwrap(), Value::I64(8080));
    }

    #[test]
    fn get_resolves_nested_block_path() {
        let doc = open(r#"service "web" { port = 9090 }"#);
        let r = doc
            .get("service.port")
            .expect("service.port should resolve");
        assert_eq!(r.value().unwrap(), Value::I64(9090));
    }

    #[test]
    fn get_resolves_deeply_nested_path() {
        let doc = open(
            r#"
            service "web" {
              metadata {
                region = "us-east-1"
              }
            }
            "#,
        );
        let r = doc
            .get("service.metadata.region")
            .expect("path should resolve");
        assert_eq!(r.value().unwrap(), Value::Utf8("us-east-1".into()));
    }

    #[test]
    fn get_returns_none_for_missing_segment() {
        let doc = open(r#"service "web" { port = 1 }"#);
        assert!(doc.get("service.missing").is_none());
        assert!(doc.get("nonexistent").is_none());
    }

    #[test]
    fn get_intermediate_node_is_not_a_leaf() {
        let doc = open(r#"service "web" { port = 1 }"#);
        let svc = doc.get("service").expect("service block");
        let err = svc.value().unwrap_err();
        assert!(matches!(err, EvalError::NotALeaf { .. }));
    }

    #[test]
    fn get_descends_into_type_decl_field() {
        let doc = open("type User { name: utf8 age: u32 }");
        let f = doc.get("User.name").expect("User.name");
        assert_eq!(f.kind(), "type_field");
    }

    #[test]
    fn get_descends_into_union_variant() {
        let doc = open("union Shape { Circle { r: f64 } Square none }");
        let v = doc.get("Shape.Circle").expect("variant");
        assert_eq!(v.kind(), "variant");
        let r = doc.get("Shape.Circle.r").expect("variant field");
        assert_eq!(r.kind(), "type_field");
    }

    #[test]
    fn get_descends_into_symbol_set_entry() {
        let doc = open("symbol_set Color { red green }");
        let s = doc.get("Color.red").expect("symbol entry");
        assert_eq!(s.kind(), "symbol_entry");
    }

    #[test]
    fn block_labels_cached_across_calls() {
        // Inspect the labels OnceLock directly: empty before first call,
        // populated after.
        let doc = open(r#"my_block first "two" 3 { x = 1 }"#);
        let b = doc.block("my_block").unwrap();
        let labels_cell = match &b.cells.kind {
            ItemCellKind::Block { labels, .. } => labels,
            _ => unreachable!(),
        };
        assert!(labels_cell.get().is_none());
        let v1 = b.labels().unwrap();
        assert!(labels_cell.get().is_some());
        let v2 = b.labels().unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn decorator_positional_cached_across_calls() {
        let counter = Arc::new(AtomicUsize::new(0));
        let bumper = {
            let c = counter.clone();
            crate::builtins::from_fn(move || {
                c.fetch_add(1, AtomicOrdering::Relaxed);
                7i64
            })
        };
        let mut env = Environment::empty();
        env.add_builtin("bump", bumper);

        let doc = Document::open_with(r#"@x(bump()) type T {}"#, "test", &env).expect("open");
        let t = doc.type_decl("T").unwrap();
        let d = t.decorators().next().unwrap();
        let _ = d.positional().unwrap();
        let _ = d.positional().unwrap();
        let _ = d.positional().unwrap();
        assert_eq!(counter.load(AtomicOrdering::Relaxed), 1);
    }

    #[test]
    fn decorator_named_arg_cached_across_calls() {
        let counter = Arc::new(AtomicUsize::new(0));
        let bumper = {
            let c = counter.clone();
            crate::builtins::from_fn(move || {
                c.fetch_add(1, AtomicOrdering::Relaxed);
                3i64
            })
        };
        let mut env = Environment::empty();
        env.add_builtin("bump", bumper);

        let doc =
            Document::open_with(r#"@x(amount = bump()) type T {}"#, "test", &env).expect("open");
        let t = doc.type_decl("T").unwrap();
        let d = t.decorators().next().unwrap();
        let _ = d.named_arg("amount").unwrap().unwrap();
        let _ = d.named_arg("amount").unwrap().unwrap();
        let _ = d.named_arg("amount").unwrap().unwrap();
        assert_eq!(counter.load(AtomicOrdering::Relaxed), 1);
    }

    // ─── Nested-block schema (`@child` / `@children`) ─────────────────

    fn open_nested() -> Document {
        Document::open(
            r#"
            @block("service", max_children = 50)
            type Service {
              @inline(0) id: identifier
              @child("config")             config:  Config
              @children("route", max = 32) routes:  list<Route>
            }

            @block("config")
            type Config { region: utf8  tier: symbol }

            @block("route")
            type Route {
              @inline(0) path: utf8
              method: utf8
            }

            service web {
              config { region = "us-east-1"  tier = :gold }
              route "/api"     { method = "GET" }
              route "/healthz" { method = "GET" }
            }
            "#,
            "test",
        )
        .expect("open")
    }

    #[test]
    fn typefield_reports_child_kind() {
        let doc = open_nested();
        let svc = doc.type_decl("Service").unwrap();
        assert_eq!(
            svc.field("config").unwrap().child_block_kind().as_deref(),
            Some("config")
        );
        assert_eq!(
            svc.field("routes")
                .unwrap()
                .children_block_kind()
                .as_deref(),
            Some("route")
        );
        assert_eq!(svc.field("routes").unwrap().children_max(), Some(32));
    }

    #[test]
    fn typedecl_reports_max_children_and_allowed_set() {
        let doc = open_nested();
        let svc = doc.type_decl("Service").unwrap();
        assert_eq!(svc.max_children(), Some(50));
        let mut allowed = svc.allowed_child_kinds();
        allowed.sort();
        assert_eq!(allowed, vec!["config".to_string(), "route".to_string()]);
    }

    #[test]
    fn data_ref_resolves_child_field_to_nested_block() {
        let doc = open_nested();
        let cfg = doc.get("service.config").expect("service.config");
        assert_eq!(cfg.kind(), "block");
        let region = cfg.get("region").expect("region");
        assert_eq!(region.value().unwrap(), Value::Utf8("us-east-1".into()));
    }

    #[test]
    fn data_ref_resolves_children_field_to_block_list() {
        let doc = open_nested();
        let routes = doc.get("service.routes").expect("service.routes");
        assert_eq!(routes.kind(), "block_list");
        assert_eq!(routes.len(), Some(2));
        let first = routes.children().next().unwrap();
        let method = first.get("method").unwrap().value().unwrap();
        assert_eq!(method, Value::Utf8("GET".into()));
    }

    #[test]
    fn raw_ast_view_still_works() {
        // Block::blocks() / Block::field() (raw AST) keep their
        // structural semantics regardless of schema.
        let doc = open_nested();
        let svc = doc.block("service").unwrap();
        let raw_kinds: Vec<&str> = svc.blocks().map(|b| b.kind()).collect();
        assert_eq!(raw_kinds, vec!["config", "route", "route"]);
    }

    #[test]
    fn schema_errors_empty_for_clean_block() {
        let doc = open_nested();
        let svc = doc.block("service").unwrap();
        assert!(svc.schema_errors().is_empty());
    }

    #[test]
    fn schema_errors_missing_required_child() {
        let doc = Document::open(
            r#"
            @block("service") type Service {
              @child("config") config: Config
            }
            @block("config") type Config {}
            service web {}
            "#,
            "test",
        )
        .expect("open");
        let svc = doc.block("service").unwrap();
        let errs = svc.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::MissingRequired,
                    ..
                }
            )),
            "expected MissingRequired, got {errs:?}"
        );
    }

    #[test]
    fn schema_errors_disallowed_child_kind() {
        let doc = Document::open(
            r#"
            @block("service") type Service {
              @child("config") config: Config?
            }
            @block("config") type Config {}
            service web { config {}  rogue {} }
            "#,
            "test",
        )
        .expect("open");
        let svc = doc.block("service").unwrap();
        let errs = svc.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::DisallowedChild,
                    ..
                }
            )),
            "expected DisallowedChild, got {errs:?}"
        );
    }

    #[test]
    fn schema_errors_children_max_violated() {
        let doc = Document::open(
            r#"
            @block("service") type Service {
              @children("route", max = 1) routes: list<Route>
            }
            @block("route") type Route {}
            service web { route {} route {} }
            "#,
            "test",
        )
        .expect("open");
        let svc = doc.block("service").unwrap();
        let errs = svc.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::ChildrenTooMany,
                    ..
                }
            )),
            "expected ChildrenTooMany, got {errs:?}"
        );
    }

    #[test]
    fn schema_errors_block_max_children_violated() {
        let doc = Document::open(
            r#"
            @block("service", max_children = 1) type Service {
              @children("route") routes: list<Route>
            }
            @block("route") type Route {}
            service web { route {} route {} }
            "#,
            "test",
        )
        .expect("open");
        let svc = doc.block("service").unwrap();
        let errs = svc.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::BlockChildrenOverflow,
                    ..
                }
            )),
            "expected BlockChildrenOverflow, got {errs:?}"
        );
    }

    #[test]
    fn schema_errors_cached_after_first_call() {
        let doc = open_nested();
        let svc = doc.block("service").unwrap();
        let p1 = svc.schema_errors().as_ptr();
        let p2 = svc.schema_errors().as_ptr();
        assert_eq!(p1, p2, "schema_errors should be cached (same Vec address)");
    }

    #[test]
    fn schemaless_block_passes_validation() {
        // Strict mode: un-schema'd kinds normally error
        // `UnregisteredKind`, but `@schemaless` opts out.
        let doc = Document::open(r#"@schemaless random "label" { x = 1 }"#, "test").expect("open");
        let b = doc.block("random").unwrap();
        assert!(b.schema_errors().is_empty());
    }

    #[test]
    fn unschemad_block_surfaces_unregistered_kind() {
        // The opposite of `schemaless_block_passes_validation` —
        // without `@schemaless`, the un-registered kind itself is
        // an error.
        let doc = Document::open(r#"random "label" { x = 1 }"#, "test").expect("open");
        let b = doc.block("random").unwrap();
        let errs = b.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::UnregisteredKind,
                    ..
                }
            )),
            "expected UnregisteredKind, got {errs:?}"
        );
    }

    // ─── List literals + required_children ───────────────────────────

    #[test]
    fn eval_list_literal_to_value_list() {
        let doc = open("x = [1, 2, 3]");
        assert_eq!(
            doc.field("x").unwrap().value().unwrap(),
            &Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
        );
    }

    #[test]
    fn eval_empty_list_literal() {
        let doc = open("x = []");
        assert_eq!(
            doc.field("x").unwrap().value().unwrap(),
            &Value::List(vec![])
        );
    }

    #[test]
    fn eval_nested_list_literal() {
        let doc = open("x = [[1, 2], [3, 4]]");
        let v = doc.field("x").unwrap().value().unwrap();
        let Value::List(outer) = v else {
            panic!("expected outer list")
        };
        assert_eq!(outer.len(), 2);
        assert_eq!(outer[0], Value::List(vec![Value::I64(1), Value::I64(2)]));
    }

    #[test]
    fn eval_list_literal_resolves_identifiers() {
        let doc = open("a = 1\nb = 2\nx = [a, b, 3]");
        assert_eq!(
            doc.field("x").unwrap().value().unwrap(),
            &Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
        );
    }

    #[test]
    fn eval_decorator_arg_with_list_literal() {
        let doc = open(r#"@v(items = [1, 2, 3]) type T {}"#);
        let t = doc.type_decl("T").unwrap();
        let d = t.decorators().next().unwrap();
        let arg = d.named_arg("items").unwrap().unwrap();
        assert_eq!(
            arg,
            Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
        );
    }

    #[test]
    fn required_children_reads_list_arg() {
        let doc = open(
            r#"
            @block("service", required_children = ["config", "audit"])
            type Service {
              @child("config")  config:  Config?
              @child("audit")   audit:   Audit?
            }
            @block("config") type Config {}
            @block("audit")  type Audit {}
            "#,
        );
        let svc = doc.type_decl("Service").unwrap();
        assert_eq!(
            svc.required_children(),
            vec!["config".to_string(), "audit".to_string()]
        );
    }

    #[test]
    fn required_children_present_no_error() {
        let doc = open(
            r#"
            @block("service", required_children = ["config"])
            type Service {
              @child("config") config: Config?
            }
            @block("config") type Config {}
            service web { config {} }
            "#,
        );
        let svc = doc.block("service").unwrap();
        assert!(svc.schema_errors().is_empty(), "{:?}", svc.schema_errors());
    }

    #[test]
    fn required_children_missing_errors() {
        let doc = open(
            r#"
            @block("service", required_children = ["config"])
            type Service {
              @child("config") config: Config?
            }
            @block("config") type Config {}
            service web {}
            "#,
        );
        let svc = doc.block("service").unwrap();
        let errs = svc.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::MissingRequired,
                    message,
                    ..
                } if message.contains("required child kind 'config'")
            )),
            "expected MissingRequired for kind 'config', got {errs:?}"
        );
    }

    // ─── Tables (`@table` + pipe-row syntax) ─────────────────────────

    fn open_table_doc() -> Document {
        Document::open(
            r#"
            @table("user")
            type User { name: utf8  age: u32  active: bool }

            @block("db")
            type DB { @children("user") users: list<User> }

            db production {
              users:
                | "alice" | 30 | true |
                | "bob"   | 25 | false |
                | "cara"  | 42 | true |
            }
            "#,
            "test",
        )
        .expect("open")
    }

    #[test]
    fn table_schema_lookup() {
        let doc = open_table_doc();
        assert!(doc.table_schema("user").is_some());
        assert!(doc.table_schema("nope").is_none());
    }

    #[test]
    fn child_kind_table_yields_data_kind_table() {
        let doc = open_table_doc();
        let users = doc.get("db.users").expect("db.users");
        assert_eq!(users.kind(), "table");
    }

    #[test]
    fn row_count_matches_source_rows() {
        let doc = open_table_doc();
        let users = doc.get("db.users").unwrap();
        assert_eq!(users.row_count(), Some(3));
        assert_eq!(users.len(), Some(3));
    }

    #[test]
    fn row_returns_block_with_labels() {
        let doc = open_table_doc();
        let users = doc.get("db.users").unwrap();
        let alice = users.row(0).expect("row 0");
        assert_eq!(alice.kind(), "block");
        let block = alice.as_block().unwrap();
        let labels = block.labels().unwrap();
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0], Value::Utf8("alice".into()));
        // Number literals default to i64 — element-type coercion to the
        // schema's u32 isn't done in this pass.
        assert_eq!(labels[1], Value::I64(30));
        assert_eq!(labels[2], Value::Bool(true));
    }

    #[test]
    fn column_projects_named_field() {
        let doc = open_table_doc();
        let users = doc.get("db.users").unwrap();
        let names = users.column("name").unwrap();
        assert_eq!(
            names,
            vec![
                Value::Utf8("alice".into()),
                Value::Utf8("bob".into()),
                Value::Utf8("cara".into()),
            ]
        );
        let ages = users.column("age").unwrap();
        assert_eq!(ages, vec![Value::I64(30), Value::I64(25), Value::I64(42)]);
    }

    #[test]
    fn child_kind_block_still_yields_blocklist() {
        // When the kind isn't @table-schema'd, @children still returns
        // a plain DataKind::BlockList.
        let doc = Document::open(
            r#"
            @block("route") type Route { @inline(0) path: utf8 }
            @block("service") type Service { @children("route") routes: list<Route> }
            service web { route "/api" {} route "/healthz" {} }
            "#,
            "test",
        )
        .expect("open");
        let routes = doc.get("service.routes").unwrap();
        assert_eq!(routes.kind(), "block_list");
    }

    #[test]
    fn mixed_rows_and_blocks_in_same_children_field() {
        // Both literal `user { name=...; ... }` blocks and pipe-row
        // entries under `users:` contribute to the same projection.
        let doc = Document::open(
            r#"
            @table("user") type User { name: utf8  age: u32 }
            @block("db") type DB { @children("user") users: list<User> }
            db x {
              users:
                | "row-a" | 1 |
                | "row-b" | 2 |
              user { name = "block-c"  age = 3 }
            }
            "#,
            "test",
        )
        .expect("open");
        let users = doc.get("db.users").unwrap();
        // BlockList because mixing with a `@block`-form? No — User is
        // @table; result is still Table.
        assert_eq!(users.kind(), "table");
        // Three entries total: two synthesised rows + one literal
        // block.
        assert_eq!(users.row_count(), Some(3));
        let names = users.column("name").unwrap();
        assert_eq!(
            names,
            vec![
                Value::Utf8("row-a".into()),
                Value::Utf8("row-b".into()),
                Value::Utf8("block-c".into()),
            ]
        );
    }

    #[test]
    fn row_column_count_mismatch_errors() {
        let doc = Document::open(
            r#"
            @table("user") type User { name: utf8  age: u32 }
            @block("db") type DB { @children("user") users: list<User> }
            db x {
              users:
                | "alice" | 30 |
                | "bob"   |
            }
            "#,
            "test",
        )
        .expect("open");
        let users = doc.get("db.users").unwrap();
        let bob = users.row(1).unwrap().as_block().unwrap();
        let errs = bob.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::ChildrenTooFew,
                    ..
                }
            )),
            "expected ChildrenTooFew for short row, got {errs:?}"
        );
    }

    #[test]
    fn required_children_non_string_entries_ignored() {
        let doc = open(
            r#"
            @block("service", required_children = ["config", 42, true])
            type Service { @child("config") config: Config? }
            @block("config") type Config {}
            service web { config {} }
            "#,
        );
        let svc = doc.type_decl("Service").unwrap();
        // Only the string entry survives.
        assert_eq!(svc.required_children(), vec!["config".to_string()]);
    }

    // ─── References (scope-aware lookup + parent/self) ────────────────

    fn open_refs() -> Document {
        Document::open(
            r#"
            @table("user") type User { name: utf8  age: u32 }
            @block("db") type DB {
              @children("user") users: list<User>
              active: &User
              pinned: &User
            }
            db production {
              users:
                | "alice" | 30 |
                | "bob"   | 25 |
                | "cara"  | 42 |
              active = users.alice
              pinned = users.bob
            }
            "#,
            "refs",
        )
        .expect("open refs")
    }

    #[test]
    fn parser_accepts_self_and_parent_in_expression_position() {
        let doc = Document::open(
            r#"
            @schemaless anchor = 1
            @schemaless x = parent
            @schemaless y = self
            "#,
            "test",
        )
        .expect("parses");
        // Just confirm the field eval path triggers — `parent` at
        // doc root errors, `self` resolves to the document.
        let parent_err = doc.field("x").unwrap().value().unwrap_err();
        assert!(
            matches!(parent_err, EvalError::UnresolvedReference { .. }),
            "{parent_err:?}"
        );
        // `self` at the doc root yields the document, which is not a
        // leaf so materialise_dataref returns NotALeaf.
        let self_err = doc.field("y").unwrap().value().unwrap_err();
        assert!(
            matches!(self_err, EvalError::NotALeaf { .. }),
            "{self_err:?}"
        );
    }

    #[test]
    fn parent_keyword_remains_valid_as_type_field_name() {
        // Existing source like `type User { parent: &User? }` must
        // keep parsing — `parent`/`self` are contextual keywords,
        // only special in expression atom position.
        let doc = Document::open(r#"type User { parent: &User? }"#, "test").expect("parses");
        let user = doc.type_decl("User").unwrap();
        assert!(user.field("parent").is_some());
    }

    #[test]
    fn ref_field_reference_returns_dataref_navigator() {
        let doc = open_refs();
        let active = doc.get("db.active").expect("db.active present");
        let target = active
            .reference()
            .expect("reference() returns Some for &T field")
            .expect("ref resolves");
        // The target is the synthesised row block for "alice".
        let labels = target.as_block().unwrap().labels().unwrap();
        assert_eq!(labels.first(), Some(&Value::Utf8("alice".into())));
    }

    #[test]
    fn ref_field_value_auto_derefs_to_target_leaf() {
        // `pinned = bob` resolves via the scope chain to the bob row
        // (a Block), which isn't a leaf, so `.value()` errors. This
        // is the expected behaviour for `&User` — host code should
        // use `.reference()` to navigate further.
        let doc = open_refs();
        let pinned = doc.get("db.pinned").expect("db.pinned present");
        let err = pinned.value().unwrap_err();
        assert!(matches!(err, EvalError::NotALeaf { .. }), "{err:?}");
    }

    #[test]
    fn non_ref_field_keeps_value_path() {
        // `count = 3` is a plain leaf assignment; no reference.
        let doc = Document::open("@schemaless count = 3", "t").unwrap();
        let count = doc.field("count").unwrap();
        assert_eq!(count.value().unwrap(), &Value::I64(3));
        assert!(count.reference().is_none());
    }

    #[test]
    fn unresolved_member_path_errors_only_when_value_called() {
        // `&User` field whose RHS dotted path points to nothing.
        let doc = Document::open(
            r#"
            @block("db") type DB { dangling: &Sentinel }
            type Sentinel {}
            db x { dangling = somewhere.nowhere }
            "#,
            "t",
        )
        .expect("opens (validation is lazy)");
        let db = doc.block("db").unwrap();
        let dangling = db.field("dangling").unwrap();
        let r = dangling.reference().expect("&T field");
        match r {
            Ok(_) => panic!("expected UnresolvedReference"),
            Err(e) => assert!(matches!(e, EvalError::UnresolvedReference { .. }), "{e:?}"),
        }
    }

    #[test]
    fn self_inside_block_returns_current_block_dataref() {
        // `self` inside a block resolves to the enclosing block.
        // Reading a child of that DataRef should produce the same
        // value as reading the child directly.
        let doc = Document::open(
            r#"
            @block("svc") type Svc { port: u32  echo: &Svc }
            svc web { port = 8080  echo = self }
            "#,
            "t",
        )
        .expect("opens");
        let svc = doc.get("svc").unwrap();
        let echo = svc.child("echo").unwrap();
        let target = echo.reference().unwrap().unwrap();
        // self → enclosing svc block; reading port through it.
        let port = target.child("port").unwrap().value().unwrap();
        assert_eq!(port, Value::I64(8080));
    }

    #[test]
    fn parent_in_nested_block_walks_up_one_level() {
        let doc = Document::open(
            r#"
            @block("outer") type Outer { name: utf8 @child("inner") inner: Inner? }
            @block("inner") type Inner { up: &Outer }
            outer top { name = "the-top"  inner { up = parent } }
            "#,
            "t",
        )
        .expect("opens");
        let up = doc.get("outer.inner.up").unwrap();
        let target = up.reference().unwrap().unwrap();
        let name = target.child("name").unwrap().value().unwrap();
        assert_eq!(name, Value::Utf8("the-top".into()));
    }

    // ─── Strict schema validation ─────────────────────────────────────

    #[test]
    fn doc_schema_resolves_when_present() {
        let doc = Document::open(
            r#"
            @document type Root { name: utf8 }
            @schemaless name = "x"
            "#,
            "t",
        )
        .expect("opens");
        assert!(doc.doc_schema().is_some());
        assert_eq!(doc.doc_schema().unwrap().name(), "Root");
    }

    #[test]
    fn doc_schema_absent_returns_none() {
        let doc = Document::open("@schemaless name = \"x\"", "t").unwrap();
        assert!(doc.doc_schema().is_none());
    }

    #[test]
    fn multiple_document_decls_surface_error() {
        let doc = Document::open(
            r#"
            @document type A { x: utf8 }
            @document type B { y: utf8 }
            "#,
            "t",
        )
        .expect("opens");
        let errs = doc.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::MultipleDocumentSchemas,
                    ..
                }
            )),
            "{errs:?}"
        );
    }

    #[test]
    fn top_level_field_without_doc_schema_errors_on_value() {
        let doc = Document::open(r#"orphan = "x""#, "t").unwrap();
        let err = doc.field("orphan").unwrap().value().unwrap_err();
        assert!(
            matches!(
                err,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::NoDocumentSchema,
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn top_level_field_with_matching_doc_schema_resolves() {
        let doc = Document::open(
            r#"
            @document type Cfg { name: utf8 }
            name = "alpha"
            "#,
            "t",
        )
        .unwrap();
        assert_eq!(
            doc.field("name").unwrap().value().unwrap(),
            &Value::Utf8("alpha".into())
        );
        assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    }

    #[test]
    fn top_level_field_with_schemaless_decorator_resolves() {
        let doc = Document::open(r#"@schemaless port = 8080"#, "t").unwrap();
        assert_eq!(
            doc.field("port").unwrap().value().unwrap(),
            &Value::I64(8080)
        );
    }

    #[test]
    fn top_level_unregistered_block_kind_errors() {
        let doc = Document::open(r#"random "x" { y = 1 }"#, "t").unwrap();
        let errs = doc.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::NoDocumentSchema
                        | crate::error::SchemaViolationKind::UnregisteredKind,
                    ..
                }
            )),
            "{errs:?}"
        );
    }

    #[test]
    fn top_level_block_kind_disallowed_by_doc_schema_errors() {
        let doc = Document::open(
            r#"
            @document type Cfg { @child("svc") svc: Svc }
            @block("svc") type Svc { @inline(0) id: utf8 }
            @block("other") type Other {}
            other "x" {}
            "#,
            "t",
        )
        .unwrap();
        let errs = doc.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::DisallowedChild,
                    ..
                }
            )),
            "{errs:?}"
        );
    }

    #[test]
    fn nested_block_kind_unregistered_errors_on_schema_errors() {
        // Parent schema explicitly allows `unregistered` as a child
        // (so DisallowedChild doesn't fire), but `unregistered` has
        // no @block/@table declaration — that's the UnregisteredKind
        // violation.
        let doc = Document::open(
            r#"
            @document type Cfg { @child("svc") svc: Svc }
            @block("svc") type Svc { @child("unregistered") nested: Whatever? }
            type Whatever {}
            svc "x" { unregistered { y = 1 } }
            "#,
            "t",
        )
        .unwrap();
        let svc = doc.block("svc").unwrap();
        let nested = svc.block("unregistered").unwrap();
        let errs = nested.schema_errors();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::UnregisteredKind,
                    ..
                }
            )),
            "{errs:?}"
        );
    }

    #[test]
    fn nested_block_under_schemaless_parent_silently_passes() {
        let doc = Document::open(
            r#"
            @document type Cfg { @child("wrapper") wrapper: Wrapper }
            @block("wrapper") type Wrapper {}
            @schemaless wrapper "x" { whatever { junk = 1 } }
            "#,
            "t",
        )
        .unwrap();
        // @schemaless on `wrapper "x"` silences the kid's
        // UnregisteredKind that would otherwise fire on `whatever`.
        assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    }

    #[test]
    fn field_not_in_block_schema_errors_on_value() {
        let doc = Document::open(
            r#"
            @document type Cfg { @child("svc") svc: Svc }
            @block("svc") type Svc { name: utf8 }
            svc "x" { name = "ok"  surprise = 1 }
            "#,
            "t",
        )
        .unwrap();
        let svc = doc.block("svc").unwrap();
        let err = svc.field("surprise").unwrap().value().unwrap_err();
        assert!(
            matches!(
                err,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::UnknownField,
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn field_marked_schemaless_resolves_even_when_unknown() {
        let doc = Document::open(
            r#"
            @document type Cfg { @child("svc") svc: Svc }
            @block("svc") type Svc { name: utf8 }
            svc "x" { name = "ok"  @schemaless surprise = 1 }
            "#,
            "t",
        )
        .unwrap();
        let svc = doc.block("svc").unwrap();
        let v = svc.field("surprise").unwrap().value().unwrap();
        assert_eq!(v, &Value::I64(1));
    }

    // ─── Interfaces and `extends` ─────────────────────────────────────

    #[test]
    fn interface_declares_and_lookup_returns_some() {
        let doc = Document::open(
            r#"
            interface Drawable { bounds: utf8 }
            "#,
            "t",
        )
        .unwrap();
        let iface = doc.interface("Drawable").expect("interface present");
        assert_eq!(iface.name(), "Drawable");
        let fields: Vec<_> = iface.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(fields, vec!["bounds".to_string()]);
    }

    #[test]
    fn type_and_interface_with_same_name_clash() {
        let err = Document::open(
            r#"
            type Foo { x: utf8 }
            interface Foo { x: utf8 }
            "#,
            "t",
        )
        .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("duplicate declaration"), "{msg}");
    }

    #[test]
    fn interface_in_bare_position_errors_at_open() {
        let err = Document::open(
            r#"
            interface Drawable { x: utf8 }
            type Holder { bare: Drawable }
            "#,
            "t",
        )
        .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("must be used through a reference"), "{msg}");
    }

    #[test]
    fn interface_in_reference_position_resolves() {
        Document::open(
            r#"
            interface Drawable { x: utf8 }
            type Holder { d: &Drawable }
            "#,
            "t",
        )
        .expect("opens");
    }

    #[test]
    fn interface_inside_list_under_reference_is_allowed() {
        Document::open(
            r#"
            interface Drawable { x: utf8 }
            type Holder { ds: list<&Drawable> }
            "#,
            "t",
        )
        .expect("opens");
    }

    #[test]
    fn interface_in_function_param_under_reference_is_allowed() {
        Document::open(
            r#"
            interface Drawable { x: utf8 }
            type Holder { f: fn(&Drawable) -> i32 }
            "#,
            "t",
        )
        .expect("opens");
    }

    #[test]
    fn extends_unknown_parent_errors_at_open() {
        let err = Document::open(
            r#"
            type Dog extends Animal { breed: utf8 }
            "#,
            "t",
        )
        .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("unknown extends target"), "{msg}");
    }

    #[test]
    fn cyclic_extends_errors_at_open() {
        let err = Document::open(
            r#"
            type A extends B {}
            type B extends A {}
            "#,
            "t",
        )
        .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("cyclic extends"), "{msg}");
    }

    #[test]
    fn extends_conflicting_field_types_errors_at_open() {
        let err = Document::open(
            r#"
            type A { x: utf8 }
            type B { x: i32 }
            type C extends A, B { y: utf8 }
            "#,
            "t",
        )
        .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("conflicting type for field"), "{msg}");
    }

    #[test]
    fn extends_redeclaration_same_type_allowed() {
        Document::open(
            r#"
            type Animal { name: utf8 }
            type Dog extends Animal { name: utf8  breed: utf8 }
            "#,
            "t",
        )
        .expect("opens");
    }

    #[test]
    fn extends_redeclaration_different_type_errors() {
        let err = Document::open(
            r#"
            type Animal { name: utf8 }
            type Dog extends Animal { name: i32  breed: utf8 }
            "#,
            "t",
        )
        .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("conflicting type for field"), "{msg}");
    }

    #[test]
    fn type_decl_effective_fields_includes_ancestors() {
        let doc = Document::open(
            r#"
            type Animal { name: utf8  age: u32 }
            type Dog extends Animal { breed: utf8 }
            "#,
            "t",
        )
        .unwrap();
        let dog = doc.type_decl("Dog").unwrap();
        let names: Vec<_> = dog
            .effective_fields()
            .into_iter()
            .map(|f| f.name().to_string())
            .collect();
        assert_eq!(names, vec!["name", "age", "breed"]);
    }

    #[test]
    fn type_decl_extends_lists_parents_in_order() {
        let doc = Document::open(
            r#"
            type A { x: utf8 }
            interface B { y: utf8 }
            type C extends A, B { z: utf8 }
            "#,
            "t",
        )
        .unwrap();
        let c = doc.type_decl("C").unwrap();
        let parents: Vec<String> = c.extends().iter().map(|p| p.join(".")).collect();
        assert_eq!(parents, vec!["A".to_string(), "B".to_string()]);
    }

    fn open_interfaces_doc() -> Document {
        Document::open(
            r#"
            interface Drawable { tag: utf8  rank: i32 }
            type Animal { name: utf8 }
            type Dog extends Animal { breed: utf8 }
            @block("animal") type AnimalBlock extends Animal { @inline(0) id: utf8 }
            @block("dog") type DogBlock extends Dog {
                @inline(0) id: utf8
                tag:  utf8
                rank: i32
            }
            @block("widget") type WidgetBlock {
                @inline(0) id: utf8
                tag: utf8
                rank: i32
            }
            @block("partial") type PartialBlock {
                @inline(0) id: utf8
                tag: utf8
                // missing `rank`
            }
            @document
            type Cfg {
                @child("animal") animal: AnimalBlock?
                @child("dog") dog: DogBlock?
                @child("widget") widget: WidgetBlock?
                @child("partial") partial: PartialBlock?
                ref_animal: &Animal
                ref_dog_as_animal: &Animal
                ref_widget_as_drawable: &Drawable
                ref_dog_as_drawable: &Drawable
                ref_partial_as_drawable: &Drawable
                ref_widget_as_animal: &Animal
            }
            animal "alice" {}
            dog "spot" { tag = "alpha"  rank = 1 }
            widget "w1" { tag = "alpha"  rank = 1 }
            partial "p1" { tag = "alpha" }
            ref_animal              = animal
            ref_dog_as_animal       = dog
            ref_widget_as_drawable  = widget
            ref_dog_as_drawable     = dog
            ref_partial_as_drawable = partial
            ref_widget_as_animal    = widget
            "#,
            "t",
        )
        .expect("opens")
    }

    #[test]
    fn descendant_satisfies_ancestor_reference() {
        // `dog.spot` (DogBlock extends Dog extends Animal) read
        // through `&Animal` should resolve.
        let doc = open_interfaces_doc();
        let f = doc.field("ref_dog_as_animal").unwrap();
        f.reference()
            .expect("&T field exposes reference()")
            .expect("dog target accepted as Animal");
    }

    #[test]
    fn exact_match_resolves_through_reference() {
        let doc = open_interfaces_doc();
        let f = doc.field("ref_animal").unwrap();
        f.reference()
            .expect("&T field")
            .expect("animal matches Animal exactly");
    }

    #[test]
    fn conformant_target_resolves_through_interface_reference() {
        // WidgetBlock has tag + rank → satisfies Drawable.
        let doc = open_interfaces_doc();
        let f = doc.field("ref_widget_as_drawable").unwrap();
        f.reference()
            .expect("&Drawable field")
            .expect("WidgetBlock conforms to Drawable");
    }

    #[test]
    fn target_missing_field_errors_interface_not_implemented() {
        // PartialBlock has `tag` but no `rank` → fails Drawable.
        let doc = open_interfaces_doc();
        let f = doc.field("ref_partial_as_drawable").unwrap();
        let r = f.reference().expect("&Drawable");
        match r {
            Ok(_) => panic!("expected InterfaceNotImplemented"),
            Err(e) => assert!(
                matches!(
                    e,
                    EvalError::SchemaViolation {
                        kind: crate::error::SchemaViolationKind::InterfaceNotImplemented,
                        ..
                    }
                ),
                "{e:?}"
            ),
        }
    }

    #[test]
    fn sibling_target_errors_through_reference() {
        // WidgetBlock has no `extends Animal` chain → fails &Animal.
        let doc = open_interfaces_doc();
        let f = doc.field("ref_widget_as_animal").unwrap();
        let r = f.reference().expect("&Animal");
        match r {
            Ok(_) => panic!("expected InterfaceNotImplemented"),
            Err(e) => assert!(
                matches!(
                    e,
                    EvalError::SchemaViolation {
                        kind: crate::error::SchemaViolationKind::InterfaceNotImplemented,
                        ..
                    }
                ),
                "{e:?}"
            ),
        }
    }

    #[test]
    fn dog_inherited_fields_also_satisfy_drawable_via_self_fields() {
        // DogBlock declares its own tag/rank, so it implements
        // Drawable directly. (The Animal-extends chain doesn't
        // contribute Drawable shape.)
        let doc = open_interfaces_doc();
        let f = doc.field("ref_dog_as_drawable").unwrap();
        f.reference()
            .expect("&Drawable")
            .expect("DogBlock implements Drawable");
    }
}
