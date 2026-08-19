//! Items: the declarations a file (or a block body) is made of.
//!
//! This is the data half — fields, blocks, tables and connection
//! statements, all of which become document data — plus the [`Item`]
//! enum that unifies them with the schema half in
//! [`decls`](super::decls), and the [`Source`] that collects them.

use super::{
    ConnectionDecl, Decorator, Expr, ImportDecl, InterfaceDecl, NamespaceDecl, Span, SymbolSetDecl,
    Trivia, TypeDecl, TypeRef, UnionDecl, UseDecl,
};

/// A `name = expr` item: one piece of document data.
///
/// This is the item form. Unlike a [`LetItem`], a field is part of the
/// document model — queryable, iterable and schema-validated.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The field name.
    pub name: String,
    /// The field's value, evaluated lazily and cached once forced.
    pub expr: Expr,
    /// Decorators attached to this field.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole field.
    pub span: Span,
    /// Comments/blank lines printed above this field.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this field.
    pub trailing_comment: Option<String>,
}

/// A `let name = expr` item declared at the file (global) scope or
/// inside a block. Unlike a [`Field`], a let binding is a composition
/// helper: it is resolvable by name from sibling/descendant
/// expressions but never appears as document data (not queryable, not
/// iterated, not schema-validated). Distinct from the expression-level
/// [`LetBinding`](super::LetBinding) (which lives inside `Expr::Block` and uses `;`).
#[derive(Debug, Clone, PartialEq)]
pub struct LetItem {
    /// The bound name.
    pub name: String,
    /// The bound expression.
    pub value: Expr,
    /// Decorators on the `fn` item form (e.g. `@doc`). Always empty for
    /// `let` syntax, which rejects decorators.
    pub decorators: Vec<Decorator>,
    /// `true` when this binding was written as a `fn name(…) -> T body`
    /// item. Sugar for `let name = fn(…) -> T body`, with two visible
    /// differences: the binding is registered in the symbol index
    /// (outline / hover / go-to-def) and the formatter re-prints the
    /// `fn` form.
    pub fn_syntax: bool,
    /// Source span of the whole item.
    pub span: Span,
    /// Comments/blank lines printed above this item.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this `let` item.
    pub trailing_comment: Option<String>,
}

/// A block instance — `kind "label" { items… }`. Blocks are how a
/// document nests: each carries its own items and is validated against
/// the type its `@block` decorator names.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The block kind, unqualified.
    pub kind: String,
    /// Namespace qualifier written before the kind with `::`, e.g.
    /// `wdoc::process` parses to `kind_ns = ["wdoc"]`, `kind = "process"`.
    /// Empty for a bare, unqualified kind. Multi-segment namespaces are
    /// dot-separated on the left of `::` (`foo.bar::process`).
    pub kind_ns: Vec<String>,
    /// A bare-name content fill may be conditional (`aside? { ... }`).
    /// The host that owns the surrounding slot contract interprets this;
    /// the language only preserves the syntax.
    pub conditional: bool,
    /// Present for the declaration form `slot name: Type[? | *]`.
    /// Slots remain ordinary blocks so host schemas can place them with
    /// `@children("slot")`, while retaining their type syntax losslessly.
    pub slot_decl: Option<SlotDecl>,
    /// Labels written after the kind. The first is conventionally the
    /// block's id, via an `@inline(0)` field on its type.
    pub labels: Vec<Expr>,
    /// The block's body.
    pub items: Vec<Item>,
    /// Decorators attached to this block.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole block.
    pub span: Span,
    /// Comments/blank lines printed above this block.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the block's `}` (or after the
    /// kind/labels line for the empty-body shorthand).
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last item, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// The type syntax of a `slot name: Type[? | *]` declaration, preserved
/// losslessly on the [`Block`] that carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotDecl {
    /// The declared slot type.
    pub ty: TypeRef,
    /// Source span of the type annotation.
    pub ty_span: Span,
    /// `true` for the `?` suffix — the slot may be left unfilled.
    pub optional: bool,
    /// `true` for the `*` suffix — the slot accepts many fills.
    pub repeated: bool,
}

/// One `| a | b | c |` row of a [`TableItem`].
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The cell expressions, in column order.
    pub values: Vec<Expr>,
    /// Source span of the row.
    pub span: Span,
}

/// A table item — a `field:` name followed by pipe-delimited rows, which
/// is sugar for a list of records.
#[derive(Debug, Clone, PartialEq)]
pub struct TableItem {
    /// The field the rows are collected into.
    pub field_name: String,
    /// The rows, in source order.
    pub rows: Vec<Row>,
    /// Source span of the whole item.
    pub span: Span,
    /// Comments/blank lines printed above the table.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the table.
    pub trailing_comment: Option<String>,
}

/// A connection statement — `lhs -> rhs`, optionally tagged with a kind,
/// linking two blocks by id under a [`ConnectionDecl`].
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionStmt {
    /// Id of the block on the left.
    pub lhs: String,
    /// Source span of `lhs`.
    pub lhs_span: Span,
    /// Id of the block on the right.
    pub rhs: String,
    /// Source span of `rhs`.
    pub rhs_span: Span,
    /// The connection kind, when the statement names one.
    pub kind: Option<String>,
    /// Source span of `kind`, when present.
    pub kind_span: Option<Span>,
    /// Source span of the whole statement.
    pub span: Span,
    /// Comments/blank lines printed above this statement.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this statement.
    pub trailing_comment: Option<String>,
}

/// One top-level (or in-block) declaration. A [`Source`] is a list of
/// these, and a [`Block`]'s body is too.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// `name = expr` — document data.
    Field(Field),
    /// `let name = expr` or `fn name(…) …` — a composition helper.
    Let(LetItem),
    /// A nested block instance.
    Block(Block),
    /// A `type` declaration.
    TypeDecl(TypeDecl),
    /// An `interface` declaration.
    InterfaceDecl(InterfaceDecl),
    /// A `union` declaration.
    UnionDecl(UnionDecl),
    /// A `namespace` declaration.
    NamespaceDecl(NamespaceDecl),
    /// A `use` declaration.
    UseDecl(UseDecl),
    /// A `symbol_set` declaration.
    SymbolSetDecl(SymbolSetDecl),
    /// An `import` declaration.
    Import(ImportDecl),
    /// A table item.
    Table(TableItem),
    /// A `connection` declaration.
    ConnectionDecl(ConnectionDecl),
    /// A connection statement.
    Connection(ConnectionStmt),
}

impl Item {
    /// Attach a same-line trailing comment to this item, whatever its
    /// variant. Used by the parser to re-attach a comment that the lexer
    /// diverted as the next token's `same_line_comment` onto the item
    /// that ended the line.
    pub(crate) fn set_trailing_comment(&mut self, comment: String) {
        match self {
            Item::Field(x) => x.trailing_comment = Some(comment),
            Item::Let(x) => x.trailing_comment = Some(comment),
            Item::Block(x) => x.trailing_comment = Some(comment),
            Item::TypeDecl(x) => x.trailing_comment = Some(comment),
            Item::InterfaceDecl(x) => x.trailing_comment = Some(comment),
            Item::UnionDecl(x) => x.trailing_comment = Some(comment),
            Item::NamespaceDecl(x) => x.trailing_comment = Some(comment),
            Item::UseDecl(x) => x.trailing_comment = Some(comment),
            Item::SymbolSetDecl(x) => x.trailing_comment = Some(comment),
            Item::Import(x) => x.trailing_comment = Some(comment),
            Item::Table(x) => x.trailing_comment = Some(comment),
            Item::ConnectionDecl(x) => x.trailing_comment = Some(comment),
            Item::Connection(x) => x.trailing_comment = Some(comment),
        }
    }
}

/// One parsed file: its items plus the trivia that follows the last one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Source {
    /// The file's top-level items, in source order.
    pub items: Vec<Item>,
    /// Comments/blank lines after the last top-level item, before EOF.
    pub trailing_trivia: Vec<Trivia>,
}
