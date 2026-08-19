//! Declarations: the schema half of [`Item`](super::Item).
//!
//! Types, interfaces, unions, symbol sets and connection declarations
//! describe what a document may contain; namespaces, `use` and `import`
//! say where those names come from. None of them are document data
//! themselves — that is [`items`](super::items).

use super::{Decorator, Expr, Span, Trivia, TypeRef};

/// A `namespace a.b.c` declaration, scoping the names that follow it.
#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceDecl {
    /// The dotted namespace path.
    pub path: Vec<String>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this declaration.
    pub trailing_comment: Option<String>,
}

/// A `use a.b.c` declaration, bringing names into the current scope.
#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    /// The dotted path being imported from.
    pub path: Vec<String>,
    /// Which names are taken, and under what local spelling.
    pub form: UseForm,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this declaration.
    pub trailing_comment: Option<String>,
}

/// What a [`UseDecl`] brings into scope.
#[derive(Debug, Clone, PartialEq)]
pub enum UseForm {
    /// `use a.b.c` or `use a.b.c as d` — the path's last segment,
    /// optionally renamed.
    Bare(Option<String>),
    /// `use a.b.{x, y as z}` — an explicit list of names.
    List(Vec<UseItem>),
}

/// One name in a [`UseForm::List`].
#[derive(Debug, Clone, PartialEq)]
pub struct UseItem {
    /// The name as declared at the source path.
    pub name: String,
    /// The local spelling, when written as `name as alias`.
    pub alias: Option<String>,
    /// Source span of the entry.
    pub span: Span,
}

/// A `type Name { fields… }` declaration, or the alias form
/// `type Name = TypeRef`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    /// The dotted type name.
    pub name: Vec<String>,
    /// Names of parent types/interfaces this declaration inherits
    /// from, in source order. Empty when no `extends` clause was
    /// written.
    pub extends: Vec<Vec<String>>,
    /// `Some` for the alias form `type Name = TypeRef` — a transparent
    /// name for the target type. `fields` and `extends` are then empty;
    /// constraint decorators (`@min` / `@max` / `@non_empty`) on the
    /// alias apply to every field declared with it.
    pub alias: Option<TypeRef>,
    /// The declared fields, in source order.
    pub fields: Vec<TypeField>,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last field, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// An `interface Name { fields… }` declaration. Unlike a [`TypeDecl`] an
/// interface is never instantiated: it constrains what a type must
/// structurally provide.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    /// The dotted interface name.
    pub name: Vec<String>,
    /// Parent types/interfaces — same shape as `TypeDecl::extends`.
    pub extends: Vec<Vec<String>>,
    /// The required fields, in source order.
    pub fields: Vec<TypeField>,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last field, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// One field of a [`TypeDecl`], [`InterfaceDecl`] or record variant.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeField {
    /// The field name.
    pub name: String,
    /// The declared (or, for the `name = expr` form, inferred) type.
    pub ty: TypeRef,
    /// Source span of the type annotation.
    pub ty_span: Span,
    /// `true` for the `?` suffix — the field may be absent.
    pub optional: bool,
    /// Decorators attached to this field.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole field.
    pub span: Span,
    /// Comments/blank lines printed above this field.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this field.
    pub trailing_comment: Option<String>,
    /// Inline default expression, set when the field is declared as
    /// `name = expr` (no explicit type). The type in `ty` is then
    /// inferred from the expression. `None` for the classical
    /// `name: type [?]` form.
    pub default_expr: Option<Expr>,
}

/// A `union Name { variants… }` declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct UnionDecl {
    /// The dotted union name.
    pub name: Vec<String>,
    /// Parent unions whose variants are inherited. Empty when this
    /// union is declared without an `extends` clause. Variants are
    /// resolved through `Document::union_decl` then composed by
    /// `UnionDecl::effective_variants`.
    pub extends: Vec<Vec<String>>,
    /// The variants declared directly on this union.
    pub variants: Vec<UnionVariant>,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last variant, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// One variant of a [`UnionDecl`].
#[derive(Debug, Clone, PartialEq)]
pub struct UnionVariant {
    /// The variant name.
    pub name: String,
    /// The variant's payload shape.
    pub body: VariantBody,
    /// Decorators attached to this variant.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole variant.
    pub span: Span,
    /// Comments/blank lines printed above this variant.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this variant.
    pub trailing_comment: Option<String>,
}

/// The payload shape a union variant *declares*. Mirrors [`VariantArgs`](super::VariantArgs),
/// which is the constructor form.
#[derive(Debug, Clone, PartialEq)]
pub enum VariantBody {
    /// Named fields, declared inline on the variant.
    Record {
        /// The declared fields.
        fields: Vec<TypeField>,
        /// Comments/blank lines after the last field, before `}`.
        trailing_trivia: Vec<Trivia>,
    },
    /// A single unnamed payload of the named type.
    TypeRef {
        /// The payload type.
        ty: TypeRef,
        /// Source span of the type annotation.
        ty_span: Span,
    },
    /// `Drawn &Drawable` — variant payload is any value whose runtime
    /// type structurally implements the named interface.
    InterfaceRef {
        /// Dotted name of the required interface.
        iface: Vec<String>,
        /// Source span of the interface name.
        iface_span: Span,
    },
    /// No payload.
    Unit,
}

/// A `symbol_set Name { symbols… }` declaration — the closed set of
/// symbols a field of that type may take.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolSetDecl {
    /// The dotted symbol-set name.
    pub name: Vec<String>,
    /// The permitted symbols, in source order.
    pub symbols: Vec<SymbolEntry>,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last symbol, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// One symbol in a [`SymbolSetDecl`].
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolEntry {
    /// The symbol name, written without the leading `:`.
    pub name: String,
    /// Decorators attached to this symbol.
    pub decorators: Vec<Decorator>,
    /// Source span of the entry.
    pub span: Span,
    /// Comments/blank lines printed above this symbol.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this symbol.
    pub trailing_comment: Option<String>,
}

/// An `import` declaration, pulling another document's items into this
/// one.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    /// The import path, as written.
    pub path: String,
    /// Source span of the path literal.
    pub path_span: Span,
    /// `true` for an angle-bracket system import (`import <wdoc/core.wcl>`,
    /// resolved through a registry); `false` for a quoted disk import
    /// (`import "./foo.wcl"`).
    pub system: bool,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this import.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this import.
    pub trailing_comment: Option<String>,
}

/// A `connection Name: Source -> Destination` declaration, defining what
/// a [`ConnectionStmt`](super::ConnectionStmt) may link and under which kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionDecl {
    /// The dotted connection name.
    pub name: Vec<String>,
    /// The type permitted on the left of a statement.
    pub source: TypeRef,
    /// Source span of the source type.
    pub source_span: Span,
    /// The type permitted on the right of a statement.
    pub destination: TypeRef,
    /// Source span of the destination type.
    pub destination_span: Span,
    /// The symbol set naming the permitted connection kinds.
    pub kind_set: Vec<String>,
    /// Source span of the kind set.
    pub kind_set_span: Span,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this declaration.
    pub trailing_comment: Option<String>,
}
