//! Decorators — the `@name(args…)` annotations attached to items,
//! fields and types.
//!
//! The language validates a decorator against its `@decorator`
//! declaration (see [`doc::decorators`](crate::doc)) and otherwise
//! leaves its meaning to the host. The one exception is `@schemaless`,
//! which the schema checker reads directly, so its interpretation sits
//! on the node itself.

use super::{Expr, NamedArg, Span};

/// A `@name(args…)` annotation attached to an item, field or type.
///
/// The language does not interpret most decorators itself: it validates
/// them against their `@decorator` declarations and exposes them to the
/// host, which gives them meaning.
#[derive(Debug, Clone, PartialEq)]
pub struct Decorator {
    /// Dotted decorator name, split on `.`.
    pub name: Vec<String>,
    /// Span of the dotted name only, excluding the leading `@` and any
    /// arguments. Decorator-level diagnostics point here.
    pub name_span: Span,
    /// Positional arguments, in source order.
    pub positional: Vec<Expr>,
    /// Source spans index-aligned with [`Self::positional`]. Synthetic
    /// decorators carry empty spans for their synthetic arguments.
    pub positional_spans: Vec<Span>,
    /// Named `key = value` arguments.
    pub named: Vec<NamedArg>,
    /// Source span of the whole decorator, including the `@`.
    pub span: Span,
}

/// How much of a type's schema `@schemaless` waives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemalessMode {
    /// Waive membership checking entirely: any field is accepted.
    Full,
    /// Waive it for annotations only, still checking declared fields.
    AnnotationsOnly,
}

impl Decorator {
    /// The `@schemaless` mode this decorator requests, or `None` when it
    /// is not a `@schemaless` decorator at all.
    pub(crate) fn schemaless_mode(&self) -> Option<SchemalessMode> {
        if self.name.len() != 1 || self.name[0] != "schemaless" {
            return None;
        }
        if self
            .named
            .iter()
            .any(|arg| arg.name == "annotations" && matches!(arg.value, Expr::Bool(true)))
        {
            Some(SchemalessMode::AnnotationsOnly)
        } else {
            Some(SchemalessMode::Full)
        }
    }
}
