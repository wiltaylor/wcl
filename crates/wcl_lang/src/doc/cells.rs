//! Per-item evaluation caches.
//!
//! Each AST item gets one [`ItemCells`] entry. The cell holds memoised
//! decorator-argument values plus a discriminated payload that mirrors the
//! AST variant ([`ItemCellKind`]). Field evaluation results are stored in
//! [`FieldCell`] with cycle-detection. Synthesised table rows produced at
//! cells-build time live in [`SynthRow`] so the view layer can iterate them
//! without re-synthesising.
//!
//! Imported files arrive as [`LoadedImport`] — same shape as a document
//! body (`items + cells + symbols + eager imports`), threaded through
//! resolution and used by the `Block::resolve_root` fallback.
//!
//! Cells are constructed up-front in `BlockCells::build` / `ItemCells::build`;
//! later evaluation only writes into the `OnceLock`s.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use crate::ast::{self, Span};
use crate::error::EvalError;
use crate::symbols::SymbolIndex;
use crate::value::Value;

#[derive(Debug)]
pub(crate) struct FieldCell {
    pub(crate) value: OnceLock<Result<Value, EvalError>>,
    pub(crate) evaluating: AtomicBool,
}

impl FieldCell {
    pub(crate) fn new() -> Self {
        Self {
            value: OnceLock::new(),
            evaluating: AtomicBool::new(false),
        }
    }
}

/// Per-decorator cache for evaluated positional and named arguments.
#[derive(Debug, Default)]
pub(crate) struct DecoratorCell {
    pub(crate) positional: OnceLock<Result<Vec<Value>, EvalError>>,
    pub(crate) named: OnceLock<HashMap<String, Result<Value, EvalError>>>,
}

#[derive(Debug)]
pub(crate) struct ItemCells {
    pub(crate) decorators: Vec<DecoratorCell>,
    pub(crate) kind: ItemCellKind,
}

#[derive(Debug)]
pub(crate) enum ItemCellKind {
    Field(FieldCell),
    /// `let name = expr` item. Reuses `FieldCell` for the memoised
    /// value + cycle-detection flag; evaluated on first name
    /// resolution, never as document output.
    Let(FieldCell),
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
        /// Synthesised `Block`s from a *computed* `@children(kind)` /
        /// `@child(kind)` field — i.e. `field = <list expr>` in place of
        /// nested block literals (a "splice"). Unlike `synth_rows`, these
        /// can't be built at cells-build time: the RHS expression
        /// (`map(...)`) needs the scope chain + lazy evaluation, only
        /// available at view time. Populated on first projection /
        /// `blocks()` walk via `Block::computed_children`. Each child is
        /// value-backed: its label/field cells are pre-seeded with the
        /// record's values, so reads never touch the placeholder AST.
        computed_children: OnceLock<Vec<SynthChild>>,
        /// Body expansions for a `@contextual` block — one per binding
        /// set (one for an instantiated body; one per element for a
        /// repetition). Each holds a **fresh** copy of the body's
        /// evaluation cells so the same body AST evaluated under different
        /// bindings (repeated instances, loop iterations) doesn't collide
        /// in the shared field-value cache. Built once, lazily, by
        /// `Block::expand_bodies` (renderer-driven).
        expansions: OnceLock<Vec<Expansion>>,
        /// Memo for the Value-producing `Block::typed_field` projections
        /// (`@connections`, union `@children`/`@child`), keyed by
        /// (block kind, field name). Without it every reference
        /// re-projected the block's connection statements / re-reified
        /// its children. The kind in the key keeps the cache sound for
        /// cells viewed under a `kind_override` (synth table rows).
        /// Expansion cells are fresh per binding set, so per-binding
        /// projections never collide here.
        typed_proj_memo: std::sync::RwLock<HashMap<(String, String), Value>>,
        /// `true` if any direct child item is an `Item::Import`. A lazy
        /// in-block import can splice lets/fields invisible to any
        /// open-time name scan, so `eval_call`'s builtin fast path
        /// stands down when such a block is on the scope chain.
        has_block_imports: bool,
        /// Memo for `Block::can_bind_name`, keyed by the viewed kind
        /// (`kind_override` variance): every name this block's scope
        /// frame could bind — lets / fields / nested kinds / table
        /// headers across its realized sources, plus its schema's
        /// effective field names. `scope_lookup` skips the frame's
        /// per-item scans when the resolving name is absent.
        bindable_names: std::sync::RwLock<HashMap<String, std::sync::Arc<HashSet<String>>>>,
    },
    TypeDecl {
        /// One inner Vec per `ast::TypeDecl.fields[i]`, holding cells for
        /// that field's decorators.
        field_decorators: Vec<Vec<DecoratorCell>>,
    },
    InterfaceDecl {
        /// One inner Vec per `ast::InterfaceDecl.fields[i]`, holding cells
        /// for that field's decorators.
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
        /// `true` for an angle-bracket system import resolved through the
        /// registry; `false` for a quoted disk import.
        system: bool,
        /// Resolved file directory for path joins. `None` means the
        /// document had no base directory (e.g. `Document::open`),
        /// which surfaces as an `ImportFailed` on first access.
        base_dir: Option<PathBuf>,
        loaded: OnceLock<Result<LoadedImport, EvalError>>,
    },
    ConnectionDecl,
    Connection,
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

/// One synthesised child-Block produced from a computed `@children` /
/// `@child` field (a "splice" — `field = <list expr>`). Built lazily at
/// view time (the RHS needs evaluation). `field_name` is the slot it
/// fills; `kind` is its concrete block kind (set via `kind_override` on
/// the handed-out view). The block's label/field cells are pre-seeded
/// with the source record's values, so `Block::labels` / `Block::field`
/// short-circuit to them — the placeholder AST exprs are never evaluated.
#[derive(Debug)]
pub(crate) struct SynthChild {
    pub(crate) field_name: String,
    pub(crate) kind: String,
    pub(crate) block: ast::Block,
    pub(crate) cells: ItemCells,
}

/// One body expansion of a `@contextual` block: a binding set (a
/// declarer's parameter values, or a loop variable) plus a fresh copy of the body
/// block's evaluation cells, so each expansion evaluates independently.
#[derive(Debug)]
pub(crate) struct Expansion {
    pub(crate) bindings: std::sync::Arc<Vec<(String, Value)>>,
    pub(crate) cells: ItemCells,
}

/// Result of loading one imported file. Transitive top-level imports
/// inside the loaded file are flattened into `eager_imports`; nested
/// (in-block) imports stay lazy inside `items`/`cells`.
#[derive(Debug)]
pub(crate) struct LoadedImport {
    pub(crate) path: PathBuf,
    /// Raw source text of the imported file, retained so diagnostics
    /// raised against this file's spans can render their snippet against
    /// the correct source (a cross-file eval error otherwise renders
    /// against the root document's text — wrong offsets / `OutOfBounds`).
    pub(crate) source: String,
    pub(crate) file_ns: Vec<String>,
    pub(crate) items: Vec<ast::Item>,
    pub(crate) cells: Vec<ItemCells>,
    /// Symbols indexed within this loaded file. Paths refer to the
    /// `items`/`cells` arrays in this same struct.
    pub(crate) symbols: SymbolIndex,
    pub(crate) eager_imports: Vec<LoadedImport>,
}

pub(crate) fn make_decorator_cells(decs: &[ast::Decorator]) -> Vec<DecoratorCell> {
    (0..decs.len()).map(|_| DecoratorCell::default()).collect()
}

impl BlockCells {
    pub(crate) fn build(items: &[ast::Item], base_dir: Option<&Path>) -> Self {
        let cells = items
            .iter()
            .map(|item| ItemCells::build(item, base_dir))
            .collect();
        Self { items: cells }
    }
}

impl ItemCells {
    /// `true` when these are Block cells whose direct items include a
    /// lazy `import` — see `ItemCellKind::Block::has_block_imports`.
    pub(crate) fn has_block_imports(&self) -> bool {
        matches!(
            &self.kind,
            ItemCellKind::Block {
                has_block_imports: true,
                ..
            }
        )
    }

    pub(crate) fn build(item: &ast::Item, base_dir: Option<&Path>) -> Self {
        match item {
            ast::Item::Field(f) => Self {
                decorators: make_decorator_cells(&f.decorators),
                kind: ItemCellKind::Field(FieldCell::new()),
            },
            ast::Item::Let(_) => Self {
                decorators: Vec::new(),
                kind: ItemCellKind::Let(FieldCell::new()),
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
                                kind_ns: Vec::new(),
                                labels: r.values.clone(),
                                items: Vec::new(),
                                decorators: Vec::new(),
                                span: r.span,
                                leading_trivia: Vec::new(),
                                trailing_comment: None,
                                trailing_trivia: Vec::new(),
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
                        computed_children: OnceLock::new(),
                        expansions: OnceLock::new(),
                        typed_proj_memo: std::sync::RwLock::new(HashMap::new()),
                        has_block_imports: b
                            .items
                            .iter()
                            .any(|item| matches!(item, ast::Item::Import(_))),
                        bindable_names: std::sync::RwLock::new(HashMap::new()),
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
            ast::Item::InterfaceDecl(i) => Self {
                decorators: make_decorator_cells(&i.decorators),
                kind: ItemCellKind::InterfaceDecl {
                    field_decorators: i
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
                            ast::VariantBody::Record { fields, .. } => fields
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
                    system: imp.system,
                    base_dir: base_dir.map(Path::to_path_buf),
                    loaded: OnceLock::new(),
                },
            },
            ast::Item::Table(_) => Self {
                decorators: Vec::new(),
                kind: ItemCellKind::Table,
            },
            ast::Item::ConnectionDecl(_) => Self {
                decorators: Vec::new(),
                kind: ItemCellKind::ConnectionDecl,
            },
            ast::Item::Connection(_) => Self {
                decorators: Vec::new(),
                kind: ItemCellKind::Connection,
            },
        }
    }
}
