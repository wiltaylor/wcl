//! View types: borrowed wrappers around AST nodes that expose
//! the cached, schema-aware document layer (decorators, type/
//! interface/union decls, fields, blocks, tables, …). Extracted
//! from `doc.rs` so the parent file can stay focused on the
//! Document container itself.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use crate::ast::{self, Span};
use crate::error::EvalError;
use crate::value::{BuiltinType, TensorDim, TypeRef, Value};

use super::cells::{DecoratorCell, FieldCell, ItemCellKind, ItemCells};
use super::effective_fields::{
    build_effective_fields, is_descendant_of_walk, lookup_effective_field,
};
use super::eval::EvalCtx;
use super::imports::{BlockSlice, load_import_lazily, push_loaded_imports};
use super::interfaces::check_interface_conformance;
use super::interfaces::{dataref_concrete_type, same_type_decl};
use super::lookup::{iter_blocks, iter_fields, iter_tables};
use super::schema_check::compute_schema_errors;
use super::scope::{Scope, ScopeFrame};
use super::variant_dispatch;
use super::{Document, find_block, find_field, find_let, has_schemaless};
use super::{expr_to_path_segments, materialise_dataref_or_path, span_to_miette};

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
    /// `@child(SomeInterface)` — match nested blocks whose `@block`
    /// type transitively `extends` the named interface.
    Interface(InterfaceDecl<'a>),
}

impl<'a> ChildKind<'a> {
    pub fn as_kind(&self) -> Option<&str> {
        match self {
            ChildKind::Kind(s) => Some(s.as_str()),
            ChildKind::Union(_) | ChildKind::Interface(_) => None,
        }
    }

    pub fn as_union(&self) -> Option<&UnionDecl<'a>> {
        match self {
            ChildKind::Union(u) => Some(u),
            ChildKind::Kind(_) | ChildKind::Interface(_) => None,
        }
    }

    pub fn as_interface(&self) -> Option<&InterfaceDecl<'a>> {
        match self {
            ChildKind::Interface(i) => Some(i),
            ChildKind::Kind(_) | ChildKind::Union(_) => None,
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
            for fqn in &candidates {
                if let Some(i) = doc.interface(fqn) {
                    return Some(ChildKind::Interface(i));
                }
            }
            None
        }
        _ => None,
    }
}

/// Build one value-backed [`SynthChild`] of `kind` from a spliced list
/// element. The element is a `Value::Record` / `Value::Variant`
/// (record payload) whose entries map onto `kind`'s schema fields —
/// `@inline(N)` entries become block labels, the rest become named
/// fields — or a bare scalar, which fills the kind's single `@inline(0)`
/// slot. The synthetic block's label/field cells are pre-seeded with the
/// values, so `Block::labels` / `Block::field` read them directly and the
/// placeholder `Expr::None`s are never evaluated. `None` when `kind` has
/// no `@block` schema (the splice is then a no-op for that element).
fn synth_child_from_value(
    doc: &Document,
    field_name: &str,
    kind: &str,
    value: &Value,
) -> Option<crate::doc::cells::SynthChild> {
    use crate::value::VariantPayload;
    use std::collections::BTreeMap;

    let schema = doc.block_schema(kind)?;

    // Normalise the element to a field map.
    let fields: BTreeMap<String, Value> = match value {
        Value::Record { fields, .. } => fields.clone(),
        Value::Variant {
            payload: VariantPayload::Record(m),
            ..
        } => m.clone(),
        scalar => {
            // A bare scalar fills the kind's single `@inline(0)` field
            // (e.g. `list { items = map(names, fn(n) -> n) }` → `li` text).
            let inline0 = schema.fields().find(|f| f.inline_slot() == Some(0))?;
            let mut m = BTreeMap::new();
            m.insert(inline0.name().to_string(), scalar.clone());
            m
        }
    };

    // Partition record entries into `@inline(slot)` labels vs named fields.
    let mut max_slot: i64 = -1;
    let mut label_pairs: Vec<(u64, Value)> = Vec::new();
    let mut named: Vec<(String, Value)> = Vec::new();
    for (k, v) in &fields {
        if let Some(slot) = schema
            .fields()
            .find(|f| f.name() == k)
            .and_then(|f| f.inline_slot())
        {
            max_slot = max_slot.max(slot as i64);
            label_pairs.push((slot, v.clone()));
        } else {
            named.push((k.clone(), v.clone()));
        }
    }

    let label_len = (max_slot + 1).max(0) as usize;
    let mut label_values = vec![Value::None; label_len];
    for (slot, v) in label_pairs {
        label_values[slot as usize] = v;
    }

    let synth_span = ast::Span::new(0, 0);
    let items: Vec<ast::Item> = named
        .iter()
        .map(|(name, _)| {
            ast::Item::Field(ast::Field {
                name: name.clone(),
                expr: ast::Expr::None,
                decorators: Vec::new(),
                span: synth_span,
                leading_trivia: Vec::new(),
            })
        })
        .collect();
    let synth_block = ast::Block {
        kind: String::new(),
        labels: vec![ast::Expr::None; label_len],
        items,
        decorators: Vec::new(),
        span: synth_span,
        leading_trivia: Vec::new(),
    };
    let synth_cells = ItemCells::build(&ast::Item::Block(synth_block.clone()), None);

    // Pre-seed the label + named-field caches with the record values, so
    // reads short-circuit the placeholder exprs.
    if let ItemCellKind::Block {
        labels,
        items: item_cells,
        ..
    } = &synth_cells.kind
    {
        let _ = labels.set(Ok(label_values));
        for ((_, v), cell) in named.iter().zip(item_cells.iter()) {
            if let ItemCellKind::Field(fc) = &cell.kind {
                let _ = fc.value.set(Ok(v.clone()));
            }
        }
    }

    Some(crate::doc::cells::SynthChild {
        field_name: field_name.to_string(),
        kind: kind.to_string(),
        block: synth_block,
        cells: synth_cells,
    })
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
    pub(in crate::doc) ast: &'a ast::UseDecl,
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

    /// Default value for this field, if any. Priority:
    /// 1. The inline `name = expr` form (stored as `default_expr`).
    /// 2. The `@default(v)` decorator (classic form).
    ///
    /// Both forms produce the same `Value`; the inline form just
    /// avoids spelling the type a second time.
    pub fn default_value(&self) -> Option<Value> {
        if let Some(expr) = &self.ast.default_expr {
            return self.doc.eval_literal(expr).ok();
        }
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
            ChildKind::Union(_) | ChildKind::Interface(_) => None,
        }
    }

    /// If this field carries an `@children("kind", min?, max?)`
    /// decorator, returns the nested block kind it binds. Returns
    /// `None` for the union form — use [`children_kind_or_union`].
    pub fn children_block_kind(&self) -> Option<String> {
        match self.children_kind_or_union()? {
            ChildKind::Kind(s) => Some(s),
            ChildKind::Union(_) | ChildKind::Interface(_) => None,
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
    pub(in crate::doc) fn field_cell(&self) -> &'a FieldCell {
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

    /// File that declares this field. `None` means "the document's
    /// main source" — the file the host passed to
    /// [`Document::from_file`] (or the in-memory source if the
    /// document was opened via [`Document::open`] from a string).
    ///
    /// Walks both eager imports and any lazy in-block imports that
    /// have been forced (a call to [`Document::get`] forces every
    /// lazy import on the path it walks, which is the regime
    /// CLI-style consumers operate in).
    pub fn source_path(&self) -> Option<&'a Path> {
        let target: *const ast::Field = self.ast;
        self.doc.find_field_source_path(target)
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
        } else if matches!(
            self.declared_type_ref(),
            Some(TypeRef::Builtin(BuiltinType::Identifier))
        ) {
            // Identifier-typed field: a *bare* identifier stays an opaque
            // name (`id = web` → `"web"`, not a variable lookup), but any
            // other expression evaluates in the field's scope — so a
            // data-derived `id = s.key` (a repeater/component binding)
            // resolves instead of being looked up at root.
            self.doc.eval_literal_in_scope(&self.ast.expr, &self.scope)
        } else {
            self.doc
                .eval_in_scope(&self.ast.expr, &self.scope)
                .and_then(|v| match self.declared_type_ref() {
                    // Coerce a bare-record value to the field's declared
                    // union variant by shape (recursing through lists).
                    Some(ty) => {
                        variant_dispatch::coerce_value_to_type(self.doc, v, ty, self.ast.span)
                    }
                    None => Ok(v),
                })
        };
        cell.evaluating.store(false, Ordering::Release);
        cell.value.get_or_init(|| result).as_ref()
    }

    /// `Some(err)` if this field's name isn't accepted by the
    /// applicable schema (parent block, or the document if top-level).
    /// `None` means the membership check passes.
    fn schema_membership_error(&self) -> Option<EvalError> {
        use crate::error::SchemaViolationKind as Kind;
        let parent_schema = match self.scope.frames().last().cloned() {
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
            None => {
                // `@document` is per-namespace: look up the schema
                // that governs this field's source.
                let field_ns = self.doc.find_field_source_ns(self.ast);
                self.doc.doc_schema_for_ns(field_ns)
            }
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
    pub(super) fn declared_type_ref(&self) -> Option<&'a TypeRef> {
        if let Some(frame) = self.scope.frames().last().cloned() {
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
        // Top-level field: consult the @document schema in this
        // field's source namespace, if any.
        let field_ns = self.doc.find_field_source_ns(self.ast);
        let doc_schema = self.doc.doc_schema_for_ns(field_ns)?;
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

/// View over a `let name = expr` item (top-level or block-level). A
/// composition helper resolved by name during evaluation but never
/// surfaced as document data. Its value is memoised in a [`FieldCell`]
/// with the same cycle-detection the field evaluator uses; evaluation
/// happens lazily on first name resolution.
#[derive(Clone)]
pub(crate) struct LetView<'a> {
    pub(super) ast: &'a ast::LetItem,
    pub(super) cell: &'a FieldCell,
    pub(super) doc: &'a Document,
    /// Scope the let's value expression is evaluated in — the
    /// declaring block's child scope (or `Scope::root()` for a
    /// top-level let), so it sees siblings and ancestors.
    pub(super) scope: Scope<'a>,
}

impl<'a> LetView<'a> {
    /// Evaluate (once) and return the bound value. Mirrors
    /// [`Field::value`]'s cycle-detection: a re-entrant evaluation
    /// caches and returns an `EvalError::Cycle`.
    pub(crate) fn value(&self) -> Result<Value, EvalError> {
        let cell = self.cell;
        if let Some(cached) = cell.value.get() {
            return cached.clone();
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
                .clone();
        }
        let result = self.doc.eval_in_scope(&self.ast.value, &self.scope);
        cell.evaluating.store(false, Ordering::Release);
        cell.value.get_or_init(|| result).clone()
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

    /// The document this block belongs to. Lets host renderers reach
    /// document-level lookups (e.g. `component_def`) from a block view.
    pub fn doc(&self) -> &'a Document {
        self.doc
    }

    /// Scope that child expressions inside this block see — the
    /// block's own `scope` extended with one frame for itself.
    pub(crate) fn child_scope(&self) -> Scope<'a> {
        self.scope.push(ScopeFrame {
            ast: self.ast,
            cells: self.cells,
            kind_override: self.kind_override,
            bindings: None,
        })
    }

    /// How many value-binding frames are in this block's scope — i.e. how
    /// deep `wdoc_component` / `wdoc_repeater` expansion currently is. The
    /// renderer caps on this to stop a self-referential component from
    /// expanding forever (iteration count doesn't inflate it: all elements
    /// of one repeater share a depth).
    pub fn binding_scope_depth(&self) -> usize {
        self.scope
            .frames()
            .iter()
            .filter(|f| f.bindings.is_some())
            .count()
    }

    /// Expand `body`'s child blocks once per binding set, each under a
    /// scope carrying that set's `name → value` bindings **and a fresh
    /// copy of the body's evaluation cells**. The fresh cells are the
    /// crux: `Field::value` memoises in a per-cell `OnceLock`, so the same
    /// body AST evaluated under different bindings (repeated component
    /// instances, repeater iterations) would otherwise collide on the
    /// first-seen value. Caching the fresh cells on `self`'s cell (the
    /// per-expansion owner — a component instance, or the `wdoc_repeater`)
    /// gives each expansion an independent cache.
    ///
    /// Returns one `Vec<Block>` of child views per binding set, in order.
    /// Child expressions (and `${…}` interpolation) resolve the bindings,
    /// shadowing like an inner `let`; nested components / repeaters stack
    /// their own frames and compose. This is the component/repeater
    /// analogue of the `@children` splice's `computed_children`.
    pub fn expand_bodies(
        &self,
        body: &Block<'a>,
        binding_sets: Vec<std::sync::Arc<Vec<(String, Value)>>>,
    ) -> Vec<Vec<Block<'a>>> {
        let ItemCellKind::Block { expansions, .. } = &self.cells.kind else {
            return Vec::new();
        };
        let groups = expansions.get_or_init(|| {
            binding_sets
                .into_iter()
                .map(|set| crate::doc::cells::Expansion {
                    bindings: set,
                    // Fresh cells matching `body.ast`'s structure; only
                    // the cells are kept (the clone feeds the builder).
                    cells: ItemCells::build(&ast::Item::Block(body.ast.clone()), None),
                })
                .collect()
        });
        let doc = self.doc;
        groups
            .iter()
            .map(|g| {
                let ItemCellKind::Block {
                    items: fresh_items, ..
                } = &g.cells.kind
                else {
                    return Vec::new();
                };
                let scope = body.scope.push(ScopeFrame {
                    ast: body.ast,
                    cells: &g.cells,
                    kind_override: body.kind_override,
                    bindings: Some(g.bindings.clone()),
                });
                iter_blocks(&body.ast.items, fresh_items, doc, scope).collect()
            })
            .collect()
    }

    /// Evaluated values for each label slot. Cached on first call; later
    /// calls return a clone of the cached `Vec`.
    pub fn labels(&self) -> Result<Vec<Value>, EvalError> {
        let (cell, _) = self.block_inner();
        // Evaluate labels in the block's own scope (not root) so an
        // interpolated `$"…${slot}…"` label resolves component/repeater
        // bindings. Bare identifiers still stay opaque literal names, and
        // plain literal labels are scope-independent, so this is
        // behaviour-preserving for every existing label form.
        let scope = self.scope.clone();
        let result = cell.get_or_init(|| {
            self.ast
                .labels
                .iter()
                .map(|e| self.doc.eval_literal_in_scope(e, &scope))
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
                system,
                base_dir,
                path_span,
                loaded,
            } = &cell.kind
            {
                let _ = loaded.get_or_init(|| {
                    load_import_lazily(
                        path,
                        base_dir.as_deref(),
                        *system,
                        *path_span,
                        self.doc.loader(),
                    )
                });
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

    /// Find a `let` binding named `name` declared directly in this
    /// block (or a block-level import). The let's value evaluates in
    /// this block's child scope, so it can reference sibling lets /
    /// fields and ancestors.
    pub(crate) fn find_let(&self, name: &str) -> Option<LetView<'a>> {
        let child_scope = self.child_scope();
        for src in self.realize_and_sources() {
            if let Some(l) = find_let(src.items, src.cells, name, self.doc, &child_scope) {
                return Some(l);
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
        let synth_scope = scope.clone();
        // Computed-children splices (`field = <list expr>` for a
        // `@children`/`@child` slot) appear here too, after the literal
        // nested blocks, so renderers that walk `blocks()` (e.g.
        // `render_list`, `render_column`) see generated children.
        let synth = self.computed_children();
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_blocks(src.items, src.cells, doc, scope.clone()))
            .chain(synth.iter().map(move |sc| Block {
                ast: &sc.block,
                cells: &sc.cells,
                doc,
                kind_override: Some(sc.kind.as_str()),
                scope: synth_scope.clone(),
            }))
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
        // Computed-children splice (`field = <list expr>`): the field's
        // declared `list<Union>` type already coerced each bare record to
        // a variant by shape (`Field::value`), so just splice them in.
        if let Some(field) = self.field(field_name)
            && let Ok(Value::List(items)) = field.value()
        {
            for it in items {
                if matches!(it, Value::Variant { .. }) {
                    out.push(it.clone());
                }
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
        // Computed children: a `field = <list expr>` splice for this
        // slot (see `computed_children`). Appended after the literal
        // nested blocks / table rows, preserving the "everything mixes,
        // in source order" rule (the splice runs last).
        for sc in self.computed_children() {
            if sc.field_name == field_name {
                out.push(Block {
                    ast: &sc.block,
                    cells: &sc.cells,
                    doc: self.doc,
                    kind_override: Some(sc.kind.as_str()),
                    scope: child_scope.clone(),
                });
            }
        }
        out
    }

    /// Lazily materialise the *computed children* of this block — the
    /// `@children(kind)` / `@child(kind)` slots authored as a value
    /// expression (`field = map(data, …)`) instead of nested block
    /// literals (a "splice"). Each list element becomes one
    /// value-backed synthetic `Block` of the slot's concrete kind,
    /// cached in the block cell so repeated projection / `blocks()` /
    /// validation passes reuse the same owned storage.
    ///
    /// Union-typed `@children(SomeUnion)` slots are **not** synthesised
    /// here — they're consumed as a coerced `Value::List` of variants by
    /// the value path (`dispatch_union_children` / `block_to_record`).
    /// Interface slots are skipped too: a bare record carries no kind
    /// tag, so a concrete child kind can't be inferred for them.
    pub(crate) fn computed_children(&self) -> &'a [crate::doc::cells::SynthChild] {
        let ItemCellKind::Block {
            computed_children, ..
        } = &self.cells.kind
        else {
            return &[];
        };
        computed_children
            .get_or_init(|| self.build_computed_children())
            .as_slice()
    }

    fn build_computed_children(&self) -> Vec<crate::doc::cells::SynthChild> {
        let mut out = Vec::new();
        let Some(schema) = self.schema() else {
            return out;
        };
        for f in schema.fields() {
            // Only concrete-kind @children / @child slots.
            let (kind, is_list) = if let Some(k) = f.children_block_kind() {
                (k, true)
            } else if let Some(k) = f.child_block_kind() {
                (k, false)
            } else {
                continue;
            };
            // A literal `field = expr` present? (Nested-block / table-row
            // authoring leaves no `Item::Field` of this name, so a hit
            // here means the splice form.)
            let Some(field) = self.field(f.name()) else {
                continue;
            };
            let Ok(value) = field.value() else {
                continue;
            };
            match value {
                Value::List(items) if is_list => {
                    for el in items {
                        if let Some(sc) = synth_child_from_value(self.doc, f.name(), &kind, el) {
                            out.push(sc);
                        }
                    }
                }
                // A single-block `@child` slot: the value is one element.
                single if !is_list && !matches!(single, Value::None) => {
                    if let Some(sc) = synth_child_from_value(self.doc, f.name(), &kind, single) {
                        out.push(sc);
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
