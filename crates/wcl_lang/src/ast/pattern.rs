//! Patterns, as written in a `match` arm or an `if let`.
//!
//! Parsed by [`parser::pattern`](crate::parser), printed by
//! [`format::pattern`](crate::format).

use super::Span;

/// A pattern, as written in a `match` arm or an `if let`.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `_` — matches anything, binds nothing.
    Wildcard(Span),
    /// A bare name — matches anything and binds it.
    Binding {
        /// The bound name.
        name: String,
        /// Source span of the name.
        span: Span,
    },
    /// `name @ inner` — binds `name` to the full value while matching
    /// `inner` against it.
    At {
        /// The name bound to the whole value.
        name: String,
        /// The pattern the value must also match.
        inner: Box<Pattern>,
        /// Source span of the whole pattern.
        span: Span,
    },
    /// Matches a specific `true` / `false`.
    LiteralBool(bool, Span),
    /// Matches a specific numeric literal.
    LiteralNumber {
        /// The literal to compare against.
        lit: crate::lexer::NumberLit,
        /// Source span of the literal.
        span: Span,
    },
    /// Matches a specific UTF-8 string.
    LiteralUtf8(String, Span),
    /// Matches a specific ASCII string.
    LiteralAscii(String, Span),
    /// Matches a specific symbol (`:gold`).
    LiteralSymbol(String, Span),
    /// Matches the `none` value.
    LiteralNone(Span),
    /// Matches one variant of a union, destructuring its payload.
    Variant {
        /// Dotted path naming the union type. May be empty when the
        /// type is inferred from context.
        type_path: Vec<String>,
        /// The variant name.
        variant: String,
        /// How the payload is destructured.
        args: VariantPatArgs,
        /// Source span of the whole pattern.
        span: Span,
    },
}

/// How a [`Pattern::Variant`] destructures its payload. The pattern-side
/// counterpart of [`VariantArgs`](super::VariantArgs).
#[derive(Debug, Clone, PartialEq)]
pub enum VariantPatArgs {
    /// No payload to destructure.
    Unit,
    /// A single unnamed payload, matched against one sub-pattern.
    Positional(Box<Pattern>),
    /// Named fields, each matched against a sub-pattern.
    Record {
        /// The `(field name, sub-pattern)` pairs written in the source.
        fields: Vec<(String, Pattern)>,
        /// `true` when the pattern ends in `..`, allowing fields the
        /// pattern does not name. Without it the pattern must be
        /// exhaustive over the variant's fields.
        rest: bool,
    },
}
