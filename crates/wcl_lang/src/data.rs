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
#[derive(Clone, Copy)]
pub struct DataRef<'a> {
    inner: DataKind<'a>,
}

#[derive(Clone, Copy)]
pub enum DataKind<'a> {
    Field(Field<'a>),
    Block(Block<'a>),
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

    pub fn kind(&self) -> &'static str {
        match self.inner {
            DataKind::Field(_) => "field",
            DataKind::Block(_) => "block",
            DataKind::Type(_) => "type",
            DataKind::TypeField(_) => "type_field",
            DataKind::Union(_) => "union",
            DataKind::Variant(_) => "variant",
            DataKind::Symbols(_) => "symbol_set",
            DataKind::Symbol(_) => "symbol_entry",
        }
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
        match self.inner {
            DataKind::Field(_) | DataKind::TypeField(_) | DataKind::Symbol(_) => None,
            DataKind::Block(b) => {
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
        let mut cur = *self;
        for seg in path.split('.').filter(|s| !s.is_empty()) {
            cur = cur.child(seg)?;
        }
        Some(cur)
    }

    /// Materialise the leaf value. Returns `Err(NotALeaf)` for any
    /// variant that isn't a `Field`. The underlying `Field::value` is
    /// cached, so repeated calls are O(1).
    pub fn value(&self) -> Result<Value, EvalError> {
        match self.inner {
            DataKind::Field(f) => match f.value() {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(e.clone()),
            },
            _ => Err(EvalError::not_a_leaf(self.kind(), self.span())),
        }
    }

    /// Span of the underlying AST node, useful for diagnostics.
    pub fn span(&self) -> crate::ast::Span {
        match self.inner {
            DataKind::Field(f) => f.span(),
            DataKind::Block(b) => b.span(),
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
        match self.inner {
            DataKind::Field(f) => Some(f),
            _ => None,
        }
    }
    pub fn as_block(&self) -> Option<Block<'a>> {
        match self.inner {
            DataKind::Block(b) => Some(b),
            _ => None,
        }
    }
    pub fn as_type(&self) -> Option<TypeDecl<'a>> {
        match self.inner {
            DataKind::Type(t) => Some(t),
            _ => None,
        }
    }
    pub fn as_union(&self) -> Option<UnionDecl<'a>> {
        match self.inner {
            DataKind::Union(u) => Some(u),
            _ => None,
        }
    }
    pub fn as_symbol_set(&self) -> Option<SymbolSetDecl<'a>> {
        match self.inner {
            DataKind::Symbols(s) => Some(s),
            _ => None,
        }
    }

    /// Iterate over fields. Only `Block` yields entries; other variants
    /// produce an empty iterator.
    pub fn fields(&self) -> Box<dyn Iterator<Item = DataRef<'a>> + 'a> {
        match self.inner {
            DataKind::Block(b) => Box::new(b.fields().map(DataRef::from_field)),
            _ => Box::new(std::iter::empty()),
        }
    }

    /// Iterate over nested blocks. Only `Block` yields entries.
    pub fn blocks(&self) -> Box<dyn Iterator<Item = DataRef<'a>> + 'a> {
        match self.inner {
            DataKind::Block(b) => Box::new(b.blocks().map(DataRef::from_block)),
            _ => Box::new(std::iter::empty()),
        }
    }

    /// Walk every immediate child — fields and blocks for a `Block`,
    /// variants for a `Union`, type-fields for a `Type`, symbol entries
    /// for a `SymbolSet`. Order matches source order within each kind.
    pub fn children(&self) -> Box<dyn Iterator<Item = DataRef<'a>> + 'a> {
        match self.inner {
            DataKind::Block(b) => Box::new(
                b.fields()
                    .map(DataRef::from_field)
                    .chain(b.blocks().map(DataRef::from_block)),
            ),
            DataKind::Type(t) => Box::new(t.fields().map(|f| DataRef::new(DataKind::TypeField(f)))),
            DataKind::Union(u) => {
                Box::new(u.variants().map(|v| DataRef::new(DataKind::Variant(v))))
            }
            DataKind::Variant(v) => {
                Box::new(v.fields().map(|f| DataRef::new(DataKind::TypeField(f))))
            }
            DataKind::Symbols(s) => {
                Box::new(s.symbols().map(|e| DataRef::new(DataKind::Symbol(e))))
            }
            DataKind::Field(_) | DataKind::TypeField(_) | DataKind::Symbol(_) => {
                Box::new(std::iter::empty())
            }
        }
    }
}
