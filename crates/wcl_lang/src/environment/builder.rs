//! Building a declaration from Rust.
//!
//! A host that ships schemas has no `.wcl` source to parse, so it needs
//! some way to hand the document an [`ast::TypeDecl`]. Constructing one
//! by hand means filling in spans and trivia that mean nothing for a
//! node with no source behind it; these builders do that, over
//! [`ast::synthetic`](crate::ast), and validate what they can on the way.
//!
//! The language builds its own declarations through the same API — see
//! [`stdlib`](super::stdlib).

use crate::ast;
use crate::ast::{TypeRef, synthetic_span};
use crate::value::Value;

/// Output of [`TypeBuilder::build`] — a finished synthetic type declaration
/// ready to register with an [`Environment`](super::Environment).
pub struct BuiltType {
    /// The finished declaration.
    pub(crate) inner: ast::TypeDecl,
}

/// Builder for synthetic type declarations.
pub struct TypeBuilder {
    /// Dotted name of the type being built.
    name: Vec<String>,
    /// Fields accumulated so far.
    fields: Vec<ast::TypeField>,
    /// Decorators accumulated so far.
    decorators: Vec<ast::Decorator>,
}

impl TypeBuilder {
    /// Start building a type declaration with the given dotted name.
    pub fn new<I, S>(name: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            name: name.into_iter().map(Into::into).collect(),
            fields: Vec::new(),
            decorators: Vec::new(),
        }
    }

    /// Attach a decorator to the type being built.
    pub fn decorator(mut self, d: DecoratorBuilder) -> Self {
        self.decorators.push(d.build());
        self
    }

    /// Append a field to the type being built.
    pub fn field(mut self, f: TypeFieldBuilder) -> Self {
        self.fields.push(f.build());
        self
    }

    /// Finish the type declaration.
    pub fn build(self) -> BuiltType {
        BuiltType {
            inner: ast::TypeDecl {
                name: self.name,
                extends: Vec::new(),
                alias: None,
                fields: self.fields,
                decorators: self.decorators,
                span: synthetic_span(),
                leading_trivia: Vec::new(),
                trailing_comment: None,
                trailing_trivia: Vec::new(),
            },
        }
    }
}

/// Builder for synthetic type fields.
pub struct TypeFieldBuilder {
    /// Field name.
    name: String,
    /// Declared type.
    ty: TypeRef,
    /// Whether the field is optional.
    optional: bool,
    /// Decorators accumulated so far.
    decorators: Vec<ast::Decorator>,
}

impl TypeFieldBuilder {
    /// Start a required field of the given name and type.
    pub fn new(name: impl Into<String>, ty: TypeRef) -> Self {
        Self {
            name: name.into(),
            ty,
            optional: false,
            decorators: Vec::new(),
        }
    }

    /// Mark the field optional (or not).
    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Attach a decorator to the field being built.
    pub fn decorator(mut self, d: DecoratorBuilder) -> Self {
        self.decorators.push(d.build());
        self
    }

    /// Finish the field declaration.
    pub(crate) fn build(self) -> ast::TypeField {
        ast::TypeField {
            name: self.name,
            ty: self.ty,
            ty_span: synthetic_span(),
            optional: self.optional,
            decorators: self.decorators,
            span: synthetic_span(),
            default_expr: None,
            leading_trivia: Vec::new(),
            trailing_comment: None,
        }
    }
}

/// Builder for synthetic decorators.
pub struct DecoratorBuilder {
    /// Dotted decorator name.
    name: Vec<String>,
    /// Positional arguments accumulated so far.
    positional: Vec<ast::Expr>,
    /// Named arguments accumulated so far.
    named: Vec<ast::NamedArg>,
}

impl DecoratorBuilder {
    /// Start building a decorator with the given dotted name.
    pub fn new<I, S>(name: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            name: name.into_iter().map(Into::into).collect(),
            positional: Vec::new(),
            named: Vec::new(),
        }
    }

    /// Append a positional argument.
    pub fn positional(mut self, value: Value) -> Self {
        self.positional.push(value_to_expr(value));
        self
    }

    /// Set a named argument.
    pub fn named(mut self, name: impl Into<String>, value: Value) -> Self {
        self.named.push(ast::NamedArg {
            name: name.into(),
            value: value_to_expr(value),
            span: synthetic_span(),
            leading_trivia: Vec::new(),
            trailing_comment: None,
        });
        self
    }

    /// Finish the decorator.
    pub(crate) fn build(self) -> ast::Decorator {
        let positional_spans = vec![synthetic_span(); self.positional.len()];
        ast::Decorator {
            name: self.name,
            name_span: synthetic_span(),
            positional: self.positional,
            positional_spans,
            named: self.named,
            span: synthetic_span(),
        }
    }
}

/// Lift an already-evaluated value back into the literal expression
/// that produces it, so synthesised declarations can carry values.
fn value_to_expr(v: Value) -> ast::Expr {
    match v {
        Value::Bool(b) => ast::Expr::Bool(b),
        Value::I8(n) => ast::Expr::I8(n),
        Value::I16(n) => ast::Expr::I16(n),
        Value::I32(n) => ast::Expr::I32(n),
        Value::I64(n) => ast::Expr::I64(n),
        Value::I128(n) => ast::Expr::I128(n),
        Value::Isize(n) => ast::Expr::Isize(n),
        Value::U8(n) => ast::Expr::U8(n),
        Value::U16(n) => ast::Expr::U16(n),
        Value::U32(n) => ast::Expr::U32(n),
        Value::U64(n) => ast::Expr::U64(n),
        Value::U128(n) => ast::Expr::U128(n),
        Value::Usize(n) => ast::Expr::Usize(n),
        Value::F32(n) => ast::Expr::F32(n),
        Value::F64(n) => ast::Expr::F64(n),
        Value::Utf8(s) => ast::Expr::Utf8(s),
        Value::Ascii(s) => ast::Expr::Ascii(s),
        Value::Utf16(v) => ast::Expr::Utf16(v),
        Value::Utf32(v) => ast::Expr::Utf32(v),
        Value::Identifier(s) => ast::Expr::Identifier(s, ast::Span::new(0, 0)),
        Value::Symbol(s) => ast::Expr::Symbol(s),
        Value::None => ast::Expr::None,
        Value::Function(_) => {
            unreachable!("function values are not constructible via the schema builder API")
        }
        Value::List(items) => ast::Expr::ListLit {
            elements: std::sync::Arc::unwrap_or_clone(items)
                .into_iter()
                .map(value_to_expr)
                .collect(),
            elem_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
            span: synthetic_span(),
        },
        Value::Tensor { .. } => {
            unreachable!("tensor values are not constructible via the schema builder API")
        }
        Value::Variant { .. } => {
            unreachable!("variant values are not constructible via the schema builder API")
        }
        Value::Record { .. } => {
            unreachable!("record values are not constructible via the schema builder API")
        }
        Value::DataPath { .. } => {
            unreachable!("data path values are not constructible via the schema builder API")
        }
        Value::PendingUnit { .. } => {
            unreachable!(
                "unresolved unit literals are not constructible via the schema builder API"
            )
        }
    }
}
