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
    build_effective_fields, build_merged_decorators, is_descendant_of_walk, lookup_effective_field,
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

/// Join the contiguous run of line comments immediately above a
/// declaration (its `leading_trivia`) into a single doc-comment string.
/// Walks from the end so only the block touching the declaration counts —
/// a comment separated from the declaration by a blank line is unrelated
/// and dropped. Each line is trimmed; returns `None` when no attached
/// comment is present. Backs the `doc_comment` reflection builtin.
pub(crate) fn doc_comment_from_trivia(trivia: &[ast::Trivia]) -> Option<String> {
    let mut lines: Vec<&str> = Vec::new();
    for t in trivia.iter().rev() {
        match t {
            ast::Trivia::LineComment(s) => lines.push(s.trim()),
            ast::Trivia::BlankLine => break,
        }
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(lines.join("\n"))
}

/// How deep a `@contextual` block's expansion may nest before
/// [`Block::expand_children`] stops descending. Bounds a self-referential
/// expansion, whose block tree is otherwise unbounded; mirrors the
/// renderer-side lowering guard.
const MAX_EXPANSION_DEPTH: usize = 32;

/// Closed set of decorator names the document layer special-cases:
/// schema dispatch (`@block`, `@table`, `@document`, `@decorator`),
/// field shape (`@inline`, `@default`, `@child`, `@children`),
/// connection decomposition (`@connections`), per-block schema opt-out
/// (`@schemaless`), context-decided placement (`@contextual`), and the
/// host-neutral nested slot-type marker (`@block_slot`).
/// User-defined decorators are matched by their declared name and don't
/// go through this enum.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum BuiltinDecorator {
    Block,
    Table,
    Document,
    Decorator,
    Schemaless,
    Contextual,
    BlockSlot,
    DeclaresKind,
    Inline,
    Default,
    Child,
    Children,
    Connections,
    Dynamic,
    Ref,
    ByRef,
}

impl BuiltinDecorator {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            BuiltinDecorator::Block => "block",
            BuiltinDecorator::Table => "table",
            BuiltinDecorator::Document => "document",
            BuiltinDecorator::Decorator => "decorator",
            BuiltinDecorator::Schemaless => "schemaless",
            BuiltinDecorator::Contextual => "contextual",
            BuiltinDecorator::BlockSlot => "block_slot",
            BuiltinDecorator::DeclaresKind => "declares_kind",
            BuiltinDecorator::Inline => "inline",
            BuiltinDecorator::Default => "default",
            BuiltinDecorator::Child => "child",
            BuiltinDecorator::Children => "children",
            BuiltinDecorator::Connections => "connections",
            BuiltinDecorator::Dynamic => "dynamic",
            BuiltinDecorator::Ref => "ref",
            BuiltinDecorator::ByRef => "by_ref",
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

/// How many alias links a walk down an alias chain will peel before
/// giving up — the cap that stops a cycle (`type A = B  type B = A`,
/// which parses) from hanging the walk. Shared by every walker, so the
/// document can't disagree with itself about how deep an alias goes.
pub(crate) const ALIAS_DEPTH: u8 = 8;

/// The **shape** of a schema field's declared type: what a consumer needs
/// in order to decide how to treat the field, without printing the type
/// and matching on the text.
///
/// [`ResolvedType`] answers "what does this name point at", one link at a
/// time. A shape answers the question a form, a graph or a validator
/// actually asks — is this field one scalar, a list of them, a closed
/// vocabulary, a nested block, a function? Two differences from rendering
/// [`TypeField::type_ref`] and comparing the text matter, because both of
/// them fail *silently* — the field simply stops being whatever the
/// consumer was classifying it as:
///
/// - **Aliases are transparent.** A field declared `id: NodeId`, where
///   `type NodeId = identifier`, has the shape `Scalar(Identifier)` —
///   what the field *is*. Its rendering is `NodeId`, which matches no
///   string a consumer would have thought to write.
/// - **Names can't collide with syntax.** A field typed by a declaration
///   named `fnord` is a `Block`; `starts_with("fn")` on the rendering
///   says it is a function.
#[derive(Debug, Clone)]
pub enum FieldShape<'a> {
    /// One builtin scalar — `utf8`, `identifier`, `bool`, `u32`, …
    Scalar(BuiltinType),
    /// A closed vocabulary (`symbol_set`).
    Symbols(SymbolSetDecl<'a>),
    /// A declared record / block type.
    Block(TypeDecl<'a>),
    /// A declared `interface`.
    Interface(InterfaceDecl<'a>),
    /// A declared `union`.
    Union(UnionDecl<'a>),
    /// A declared `connection`.
    Connection(ConnectionDecl<'a>),
    /// `list<T>`, carrying the element's shape.
    List(Box<FieldShape<'a>>),
    /// `&T`, carrying the referent's shape.
    Reference(Box<FieldShape<'a>>),
    /// `tensor<T, [...]>`, carrying the element's shape.
    Tensor(Box<FieldShape<'a>>),
    /// A function-valued field. The signature is not part of the shape —
    /// ask [`TypeField::resolved_type`] for it.
    Function,
    /// An alias chain too long or too tangled to peel ([`ALIAS_DEPTH`]),
    /// carrying the link the walk stopped on. The declaration is an
    /// alias, so it is not a [`Block`](FieldShape::Block); what it stands
    /// for is unknown, so it is nothing else either. Saying so is the
    /// point — a shape that guessed here would misclassify the field in
    /// exactly the silence this type exists to end.
    Unresolved(TypeDecl<'a>),
}

impl<'a> FieldShape<'a> {
    /// The builtin behind a scalar field; `None` for every other shape,
    /// containers included. "This field holds one builtin scalar" is the
    /// question — a `list<utf8>` is not a `utf8`.
    pub fn builtin(&self) -> Option<BuiltinType> {
        match self {
            FieldShape::Scalar(b) => Some(*b),
            _ => None,
        }
    }

    /// The element shape of a `list<T>`; `None` for every other shape.
    pub fn list_element(&self) -> Option<&FieldShape<'a>> {
        match self {
            FieldShape::List(inner) => Some(inner),
            _ => None,
        }
    }

    /// Whether the field holds a function.
    pub fn is_function(&self) -> bool {
        matches!(self, FieldShape::Function)
    }

    /// Read a resolved type as a shape, peeling transparent type aliases
    /// (`type Port = u16`) as it goes. Only alias links spend `depth`:
    /// `list<list<Port>>` is as peelable as `Port`.
    fn from_resolved(doc: &'a Document, ty: ResolvedType<'a>, depth: u8) -> Self {
        let nest = |inner: Box<ResolvedType<'a>>| Box::new(Self::from_resolved(doc, *inner, depth));
        match ty {
            ResolvedType::Builtin(b) => FieldShape::Scalar(b),
            ResolvedType::SymbolSet(ss) => FieldShape::Symbols(ss),
            ResolvedType::Interface(i) => FieldShape::Interface(i),
            ResolvedType::Union(u) => FieldShape::Union(u),
            ResolvedType::Connection(c) => FieldShape::Connection(c),
            ResolvedType::Named(d) => match d.ast.alias.as_ref() {
                // The alias target resolves in the ALIAS's namespace, not
                // the field's — `type Port = u16` means what it meant
                // where it was written.
                Some(_) if depth == 0 => FieldShape::Unresolved(d),
                Some(target) => {
                    Self::from_resolved(doc, doc.resolve_in(target, d.file_ns), depth - 1)
                }
                None => FieldShape::Block(d),
            },
            ResolvedType::List(inner) => FieldShape::List(nest(inner)),
            ResolvedType::Reference(inner) => FieldShape::Reference(nest(inner)),
            ResolvedType::Tensor { element, .. } => FieldShape::Tensor(nest(element)),
            ResolvedType::Function { .. } => FieldShape::Function,
        }
    }
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
        self.fqn_segments().join(".")
    }

    /// Fully-qualified name as segments: `file_ns + name_segments`.
    fn fqn_segments(&self) -> Vec<String> {
        self.file_ns()
            .iter()
            .chain(self.name_segments().iter())
            .cloned()
            .collect()
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
        iter_decorators(
            &self.ast.decorators,
            &self.cells.decorators,
            self.doc,
            self.file_ns,
        )
    }

    /// The doc comment (contiguous `#` / `//` lines) directly above this
    /// union declaration, or `None`.
    pub fn doc_comment(&self) -> Option<String> {
        doc_comment_from_trivia(&self.ast.leading_trivia)
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this union declaration.
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::UnionDecl(self.ast.clone()))
    }

    pub fn variants(&self) -> impl Iterator<Item = UnionVariant<'a>> + 'a {
        let doc = self.doc;
        let variant_cells = self.variant_decorator_cells();
        let field_cells = self.variant_field_cells();
        let file_ns = self.file_ns;
        self.ast
            .variants
            .iter()
            .enumerate()
            .map(move |(i, v)| UnionVariant {
                ast: v,
                decorator_cells: &variant_cells[i],
                field_decorator_cells: &field_cells[i],
                doc,
                file_ns,
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
                file_ns: self.file_ns,
            })
    }
}

#[derive(Clone, Copy)]
pub struct UnionVariant<'a> {
    ast: &'a ast::UnionVariant,
    decorator_cells: &'a [DecoratorCell],
    field_decorator_cells: &'a [Vec<DecoratorCell>],
    doc: &'a Document,
    /// Namespace of the union that declares this variant — propagated to
    /// the variant's record fields for namespace-relative resolution.
    file_ns: &'a [String],
}

impl<'a> UnionVariant<'a> {
    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        iter_decorators(
            &self.ast.decorators,
            self.decorator_cells,
            self.doc,
            self.file_ns,
        )
    }

    /// The doc comment (contiguous `#` / `//` lines) directly above this
    /// variant, or `None`.
    pub fn doc_comment(&self) -> Option<String> {
        doc_comment_from_trivia(&self.ast.leading_trivia)
    }

    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this union variant.
    pub fn to_source(&self) -> String {
        crate::format::to_source_union_variant(self.ast)
    }

    pub fn body(&self) -> VariantBodyView<'a> {
        match &self.ast.body {
            ast::VariantBody::Record { .. } => VariantBodyView::Record,
            ast::VariantBody::TypeRef { ty, .. } => VariantBodyView::TypeRef(ty),
            ast::VariantBody::InterfaceRef { iface, .. } => VariantBodyView::InterfaceRef(iface),
            ast::VariantBody::Unit => VariantBodyView::Unit,
        }
    }

    pub fn fields(&self) -> Box<dyn Iterator<Item = TypeField<'a>> + 'a> {
        let doc = self.doc;
        let field_cells = self.field_decorator_cells;
        match &self.ast.body {
            ast::VariantBody::Record { fields, .. } => {
                let file_ns = self.file_ns;
                Box::new(fields.iter().enumerate().map(move |(i, f)| TypeField {
                    ast: f,
                    decorator_cells: &field_cells[i],
                    doc,
                    file_ns,
                }))
            }
            _ => Box::new(std::iter::empty()),
        }
    }

    pub fn field(&self, name: &str) -> Option<TypeField<'a>> {
        match &self.ast.body {
            ast::VariantBody::Record { fields, .. } => fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.name == name)
                .map(|(i, f)| TypeField {
                    ast: f,
                    decorator_cells: &self.field_decorator_cells[i],
                    doc: self.doc,
                    file_ns: self.file_ns,
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

fn resolve_child_kind_arg<'a>(
    doc: &'a Document,
    file_ns: &[String],
    positional: &[Value],
) -> Option<ChildKind<'a>> {
    let first = positional.first()?;
    match first {
        Value::Utf8(s) | Value::Ascii(s) => Some(ChildKind::Kind(s.clone())),
        Value::Identifier(name) => {
            // Resolve the referenced union/interface relative to the
            // declaring field's namespace (then aliases/absolute), so a
            // stdlib `@children(ContentBlock)` under `namespace wdoc` finds
            // `wdoc.ContentBlock`.
            let resolved = doc.resolve_path_in(std::slice::from_ref(name), file_ns);
            let key = resolved
                .map(|p| p.join("."))
                .unwrap_or_else(|| name.clone());
            if let Some(u) = doc.union_decl(&key) {
                return Some(ChildKind::Union(u));
            }
            if let Some(i) = doc.interface(&key) {
                return Some(ChildKind::Interface(i));
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
        Value::Record { fields, .. } => fields.as_ref().clone(),
        Value::Variant {
            payload: VariantPayload::Record(m),
            ..
        } => m.as_ref().clone(),
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
                trailing_comment: None,
            })
        })
        .collect();
    let synth_block = ast::Block {
        kind: String::new(),
        kind_ns: Vec::new(),
        conditional: false,
        slot_decl: None,
        labels: vec![ast::Expr::None; label_len],
        items,
        decorators: Vec::new(),
        span: synth_span,
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
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
    /// Namespace of the source that carries this decorator. Bare names
    /// resolve relative to their use site, not the root document.
    file_ns: &'a [String],
}

fn iter_decorators<'a>(
    ast: &'a [ast::Decorator],
    cells: &'a [DecoratorCell],
    doc: &'a Document,
    file_ns: &'a [String],
) -> impl Iterator<Item = Decorator<'a>> + 'a {
    ast.iter().zip(cells).map(move |(ast, cell)| Decorator {
        ast,
        cell,
        doc,
        file_ns,
    })
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

    /// Span of the dotted decorator name, excluding `@` and arguments.
    pub fn name_span(&self) -> Span {
        self.ast.name_span
    }

    /// Source spans index-aligned with this decorator's positional arguments.
    pub fn positional_spans(&self) -> &'a [Span] {
        &self.ast.positional_spans
    }

    /// The decorator schema selected by this decorator's authored name.
    /// A dotted prefix is a namespace qualifier; a bare name prefers this
    /// decorator's use-site namespace before the root and first match.
    pub fn schema(&self) -> Option<TypeDecl<'a>> {
        let (name, qualifier) = self.ast.name.split_last()?;
        self.doc.decorator_schema_in(qualifier, name, self.file_ns)
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
    /// `@inline(N)` index — so `@block("books")` resolves the `name`
    /// slot from positional[0] when no `name = ...` was written. A slot
    /// without `@inline` is named-only.
    ///
    /// If neither a named nor positional argument fills the slot, its
    /// declared default is returned. Returns `None` when the decorator has
    /// no registered schema, the schema doesn't declare the named slot, or
    /// the slot is absent and has no default.
    pub fn resolved_arg_value(&self, slot_name: &str) -> Option<Result<Value, EvalError>> {
        let schema = self.schema()?;
        let slot = schema.field(slot_name)?;
        // Resolve a union slot from the decorator schema's namespace, not
        // the decorator use site's. Two libraries may both call the union
        // `Choice`, just as they may both declare this decorator name.
        if let ResolvedType::Union(union) = slot.resolved_type() {
            return Some(self.dispatch_into_union(union));
        }
        if let Some(v) = self.named_arg(slot_name) {
            return Some(v);
        }
        let slot_idx = slot.inline_slot()? as usize;
        let positional = match self.positional() {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        positional
            .into_iter()
            .nth(slot_idx)
            .map(Ok)
            .or_else(|| slot.default_value().map(Ok))
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

    /// `true` when the connection carries the `@dynamic` decorator,
    /// opting its endpoints into dynamic resolution: a `->` operand
    /// that doesn't name a literal block (e.g. an id generated by a
    /// `@contextual` block's expansion) is projected as a raw id
    /// rather than dropped, and `wcl check` won't flag it. Mirrors
    /// `Decorator::is` (single-segment builtin name match) so we don't
    /// need decorator cells on this view.
    pub fn is_dynamic(&self) -> bool {
        self.ast
            .decorators
            .iter()
            .any(|d| d.name.len() == 1 && d.name[0] == BuiltinDecorator::Dynamic.as_str())
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
        iter_decorators(
            &self.ast.decorators,
            &self.cells.decorators,
            self.doc,
            self.file_ns,
        )
    }

    /// The doc comment (contiguous `#` / `//` lines) directly above this
    /// symbol-set declaration, or `None`.
    pub fn doc_comment(&self) -> Option<String> {
        doc_comment_from_trivia(&self.ast.leading_trivia)
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this symbol-set declaration.
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::SymbolSetDecl(self.ast.clone()))
    }

    pub fn symbols(&self) -> impl Iterator<Item = SymbolEntry<'a>> + 'a {
        let doc = self.doc;
        let cells = self.symbol_decorator_cells();
        let file_ns = self.file_ns;
        self.ast
            .symbols
            .iter()
            .enumerate()
            .map(move |(i, s)| SymbolEntry {
                ast: s,
                decorator_cells: &cells[i],
                doc,
                file_ns,
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
    file_ns: &'a [String],
}

impl<'a> SymbolEntry<'a> {
    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        iter_decorators(
            &self.ast.decorators,
            self.decorator_cells,
            self.doc,
            self.file_ns,
        )
    }

    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this symbol-set entry.
    pub fn to_source(&self) -> String {
        crate::format::to_source_symbol_entry(self.ast)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TypeDecl<'a> {
    pub(super) ast: &'a ast::TypeDecl,
    pub(super) file_ns: &'a [String],
    pub(super) cells: &'a ItemCells,
    pub(super) doc: &'a Document,
    /// `true` when this declaration comes from an imported source
    /// (a disk or system/registry import) rather than the root
    /// document. Drives document-schema composition: a root-authored
    /// `@document` is authoritative, imported ones are library
    /// defaults that merge in. See `Document::schema_errors`.
    pub(super) is_imported: bool,
    /// `true` when this declaration was *derived* from a
    /// `@declares_kind` instance rather than written as a type — see
    /// [`Document::block_schema`](crate::Document::block_schema). A
    /// derived schema describes the declarer's params and nothing else,
    /// so the child walk stops at an instance of it.
    pub(super) is_derived: bool,
}

/// The `@declares_kind(...)` contract a `@block` type carries: its
/// *instances* declare new block kinds, which the language schemas by
/// deriving a type from each instance's params.
///
/// The decorator's name belongs to the language; its use belongs to the
/// consumer — wdoc applies it to its own component type, and any host
/// declaring a template-like kind can do the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaresKind {
    /// `name = N` — the `@inline(N)` label slot of the declarer holding
    /// the declared kind's name. Defaults to `0`.
    pub name_slot: usize,
    /// `params = "field"` — the declarer field holding its param blocks
    /// (a `@children(K)` slot). Each param becomes one field of the
    /// derived schema.
    pub params_field: String,
    /// `body = "field"` — the declarer field holding the template body,
    /// if it names one. The language never reads the body (expansion is
    /// the host's, through its [`Expander`](crate::Expander)); it is
    /// carried here so a consumer reads one contract, not two.
    pub body_field: Option<String>,
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
        iter_decorators(
            &self.ast.decorators,
            &self.cells.decorators,
            self.doc,
            self.file_ns,
        )
    }

    /// The doc comment (contiguous `#` / `//` lines) directly above this
    /// type declaration, or `None`.
    pub fn doc_comment(&self) -> Option<String> {
        doc_comment_from_trivia(&self.ast.leading_trivia)
    }

    /// The external decorator name declared by this schema, or `None` for
    /// an ordinary type declaration.
    pub fn decorator_name(&self) -> Option<String> {
        let dec = self
            .decorators()
            .find(|d| d.is(BuiltinDecorator::Decorator))?;
        match dec.resolved_arg_value("name")? {
            Ok(Value::Utf8(name) | Value::Ascii(name)) => Some(name),
            _ => None,
        }
    }

    /// Whether this decorator schema is legal at `position`. For block
    /// positions, `block_kind` narrows an authored non-empty `kinds` list.
    /// Missing or unreadable applicability metadata is treated as
    /// unrestricted so editor tooling never hides a valid completion due
    /// to a half-written schema.
    pub fn decorator_applies_to(&self, position: &str, block_kind: Option<&str>) -> bool {
        let Some(applies_to) = self.decorators().find(|d| d.full_name() == "applies_to") else {
            return true;
        };
        let read_arg = |name: &str| {
            applies_to
                .resolved_arg_value(name)
                .or_else(|| applies_to.named_arg(name))
                .and_then(Result::ok)
        };
        let Some(Value::List(on)) = read_arg("on") else {
            return true;
        };
        let position_allowed = on.iter().any(|value| {
            matches!(
                value,
                Value::Symbol(name)
                    | Value::Identifier(name)
                    | Value::Utf8(name)
                    | Value::Ascii(name)
                    if name == position
            )
        });
        if !position_allowed {
            return false;
        }
        if position != "block" {
            return true;
        }
        let Some(kind) = block_kind else {
            return true;
        };
        let Some(Value::List(kinds)) = read_arg("kinds") else {
            return true;
        };
        kinds.iter().any(|value| {
            matches!(
                value,
                Value::Utf8(name) | Value::Ascii(name) | Value::Identifier(name)
                    if name == kind
            )
        })
    }

    /// `true` when this type declaration carries `@schemaless` — its
    /// instances accept undeclared fields (and children) without a
    /// membership error, exactly as if every instance were itself marked
    /// `@schemaless`. Lets a dynamic, open block kind (whose forwarded
    /// fields can't be declared up front) be authored without
    /// per-instance annotation.
    pub(crate) fn is_schemaless(&self) -> bool {
        has_schemaless(&self.ast.decorators)
    }

    /// `true` when this type declaration carries `@contextual` — its
    /// placement is decided by context rather than by kind. Such a block
    /// emits whatever its body generates once expanded (page content in a
    /// page, shapes in a diagram, rows in a table), so it is legal
    /// wherever children are allowed at all and its body is not recursed
    /// into by the child walk. Its generated children come from the
    /// host's [`Expander`](crate::Expander) — see
    /// [`Block::expand_children`].
    pub(crate) fn is_contextual(&self) -> bool {
        crate::doc::schema_check::has_contextual(&self.ast.decorators)
    }

    /// `true` when this type declaration carries `@by_ref`. When a block of
    /// this kind sits in a `@child`/`@children` slot of a block being
    /// reified to a record value, the slot reifies to a resolvable
    /// `Value::DataPath` reference instead of inlining the block's content.
    /// Lets renderable content (e.g. wdoc's `body`) ride on a data record as
    /// a property and be projected elsewhere by reference.
    pub(crate) fn is_by_ref(&self) -> bool {
        crate::doc::schema_check::has_by_ref(&self.ast.decorators)
    }

    /// The [`DeclaresKind`] contract this type carries, if any: its
    /// instances declare block kinds of their own, schema'd by the type
    /// the language derives from each instance's params.
    ///
    /// `None` for the overwhelming majority of types. `params` is
    /// required — a declarer with no param field declares kinds nothing
    /// can be said about, which is indistinguishable from not declaring
    /// any.
    pub fn declares_kind(&self) -> Option<DeclaresKind> {
        let decs: Vec<_> = self.decorators().collect();
        let dec = find_builtin_dec(&decs, BuiltinDecorator::DeclaresKind)?;
        let string_arg = |name: &str| match dec.resolved_arg_value(name) {
            Some(Ok(Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s))) => Some(s),
            _ => None,
        };
        let name_slot = match dec.resolved_arg_value("name") {
            Some(Ok(v)) => v.as_u64().unwrap_or(0) as usize,
            _ => 0,
        };
        Some(DeclaresKind {
            name_slot,
            params_field: string_arg("params")?,
            body_field: string_arg("body"),
        })
    }

    /// `true` when this schema was derived from a `@declares_kind`
    /// instance rather than written as a type declaration. Such a schema
    /// is reachable only through kind lookup — it is deliberately absent
    /// from [`type_decls`](crate::Document::type_decls), which walks
    /// what the document *declares*.
    pub fn is_derived(&self) -> bool {
        self.is_derived
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// `true` when this type declaration was loaded from an import
    /// rather than authored in the root document.
    pub fn is_imported(&self) -> bool {
        self.is_imported
    }

    /// Pretty-printed source for this type declaration.
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::TypeDecl(self.ast.clone()))
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
        self.block_string_list("required_children")
    }

    /// `required_fields = ["name", ...]` named arg on the type's
    /// `@block(...)` decorator. Each listed field must be written in
    /// any instance of this type. The twin of
    /// [`required_children`](Self::required_children) for fields: the
    /// language does not otherwise require a declared field to be
    /// supplied, so a schema that means it says so. Non-string entries
    /// in the list are silently dropped.
    pub fn required_fields(&self) -> Vec<String> {
        self.block_string_list("required_fields")
    }

    /// The string entries of a list-valued named arg on this type's
    /// `@block(...)` decorator — the shape both `required_children` and
    /// `required_fields` read. Absent decorator, absent arg, erroring
    /// arg and non-string entries all yield nothing.
    fn block_string_list(&self, arg_name: &str) -> Vec<String> {
        let decs: Vec<_> = self.decorators().collect();
        let Some(dec) = find_builtin_dec(&decs, BuiltinDecorator::Block) else {
            return Vec::new();
        };
        let Some(Ok(Value::List(items))) = dec.named_arg(arg_name) else {
            return Vec::new();
        };
        std::sync::Arc::unwrap_or_clone(items)
            .into_iter()
            .filter_map(|v| match v {
                Value::Utf8(s) | Value::Ascii(s) => Some(s),
                _ => None,
            })
            .collect()
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
        let file_ns = self.file_ns;
        self.ast
            .fields
            .iter()
            .enumerate()
            .map(move |(i, f)| TypeField {
                ast: f,
                decorator_cells: &cells[i],
                doc,
                file_ns,
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
                file_ns: self.file_ns,
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
        build_effective_fields(self.doc, &self.ast.extends, self.file_ns, self.fields())
    }

    /// Each field's decorators merged with those of any same-named field
    /// inherited via `extends` (own wins per-decorator). Lets a redeclared
    /// interface field inherit the interface's `@doc` / `@hidden`.
    pub fn merged_field_decorators(&self) -> std::collections::HashMap<String, Vec<Decorator<'a>>> {
        build_merged_decorators(self.doc, &self.ast.extends, self.file_ns, self.fields())
    }

    /// Like `effective_fields()` but optimised for a one-shot
    /// lookup. Returns the resolved `TypeField` for the named field
    /// considering the full extends chain.
    pub fn effective_field(&self, name: &str) -> Option<TypeField<'a>> {
        lookup_effective_field(
            self.doc,
            &self.ast.extends,
            self.file_ns,
            |n| self.field(n),
            name,
        )
    }

    /// `true` if `other` appears anywhere in `self`'s transitive
    /// `extends` chain. Used by the reference-acceptance check.
    pub fn is_descendant_of(&self, other_fqn: &str) -> bool {
        let mut seen: HashSet<String> = HashSet::new();
        is_descendant_of_walk(
            self.doc,
            &self.ast.extends,
            self.file_ns,
            other_fqn,
            &mut seen,
        )
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
        iter_decorators(
            &self.ast.decorators,
            &self.cells.decorators,
            self.doc,
            self.file_ns,
        )
    }

    /// The doc comment (contiguous `#` / `//` lines) directly above this
    /// interface declaration, or `None`.
    pub fn doc_comment(&self) -> Option<String> {
        doc_comment_from_trivia(&self.ast.leading_trivia)
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this interface declaration.
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::InterfaceDecl(self.ast.clone()))
    }

    pub fn fields(&self) -> impl Iterator<Item = TypeField<'a>> + 'a {
        let doc = self.doc;
        let cells = self.field_decorator_cells();
        let file_ns = self.file_ns;
        self.ast
            .fields
            .iter()
            .enumerate()
            .map(move |(i, f)| TypeField {
                ast: f,
                decorator_cells: &cells[i],
                doc,
                file_ns,
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
                file_ns: self.file_ns,
            })
    }

    /// Names of parent types/interfaces this interface extends.
    pub fn extends(&self) -> &'a [Vec<String>] {
        &self.ast.extends
    }

    pub fn effective_fields(&self) -> Vec<TypeField<'a>> {
        build_effective_fields(self.doc, &self.ast.extends, self.file_ns, self.fields())
    }

    /// See [`TypeDecl::merged_field_decorators`].
    pub fn merged_field_decorators(&self) -> std::collections::HashMap<String, Vec<Decorator<'a>>> {
        build_merged_decorators(self.doc, &self.ast.extends, self.file_ns, self.fields())
    }

    pub fn effective_field(&self, name: &str) -> Option<TypeField<'a>> {
        lookup_effective_field(
            self.doc,
            &self.ast.extends,
            self.file_ns,
            |n| self.field(n),
            name,
        )
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
    /// Namespace of the declaration (type / interface / union variant)
    /// that owns this field. Type references in the field's decorators
    /// (`@child`/`@children`/`@connections`) and its declared type
    /// resolve relative to this namespace first.
    pub(super) file_ns: &'a [String],
}

impl<'a> TypeField<'a> {
    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        iter_decorators(
            &self.ast.decorators,
            self.decorator_cells,
            self.doc,
            self.file_ns,
        )
    }

    /// The doc comment (contiguous `#` / `//` lines) directly above this
    /// field, or `None`.
    pub fn doc_comment(&self) -> Option<String> {
        doc_comment_from_trivia(&self.ast.leading_trivia)
    }

    /// The field's declared type, resolved relative to the namespace of the
    /// declaration that owns it. Prefer this over
    /// `doc.resolve(field.type_ref())`, which resolves from the document's
    /// root namespace and so can answer a same-named type from another
    /// namespace (`wdoc.Container` for a `wcl.wad` field typed `Container`).
    pub fn resolved_type(&self) -> ResolvedType<'a> {
        self.doc.resolve_in(&self.ast.ty, self.file_ns)
    }

    /// The field's [`FieldShape`] — the typed answer to "what kind of
    /// thing does this field hold", resolved in the declaring namespace
    /// with type aliases peeled. Prefer this over matching on
    /// `type_ref().to_string()`, which an alias or a type whose name
    /// starts with `fn` defeats without saying so.
    pub fn shape(&self) -> FieldShape<'a> {
        FieldShape::from_resolved(self.doc, self.resolved_type(), ALIAS_DEPTH)
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
        resolve_child_kind_arg(self.doc, self.file_ns, &dec.positional().ok()?)
    }

    /// Resolves the positional arg of `@children(...)` into either a
    /// string kind or a union declaration. `None` when the decorator
    /// is absent or the arg is neither.
    pub fn children_kind_or_union(&self) -> Option<ChildKind<'a>> {
        let dec = self
            .decorators()
            .find(|d| d.is(BuiltinDecorator::Children))?;
        resolve_child_kind_arg(self.doc, self.file_ns, &dec.positional().ok()?)
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
        // Resolve relative to the declaring field's namespace first.
        let resolved = self
            .doc
            .resolve_path_in(std::slice::from_ref(name), self.file_ns);
        let key = resolved
            .map(|p| p.join("."))
            .unwrap_or_else(|| name.clone());
        self.doc.connection_decl(&key)
    }

    /// If this field carries a `@ref("kind")` decorator, returns the
    /// block kind its value must reference (the id / first label of an
    /// existing block of that kind). `None` when the decorator is absent
    /// or its positional arg isn't a string kind. Drives the
    /// dangling-reference check in `wcl check`.
    pub fn ref_block_kind(&self) -> Option<String> {
        let dec = self.decorators().find(|d| d.is(BuiltinDecorator::Ref))?;
        match dec.positional().ok()?.first()? {
            Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => Some(s.clone()),
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

    /// Pretty-printed source for this type/interface field declaration.
    pub fn to_source(&self) -> String {
        crate::format::to_source_type_field(self.ast)
    }

    pub fn optional(&self) -> bool {
        self.ast.optional
    }

    pub fn type_ref(&self) -> &'a TypeRef {
        &self.ast.ty
    }

    /// FQN segments of this field's element type, peeling `list<…>` and
    /// `&…` wrappers and resolving the name in the declaring file's
    /// namespace (so a `@children("gizmo") gizmos: list<Gizmo>` under
    /// `namespace lib` yields `["lib", "Gizmo"]`). Falls back to the
    /// source-written segments when the name doesn't resolve. `None`
    /// for builtin / function / tensor element types.
    pub(crate) fn element_type_fqn_segments(&self) -> Option<Vec<String>> {
        fn peel(ty: &TypeRef) -> Option<&[String]> {
            match ty {
                TypeRef::Named { path: segs, .. } => Some(segs),
                TypeRef::List(inner) | TypeRef::Reference(inner) => peel(inner),
                _ => None,
            }
        }
        let segs = peel(self.type_ref())?;
        Some(
            self.doc
                .resolve_path_in(segs, self.file_ns)
                .unwrap_or_else(|| segs.to_vec()),
        )
    }
}

#[derive(Clone)]
pub struct Field<'a> {
    pub(super) ast: &'a ast::Field,
    pub(super) cells: &'a ItemCells,
    pub(super) doc: &'a Document,
    pub(super) file_ns: &'a [String],
    pub(super) scope: Scope<'a>,
}

impl<'a> Field<'a> {
    pub(in crate::doc) fn field_cell(&self) -> &'a FieldCell {
        let ItemCellKind::Field(c) = &self.cells.kind else {
            unreachable!("Field view wraps a Field cell")
        };
        c
    }

    /// `true` while this field's RHS is being evaluated higher up the
    /// (single-threaded) evaluation stack — see
    /// [`LetView::mid_evaluation`].
    pub(in crate::doc) fn mid_evaluation(&self) -> bool {
        let cell = self.field_cell();
        cell.value.get().is_none() && cell.evaluating.load(Ordering::Acquire)
    }

    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        iter_decorators(
            &self.ast.decorators,
            &self.cells.decorators,
            self.doc,
            self.file_ns,
        )
    }

    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Return the authored symbol when this field is exactly a symbol
    /// literal (`template = :book`). Computed expressions deliberately
    /// return `None`; hosts use this to avoid false-positive static checks.
    pub fn literal_symbol(&self) -> Option<&'a str> {
        match &self.ast.expr {
            ast::Expr::Symbol(name) => Some(name.as_str()),
            _ => None,
        }
    }

    /// Pretty-printed source for this field (`name = expr`).
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::Field(self.ast.clone()))
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
            // resolves instead of being looked up at root. A string result
            // (quoted ref, interpolation) coerces to `Value::Identifier` so
            // `ref == id` joins hold regardless of authoring style.
            self.doc
                .eval_literal_in_scope(&self.ast.expr, &self.scope)
                .map(str_to_identifier)
        } else if matches!(
            self.declared_type_ref(),
            Some(TypeRef::List(inner)) if matches!(inner.as_ref(), TypeRef::Builtin(BuiltinType::Identifier))
        ) {
            // `list<identifier>` field: the element-wise lift of the rule
            // above. A bare-id list (`members = [shop, stripe]`) keeps each
            // name opaque so it can reference shapes by id, while a
            // data-derived expression still evaluates normally. String
            // elements coerce to identifiers like the scalar rule above.
            self.doc
                .eval_identifier_list_in_scope(&self.ast.expr, &self.scope)
                .map(|v| match v {
                    Value::List(items) => Value::List(std::sync::Arc::new(
                        std::sync::Arc::unwrap_or_clone(items)
                            .into_iter()
                            .map(str_to_identifier)
                            .collect(),
                    )),
                    other => other,
                })
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
        // A literal unit that reached here unresolved (e.g. an untyped
        // field, or a `&T`/identifier-typed field that doesn't coerce) has
        // no declared type to resolve against — that is an error, not a
        // silent `PendingUnit`. (A typed field already resolved or errored
        // in `coerce_value_to_type` above.)
        let result = result.and_then(|v| match v {
            Value::PendingUnit { unit, .. } => {
                Err(EvalError::unit_without_type(unit, self.ast.span))
            }
            other => Ok(other),
        });
        cell.evaluating.store(false, Ordering::Release);
        cell.value.get_or_init(|| result).as_ref()
    }

    /// Evaluate this field against a caller-supplied type instead of the
    /// schema-declared one, named by its fully-qualified name
    /// (`"std.Duration"`).
    ///
    /// The escape hatch for hosts whose blocks are `@schemaless`: a unit
    /// literal resolves against a *declared* type, so in a block that
    /// declares no fields `value()` can only report
    /// [`EvalError::UnitWithoutType`]. A host that knows out-of-band what
    /// a field means (say, from its own parameter schema) resolves it
    /// here.
    ///
    /// A value that isn't a unit literal passes through whatever coercion
    /// the named type implies, exactly as a declared field would. The
    /// result is *not* cached — `value()`'s cache keeps meaning "the
    /// schema-typed value" — so call this once per field and keep the
    /// result.
    pub fn value_typed(&self, type_fqn: &str) -> Result<Value, EvalError> {
        let ty = TypeRef::named(type_fqn.split('.').map(str::to_string).collect());
        let value = self.doc.eval_in_scope(&self.ast.expr, &self.scope)?;
        variant_dispatch::coerce_value_to_type(self.doc, value, &ty, self.ast.span)
    }

    /// `Some(err)` if this field's name isn't accepted by the
    /// applicable schema (parent block, or the document if top-level).
    /// `None` means the membership check passes.
    fn schema_membership_error(&self) -> Option<EvalError> {
        use crate::error::SchemaViolationKind as Kind;
        match self.scope.frames().last().cloned() {
            Some(frame) => {
                // Whole-block opt-out shadows individual fields too.
                if has_schemaless(&frame.ast.decorators) {
                    return None;
                }
                let block = Block {
                    ast: frame.ast,
                    cells: frame.cells,
                    doc: self.doc,
                    file_ns: frame.file_ns,
                    kind_override: frame.kind_override,
                    scope: Scope::root(),
                };
                match block.schema() {
                    // A `@schemaless` type declaration opens all its
                    // instances — every undeclared field passes, with no
                    // per-instance annotation.
                    Some(schema) if schema.is_schemaless() => None,
                    Some(schema) if schema.field(self.name()).is_some() => None,
                    Some(schema) => Some(EvalError::schema_violation(
                        Kind::UnknownField,
                        format!(
                            "field '{}' is not declared by schema '{}'",
                            self.name(),
                            schema.name()
                        ),
                        self.ast.span,
                    )),
                    // Inside an un-schema'd block — the enclosing
                    // block's UnregisteredKind covers it.
                    None => None,
                }
            }
            None => {
                // Top-level field: validate against the *merged*
                // `@document` schema(s) governing this field's source
                // namespace. The field is fine if any of them declares
                // it (so a user's own root `@document` composes with
                // library schemas pulled in by imports).
                let field_ns = self.doc.find_field_source_ns(self.ast);
                let schemas = self.doc.doc_schemas_for_ns(field_ns);
                if schemas.is_empty() {
                    Some(EvalError::schema_violation(
                        Kind::NoDocumentSchema,
                        format!("top-level field '{}' has no @document schema", self.name()),
                        self.ast.span,
                    ))
                } else if schemas.declares_field(self.name()) {
                    None
                } else {
                    Some(EvalError::schema_violation(
                        Kind::UnknownField,
                        format!(
                            "field '{}' is not declared by @document schema '{}'",
                            self.name(),
                            schemas.names()
                        ),
                        self.ast.span,
                    ))
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
                file_ns: frame.file_ns,
                kind_override: frame.kind_override,
                scope: Scope::root(),
            };
            let schema = block.schema()?;
            let schema_field = schema.field(self.name())?;
            return Some(schema_field.type_ref());
        }
        // Top-level field: consult the merged @document schema(s) in
        // this field's source namespace, preferring a root-authored
        // declaration over an imported one.
        let field_ns = self.doc.find_field_source_ns(self.ast);
        let schema_field = self.doc.doc_schemas_for_ns(field_ns).field(self.name())?;
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
        if let TypeRef::Named { path, .. } = inner.as_ref()
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
    /// `true` while this let's RHS is being evaluated higher up the
    /// (single-threaded) evaluation stack. Used by `scope_lookup` to
    /// give `a = a` outward-shadowing semantics: a mid-evaluation match
    /// is skipped so the name resolves to an outer binding instead of
    /// the binding being defined.
    pub(crate) fn mid_evaluation(&self) -> bool {
        self.cell.value.get().is_none() && self.cell.evaluating.load(Ordering::Acquire)
    }

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
    /// Namespace declared by the file this block instance lexically
    /// lives in — the root document or the import that carries it.
    /// Synthesised blocks (table rows, computed-children splices,
    /// component bodies) inherit their owning declaration's. Bare-kind
    /// schema resolution prefers this namespace (see [`Block::schema`]).
    pub(super) file_ns: &'a [String],
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
        iter_decorators(
            &self.ast.decorators,
            &self.cells.decorators,
            self.doc,
            self.file_ns,
        )
    }

    pub fn kind(&self) -> &'a str {
        self.kind_override.unwrap_or(&self.ast.kind)
    }

    /// Whether this block was authored as a conditional bare-name fill
    /// (`aside? { ... }`). Hosts interpret conditionality against their
    /// resolved slot contract.
    pub fn is_conditional(&self) -> bool {
        self.ast.conditional
    }

    /// Declared type of a `slot name: Type` block.
    pub fn slot_type_ref(&self) -> Option<&'a TypeRef> {
        self.ast.slot_decl.as_ref().map(|slot| &slot.ty)
    }

    /// `true` for a declaration carrying `?` after its type.
    pub fn slot_optional(&self) -> bool {
        self.ast
            .slot_decl
            .as_ref()
            .is_some_and(|slot| slot.optional)
    }

    /// `true` for a declaration carrying `*` after its type.
    pub fn slot_repeated(&self) -> bool {
        self.ast
            .slot_decl
            .as_ref()
            .is_some_and(|slot| slot.repeated)
    }

    /// The document this block belongs to. Lets host renderers reach
    /// document-level lookups (e.g. `kind_declarer`) from a block view.
    pub fn doc(&self) -> &'a Document {
        self.doc
    }

    /// The miette source (name + text) of the file that declares this
    /// block — the root document or the imported file it lives in. Hosts
    /// use this to render an eval diagnostic raised while processing this
    /// block against the correct file's snippet (a cross-file span won't
    /// line up with the root source's text). Falls back to the root
    /// source for synthesised blocks not backed by on-disk AST.
    pub fn named_source(&self) -> miette::NamedSource<String> {
        self.doc.named_source_for_block(self.ast)
    }

    /// Scope that child expressions inside this block see — the
    /// block's own `scope` extended with one frame for itself.
    /// Read a memoised `typed_field` projection for this block's cells.
    /// Keyed by (kind, field name) so a synth-row cell viewed under a
    /// `kind_override` can't collide with another view of the same cell.
    fn typed_proj_memo_get(&self, name: &str) -> Option<Value> {
        let ItemCellKind::Block {
            typed_proj_memo, ..
        } = &self.cells.kind
        else {
            return None;
        };
        let memo = typed_proj_memo.read().ok()?;
        memo.get(&(self.kind().to_string(), name.to_string()))
            .cloned()
    }

    /// Store a `typed_field` projection in this block's cells.
    fn typed_proj_memo_insert(&self, name: &str, value: Value) {
        let ItemCellKind::Block {
            typed_proj_memo, ..
        } = &self.cells.kind
        else {
            return;
        };
        if let Ok(mut memo) = typed_proj_memo.write() {
            memo.insert((self.kind().to_string(), name.to_string()), value);
        }
    }

    /// `true` when this block's scope frame could bind `name`: a let /
    /// field / nested block kind / table header in its realized sources
    /// (own items + in-block imports), or a field on its schema
    /// (including extends-inherited ones, which `typed_field` projects).
    /// Over-approximate; built once per cell per viewed kind so
    /// `scope_lookup` can skip a frame's per-item scans on a miss.
    pub(crate) fn can_bind_name(&self, name: &str) -> bool {
        let ItemCellKind::Block { bindable_names, .. } = &self.cells.kind else {
            return true; // not a block cell — stay conservative
        };
        if let Ok(memo) = bindable_names.read()
            && let Some(set) = memo.get(self.kind())
        {
            return set.contains(name);
        }
        let mut set = std::collections::HashSet::new();
        for src in self.realize_and_sources() {
            for item in src.items {
                match item {
                    ast::Item::Let(l) => {
                        set.insert(l.name.clone());
                    }
                    ast::Item::Field(f) => {
                        set.insert(f.name.clone());
                    }
                    ast::Item::Block(b) => {
                        set.insert(b.kind.clone());
                    }
                    ast::Item::Table(t) => {
                        set.insert(t.field_name.clone());
                    }
                    _ => {}
                }
            }
        }
        if let Some(schema) = self.schema() {
            for f in schema.effective_fields() {
                set.insert(f.name().to_string());
            }
        }
        let contains = set.contains(name);
        if let Ok(mut memo) = bindable_names.write() {
            memo.insert(self.kind().to_string(), std::sync::Arc::new(set));
        }
        contains
    }

    pub(crate) fn child_scope(&self) -> Scope<'a> {
        self.scope.push(ScopeFrame {
            ast: self.ast,
            cells: self.cells,
            file_ns: self.file_ns,
            kind_override: self.kind_override,
            bindings: None,
            content: None,
            expansion_depth: 0,
        })
    }

    /// How deep `@contextual` expansion currently is at this block.
    /// [`Block::expand_children`] caps on this to stop a self-referential
    /// expansion from growing forever (iteration count doesn't inflate
    /// it: all elements of one repetition share a depth).
    ///
    /// This is the max of the frames' **dynamic** `expansion_depth`, not a
    /// count of binding frames: an instantiated body's expansion scope is
    /// rebuilt from the *declaration's* (shallow) lexical scope, so a
    /// frame count would stay constant across nested instantiations and
    /// the guard would never fire.
    pub fn binding_scope_depth(&self) -> usize {
        self.scope
            .frames()
            .iter()
            .map(|f| f.expansion_depth)
            .max()
            .unwrap_or(0)
    }

    /// Structural content supplied by the nearest renderer-driven component
    /// expansion, if any. Returned as block nodes so a placement marker can
    /// feed them back through the caller's normal recursion without lowering
    /// them to a value or rendered string first.
    pub fn structural_content(&self) -> Option<Vec<Block<'a>>> {
        self.structural_slot_content("content")
    }

    /// Structural block nodes supplied for the named slot by the nearest
    /// renderer-driven expansion. Unlike a global block-kind lookup, this is
    /// scoped to one instantiated holder, so two components may declare the
    /// same slot name with unrelated contracts.
    pub fn structural_slot_content(&self, name: &str) -> Option<Vec<Block<'a>>> {
        self.scope
            .frames()
            .iter()
            .rev()
            .find_map(|frame| frame.content.as_deref()?.get(name).cloned())
    }

    /// Expand `body`'s child blocks once per binding set, each under a
    /// scope carrying that set's `name → value` bindings **and a fresh
    /// copy of the body's evaluation cells**. The fresh cells are the
    /// crux: `Field::value` memoises in a per-cell `OnceLock`, so the same
    /// body AST evaluated under different bindings (repeated instances,
    /// repetition iterations) would otherwise collide on the first-seen
    /// value. Caching the fresh cells on `self`'s cell (the per-expansion
    /// owner — the `@contextual` block being expanded) gives each
    /// expansion an independent cache.
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
        self.expand_bodies_inner(body, binding_sets, None)
    }

    /// [`expand_bodies`](Self::expand_bodies) with structural content attached
    /// to every expanded descendant's scope. Used by component renderers so a
    /// placement marker can recover the instance's child block nodes even
    /// after recursion crosses a native wrapper or container.
    pub fn expand_bodies_with_content(
        &self,
        body: &Block<'a>,
        binding_sets: Vec<std::sync::Arc<Vec<(String, Value)>>>,
        content: std::rc::Rc<Vec<Block<'a>>>,
    ) -> Vec<Vec<Block<'a>>> {
        let slots = std::rc::Rc::new(std::collections::BTreeMap::from([(
            "content".to_string(),
            content.as_ref().clone(),
        )]));
        self.expand_bodies_inner(body, binding_sets, Some(slots))
    }

    /// [`expand_bodies`](Self::expand_bodies) with several independently
    /// named structural content slots attached to the expansion scope.
    pub fn expand_bodies_with_slots(
        &self,
        body: &Block<'a>,
        binding_sets: Vec<std::sync::Arc<Vec<(String, Value)>>>,
        slots: std::rc::Rc<std::collections::BTreeMap<String, Vec<Block<'a>>>>,
    ) -> Vec<Vec<Block<'a>>> {
        self.expand_bodies_inner(body, binding_sets, Some(slots))
    }

    fn expand_bodies_inner(
        &self,
        body: &Block<'a>,
        binding_sets: Vec<std::sync::Arc<Vec<(String, Value)>>>,
        content: Option<std::rc::Rc<std::collections::BTreeMap<String, Vec<Block<'a>>>>>,
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
                    // The body's children live in the component
                    // *definition's* file: a library component using its
                    // own namespaced kinds bare must resolve them there,
                    // not at the instantiation site.
                    file_ns: body.file_ns,
                    kind_override: body.kind_override,
                    bindings: Some(g.bindings.clone()),
                    content: content.clone(),
                    // One deeper than the *instance* (`self`), whose own
                    // scope carries the dynamic depth at the
                    // instantiation site — `body.scope` is the
                    // definition's lexical chain and carries none.
                    expansion_depth: self.binding_scope_depth() + 1,
                });
                iter_blocks(&body.ast.items, fresh_items, doc, body.file_ns, scope).collect()
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

    /// Pretty-printed source for this block.
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::Block(self.ast.clone()))
    }

    /// Realise any pending block-level imports, then return one
    /// `BlockSlice` for the block's own items plus one for each
    /// successfully-loaded import (transitively). The block's own
    /// slice is always element 0.
    pub(super) fn realize_and_sources(&self) -> Vec<BlockSlice<'a>> {
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
            file_ns: self.file_ns,
        }];
        push_loaded_imports(items_cells, &mut out);
        out
    }

    /// Slices spliced in by this block's in-block `import`s, excluding the
    /// block's own items (which `realize_and_sources` returns as element
    /// 0). Used by the `@child(ren)` projections to nest imported block
    /// instances after the block's own children — the own slice is handled
    /// separately because table-row interleaving and computed children are
    /// keyed to the block's own cell.
    fn imported_slices(&self) -> Vec<BlockSlice<'a>> {
        let mut all = self.realize_and_sources();
        if !all.is_empty() {
            all.remove(0);
        }
        all
    }

    pub fn field(&self, name: &str) -> Option<Field<'a>> {
        let child_scope = self.child_scope();
        for src in self.realize_and_sources() {
            if let Some(f) = find_field(
                src.items,
                src.cells,
                name,
                self.doc,
                src.file_ns,
                &child_scope,
            ) {
                return Some(f);
            }
        }
        None
    }

    pub fn block(&self, kind: &str) -> Option<Block<'a>> {
        let child_scope = self.child_scope();
        for src in self.realize_and_sources() {
            if let Some(b) = find_block(
                src.items,
                src.cells,
                kind,
                self.doc,
                src.file_ns,
                &child_scope,
            ) {
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
            .flat_map(move |src| iter_fields(src.items, src.cells, doc, src.file_ns, scope.clone()))
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block<'a>> + 'a {
        let doc = self.doc;
        let scope = self.child_scope();
        let synth_scope = scope.clone();
        let synth_ns = self.file_ns;
        // Computed-children splices (`field = <list expr>` for a
        // `@children`/`@child` slot) appear here too, after the literal
        // nested blocks, so renderers that walk `blocks()` (e.g.
        // `render_list`, `render_column`) see generated children.
        let synth = self.computed_children();
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_blocks(src.items, src.cells, doc, src.file_ns, scope.clone()))
            .chain(synth.iter().map(move |sc| Block {
                ast: &sc.block,
                cells: &sc.cells,
                doc,
                file_ns: synth_ns,
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

    /// The namespace qualifier written before this block's kind with
    /// `::` (`wdoc::process` → `["wdoc"]`). Empty for a bare kind and for
    /// synthesised blocks.
    pub fn kind_ns(&self) -> &'a [String] {
        &self.ast.kind_ns
    }

    /// Namespace declared by the file this block instance lives in.
    pub(crate) fn file_ns(&self) -> &'a [String] {
        self.file_ns
    }

    /// The schema (`TypeDecl`) for this block's `kind`, if any. Resolved
    /// namespace-aware: a `::` qualifier selects an explicit namespace,
    /// and a bare kind prefers a declaration in the namespace of the
    /// file this instance lives in (so a `namespace lib` data file's
    /// blocks resolve to `lib`'s schemas regardless of import order).
    pub fn schema(&self) -> Option<TypeDecl<'a>> {
        let k = self.kind();
        let q = self.kind_ns();
        let ctx = self.file_ns;
        self.doc
            .block_schema_in(q, k, ctx)
            .or_else(|| self.doc.table_schema_in(q, k, ctx))
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
        // statements through the named connection schema. Memoised per
        // (kind, name) — see `typed_proj_memo`.
        if let Some(conn_schema) = f.connection_schema() {
            if let Some(hit) = self.typed_proj_memo_get(name) {
                return Some(crate::data::DataRef::from_variant_value(hit));
            }
            let scope = self.child_scope();
            // Connection statements live in the block's own items and in
            // any in-block import it splices in; project across both.
            let mut values = Vec::new();
            for src in self.realize_and_sources() {
                values.extend(self.doc.project_connections(src.items, conn_schema, &scope));
            }
            let projected = Value::List(std::sync::Arc::new(values));
            self.typed_proj_memo_insert(name, projected.clone());
            return Some(crate::data::DataRef::from_variant_value(projected));
        }

        // Union-typed @children: dispatch every nested block / table
        // row to a Value::Variant via structural-shape matching.
        if let Some(crate::doc::ChildKind::Union(union)) = f.children_kind_or_union() {
            if let Some(Value::List(items)) = self.typed_proj_memo_get(name) {
                return Some(crate::data::DataRef::from_variant_value_list(
                    items.to_vec(),
                ));
            }
            let dr = self.dispatch_union_children(name, union);
            if let crate::data::DataKind::VariantValueList(items) = dr.inner() {
                self.typed_proj_memo_insert(name, Value::List(std::sync::Arc::new(items.clone())));
            }
            return Some(dr);
        }
        // Union-typed @child: dispatch the single matching nested block.
        if let Some(crate::doc::ChildKind::Union(union)) = f.child_kind_or_union() {
            if let Some(hit) = self.typed_proj_memo_get(name) {
                return Some(crate::data::DataRef::from_variant_value(hit));
            }
            let dr = self.dispatch_union_child(union);
            if let crate::data::DataKind::VariantValue(v) = dr.inner() {
                self.typed_proj_memo_insert(name, v.clone());
            }
            return Some(dr);
        }

        if let Some(kind) = f.children_block_kind_str() {
            // Use the projection: combines literal nested blocks of
            // this kind with synthesised blocks from `Item::Table`
            // rows under the matching field name.
            let blocks = match self.children_projection(name, kind) {
                Ok(blocks) => blocks,
                Err(e) => return Some(crate::data::DataRef::from_error(e)),
            };
            let is_table = self.doc.table_schema_in(&[], kind, self.file_ns).is_some();
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
        if let Some(slot) = f.inline_slot() {
            // Inline labels become a synthetic field — we don't have a `Field`
            // view for a label, so return the typed-field view. But when the
            // inline label was omitted and an explicit `name = value` field of
            // the same name is present, read that instead, so `block { text =
            // "x" }` works like the inline `block "x"`. Hosts wanting the label
            // value should access `block.labels()` directly.
            let has_label = self
                .labels()
                .map(|ls| (slot as usize) < ls.len())
                .unwrap_or(false);
            if !has_label && let Some(field) = self.field(name) {
                return Some(crate::data::DataRef::from_field(field));
            }
            return Some(crate::data::DataRef::new(crate::data::DataKind::TypeField(
                f,
            )));
        }
        // Plain schema field → look it up in literal block items. An
        // unset optional (or `@default`-carrying) field projects its
        // default / `none` so member access reads it as `none` rather
        // than failing with an unresolved-reference error — `??` and
        // `match` over `block.optional_field` then behave as authored.
        if let Some(field) = self.field(name) {
            return Some(crate::data::DataRef::from_field(field));
        }
        if f.optional() || f.default_value().is_some() {
            return Some(crate::data::DataRef::from_variant_value(
                f.default_value().unwrap_or(Value::None),
            ));
        }
        None
    }

    /// Reify this block as a `Value::Record`, so a `@children`/`@child`
    /// reference can be consumed as ordinary list/record data by builtins
    /// (`len`, `map`, …), arithmetic, and a repetition block's `each`.
    ///
    /// Schema-aware, mirroring the wdoc lowering reifier: each declared
    /// field is populated from its `@inline(N)` label slot, its
    /// `@children`/`@child` projection (recursively reified), a literal
    /// block field, a leaf typed projection, or the schema default — with
    /// missing optionals becoming `Value::None`. An un-schema'd block
    /// reifies its literal fields verbatim, keyed by its block kind.
    pub fn to_record_value(&self) -> Result<Value, EvalError> {
        self.to_record_value_at(&[])
    }

    /// [`to_record_value`](Self::to_record_value) with the document path
    /// that re-resolves this block from root (`base`). The path is only
    /// consulted to address `@by_ref` child slots: a `@child`/`@children`
    /// slot whose kind is marked `@by_ref` reifies to a
    /// `Value::DataPath { segments: base + [field, …] }` reference instead
    /// of inlining its content (so the referenced content — e.g. a wdoc
    /// `body` — can be projected elsewhere). Every other field reifies
    /// identically regardless of `base`, so a document with no `@by_ref`
    /// kinds produces byte-identical records. An empty `base` (the entry
    /// reifier couldn't address this block) still reifies normally; the
    /// reference it emits simply won't resolve, which the consumer handles.
    pub(crate) fn to_record_value_at(&self, base: &[String]) -> Result<Value, EvalError> {
        use std::collections::BTreeMap;
        let Some(schema) = self.schema() else {
            // Un-schema'd block: literal fields only.
            let mut map = BTreeMap::new();
            for field in self.fields() {
                map.insert(
                    field.name().to_string(),
                    field.value().cloned().map_err(|e| e.clone())?,
                );
            }
            return Ok(Value::Record {
                ty: vec![self.kind().to_string()],
                fields: std::sync::Arc::new(map),
            });
        };
        let labels = self.labels().unwrap_or_default();
        let mut map = BTreeMap::new();
        for f in schema.fields() {
            let name = f.name();
            let is_child_slot =
                f.children_kind_or_union().is_some() || f.child_kind_or_union().is_some();
            let val = if let Some(slot) = f.inline_slot() {
                // An `@inline(N)` field is normally the positional label slot,
                // but also honour an explicit `name = value` field of the same
                // name, so `node { text = "x" }` reads the same as `node "x"`.
                // Prefer the label when it was actually given.
                if let Some(v) = labels.get(slot as usize).cloned() {
                    v
                } else if let Some(field) = self.field(name) {
                    field.value().cloned().map_err(|e| e.clone())?
                } else {
                    f.default_value().unwrap_or(Value::None)
                }
            } else if is_child_slot {
                // Project the slot and recursively reify; a computed splice
                // is schema-completed exactly like statically-nested blocks.
                // Extend the address by this field name so a nested `@by_ref`
                // slot emits a root-resolvable reference.
                let mut child_base = base.to_vec();
                child_base.push(name.to_string());
                match self
                    .typed_field(name)
                    .and_then(|dr| dataref_to_value_at(&dr, &child_base))
                {
                    Some(v) => v?,
                    None => f.default_value().unwrap_or(Value::None),
                }
            } else if let Some(field) = self.field(name) {
                field.value().cloned().map_err(|e| e.clone())?
            } else if let Some(dr) = self.typed_field(name) {
                dr.value()
                    .unwrap_or_else(|_| f.default_value().unwrap_or(Value::None))
            } else {
                f.default_value().unwrap_or(Value::None)
            };
            map.insert(name.to_string(), val);
        }
        Ok(Value::Record {
            ty: vec![schema.name().to_string()],
            fields: std::sync::Arc::new(map),
        })
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
            for it in items.iter() {
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
                            file_ns: self.file_ns,
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
                                    file_ns: self.file_ns,
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
        // In-block imports splice their top-level block instances in as
        // nested children too; dispatch decides which (if any) variant
        // each matches by shape.
        for src in self.imported_slices() {
            for (item, cells) in src.items.iter().zip(src.cells.iter()) {
                if let ast::Item::Block(b) = item {
                    out.push((
                        UnionChildKind::Nested,
                        Block {
                            ast: b,
                            cells,
                            doc: self.doc,
                            file_ns: src.file_ns,
                            kind_override: None,
                            scope: child_scope.clone(),
                        },
                    ));
                }
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
                    file_ns: self.file_ns,
                    kind_override: None,
                    scope: child_scope.clone(),
                };
                if let Ok(v) = variant_dispatch::block_to_variant(self.doc, &blk, union) {
                    return crate::data::DataRef::from_variant_value(v);
                }
            }
        }
        // Fall back to in-block imports: the first imported block instance
        // that matches a variant wins.
        for src in self.imported_slices() {
            for (item, cells) in src.items.iter().zip(src.cells.iter()) {
                if let ast::Item::Block(b) = item {
                    let blk = Block {
                        ast: b,
                        cells,
                        doc: self.doc,
                        file_ns: src.file_ns,
                        kind_override: None,
                        scope: child_scope.clone(),
                    };
                    if let Ok(v) = variant_dispatch::block_to_variant(self.doc, &blk, union) {
                        return crate::data::DataRef::from_variant_value(v);
                    }
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
    ///
    /// Fails only when a `@contextual` child's generated blocks are
    /// demanded from a document opened without an expander — see
    /// [`Block::expand_children`].
    fn children_projection(
        &self,
        field_name: &str,
        kind: &'a str,
    ) -> Result<Vec<Block<'a>>, EvalError> {
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
                        file_ns: self.file_ns,
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
                                file_ns: self.file_ns,
                                kind_override: Some(kind),
                                scope: child_scope.clone(),
                            });
                        }
                    }
                }
                // A `@contextual` child expands here so its generated
                // blocks of the matching kind participate in the slot
                // exactly like literal ones — without this, data-driven
                // children inside a custom shape silently vanished from
                // `@children` projections (the lowering record).
                ast::Item::Block(b) => {
                    let view = Block {
                        ast: b,
                        cells,
                        doc: self.doc,
                        file_ns: self.file_ns,
                        kind_override: None,
                        scope: child_scope.clone(),
                    };
                    if view.is_contextual() {
                        push_generated_matching(&view, kind, &mut out)?;
                    }
                }
                _ => {}
            }
        }
        // In-block imports splice their top-level block instances of the
        // matching kind in as children, after the block's own nested
        // blocks / table rows (mirroring `blocks()` ordering).
        for src in self.imported_slices() {
            for (item, cells) in src.items.iter().zip(src.cells.iter()) {
                if let ast::Item::Block(b) = item
                    && b.kind == kind
                {
                    out.push(Block {
                        ast: b,
                        cells,
                        doc: self.doc,
                        file_ns: src.file_ns,
                        kind_override: None,
                        scope: child_scope.clone(),
                    });
                }
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
                    file_ns: self.file_ns,
                    kind_override: Some(sc.kind.as_str()),
                    scope: child_scope.clone(),
                });
            }
        }
        Ok(out)
    }

    /// `true` when this block is context-polymorphic — its `@block` type
    /// carries `@contextual`. Such a block is legal wherever children
    /// are allowed at all, its body is not recursed into by the child
    /// walk, and its children come from the host's expander.
    ///
    /// An instance of a `@declares_kind`-declared kind answers `true`
    /// through the same route as any other block: the schema derived
    /// from its declarer carries `@contextual`, because what such an
    /// instance emits is whatever its declarer's body expands to.
    pub(crate) fn is_contextual(&self) -> bool {
        self.schema().is_some_and(|t| t.is_contextual())
    }

    /// The blocks this `@contextual` block generates, flattened across
    /// every expansion (a repetition's iterations, an instantiated
    /// body). Produced by the [`Expander`](crate::Expander) the host
    /// registered on the [`Environment`](crate::Environment), so the
    /// blocks a projection sees are exactly the ones the host's own
    /// renderer sees — including the diagnostics it records on the way.
    ///
    /// Empty for a block that is not `@contextual`, and for one past
    /// the expansion-depth cap. Demanding the children of a `@contextual`
    /// block with no registered expander is a hard error: the language
    /// declines to guess at expansion semantics it does not own.
    pub fn expand_children(&self) -> Result<Vec<Block<'a>>, EvalError> {
        if !self.is_contextual() {
            return Ok(Vec::new());
        }
        // Mirrors the renderer's own recursion guard: a self-referential
        // expansion stops here rather than growing without bound.
        if self.binding_scope_depth() > MAX_EXPANSION_DEPTH {
            return Ok(Vec::new());
        }
        match self.doc.expander() {
            Some(e) => Ok(e.expand(self)),
            None => Err(EvalError::missing_expander(self.kind(), self.span())),
        }
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
                    for el in items.iter() {
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

/// Collect every block of `kind` from a `@contextual` block's expansion,
/// recursing through nested contextual blocks (a repetition's body may
/// contain another repetition or an instance). The expansion-depth cap
/// inside [`Block::expand_children`] bounds the recursion.
fn push_generated_matching<'a>(
    contextual: &Block<'a>,
    kind: &str,
    out: &mut Vec<Block<'a>>,
) -> Result<(), EvalError> {
    for child in contextual.expand_children()? {
        if child.kind() == kind {
            out.push(child);
        } else if child.is_contextual() {
            push_generated_matching(&child, kind, out)?;
        }
    }
    Ok(())
}

/// Reify a block's first label into one address segment, or `None` for an
/// absent / unaddressable label (an anonymous nested block, or a non-scalar
/// label). Strings, symbols, and integers all address (`Value::as_path_segment`),
/// exactly the labels `BlockList::child` matches.
fn block_label_segment(b: &Block<'_>) -> Option<String> {
    b.labels().ok()?.first()?.as_path_segment()
}

/// Coerce a string value to an identifier — the value-level rule for
/// identifier-declared fields, so a quoted ref (`system = "web"`) and a
/// bare one (`system = web`) evaluate identically and `ref == id`
/// comparisons in templates hold. Non-string values pass through.
fn str_to_identifier(v: Value) -> Value {
    match v {
        Value::Utf8(s) | Value::Ascii(s) => Value::Identifier(s),
        other => other,
    }
}

/// Reify a single block at document path `base`. A block whose kind is
/// `@by_ref` becomes a `Value::DataPath` reference (segments = `base`)
/// rather than an inlined record, so its content is reached only by
/// re-resolving the path. Everything else reifies via `to_record_value_at`.
fn reify_block_at(b: &Block<'_>, base: &[String]) -> Result<Value, EvalError> {
    if b.schema().is_some_and(|s| s.is_by_ref()) {
        return Ok(Value::DataPath {
            kind: "block".to_string(),
            segments: base.to_vec(),
        });
    }
    b.to_record_value_at(base)
}

/// Materialise a [`DataRef`](crate::data::DataRef) into an owned [`Value`]
/// for expression consumption: leaf fields / pre-evaluated variant values
/// pass through, a single block reifies to a record, and a block list /
/// table / variant list reifies to a `Value::List`. Returns `None` for
/// kinds that have no list/record value (types, unions, symbols, …) so the
/// caller can fall back to a `Value::DataPath` handle for reflective
/// builtins.
///
/// Carries the document path that re-resolves `dr` from root (`base`),
/// threaded so `@by_ref` slots can emit resolvable references. For a block
/// list, each element's address extends `base` with that element's label.
pub(crate) fn dataref_to_value_at<'a>(
    dr: &crate::data::DataRef<'a>,
    base: &[String],
) -> Option<Result<Value, EvalError>> {
    use crate::data::DataKind;
    match dr.inner() {
        DataKind::Field(f) => Some(f.value().cloned().map_err(|e| e.clone())),
        DataKind::Error(e) => Some(Err(e.clone())),
        DataKind::VariantValue(v) => Some(Ok(v.clone())),
        DataKind::VariantValueList(vs) => Some(Ok(Value::List(std::sync::Arc::new(vs.clone())))),
        DataKind::Block(b) => Some(reify_block_at(b, base)),
        DataKind::BlockList(v) | DataKind::Table(v) => Some((|| {
            let mut out = Vec::with_capacity(v.len());
            for b in v {
                let elem_base = match block_label_segment(b) {
                    Some(seg) => {
                        let mut p = base.to_vec();
                        p.push(seg);
                        p
                    }
                    // Unaddressable element: pass an empty base so any nested
                    // `@by_ref` reference is emitted but simply won't resolve.
                    None => Vec::new(),
                };
                out.push(reify_block_at(b, &elem_base)?);
            }
            Ok(Value::List(std::sync::Arc::new(out)))
        })()),
        _ => None,
    }
}
