use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use miette::{NamedSource, SourceSpan};

use std::collections::{HashMap, HashSet};

use crate::ast::{self, Span};
use crate::environment::Environment;
use crate::error::{EvalError, ParseError};
use crate::parser::Parser;
use crate::symbols::{SymbolIndex, SymbolKind};
use crate::value::{BuiltinType, FnParam, FnValue, TensorDim, TypeRef, Value};

#[derive(Debug)]
pub(crate) struct FieldCell {
    value: OnceLock<Result<Value, EvalError>>,
    evaluating: AtomicBool,
}

impl FieldCell {
    fn new() -> Self {
        Self {
            value: OnceLock::new(),
            evaluating: AtomicBool::new(false),
        }
    }
}

/// Per-decorator cache for evaluated positional and named arguments.
#[derive(Debug, Default)]
pub(crate) struct DecoratorCell {
    positional: OnceLock<Result<Vec<Value>, EvalError>>,
    named: OnceLock<HashMap<String, Result<Value, EvalError>>>,
}

#[derive(Debug)]
pub(crate) struct ItemCells {
    pub(crate) decorators: Vec<DecoratorCell>,
    pub(crate) kind: ItemCellKind,
}

#[derive(Debug)]
pub(crate) enum ItemCellKind {
    Field(FieldCell),
    Block {
        labels: OnceLock<Result<Vec<Value>, EvalError>>,
        items: Vec<ItemCells>,
        /// Lazy schema-content validation cache. Populated on first call
        /// to `Block::schema_errors()`.
        schema_validation: OnceLock<Vec<EvalError>>,
        /// Synthesised `Block`s from `Item::Table` rows, built once at
        /// cells-build time. Each entry remembers the parent field
        /// name the row's table-header bound to; the `kind` is left
        /// blank in the stored AST and overridden at view time using
        /// the parent type's `@children(kind)` declaration.
        synth_rows: Vec<SynthRow>,
    },
    TypeDecl {
        /// One inner Vec per `ast::TypeDecl.fields[i]`, holding cells for
        /// that field's decorators.
        field_decorators: Vec<Vec<DecoratorCell>>,
    },
    UnionDecl {
        variant_decorators: Vec<Vec<DecoratorCell>>,
        /// `[variant_idx][field_idx]` decorator cells for record-variant
        /// fields. Empty inner vecs for non-record variants.
        variant_field_decorators: Vec<Vec<Vec<DecoratorCell>>>,
    },
    SymbolSetDecl {
        symbol_decorators: Vec<Vec<DecoratorCell>>,
    },
    NamespaceDecl,
    UseDecl,
    /// Stub variant for `Item::Table` AST entries. The actual cells
    /// for the rows (synthesised `Block`s) live in the enclosing
    /// block's `table_rows` projection cache, keyed by field name.
    Table,
    /// Lazy import. Populated on first read-access of the enclosing
    /// block. Top-level imports also get this cell but are never
    /// triggered through it — they're expanded eagerly into
    /// `Document::eager_imports` at `open_with` time.
    Import {
        /// As written in the source.
        path: String,
        /// Span of the path string literal — used for error labels.
        path_span: Span,
        /// Resolved file directory for path joins. `None` means the
        /// document had no base directory (e.g. `Document::open`),
        /// which surfaces as an `ImportFailed` on first access.
        base_dir: Option<PathBuf>,
        loaded: OnceLock<Result<LoadedImport, EvalError>>,
    },
}

#[derive(Debug)]
pub(crate) struct BlockCells {
    pub(crate) items: Vec<ItemCells>,
}

/// One synthesised row-Block, owned by a parent `Block` cell. Built
/// at cells-build time from an `Item::Table` row. The `block.kind`
/// field is intentionally blank — the kind comes from the parent
/// type's `@children` decoration at view time.
#[derive(Debug)]
pub(crate) struct SynthRow {
    pub(crate) field_name: String,
    pub(crate) block: ast::Block,
    pub(crate) cells: ItemCells,
}

fn make_decorator_cells(decs: &[ast::Decorator]) -> Vec<DecoratorCell> {
    (0..decs.len()).map(|_| DecoratorCell::default()).collect()
}

impl BlockCells {
    fn build(items: &[ast::Item], base_dir: Option<&Path>) -> Self {
        let cells = items
            .iter()
            .map(|item| ItemCells::build(item, base_dir))
            .collect();
        Self { items: cells }
    }
}

impl ItemCells {
    fn build(item: &ast::Item, base_dir: Option<&Path>) -> Self {
        match item {
            ast::Item::Field(f) => Self {
                decorators: make_decorator_cells(&f.decorators),
                kind: ItemCellKind::Field(FieldCell::new()),
            },
            ast::Item::Block(b) => {
                // Eagerly synthesise per-row Blocks from Item::Table
                // entries nested in this block. The `kind` is filled
                // in at view time; the labels carry the row values
                // verbatim.
                let mut synth_rows: Vec<SynthRow> = Vec::new();
                for item in &b.items {
                    if let ast::Item::Table(t) = item {
                        for r in &t.rows {
                            let synth_block = ast::Block {
                                kind: String::new(),
                                labels: r.values.clone(),
                                items: Vec::new(),
                                decorators: Vec::new(),
                                span: r.span,
                            };
                            let synth_cells =
                                ItemCells::build(&ast::Item::Block(synth_block.clone()), None);
                            synth_rows.push(SynthRow {
                                field_name: t.field_name.clone(),
                                block: synth_block,
                                cells: synth_cells,
                            });
                        }
                    }
                }
                Self {
                    decorators: make_decorator_cells(&b.decorators),
                    kind: ItemCellKind::Block {
                        labels: OnceLock::new(),
                        items: b
                            .items
                            .iter()
                            .map(|item| ItemCells::build(item, base_dir))
                            .collect(),
                        schema_validation: OnceLock::new(),
                        synth_rows,
                    },
                }
            }
            ast::Item::TypeDecl(t) => Self {
                decorators: make_decorator_cells(&t.decorators),
                kind: ItemCellKind::TypeDecl {
                    field_decorators: t
                        .fields
                        .iter()
                        .map(|f| make_decorator_cells(&f.decorators))
                        .collect(),
                },
            },
            ast::Item::UnionDecl(u) => Self {
                decorators: make_decorator_cells(&u.decorators),
                kind: ItemCellKind::UnionDecl {
                    variant_decorators: u
                        .variants
                        .iter()
                        .map(|v| make_decorator_cells(&v.decorators))
                        .collect(),
                    variant_field_decorators: u
                        .variants
                        .iter()
                        .map(|v| match &v.body {
                            ast::VariantBody::Record(fields) => fields
                                .iter()
                                .map(|f| make_decorator_cells(&f.decorators))
                                .collect(),
                            _ => Vec::new(),
                        })
                        .collect(),
                },
            },
            ast::Item::SymbolSetDecl(s) => Self {
                decorators: make_decorator_cells(&s.decorators),
                kind: ItemCellKind::SymbolSetDecl {
                    symbol_decorators: s
                        .symbols
                        .iter()
                        .map(|sym| make_decorator_cells(&sym.decorators))
                        .collect(),
                },
            },
            ast::Item::NamespaceDecl(_) => Self {
                decorators: Vec::new(),
                kind: ItemCellKind::NamespaceDecl,
            },
            ast::Item::UseDecl(_) => Self {
                decorators: Vec::new(),
                kind: ItemCellKind::UseDecl,
            },
            ast::Item::Import(imp) => Self {
                decorators: Vec::new(),
                kind: ItemCellKind::Import {
                    path: imp.path.clone(),
                    path_span: imp.path_span,
                    base_dir: base_dir.map(Path::to_path_buf),
                    loaded: OnceLock::new(),
                },
            },
            ast::Item::Table(_) => Self {
                decorators: Vec::new(),
                kind: ItemCellKind::Table,
            },
        }
    }
}

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

/// Result of loading one imported file. Transitive top-level imports
/// inside the loaded file are flattened into `eager_imports`; nested
/// (in-block) imports stay lazy inside `items`/`cells`.
#[derive(Debug)]
pub(crate) struct LoadedImport {
    #[allow(dead_code)] // kept for introspection / future debugging
    pub(crate) path: PathBuf,
    #[allow(dead_code)] // FQNs in `symbols` already encode this
    pub(crate) file_ns: Vec<String>,
    pub(crate) items: Vec<ast::Item>,
    pub(crate) cells: Vec<ItemCells>,
    /// Symbols indexed within this loaded file. Paths refer to the
    /// `items`/`cells` arrays in this same struct.
    pub(crate) symbols: SymbolIndex,
    pub(crate) eager_imports: Vec<LoadedImport>,
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
        })
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

    fn resolve_root(&self, name: &str) -> Option<crate::data::DataRef<'_>> {
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
            .flat_map(move |src| iter_fields(src.items, src.cells, doc))
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block<'_>> + '_ {
        let doc = self;
        self.all_sources()
            .into_iter()
            .flat_map(move |src| iter_blocks(src.items, src.cells, doc))
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
    let v = dec.named_arg(arg_name)?.ok()?;
    match v {
        Value::I8(n) if n >= 0 => Some(n as u64),
        Value::I16(n) if n >= 0 => Some(n as u64),
        Value::I32(n) if n >= 0 => Some(n as u64),
        Value::I64(n) if n >= 0 => Some(n as u64),
        Value::I128(n) if n >= 0 => Some(n as u64),
        Value::Isize(n) if n >= 0 => Some(n as u64),
        Value::U8(n) => Some(n as u64),
        Value::U16(n) => Some(n as u64),
        Value::U32(n) => Some(n as u64),
        Value::U64(n) => Some(n),
        Value::U128(n) => Some(n as u64),
        Value::Usize(n) => Some(n as u64),
        _ => None,
    }
}

fn decl_fqn_matches(decl: &[String], target: &[&str]) -> bool {
    decl.len() == target.len() && decl.iter().zip(target.iter()).all(|(a, b)| a == b)
}

fn resolve_path(
    path: &[String],
    file_ns: &[String],
    item_aliases: &HashMap<String, Vec<String>>,
    ns_aliases: &HashMap<String, Vec<String>>,
    wildcards: &[Vec<String>],
    registry: &HashSet<Vec<String>>,
) -> Option<Vec<String>> {
    // 1. file_ns + path
    let candidate: Vec<String> = file_ns.iter().chain(path.iter()).cloned().collect();
    if registry.contains(&candidate) {
        return Some(candidate);
    }
    // 2. item alias on single-segment path
    if path.len() == 1
        && let Some(fqn) = item_aliases.get(&path[0])
        && registry.contains(fqn)
    {
        return Some(fqn.clone());
    }
    // 3. namespace alias on first segment of multi-segment path
    if path.len() > 1
        && let Some(prefix) = ns_aliases.get(&path[0])
    {
        let candidate: Vec<String> = prefix.iter().chain(path[1..].iter()).cloned().collect();
        if registry.contains(&candidate) {
            return Some(candidate);
        }
    }
    // 4. each wildcard prefix
    for w in wildcards {
        let candidate: Vec<String> = w.iter().chain(path.iter()).cloned().collect();
        if registry.contains(&candidate) {
            return Some(candidate);
        }
    }
    // 5. absolute
    if registry.contains(path) {
        return Some(path.to_vec());
    }
    None
}

#[derive(Debug)]
pub enum ResolvedType<'a> {
    Builtin(BuiltinType),
    Named(TypeDecl<'a>),
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

#[derive(Debug, Clone, Copy)]
pub struct UnionDecl<'a> {
    ast: &'a ast::UnionDecl,
    file_ns: &'a [String],
    cells: &'a ItemCells,
    doc: &'a Document,
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

    /// Last segment of the declared name.
    pub fn name(&self) -> &'a str {
        self.ast.name.last().expect("name has at least one segment")
    }

    /// Path as written in source (relative to file namespace).
    pub fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }

    /// Fully-qualified name as a dotted string.
    pub fn full_name(&self) -> String {
        self.file_ns
            .iter()
            .chain(self.ast.name.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(".")
    }

    /// Namespace path containing this declaration (file ns + decl path minus last).
    pub fn namespace(&self) -> Vec<String> {
        let mut v: Vec<String> = self.file_ns.to_vec();
        if self.ast.name.len() > 1 {
            v.extend(self.ast.name[..self.ast.name.len() - 1].iter().cloned());
        }
        v
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

pub enum VariantBodyView<'a> {
    Record,
    TypeRef(&'a TypeRef),
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
    ast: &'a ast::SymbolSetDecl,
    file_ns: &'a [String],
    cells: &'a ItemCells,
    doc: &'a Document,
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

    pub fn name(&self) -> &'a str {
        self.ast.name.last().expect("name has at least one segment")
    }

    pub fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }

    pub fn full_name(&self) -> String {
        self.file_ns
            .iter()
            .chain(self.ast.name.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(".")
    }

    pub fn namespace(&self) -> Vec<String> {
        let mut v: Vec<String> = self.file_ns.to_vec();
        if self.ast.name.len() > 1 {
            v.extend(self.ast.name[..self.ast.name.len() - 1].iter().cloned());
        }
        v
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
    ast: &'a ast::TypeDecl,
    file_ns: &'a [String],
    cells: &'a ItemCells,
    doc: &'a Document,
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

    pub fn name(&self) -> &'a str {
        self.ast.name.last().expect("name has at least one segment")
    }

    pub fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }

    pub fn full_name(&self) -> String {
        self.file_ns
            .iter()
            .chain(self.ast.name.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(".")
    }

    pub fn namespace(&self) -> Vec<String> {
        let mut v: Vec<String> = self.file_ns.to_vec();
        if self.ast.name.len() > 1 {
            v.extend(self.ast.name[..self.ast.name.len() - 1].iter().cloned());
        }
        v
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
    ast: &'a ast::TypeField,
    decorator_cells: &'a [DecoratorCell],
    doc: &'a Document,
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
        let positional = dec.positional().ok()?;
        match positional.first()? {
            Value::I8(n) if *n >= 0 => Some(*n as u64),
            Value::I16(n) if *n >= 0 => Some(*n as u64),
            Value::I32(n) if *n >= 0 => Some(*n as u64),
            Value::I64(n) if *n >= 0 => Some(*n as u64),
            Value::I128(n) if *n >= 0 => Some(*n as u64),
            Value::Isize(n) if *n >= 0 => Some(*n as u64),
            Value::U8(n) => Some(*n as u64),
            Value::U16(n) => Some(*n as u64),
            Value::U32(n) => Some(*n as u64),
            Value::U64(n) => Some(*n),
            Value::U128(n) => Some(*n as u64),
            Value::Usize(n) => Some(*n as u64),
            _ => None,
        }
    }

    /// If this field carries an `@default(v)` decorator, returns v.
    pub fn default_value(&self) -> Option<Value> {
        let dec = self.decorators().find(|d| d.full_name() == "default")?;
        dec.positional().ok()?.into_iter().next()
    }

    /// If this field carries an `@child("kind")` decorator, returns the
    /// nested block kind it binds.
    pub fn child_block_kind(&self) -> Option<String> {
        let dec = self.decorators().find(|d| d.full_name() == "child")?;
        match dec.positional().ok()?.into_iter().next()? {
            Value::Utf8(s) | Value::Ascii(s) => Some(s),
            _ => None,
        }
    }

    /// If this field carries an `@children("kind", min?, max?)` decorator,
    /// returns the nested block kind it binds.
    pub fn children_block_kind(&self) -> Option<String> {
        let dec = self.decorators().find(|d| d.full_name() == "children")?;
        match dec.positional().ok()?.into_iter().next()? {
            Value::Utf8(s) | Value::Ascii(s) => Some(s),
            _ => None,
        }
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

#[derive(Clone, Copy)]
pub struct Field<'a> {
    ast: &'a ast::Field,
    cells: &'a ItemCells,
    doc: &'a Document,
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
        let result = self.doc.eval(&self.ast.expr);
        cell.evaluating.store(false, Ordering::Release);
        cell.value.get_or_init(|| result).as_ref()
    }
}

#[derive(Clone, Copy)]
pub struct Block<'a> {
    ast: &'a ast::Block,
    cells: &'a ItemCells,
    doc: &'a Document,
    /// When `Some`, overrides `ast.kind` for views derived from a
    /// synthesised row-Block (its stored `kind` is blank). Real
    /// blocks always have `None`.
    kind_override: Option<&'a str>,
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

    /// Evaluated values for each label slot. Cached on first call; later
    /// calls return a clone of the cached `Vec`.
    pub fn labels(&self) -> Result<Vec<Value>, EvalError> {
        let (cell, _) = self.block_inner();
        let result =
            cell.get_or_init(|| self.ast.labels.iter().map(|e| self.doc.eval(e)).collect());
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
        for src in self.realize_and_sources() {
            if let Some(f) = find_field(src.items, src.cells, name, self.doc) {
                return Some(f);
            }
        }
        None
    }

    pub fn block(&self, kind: &str) -> Option<Block<'a>> {
        for src in self.realize_and_sources() {
            if let Some(b) = find_block(src.items, src.cells, kind, self.doc) {
                return Some(b);
            }
        }
        None
    }

    pub fn fields(&self) -> impl Iterator<Item = Field<'a>> + 'a {
        let doc = self.doc;
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_fields(src.items, src.cells, doc))
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block<'a>> + 'a {
        let doc = self.doc;
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_blocks(src.items, src.cells, doc))
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
        let mut synth_iter = synth_rows.iter().filter(|r| r.field_name == field_name);
        for (item, cells) in self.ast.items.iter().zip(items_cells.iter()) {
            match item {
                ast::Item::Block(b) if b.kind == kind => {
                    out.push(Block {
                        ast: b,
                        cells,
                        doc: self.doc,
                        kind_override: None,
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
        let this = *self;
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

/// Compute schema-content validation errors for a block. Called once
/// per block via the `schema_validation` OnceLock; subsequent calls
/// return the cached vector. No-op for blocks without a schema or for
/// schemas that don't declare any nested-block rules.
fn compute_schema_errors<'a>(block: &Block<'a>) -> Vec<EvalError> {
    use crate::error::SchemaViolationKind as Kind;
    let mut errs = Vec::new();
    let Some(schema) = block.schema() else {
        return errs;
    };

    // 0. Table row-form validation: if this block's schema is a
    // `@table`, its labels are the row's column values and must
    // match the schema field count.
    if block.doc.table_schema(block.kind()).is_some() {
        let label_count = block.labels().map(|v| v.len()).unwrap_or(0);
        let field_count = schema.fields().count();
        if label_count < field_count {
            errs.push(EvalError::schema_violation(
                Kind::ChildrenTooFew,
                format!(
                    "table row for '{}' has {} values, expected {}",
                    block.kind(),
                    label_count,
                    field_count
                ),
                block.span(),
            ));
        } else if label_count > field_count {
            errs.push(EvalError::schema_violation(
                Kind::ChildrenTooMany,
                format!(
                    "table row for '{}' has {} values, expected {}",
                    block.kind(),
                    label_count,
                    field_count
                ),
                block.span(),
            ));
        }
        // Tables don't carry nested-block children themselves, so the
        // rest of the validation doesn't apply.
        return errs;
    }

    // 1. Gather per-kind counts of nested blocks. Both literal
    // `Item::Block` entries and synthesised `Item::Table` rows
    // contribute. For synth rows the kind comes from the parent
    // schema's `@children(K)` decoration on the matching field.
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut total: usize = 0;
    for nested in block.blocks() {
        *counts.entry(nested.kind().to_string()).or_insert(0) += 1;
        total += 1;
    }
    if let ItemCellKind::Block { synth_rows, .. } = &block.cells.kind {
        for sr in synth_rows {
            // Find the schema field matching the table header's
            // field_name and read its @children(kind).
            if let Some(field) = schema.field(&sr.field_name)
                && let Some(kind) = field.children_block_kind_str()
            {
                *counts.entry(kind.to_string()).or_insert(0) += 1;
                total += 1;
            }
        }
    }

    // 2. Build the allowed-child set: union of @child/@children kinds
    // across this type's fields.
    let allowed = schema.allowed_child_kinds();

    // 3. Per-kind: any nested block whose kind isn't in `allowed`
    // is a DisallowedChild.
    for nested in block.blocks() {
        if !allowed.iter().any(|k| k == nested.kind()) {
            errs.push(EvalError::schema_violation(
                Kind::DisallowedChild,
                format!(
                    "block kind '{}' is not allowed inside '{}'",
                    nested.kind(),
                    block.kind()
                ),
                nested.span(),
            ));
        }
    }

    // 4. `max_children = N` on @block: total nested-block count ≤ N.
    if let Some(maxn) = schema.max_children()
        && (total as u64) > maxn
    {
        errs.push(EvalError::schema_violation(
            Kind::BlockChildrenOverflow,
            format!(
                "block '{}' contains {} children (max allowed: {})",
                block.kind(),
                total,
                maxn
            ),
            block.span(),
        ));
    }

    // 4b. `required_children = ["kind", ...]` on @block: each listed
    //     kind must appear at least once.
    for required in schema.required_children() {
        if *counts.get(&required).unwrap_or(&0) == 0 {
            errs.push(EvalError::schema_violation(
                Kind::MissingRequired,
                format!(
                    "block '{}' is missing required child kind '{}'",
                    block.kind(),
                    required
                ),
                block.span(),
            ));
        }
    }

    // 5. Field-level cardinality (@child / @children).
    for f in schema.fields() {
        if let Some(kind) = f.child_block_kind() {
            // @child(K): expect exactly 1 (or 0..1 if field is optional).
            let count = *counts.get(&kind).unwrap_or(&0);
            if count == 0 && !f.optional() {
                errs.push(EvalError::schema_violation(
                    Kind::MissingRequired,
                    format!(
                        "block '{}' is missing required child '{}' (for field '{}')",
                        block.kind(),
                        kind,
                        f.name()
                    ),
                    block.span(),
                ));
            } else if count > 1 {
                errs.push(EvalError::schema_violation(
                    Kind::ChildrenTooMany,
                    format!(
                        "field '{}' expects a single '{}' child, found {}",
                        f.name(),
                        kind,
                        count
                    ),
                    block.span(),
                ));
            }
        } else if let Some(kind) = f.children_block_kind() {
            let count = *counts.get(&kind).unwrap_or(&0) as u64;
            if let Some(min) = f.children_min()
                && count < min
            {
                errs.push(EvalError::schema_violation(
                    Kind::ChildrenTooFew,
                    format!(
                        "field '{}' requires at least {} '{}' children, found {}",
                        f.name(),
                        min,
                        kind,
                        count
                    ),
                    block.span(),
                ));
            }
            if let Some(maxn) = f.children_max()
                && count > maxn
            {
                errs.push(EvalError::schema_violation(
                    Kind::ChildrenTooMany,
                    format!(
                        "field '{}' allows at most {} '{}' children, found {}",
                        f.name(),
                        maxn,
                        kind,
                        count
                    ),
                    block.span(),
                ));
            }
        }
    }

    errs
}

/// One source of (items, cells) within a block: either the block's own
/// items or one of its realised imports.
#[derive(Clone, Copy)]
struct BlockSlice<'a> {
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
}

fn push_loaded_imports<'a>(cells: &'a [ItemCells], out: &mut Vec<BlockSlice<'a>>) {
    for cell in cells {
        if let ItemCellKind::Import { loaded, .. } = &cell.kind
            && let Some(Ok(li)) = loaded.get()
        {
            out.push(BlockSlice {
                items: &li.items,
                cells: &li.cells,
            });
            push_eager_imports(&li.eager_imports, out);
        }
    }
}

fn push_eager_imports<'a>(imps: &'a [LoadedImport], out: &mut Vec<BlockSlice<'a>>) {
    for imp in imps {
        out.push(BlockSlice {
            items: &imp.items,
            cells: &imp.cells,
        });
        push_eager_imports(&imp.eager_imports, out);
    }
}

fn load_import_lazily(
    path_str: &str,
    base_dir: Option<&Path>,
    path_span: Span,
) -> Result<LoadedImport, EvalError> {
    let path = resolve_import_path(base_dir, path_str)
        .map_err(|e| EvalError::import_failed(path_str, e, path_span))?;
    let src = std::fs::read_to_string(&path)
        .map_err(|e| EvalError::import_failed(path_str, format!("io: {e}"), path_span))?;
    let display = path.display().to_string();
    let (parsed_ast, parsed_symbols) = Parser::new(&src, &display)
        .parse_source()
        .map_err(|e| EvalError::import_failed(path_str, format!("{e}"), path_span))?;
    let imported_base = path.parent().map(Path::to_path_buf);
    let file_ns = first_namespace(&parsed_ast.items);

    let mut loading: HashSet<PathBuf> = HashSet::new();
    loading.insert(path.clone());
    let mut child_eager: Vec<LoadedImport> = Vec::new();
    expand_top_level_imports(
        &parsed_ast.items,
        imported_base.as_deref(),
        &mut loading,
        &mut child_eager,
        &display,
        &src,
    )
    .map_err(|e| EvalError::import_failed(path_str, format!("{e}"), path_span))?;

    let cells = parsed_ast
        .items
        .iter()
        .map(|i| ItemCells::build(i, imported_base.as_deref()))
        .collect();
    Ok(LoadedImport {
        path,
        file_ns,
        items: parsed_ast.items,
        cells,
        symbols: parsed_symbols,
        eager_imports: child_eager,
    })
}

fn find_field<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    name: &str,
    doc: &'a Document,
) -> Option<Field<'a>> {
    items
        .iter()
        .zip(cells)
        .find_map(|(item, cells)| match (item, &cells.kind) {
            (ast::Item::Field(f), ItemCellKind::Field(_)) if f.name == name => {
                Some(Field { ast: f, cells, doc })
            }
            _ => None,
        })
}

fn find_block<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    kind: &str,
    doc: &'a Document,
) -> Option<Block<'a>> {
    items
        .iter()
        .zip(cells)
        .find_map(|(item, cells)| match (item, &cells.kind) {
            (ast::Item::Block(b), ItemCellKind::Block { .. }) if b.kind == kind => Some(Block {
                ast: b,
                cells,
                doc,
                kind_override: None,
            }),
            _ => None,
        })
}

fn iter_fields<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    doc: &'a Document,
) -> impl Iterator<Item = Field<'a>> + 'a {
    items
        .iter()
        .zip(cells)
        .filter_map(move |(item, cells)| match (item, &cells.kind) {
            (ast::Item::Field(f), ItemCellKind::Field(_)) => Some(Field { ast: f, cells, doc }),
            _ => None,
        })
}

fn iter_blocks<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    doc: &'a Document,
) -> impl Iterator<Item = Block<'a>> + 'a {
    items
        .iter()
        .zip(cells)
        .filter_map(move |(item, cells)| match (item, &cells.kind) {
            (ast::Item::Block(b), ItemCellKind::Block { .. }) => Some(Block {
                ast: b,
                cells,
                doc,
                kind_override: None,
            }),
            _ => None,
        })
}

fn iter_tables<'a>(
    items: &'a [ast::Item],
    doc: &'a Document,
) -> impl Iterator<Item = TableView<'a>> + 'a {
    items.iter().filter_map(move |item| match item {
        ast::Item::Table(t) => Some(TableView { ast: t, doc }),
        _ => None,
    })
}

/// Source-level view of an `Item::Table` (a `FIELD:` header followed
/// by one or more `| ... |` rows) within a parent block.
#[derive(Clone, Copy)]
pub struct TableView<'a> {
    ast: &'a ast::TableItem,
    doc: &'a Document,
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
    pub fn values(&self) -> Result<Vec<Value>, EvalError> {
        self.ast.values.iter().map(|e| self.doc.eval(e)).collect()
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }
}

struct EvalCtx {
    /// Stack of name → value bindings introduced by `Block` let-bindings.
    /// Searched right-to-left so the most recent binding shadows older ones.
    locals: Vec<(String, Value)>,
}

impl EvalCtx {
    fn new() -> Self {
        Self { locals: Vec::new() }
    }

    fn lookup(&self, name: &str) -> Option<&Value> {
        self.locals
            .iter()
            .rev()
            .find_map(|(n, v)| if n == name { Some(v) } else { None })
    }
}

impl Document {
    pub(crate) fn eval(&self, expr: &ast::Expr) -> Result<Value, EvalError> {
        let mut ctx = EvalCtx::new();
        self.eval_in(expr, &mut ctx)
    }

    fn eval_in(&self, expr: &ast::Expr, ctx: &mut EvalCtx) -> Result<Value, EvalError> {
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
                Value::Function(FnValue::new(params, f.return_ty.clone(), f.body.clone()))
            }
            E::Identifier(name) => {
                // Locals (let-binding scope) shadow top-level fields.
                if let Some(v) = ctx.lookup(name) {
                    return Ok(v.clone());
                }
                let fqn = self.qualified_name(name);
                if let Some(rec) = self.symbols.lookup(&fqn)
                    && matches!(rec.kind, SymbolKind::Field)
                    && let Some(field) = self.field(name)
                {
                    return field.value().cloned().map_err(|e| e.clone());
                }
                // Unresolved identifiers pass through as `Value::Identifier`.
                // Builtin arguments that need a real value will surface a
                // type-mismatch from the `FromValue` impl; block labels (which
                // legitimately use bare identifiers) keep working.
                Value::Identifier(name.clone())
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
                let name = match callee.as_ref() {
                    E::Identifier(n) => n.clone(),
                    _ => return Err(EvalError::non_callable(*span)),
                };
                let Some(builtin) = self.env.builtin(&name) else {
                    return Err(EvalError::unknown_builtin(name, *span));
                };
                if args.len() != builtin.arity {
                    return Err(EvalError::builtin_arity(
                        name,
                        builtin.arity,
                        args.len(),
                        *span,
                    ));
                }
                let mut evald = Vec::with_capacity(args.len());
                for arg in args {
                    evald.push(self.eval_in(arg, ctx)?);
                }
                return (builtin.body)(&evald)
                    .map_err(|msg| EvalError::builtin_type(name, msg, *span));
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
        })
    }

    /// Compose `name` with the document's file namespace into a dotted
    /// FQN suitable for [`SymbolIndex::lookup`].
    fn qualified_name(&self, name: &str) -> String {
        if self.file_ns.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", self.file_ns.join("."), name)
        }
    }
}

fn as_bool(v: &Value, op: ast::BinOp, span: Span) -> Result<bool, EvalError> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(EvalError::type_mismatch(
            op_name(op),
            other.type_name(),
            "—",
            span,
        )),
    }
}

fn op_name(op: ast::BinOp) -> &'static str {
    match op {
        ast::BinOp::Add => "+",
        ast::BinOp::Sub => "-",
        ast::BinOp::Mul => "*",
        ast::BinOp::Div => "/",
        ast::BinOp::Mod => "%",
        ast::BinOp::Eq => "==",
        ast::BinOp::Ne => "!=",
        ast::BinOp::Lt => "<",
        ast::BinOp::Le => "<=",
        ast::BinOp::Gt => ">",
        ast::BinOp::Ge => ">=",
        ast::BinOp::And => "&&",
        ast::BinOp::Or => "||",
    }
}

fn apply_unary(op: ast::UnaryOp, v: Value, span: Span) -> Result<Value, EvalError> {
    match op {
        ast::UnaryOp::Neg => match v {
            Value::I8(n) => Ok(Value::I8(-n)),
            Value::I16(n) => Ok(Value::I16(-n)),
            Value::I32(n) => Ok(Value::I32(-n)),
            Value::I64(n) => Ok(Value::I64(-n)),
            Value::I128(n) => Ok(Value::I128(-n)),
            Value::Isize(n) => Ok(Value::Isize(-n)),
            Value::F32(n) => Ok(Value::F32(-n)),
            Value::F64(n) => Ok(Value::F64(-n)),
            other => Err(EvalError::type_mismatch("-", other.type_name(), "—", span)),
        },
        ast::UnaryOp::Not => match v {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            other => Err(EvalError::type_mismatch("!", other.type_name(), "—", span)),
        },
    }
}

fn apply_binary(op: ast::BinOp, l: Value, r: Value, span: Span) -> Result<Value, EvalError> {
    use ast::BinOp as B;
    let mismatch = || EvalError::type_mismatch(op_name(op), l.type_name(), r.type_name(), span);

    macro_rules! numeric_arith {
        ($lhs:expr, $rhs:expr, $variant:ident, $op:tt) => {
            match (&$lhs, &$rhs) {
                (Value::$variant(a), Value::$variant(b)) => Ok(Value::$variant(a $op b)),
                _ => Err(mismatch()),
            }
        }
    }

    macro_rules! arith_op {
        ($op:tt) => {
            match (&l, &r) {
                (Value::I8(_), Value::I8(_)) => numeric_arith!(l, r, I8, $op),
                (Value::I16(_), Value::I16(_)) => numeric_arith!(l, r, I16, $op),
                (Value::I32(_), Value::I32(_)) => numeric_arith!(l, r, I32, $op),
                (Value::I64(_), Value::I64(_)) => numeric_arith!(l, r, I64, $op),
                (Value::I128(_), Value::I128(_)) => numeric_arith!(l, r, I128, $op),
                (Value::Isize(_), Value::Isize(_)) => numeric_arith!(l, r, Isize, $op),
                (Value::U8(_), Value::U8(_)) => numeric_arith!(l, r, U8, $op),
                (Value::U16(_), Value::U16(_)) => numeric_arith!(l, r, U16, $op),
                (Value::U32(_), Value::U32(_)) => numeric_arith!(l, r, U32, $op),
                (Value::U64(_), Value::U64(_)) => numeric_arith!(l, r, U64, $op),
                (Value::U128(_), Value::U128(_)) => numeric_arith!(l, r, U128, $op),
                (Value::Usize(_), Value::Usize(_)) => numeric_arith!(l, r, Usize, $op),
                (Value::F32(_), Value::F32(_)) => numeric_arith!(l, r, F32, $op),
                (Value::F64(_), Value::F64(_)) => numeric_arith!(l, r, F64, $op),
                _ => Err(mismatch()),
            }
        };
    }

    match op {
        B::Add => arith_op!(+),
        B::Sub => arith_op!(-),
        B::Mul => arith_op!(*),
        B::Div => arith_op!(/),
        B::Mod => arith_op!(%),
        B::Eq => Ok(Value::Bool(values_eq(&l, &r))),
        B::Ne => Ok(Value::Bool(!values_eq(&l, &r))),
        B::Lt => compare(&l, &r, span, |c| c == std::cmp::Ordering::Less),
        B::Le => compare(&l, &r, span, |c| c != std::cmp::Ordering::Greater),
        B::Gt => compare(&l, &r, span, |c| c == std::cmp::Ordering::Greater),
        B::Ge => compare(&l, &r, span, |c| c != std::cmp::Ordering::Less),
        B::And | B::Or => unreachable!("handled with short-circuit eval"),
    }
}

fn values_eq(l: &Value, r: &Value) -> bool {
    // Same-typed equality. Cross-type comparisons (e.g. i32 vs i64) are
    // not equal — there is no implicit numeric coercion.
    l == r
}

fn compare<F>(l: &Value, r: &Value, span: Span, pick: F) -> Result<Value, EvalError>
where
    F: Fn(std::cmp::Ordering) -> bool,
{
    use std::cmp::Ordering;
    let ord = match (l, r) {
        (Value::I8(a), Value::I8(b)) => a.cmp(b),
        (Value::I16(a), Value::I16(b)) => a.cmp(b),
        (Value::I32(a), Value::I32(b)) => a.cmp(b),
        (Value::I64(a), Value::I64(b)) => a.cmp(b),
        (Value::I128(a), Value::I128(b)) => a.cmp(b),
        (Value::Isize(a), Value::Isize(b)) => a.cmp(b),
        (Value::U8(a), Value::U8(b)) => a.cmp(b),
        (Value::U16(a), Value::U16(b)) => a.cmp(b),
        (Value::U32(a), Value::U32(b)) => a.cmp(b),
        (Value::U64(a), Value::U64(b)) => a.cmp(b),
        (Value::U128(a), Value::U128(b)) => a.cmp(b),
        (Value::Usize(a), Value::Usize(b)) => a.cmp(b),
        (Value::F32(a), Value::F32(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (Value::F64(a), Value::F64(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (Value::Utf8(a), Value::Utf8(b)) | (Value::Ascii(a), Value::Ascii(b)) => a.cmp(b),
        _ => {
            return Err(EvalError::type_mismatch(
                "<>",
                l.type_name(),
                r.type_name(),
                span,
            ));
        }
    };
    Ok(Value::Bool(pick(ord)))
}

fn span_to_miette(span: Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.len().max(1))
}

struct Resolved {
    file_ns: Vec<String>,
    item_aliases: HashMap<String, Vec<String>>,
    ns_aliases: HashMap<String, Vec<String>>,
    wildcards: Vec<Vec<String>>,
}

fn validate_document(
    ast: &ast::Source,
    symbols: &SymbolIndex,
    synthetic: &[ast::TypeDecl],
    source: &str,
    file: &str,
) -> Result<Resolved, ParseError> {
    // 1. Namespace must be first if present; at most one.
    let mut file_ns: Vec<String> = Vec::new();
    let mut saw_ns = false;
    for (idx, item) in ast.items.iter().enumerate() {
        if let ast::Item::NamespaceDecl(n) = item {
            if saw_ns {
                return Err(open_error(
                    source,
                    file,
                    "duplicate namespace declaration".to_string(),
                    n.span,
                    "duplicate namespace",
                ));
            }
            if idx != 0 {
                return Err(open_error(
                    source,
                    file,
                    "namespace declaration must be the first item in the file".to_string(),
                    n.span,
                    "must be first item",
                ));
            }
            file_ns = n.path.clone();
            saw_ns = true;
        }
    }

    // 2. Build the declared-FQN set and prefix set used for name resolution.
    // Top-level decls were already added to `symbols` by the parser (and the
    // duplicate check already fired there); we just project them into the
    // shapes that the rest of this function expects.
    let mut declared: HashSet<Vec<String>> = HashSet::new();
    let mut prefixes: HashSet<Vec<String>> = HashSet::new();
    for t in synthetic {
        let fqn = t.name.clone();
        declared.insert(fqn.clone());
        for n in 1..fqn.len() {
            prefixes.insert(fqn[..n].to_vec());
        }
    }
    for rec in symbols.iter() {
        if !matches!(
            rec.kind,
            SymbolKind::TypeDecl | SymbolKind::UnionDecl | SymbolKind::SymbolSetDecl
        ) {
            continue;
        }
        let fqn: Vec<String> = rec.fqn.split('.').map(str::to_string).collect();
        if !declared.insert(fqn.clone()) {
            // A registry-injected (synthetic) type already owns this FQN.
            return Err(open_error(
                source,
                file,
                format!("duplicate declaration '{}'", rec.fqn),
                rec.span,
                "duplicate declaration",
            ));
        }
        for n in 1..fqn.len() {
            prefixes.insert(fqn[..n].to_vec());
        }
    }

    // 3. Use declarations.
    let mut item_aliases: HashMap<String, Vec<String>> = HashMap::new();
    let mut ns_aliases: HashMap<String, Vec<String>> = HashMap::new();
    let mut wildcards: Vec<Vec<String>> = Vec::new();
    let mut alias_taken: HashSet<String> = HashSet::new();

    let record_alias =
        |alias: String, span: Span, taken: &mut HashSet<String>| -> Result<(), ParseError> {
            if !taken.insert(alias.clone()) {
                return Err(open_error(
                    source,
                    file,
                    format!("duplicate use alias '{alias}'"),
                    span,
                    "duplicate alias",
                ));
            }
            Ok(())
        };

    for item in &ast.items {
        let ast::Item::UseDecl(u) = item else {
            continue;
        };
        match &u.form {
            ast::UseForm::Bare(alias) => {
                let path_is_leaf = declared.contains(&u.path);
                let path_is_prefix = prefixes.contains(&u.path);
                match alias {
                    None => {
                        if path_is_leaf {
                            let local = u.path.last().expect("non-empty path").clone();
                            record_alias(local.clone(), u.span, &mut alias_taken)?;
                            item_aliases.insert(local, u.path.clone());
                        } else if path_is_prefix {
                            wildcards.push(u.path.clone());
                        } else {
                            return Err(open_error(
                                source,
                                file,
                                format!("unknown use target '{}'", u.path.join(".")),
                                u.span,
                                "not declared",
                            ));
                        }
                    }
                    Some(alias_name) => {
                        if path_is_leaf {
                            record_alias(alias_name.clone(), u.span, &mut alias_taken)?;
                            item_aliases.insert(alias_name.clone(), u.path.clone());
                        } else if path_is_prefix {
                            record_alias(alias_name.clone(), u.span, &mut alias_taken)?;
                            ns_aliases.insert(alias_name.clone(), u.path.clone());
                        } else {
                            return Err(open_error(
                                source,
                                file,
                                format!("unknown use target '{}'", u.path.join(".")),
                                u.span,
                                "not declared",
                            ));
                        }
                    }
                }
            }
            ast::UseForm::List(items) => {
                if declared.contains(&u.path) {
                    return Err(open_error(
                        source,
                        file,
                        format!(
                            "expected namespace, but '{}' names a type",
                            u.path.join(".")
                        ),
                        u.span,
                        "not a namespace",
                    ));
                }
                if !u.path.is_empty() && !prefixes.contains(&u.path) {
                    return Err(open_error(
                        source,
                        file,
                        format!("unknown use target '{}'", u.path.join(".")),
                        u.span,
                        "not declared",
                    ));
                }
                for it in items {
                    let mut full = u.path.clone();
                    full.push(it.name.clone());
                    if !declared.contains(&full) {
                        return Err(open_error(
                            source,
                            file,
                            format!("unknown use target '{}'", full.join(".")),
                            it.span,
                            "not declared",
                        ));
                    }
                    let local = it.alias.clone().unwrap_or_else(|| it.name.clone());
                    record_alias(local.clone(), it.span, &mut alias_taken)?;
                    item_aliases.insert(local, full);
                }
            }
        }
    }

    // 4. TypeRef resolution + variant-name uniqueness.
    for item in &ast.items {
        match item {
            ast::Item::TypeDecl(t) => {
                for f in &t.fields {
                    check_type_ref(
                        &f.ty,
                        f.ty_span,
                        &declared,
                        &file_ns,
                        &item_aliases,
                        &ns_aliases,
                        &wildcards,
                        source,
                        file,
                    )?;
                }
            }
            ast::Item::UnionDecl(u) => {
                let mut seen: HashSet<String> = HashSet::new();
                for v in &u.variants {
                    if !seen.insert(v.name.clone()) {
                        return Err(open_error(
                            source,
                            file,
                            format!(
                                "duplicate variant '{}' in union '{}'",
                                v.name,
                                u.name.join(".")
                            ),
                            v.span,
                            "duplicate variant",
                        ));
                    }
                    match &v.body {
                        ast::VariantBody::Record(fields) => {
                            for f in fields {
                                check_type_ref(
                                    &f.ty,
                                    f.ty_span,
                                    &declared,
                                    &file_ns,
                                    &item_aliases,
                                    &ns_aliases,
                                    &wildcards,
                                    source,
                                    file,
                                )?;
                            }
                        }
                        ast::VariantBody::TypeRef { ty, ty_span } => {
                            check_type_ref(
                                ty,
                                *ty_span,
                                &declared,
                                &file_ns,
                                &item_aliases,
                                &ns_aliases,
                                &wildcards,
                                source,
                                file,
                            )?;
                        }
                        ast::VariantBody::Unit => {}
                    }
                }
            }
            ast::Item::SymbolSetDecl(s) => {
                let mut seen: HashSet<String> = HashSet::new();
                for entry in &s.symbols {
                    if !seen.insert(entry.name.clone()) {
                        return Err(open_error(
                            source,
                            file,
                            format!(
                                "duplicate symbol '{}' in symbol_set '{}'",
                                entry.name,
                                s.name.join(".")
                            ),
                            entry.span,
                            "duplicate symbol",
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Resolved {
        file_ns,
        item_aliases,
        ns_aliases,
        wildcards,
    })
}

#[allow(clippy::too_many_arguments)]
fn check_type_ref(
    t: &TypeRef,
    ty_span: Span,
    declared: &HashSet<Vec<String>>,
    file_ns: &[String],
    item_aliases: &HashMap<String, Vec<String>>,
    ns_aliases: &HashMap<String, Vec<String>>,
    wildcards: &[Vec<String>],
    source: &str,
    file: &str,
) -> Result<(), ParseError> {
    match t {
        TypeRef::Builtin(_) => Ok(()),
        TypeRef::Named(path) => {
            if resolve_path(path, file_ns, item_aliases, ns_aliases, wildcards, declared).is_some()
            {
                Ok(())
            } else {
                Err(open_error(
                    source,
                    file,
                    format!("unknown type '{}'", path.join(".")),
                    ty_span,
                    "type not declared",
                ))
            }
        }
        TypeRef::Reference(inner) => check_type_ref(
            inner,
            ty_span,
            declared,
            file_ns,
            item_aliases,
            ns_aliases,
            wildcards,
            source,
            file,
        ),
        TypeRef::List(inner) => check_type_ref(
            inner,
            ty_span,
            declared,
            file_ns,
            item_aliases,
            ns_aliases,
            wildcards,
            source,
            file,
        ),
        TypeRef::Tensor { element, .. } => check_type_ref(
            element,
            ty_span,
            declared,
            file_ns,
            item_aliases,
            ns_aliases,
            wildcards,
            source,
            file,
        ),
        TypeRef::Function { params, return_ty } => {
            for p in params {
                check_type_ref(
                    p,
                    ty_span,
                    declared,
                    file_ns,
                    item_aliases,
                    ns_aliases,
                    wildcards,
                    source,
                    file,
                )?;
            }
            check_type_ref(
                return_ty,
                ty_span,
                declared,
                file_ns,
                item_aliases,
                ns_aliases,
                wildcards,
                source,
                file,
            )
        }
    }
}

fn open_error(source: &str, file: &str, message: String, span: Span, label: &str) -> ParseError {
    ParseError::syntax(
        message,
        NamedSource::new(file, source.to_string()),
        span_to_miette(span),
        label.to_string(),
    )
}

/// Resolve an `import "path"` literal against an optional base
/// directory. Returns the canonicalised path on success. Returns
/// `Err(_)` when there's no base directory and the path is relative,
/// or when canonicalisation fails (file not found).
fn resolve_import_path(base_dir: Option<&Path>, path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        match base_dir {
            Some(dir) => dir.join(p),
            None => {
                return Err(format!(
                    "no base directory to resolve relative import '{path}'; \
                     use Document::from_file or supply a base directory"
                ));
            }
        }
    };
    std::fs::canonicalize(&joined)
        .map_err(|e| format!("failed to resolve '{}': {e}", joined.display()))
}

/// Extract the file-namespace declared by the first `NamespaceDecl`
/// (if any) in an items list.
fn first_namespace(items: &[ast::Item]) -> Vec<String> {
    items
        .iter()
        .find_map(|i| match i {
            ast::Item::NamespaceDecl(n) => Some(n.path.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Eagerly walk `items`, follow each top-level `Item::Import`, parse
/// the imported file, and append the resulting `LoadedImport` records
/// to `out`. Each `LoadedImport` carries its own symbol index whose
/// paths point into that loaded file's `items`/`cells` — lookups
/// across the document tree check the importer's index and then each
/// import's index in source order.
fn expand_top_level_imports(
    items: &[ast::Item],
    base_dir: Option<&Path>,
    loading: &mut HashSet<PathBuf>,
    out: &mut Vec<LoadedImport>,
    importer_file: &str,
    importer_source: &str,
) -> Result<(), ParseError> {
    for item in items {
        let ast::Item::Import(imp) = item else {
            continue;
        };
        let path = resolve_import_path(base_dir, &imp.path).map_err(|msg| {
            open_error(
                importer_source,
                importer_file,
                format!("failed to import '{}': {}", imp.path, msg),
                imp.path_span,
                "cannot resolve import",
            )
        })?;
        if !loading.insert(path.clone()) {
            return Err(open_error(
                importer_source,
                importer_file,
                format!("import cycle detected at '{}'", path.display()),
                imp.path_span,
                "cycle",
            ));
        }

        let src = std::fs::read_to_string(&path).map_err(|e| {
            open_error(
                importer_source,
                importer_file,
                format!("failed to read '{}': {e}", path.display()),
                imp.path_span,
                "io error",
            )
        })?;
        let display = path.display().to_string();
        let (parsed_ast, parsed_symbols) = Parser::new(&src, &display).parse_source()?;
        let imported_base = path.parent().map(Path::to_path_buf);
        let file_ns = first_namespace(&parsed_ast.items);

        // Recursively process the imported file's own top-level imports.
        let mut child_eager: Vec<LoadedImport> = Vec::new();
        expand_top_level_imports(
            &parsed_ast.items,
            imported_base.as_deref(),
            loading,
            &mut child_eager,
            &display,
            &src,
        )?;

        // Build cells for the imported file with its own base_dir.
        let cells = parsed_ast
            .items
            .iter()
            .map(|i| ItemCells::build(i, imported_base.as_deref()))
            .collect();

        out.push(LoadedImport {
            path: path.clone(),
            file_ns,
            items: parsed_ast.items,
            cells,
            symbols: parsed_symbols,
            eager_imports: child_eager,
        });

        loading.remove(&path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(src: &str) -> Document {
        // Use an empty registry so existing tests aren't polluted with the
        // four built-in decorator schemas. Explicit `Document::open` /
        // `open_with` behaviour is tested separately.
        Document::open_with(src, "test", &Environment::empty()).expect("open")
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
    fn reference_value_resolves() {
        let doc = open("owner = wil_taylor");
        assert_eq!(
            doc.field("owner").unwrap().value().unwrap(),
            &Value::Identifier("wil_taylor".into())
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
        Document::open_with(src, "test", &env_with_test_builtins()).expect("open")
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
    fn eval_user_function_call_is_non_callable() {
        // Function-literal values aren't yet executable.
        let doc = open_with_builtins(
            r#"
            f = fn(x: i32) -> i32 x
            y = f(3)
            "#,
        );
        let err = doc.field("y").unwrap().value().unwrap_err();
        // `f` evaluates first as an Identifier → looks up the Field, finds
        // a Value::Function — but Call's callee must be an `Identifier`
        // bound to a builtin name. Since `f` isn't a registered builtin,
        // we get UnknownBuiltin.
        assert!(matches!(err, EvalError::UnknownBuiltin { .. }));
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
    fn schema_errors_empty_for_unschemad_block() {
        let doc = Document::open(r#"random "label" { x = 1 }"#, "test").expect("open");
        let b = doc.block("random").unwrap();
        assert!(b.schema_errors().is_empty());
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
}
