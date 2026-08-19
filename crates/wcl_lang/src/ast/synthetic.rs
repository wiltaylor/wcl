//! Nodes fabricated with no source behind them.
//!
//! The evaluator derives type declarations for constructs the document
//! never wrote out — a block's implicit schema, the stdlib's decorator
//! schemas built by [`crate::Environment`]. Those nodes still have to
//! satisfy the same struct shapes as parsed ones, so they take an empty
//! [`Span`] and empty trivia; these constructors are the one place that
//! decision is spelled out.

use super::{Decorator, Expr, Span, TypeField, TypeRef};

/// The span every synthesised AST node carries: there is no source text
/// behind it. Shared with the schema derivation in `doc::schema_lookup`,
/// which fabricates type declarations the same way.
pub(crate) fn synthetic_span() -> Span {
    Span::new(0, 0)
}

/// A decorator with no named args, spanning nothing — the shape every
/// synthesised `@block("x")` / `@contextual` / `@decorator("y")` takes.
pub(crate) fn synthetic_decorator(name: &str, positional: Vec<Expr>) -> Decorator {
    let positional_spans = vec![synthetic_span(); positional.len()];
    Decorator {
        name: vec![name.to_string()],
        name_span: synthetic_span(),
        positional,
        positional_spans,
        named: Vec::new(),
        span: synthetic_span(),
    }
}

/// A field of a synthesised type: no decorators, no default, no span.
pub(crate) fn synthetic_field(name: &str, ty: TypeRef, optional: bool) -> TypeField {
    TypeField {
        name: name.to_string(),
        ty,
        ty_span: synthetic_span(),
        optional,
        decorators: Vec::new(),
        span: synthetic_span(),
        default_expr: None,
        leading_trivia: Vec::new(),
        trailing_comment: None,
    }
}
