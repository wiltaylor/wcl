//! Views over declarations: `type`, `interface`, `union`,
//! `symbol_set`, `connection` and `use`.
//!
//! Each pairs its AST node with the document that declared it, so a name
//! written inside it resolves in the right namespace and its decorators
//! can be evaluated on demand.

use super::decorator::iter_decorators;
use super::*;

#[derive(Debug, Clone, Copy)]
/// A `union` declaration, with its variants and decorators resolved
/// against the document.
pub struct UnionDecl<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::UnionDecl,
    /// Namespace of the file that declared this item, prefixed to
    /// its own name to form the fully-qualified name.
    pub(in crate::doc) file_ns: &'a [String],
    /// Lazily-evaluated caches for this item's decorators and fields.
    pub(in crate::doc) cells: &'a ItemCells,
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
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
    /// Decorator caches for this union's variants, one entry per variant.
    fn variant_decorator_cells(&self) -> &'a [Vec<DecoratorCell>] {
        let ItemCellKind::UnionDecl {
            variant_decorators, ..
        } = &self.cells.kind
        else {
            unreachable!("UnionDecl view wraps a UnionDecl cell")
        };
        variant_decorators
    }

    /// Decorator caches for each variant's fields, indexed variant-then-field.
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

    /// Decorators attached to this item, in source order.
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

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this union declaration.
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::UnionDecl(self.ast.clone()))
    }

    /// The variants declared directly on this union, in source order.
    /// Variants inherited via `extends` are not included.
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

    /// The directly-declared variant with this name, if any.
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

    /// Parent union paths, resolved in this declaration's namespace.
    pub fn extends(&self) -> &'a [Vec<String>] {
        &self.ast.extends
    }
}

#[derive(Clone, Copy)]
/// One variant of a [`UnionDecl`].
pub struct UnionVariant<'a> {
    /// The AST node this view borrows.
    ast: &'a ast::UnionVariant,
    /// Lazily-evaluated caches for this item's decorators.
    decorator_cells: &'a [DecoratorCell],
    /// Lazily-evaluated decorator caches, one per declared field.
    field_decorator_cells: &'a [Vec<DecoratorCell>],
    /// The document these views read through.
    doc: &'a Document,
    /// Namespace of the union that declares this variant — propagated to
    /// the variant's record fields for namespace-relative resolution.
    file_ns: &'a [String],
}

impl<'a> UnionVariant<'a> {
    /// Decorators attached to this item, in source order.
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

    /// The declared name.
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this union variant.
    pub fn to_source(&self) -> String {
        crate::format::to_source_union_variant(self.ast)
    }

    /// The variant's payload shape.
    pub fn body(&self) -> VariantBodyView<'a> {
        match &self.ast.body {
            ast::VariantBody::Record { .. } => VariantBodyView::Record,
            ast::VariantBody::TypeRef { ty, .. } => VariantBodyView::TypeRef(ty),
            ast::VariantBody::InterfaceRef { iface, .. } => VariantBodyView::InterfaceRef(iface),
            ast::VariantBody::Unit => VariantBodyView::Unit,
        }
    }

    /// Fields of this variant's record payload, in source order. Empty
    /// for a unit or positional variant.
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

    /// The directly-declared field with this name, if any.
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
    /// A block written as a nested block in the source.
    Nested,
    /// A block synthesised from one row of an `Item::Table`.
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
    /// The literal block kind, when the decorator named one; `None` for
    /// the union and interface forms.
    pub fn as_kind(&self) -> Option<&str> {
        match self {
            ChildKind::Kind(s) => Some(s.as_str()),
            ChildKind::Union(_) | ChildKind::Interface(_) => None,
        }
    }

    /// The union this matches against, when the decorator named one.
    pub fn as_union(&self) -> Option<&UnionDecl<'a>> {
        match self {
            ChildKind::Union(u) => Some(u),
            ChildKind::Kind(_) | ChildKind::Interface(_) => None,
        }
    }

    /// The interface this matches against, when the decorator named one.
    pub fn as_interface(&self) -> Option<&InterfaceDecl<'a>> {
        match self {
            ChildKind::Interface(i) => Some(i),
            ChildKind::Kind(_) | ChildKind::Union(_) => None,
        }
    }
}

/// Resolve one positional `@child` / `@children` argument to the kind,
/// union or interface it names. `None` when the argument is absent or
/// names nothing the document declares.
pub(in crate::doc) fn resolve_child_kind_arg<'a>(
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
pub(in crate::doc) fn synth_child_from_value(
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

/// The payload shape of a union variant, as the document layer sees it.
/// The view counterpart of [`ast::VariantBody`].
pub enum VariantBodyView<'a> {
    /// Named fields declared inline on the variant. Read them with
    /// [`UnionVariant::fields`].
    Record,
    /// A single unnamed payload of the given type.
    TypeRef(&'a TypeRef),
    /// Variant body of the form `&InterfaceName`: payload is any value
    /// implementing the interface. The slice borrows the path segments
    /// declared in source.
    InterfaceRef(&'a [String]),
    /// No payload.
    Unit,
}

#[derive(Debug, Clone, Copy)]
/// A `connection` declaration — what a connection statement may link,
/// and under which kinds.
pub struct ConnectionDecl<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::ConnectionDecl,
    /// Namespace of the file that declared this item, prefixed to
    /// its own name to form the fully-qualified name.
    pub(in crate::doc) file_ns: &'a [String],
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
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
    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// The type permitted on the left of a connection statement.
    pub fn source_type(&self) -> &'a TypeRef {
        &self.ast.source
    }

    /// The type permitted on the right of a connection statement.
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
/// A `symbol_set` declaration — the closed vocabulary a field of that
/// type may take.
pub struct SymbolSetDecl<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::SymbolSetDecl,
    /// Namespace of the file that declared this item, prefixed to
    /// its own name to form the fully-qualified name.
    pub(in crate::doc) file_ns: &'a [String],
    /// Lazily-evaluated caches for this item's decorators and fields.
    pub(in crate::doc) cells: &'a ItemCells,
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
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
    /// Decorator caches for this symbol set's entries, one per symbol.
    fn symbol_decorator_cells(&self) -> &'a [Vec<DecoratorCell>] {
        let ItemCellKind::SymbolSetDecl { symbol_decorators } = &self.cells.kind else {
            unreachable!("SymbolSetDecl view wraps a SymbolSetDecl cell")
        };
        symbol_decorators
    }

    /// Decorators attached to this item, in source order.
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

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this symbol-set declaration.
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::SymbolSetDecl(self.ast.clone()))
    }

    /// The symbols this set permits, in source order.
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

    /// Whether this set permits the named symbol.
    pub fn has(&self, name: &str) -> bool {
        self.ast.symbols.iter().any(|s| s.name == name)
    }
}

#[derive(Clone, Copy)]
/// One symbol of a [`SymbolSetDecl`].
pub struct SymbolEntry<'a> {
    /// The AST node this view borrows.
    ast: &'a ast::SymbolEntry,
    /// Lazily-evaluated caches for this item's decorators.
    decorator_cells: &'a [DecoratorCell],
    /// The document these views read through.
    doc: &'a Document,
    /// Namespace of the file that declared this item, prefixed to its
    /// own name to form the fully-qualified name.
    file_ns: &'a [String],
}

impl<'a> SymbolEntry<'a> {
    /// Decorators attached to this item, in source order.
    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        iter_decorators(
            &self.ast.decorators,
            self.decorator_cells,
            self.doc,
            self.file_ns,
        )
    }

    /// The declared name.
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this symbol-set entry.
    pub fn to_source(&self) -> String {
        crate::format::to_source_symbol_entry(self.ast)
    }
}

#[derive(Debug, Clone, Copy)]
/// A `type` declaration, with its fields, decorators and `extends`
/// chain resolved against the document.
pub struct TypeDecl<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::TypeDecl,
    /// Namespace of the file that declared this item, prefixed to
    /// its own name to form the fully-qualified name.
    pub(in crate::doc) file_ns: &'a [String],
    /// Lazily-evaluated caches for this item's decorators and fields.
    pub(in crate::doc) cells: &'a ItemCells,
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
    /// `true` when this declaration comes from an imported source
    /// (a disk or system/registry import) rather than the root
    /// document. Drives document-schema composition: a root-authored
    /// `@document` is authoritative, imported ones are library
    /// defaults that merge in. See `Document::schema_errors`.
    pub(in crate::doc) is_imported: bool,
    /// `true` when this declaration was *derived* from a
    /// `@declares_kind` instance rather than written as a type — see
    /// [`Document::block_schema`](crate::Document::block_schema). A
    /// derived schema describes the declarer's params and nothing else,
    /// so the child walk stops at an instance of it.
    pub(in crate::doc) is_derived: bool,
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
    /// Decorator caches for this declaration's fields, one entry per field.
    fn field_decorator_cells(&self) -> &'a [Vec<DecoratorCell>] {
        let ItemCellKind::TypeDecl { field_decorators } = &self.cells.kind else {
            unreachable!("TypeDecl view wraps a TypeDecl cell")
        };
        field_decorators
    }

    /// Decorators attached to this item, in source order.
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

    /// Source span of this node in the file that declares it.
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

    /// Fields declared directly on this declaration, in source order.
    /// Inherited fields are not included — see `effective_fields`.
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

    /// The directly-declared field with this name, if any.
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

    /// The target of a transparent type alias, if this declaration is one.
    pub fn alias_type(&self) -> Option<&'a TypeRef> {
        self.ast.alias.as_ref()
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
/// An `interface` declaration. Unlike a [`TypeDecl`] it is never
/// instantiated: it states what a conforming type must provide.
pub struct InterfaceDecl<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::InterfaceDecl,
    /// Namespace of the file that declared this item, prefixed to
    /// its own name to form the fully-qualified name.
    pub(in crate::doc) file_ns: &'a [String],
    /// Lazily-evaluated caches for this item's decorators and fields.
    pub(in crate::doc) cells: &'a ItemCells,
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
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
    /// Decorator caches for this declaration's fields, one entry per field.
    fn field_decorator_cells(&self) -> &'a [Vec<DecoratorCell>] {
        let ItemCellKind::InterfaceDecl { field_decorators } = &self.cells.kind else {
            unreachable!("InterfaceDecl view wraps an InterfaceDecl cell")
        };
        field_decorators
    }

    /// Decorators attached to this item, in source order.
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

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this interface declaration.
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::InterfaceDecl(self.ast.clone()))
    }

    /// Fields declared directly on this declaration, in source order.
    /// Inherited fields are not included — see `effective_fields`.
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

    /// The directly-declared field with this name, if any.
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

    /// Every field this interface requires, including those inherited
    /// through its `extends` chain. Contrast `fields`, which lists only
    /// what is declared directly here.
    pub fn effective_fields(&self) -> Vec<TypeField<'a>> {
        build_effective_fields(self.doc, &self.ast.extends, self.file_ns, self.fields())
    }

    /// See [`TypeDecl::merged_field_decorators`].
    pub fn merged_field_decorators(&self) -> std::collections::HashMap<String, Vec<Decorator<'a>>> {
        build_merged_decorators(self.doc, &self.ast.extends, self.file_ns, self.fields())
    }

    /// The named field, searching this interface and then its `extends`
    /// chain. The inherited counterpart of `field`.
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

/// A `use` declaration, bringing names into the file's scope.
pub struct UseDeclView<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::UseDecl,
}

impl<'a> UseDeclView<'a> {
    /// The dotted path this declaration imports from.
    pub fn path(&self) -> &'a [String] {
        &self.ast.path
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Which names the declaration brings into scope.
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

/// What a [`UseDeclView`] brings into scope.
pub enum UseFormView<'a> {
    /// `use a.b.c`, or `use a.b.c as d` — the path's last segment,
    /// optionally renamed.
    Bare(Option<&'a str>),
    /// `use a.b.{x, y as z}` — read the entries with
    /// [`UseDeclView::items`].
    List,
}

/// One name in a brace-list [`UseFormView::List`].
pub struct UseItem<'a> {
    /// The AST node this view borrows.
    ast: &'a ast::UseItem,
}

impl<'a> UseItem<'a> {
    /// The declared name.
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    /// The local spelling, when the entry was written `name as alias`.
    pub fn alias(&self) -> Option<&'a str> {
        self.ast.alias.as_deref()
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }
}
