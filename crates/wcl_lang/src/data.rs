//! Lazy data-access handle into a [`Document`].
//!
//! `DataRef` is a thin borrowed wrapper over the document's view types.
//! Each navigation step (`child`, `get`) only pays the cost of resolving
//! that step — subtree contents stay unmaterialised until the host
//! actually walks into them. Leaf evaluation (`value`) goes through the
//! same `FieldCell` caching that `Field::value` uses today, so repeated
//! reads of the same leaf are O(1).

use crate::doc::{
    Block, Field, SymbolEntry, SymbolSetDecl, TypeDecl, TypeField, UnionDecl, UnionVariant,
};
use crate::error::EvalError;
use crate::value::Value;

/// Lazy navigator into a [`Document`]. Acquire one with
/// [`Document::get`](crate::Document::get) or by wrapping an existing
/// view type via `DataRef::from(...)` /
/// [`DataRef::from_field`] etc.
#[derive(Clone)]
pub struct DataRef<'a> {
    inner: DataKind<'a>,
}

#[derive(Clone)]
pub enum DataKind<'a> {
    Field(Field<'a>),
    Block(Block<'a>),
    /// A list of `Block`s — produced by schema fields decorated with
    /// `@children("kind")`. Not name-addressable; iterate via
    /// `children()` or `len()`.
    BlockList(Vec<Block<'a>>),
    /// A list of `Block`s acting as table rows — produced by
    /// `@children("kind")` where the row kind is `@table`-schema'd.
    /// Same shape as `BlockList` but exposes row/column accessors.
    Table(Vec<Block<'a>>),
    Type(TypeDecl<'a>),
    TypeField(TypeField<'a>),
    Union(UnionDecl<'a>),
    Variant(UnionVariant<'a>),
    Symbols(SymbolSetDecl<'a>),
    Symbol(SymbolEntry<'a>),
}

impl<'a> DataRef<'a> {
    pub(crate) fn new(inner: DataKind<'a>) -> Self {
        Self { inner }
    }

    pub fn from_field(f: Field<'a>) -> Self {
        Self::new(DataKind::Field(f))
    }
    pub fn from_block(b: Block<'a>) -> Self {
        Self::new(DataKind::Block(b))
    }
    pub fn from_type(t: TypeDecl<'a>) -> Self {
        Self::new(DataKind::Type(t))
    }
    pub fn from_union(u: UnionDecl<'a>) -> Self {
        Self::new(DataKind::Union(u))
    }
    pub fn from_symbol_set(s: SymbolSetDecl<'a>) -> Self {
        Self::new(DataKind::Symbols(s))
    }
    pub fn from_block_list(blocks: Vec<Block<'a>>) -> Self {
        Self::new(DataKind::BlockList(blocks))
    }
    pub fn from_table(blocks: Vec<Block<'a>>) -> Self {
        Self::new(DataKind::Table(blocks))
    }

    pub fn kind(&self) -> &'static str {
        match &self.inner {
            DataKind::Field(_) => "field",
            DataKind::Block(_) => "block",
            DataKind::BlockList(_) => "block_list",
            DataKind::Table(_) => "table",
            DataKind::Type(_) => "type",
            DataKind::TypeField(_) => "type_field",
            DataKind::Union(_) => "union",
            DataKind::Variant(_) => "variant",
            DataKind::Symbols(_) => "symbol_set",
            DataKind::Symbol(_) => "symbol_entry",
        }
    }

    /// Number of entries in a `BlockList` or `Table`. `None` for any
    /// other variant.
    pub fn len(&self) -> Option<usize> {
        match &self.inner {
            DataKind::BlockList(v) | DataKind::Table(v) => Some(v.len()),
            _ => None,
        }
    }

    /// `Table::row_count` alias for clarity. Returns `None` for
    /// non-table variants.
    pub fn row_count(&self) -> Option<usize> {
        match &self.inner {
            DataKind::Table(v) => Some(v.len()),
            _ => None,
        }
    }

    /// Return the ith row as `DataRef::Block`. `None` if not a table
    /// or index out of range.
    pub fn row(&self, i: usize) -> Option<DataRef<'a>> {
        match &self.inner {
            DataKind::Table(v) => v.get(i).cloned().map(DataRef::from_block),
            _ => None,
        }
    }

    /// Project a single column across every row of a `Table`. Each
    /// row's `labels()` is evaluated; the requested column index is
    /// the schema field's declaration position.
    pub fn column(&self, name: &str) -> Result<Vec<crate::value::Value>, EvalError> {
        let rows = match &self.inner {
            DataKind::Table(v) => v,
            _ => {
                return Err(EvalError::not_a_leaf(
                    self.kind(),
                    crate::ast::Span::new(0, 0),
                ));
            }
        };
        let Some(first) = rows.first() else {
            return Ok(Vec::new());
        };
        let schema = first
            .schema()
            .ok_or_else(|| EvalError::not_a_leaf("table without schema", first.span()))?;
        let idx = schema.fields().position(|f| f.name() == name);
        let Some(idx) = idx else {
            return Err(EvalError::not_a_leaf("unknown table column", first.span()));
        };
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let labels = r.labels()?;
            if let Some(v) = labels.get(idx) {
                out.push(v.clone());
                continue;
            }
            // Literal block written long-form (e.g.
            // `user { name = "..." age = ... }`) has no pipe labels;
            // fall back to the named field.
            if let Some(f) = r.field(name) {
                out.push(f.value().map_err(|e| e.clone())?.clone());
            } else {
                out.push(Value::None);
            }
        }
        Ok(out)
    }

    pub fn is_empty(&self) -> Option<bool> {
        self.len().map(|n| n == 0)
    }

    /// Walk a single named child. Returns `None` for leaves and for
    /// unknown names.
    ///
    /// - `Field` is a leaf; always `None`.
    /// - `Block` tries a child Field first, then a nested Block (matches
    ///   `Document::field` / `Document::block` precedence).
    /// - `Type` matches a `TypeField` by name.
    /// - `TypeField` is a leaf-ish view; `None`.
    /// - `Union` matches a `UnionVariant` by name.
    /// - `Variant` matches a record-body field by name.
    /// - `Symbols` matches a `SymbolEntry` by name.
    /// - `Symbol` has no children; `None`.
    pub fn child(&self, name: &str) -> Option<DataRef<'a>> {
        match &self.inner {
            DataKind::Field(_)
            | DataKind::TypeField(_)
            | DataKind::Symbol(_)
            | DataKind::BlockList(_)
            | DataKind::Table(_) => None,
            DataKind::Block(b) => {
                // Schema-aware first: a schema'd block projects names
                // through its declared `@inline` / `@child` /
                // `@children` decorators.
                if let Some(r) = b.typed_field(name) {
                    return Some(r);
                }
                if let Some(f) = b.field(name) {
                    return Some(DataRef::from_field(f));
                }
                b.block(name).map(DataRef::from_block)
            }
            DataKind::Type(t) => t.field(name).map(|f| DataRef::new(DataKind::TypeField(f))),
            DataKind::Union(u) => u.variant(name).map(|v| DataRef::new(DataKind::Variant(v))),
            DataKind::Variant(v) => v.field(name).map(|f| DataRef::new(DataKind::TypeField(f))),
            DataKind::Symbols(s) => s
                .symbols()
                .find(|e| e.name() == name)
                .map(|e| DataRef::new(DataKind::Symbol(e))),
        }
    }

    /// Walk a dotted path. Empty path returns a clone of `self`. Each
    /// segment is resolved lazily via `child`; missing segments yield
    /// `None`.
    pub fn get(&self, path: &str) -> Option<DataRef<'a>> {
        let mut cur = self.clone();
        for seg in path.split('.').filter(|s| !s.is_empty()) {
            cur = cur.child(seg)?;
        }
        Some(cur)
    }

    /// Materialise the leaf value. Returns `Err(NotALeaf)` for any
    /// variant that isn't a `Field`. The underlying `Field::value` is
    /// cached, so repeated calls are O(1).
    pub fn value(&self) -> Result<Value, EvalError> {
        match &self.inner {
            DataKind::Field(f) => match f.value() {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(e.clone()),
            },
            _ => Err(EvalError::not_a_leaf(self.kind(), self.span())),
        }
    }

    /// Span of the underlying AST node, useful for diagnostics.
    /// `BlockList` returns the span of its first block (or
    /// `Span::default()` when empty).
    pub fn span(&self) -> crate::ast::Span {
        match &self.inner {
            DataKind::Field(f) => f.span(),
            DataKind::Block(b) => b.span(),
            DataKind::BlockList(v) | DataKind::Table(v) => v
                .first()
                .map(|b| b.span())
                .unwrap_or_else(|| crate::ast::Span::new(0, 0)),
            DataKind::Type(t) => t.span(),
            DataKind::TypeField(f) => f.span(),
            DataKind::Union(u) => u.span(),
            DataKind::Variant(v) => v.span(),
            DataKind::Symbols(s) => s.span(),
            DataKind::Symbol(s) => s.span(),
        }
    }

    pub fn inner(&self) -> &DataKind<'a> {
        &self.inner
    }

    /// Convenience accessors for callers that want the underlying typed
    /// view back without re-matching.
    pub fn as_field(&self) -> Option<Field<'a>> {
        match &self.inner {
            DataKind::Field(f) => Some(*f),
            _ => None,
        }
    }
    pub fn as_block(&self) -> Option<Block<'a>> {
        match &self.inner {
            DataKind::Block(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_block_list(&self) -> Option<&[Block<'a>]> {
        match &self.inner {
            DataKind::BlockList(v) => Some(v.as_slice()),
            _ => None,
        }
    }
    pub fn as_type(&self) -> Option<TypeDecl<'a>> {
        match &self.inner {
            DataKind::Type(t) => Some(*t),
            _ => None,
        }
    }
    pub fn as_union(&self) -> Option<UnionDecl<'a>> {
        match &self.inner {
            DataKind::Union(u) => Some(*u),
            _ => None,
        }
    }
    pub fn as_symbol_set(&self) -> Option<SymbolSetDecl<'a>> {
        match &self.inner {
            DataKind::Symbols(s) => Some(*s),
            _ => None,
        }
    }

    /// Iterate over fields. Only `Block` yields entries; other variants
    /// produce an empty iterator. AST-level iteration (escape hatch);
    /// for schema-projected iteration use [`children`](Self::children).
    pub fn fields(&self) -> Box<dyn Iterator<Item = DataRef<'a>> + 'a> {
        match &self.inner {
            DataKind::Block(b) => Box::new(b.fields().map(DataRef::from_field)),
            _ => Box::new(std::iter::empty()),
        }
    }

    /// Iterate over nested blocks. Only `Block` yields entries.
    /// AST-level iteration (escape hatch).
    pub fn blocks(&self) -> Box<dyn Iterator<Item = DataRef<'a>> + 'a> {
        match &self.inner {
            DataKind::Block(b) => Box::new(b.blocks().map(DataRef::from_block)),
            DataKind::BlockList(v) | DataKind::Table(v) => {
                Box::new(v.clone().into_iter().map(DataRef::from_block))
            }
            _ => Box::new(std::iter::empty()),
        }
    }

    /// Walk every immediate child. For a schema'd `Block`, yields the
    /// typed-field projection (one entry per declared field). For an
    /// un-schema'd block, yields raw AST fields followed by raw nested
    /// blocks. `BlockList` yields each block in order.
    pub fn children(&self) -> Box<dyn Iterator<Item = DataRef<'a>> + 'a> {
        match &self.inner {
            DataKind::Block(b) => {
                let projected: Vec<DataRef<'a>> =
                    b.typed_fields().map(|(_, dr)| dr).collect::<Vec<_>>();
                if !projected.is_empty() {
                    Box::new(projected.into_iter())
                } else {
                    let bb = *b;
                    Box::new(
                        bb.fields()
                            .map(DataRef::from_field)
                            .chain(bb.blocks().map(DataRef::from_block)),
                    )
                }
            }
            DataKind::BlockList(v) | DataKind::Table(v) => {
                Box::new(v.clone().into_iter().map(DataRef::from_block))
            }
            DataKind::Type(t) => {
                let t = *t;
                Box::new(t.fields().map(|f| DataRef::new(DataKind::TypeField(f))))
            }
            DataKind::Union(u) => {
                let u = *u;
                Box::new(u.variants().map(|v| DataRef::new(DataKind::Variant(v))))
            }
            DataKind::Variant(v) => {
                let v = *v;
                Box::new(v.fields().map(|f| DataRef::new(DataKind::TypeField(f))))
            }
            DataKind::Symbols(s) => {
                let s = *s;
                Box::new(s.symbols().map(|e| DataRef::new(DataKind::Symbol(e))))
            }
            DataKind::Field(_) | DataKind::TypeField(_) | DataKind::Symbol(_) => {
                Box::new(std::iter::empty())
            }
        }
    }
}
