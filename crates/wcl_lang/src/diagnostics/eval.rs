//! Failures raised while evaluating an open document.
//!
//! [`EvalError`] is the crate's runtime error: every fallible read of a
//! field, call of a function, or check of a value against its schema
//! ends here. It is one enum rather than a hierarchy because a host
//! handles them the same way — render it, or give up on the field — and
//! the machine-readable distinctions a tool *does* branch on are carried
//! as [`kinds`](super::kinds) rather than as variants.
//!
//! The constructors below exist so a call site never assembles a
//! `SourceSpan` or a message by hand: each takes the pieces it has and
//! owns the wording, which is what keeps one failure phrased identically
//! wherever it surfaces.

#![allow(unused_assignments)] // miette/thiserror derive triggers spurious lints on variant fields

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use super::{ArithmeticFault, SchemaViolationKind};

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiagnosticSource {
    /// The name that was written.
    name: String,
    text: String,
}

// Retained for callers that attach provenance directly to a cloned
// `SchemaViolation`; document-wide validation carries sources alongside
// errors so it does not currently exercise these helpers.
#[allow(dead_code)]
impl SchemaDiagnosticSource {
    /// Capture a `NamedSource` as owned name and text, so the
    /// provenance can outlive the borrow it was taken from.
    fn from_named_source(source: NamedSource<String>) -> Self {
        Self {
            name: source.name().to_string(),
            text: source.inner().clone(),
        }
    }

    /// Rebuild the `NamedSource` for rendering.
    pub(crate) fn named_source(&self) -> NamedSource<String> {
        NamedSource::new(&self.name, self.text.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Error, Diagnostic)]
/// A failure while evaluating a document: a bad expression, a broken
/// reference, or a schema violation.
///
/// Every variant carries a span so the diagnostic can point at the text
/// responsible. Errors are cached per field, so a field that fails
/// reports the same error on every later read rather than being retried.
pub enum EvalError {
    #[error("cycle while evaluating '{field}'")]
    #[diagnostic(code(wcl::eval::cycle))]
    /// A field's evaluation depends on itself, directly or through a
    /// chain of other fields. The cycle poisons only its own loop.
    Cycle {
        /// Name of the field involved.
        field: String,
        #[label("evaluated recursively")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("unknown built-in '{name}'")]
    #[diagnostic(code(wcl::eval::unknown_builtin))]
    /// A name in call position resolves to no builtin. The builtin
    /// registry is the last place the resolver looks, so this also
    /// surfaces when a name that is not callable is called.
    UnknownBuiltin {
        /// The name that was written.
        name: String,
        #[label("no builtin with this name")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("'{name}' expects {expected} argument(s), got {got}")]
    #[diagnostic(code(wcl::eval::builtin_arity))]
    /// A builtin was called with the wrong number of arguments.
    BuiltinArity {
        /// The name that was written.
        name: String,
        /// How many were expected.
        expected: usize,
        /// How many were supplied.
        got: usize,
        #[label("wrong number of arguments")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("'{name}': {message}")]
    #[diagnostic(code(wcl::eval::builtin_type))]
    /// A builtin rejected its arguments — the message is the
    /// builtin's own wording.
    BuiltinTypeMismatch {
        /// The name that was written.
        name: String,
        /// The rendered message.
        message: String,
        #[label("invalid argument(s)")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("callee is not callable")]
    #[diagnostic(code(wcl::eval::non_callable))]
    /// The callee of a call expression evaluated to something that
    /// is not a function.
    NonCallable {
        #[label("not callable")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("call expected {expected} argument(s), got {got}")]
    #[diagnostic(code(wcl::eval::call_arity))]
    /// A function was called with the wrong number of arguments.
    CallArity {
        /// How many were expected.
        expected: usize,
        /// How many were supplied.
        got: usize,
        #[label("wrong number of arguments")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("call depth limit exceeded (max {max})")]
    #[diagnostic(code(wcl::eval::call_depth_exceeded))]
    /// Function calls nested deeper than the evaluator's limit —
    /// the guard against unbounded recursion.
    CallDepthExceeded {
        /// The limit that was exceeded.
        max: usize,
        #[label("function call recurses too deeply")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("no match arm fits the value")]
    #[diagnostic(code(wcl::eval::match_no_arm))]
    /// No arm of a `match` matched the scrutinee, and no arm was a
    /// catch-all.
    MatchNoArm {
        #[label("no arm matched")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("match guard must return bool, got {kind}")]
    #[diagnostic(code(wcl::eval::guard_not_bool))]
    /// A match arm's `if` guard evaluated to something other than a
    /// `bool`.
    GuardNotBool {
        /// What was found instead, named as WCL spells it.
        kind: &'static str,
        #[label("guard expression is not a bool")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("unknown union '{path}'")]
    #[diagnostic(code(wcl::eval::unknown_union))]
    /// A variant constructor or pattern named a union the document
    /// does not declare.
    UnknownUnion {
        /// The path, as written in the source.
        path: String,
        #[label("no union with this name in scope")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("union '{union}' has no variant named '{variant}'")]
    #[diagnostic(code(wcl::eval::unknown_variant))]
    /// The union exists but declares no variant of that name,
    /// including through its `extends` chain.
    UnknownVariant {
        /// Fully-qualified name of the union.
        union: String,
        /// Name of the variant.
        variant: String,
        #[label("not a variant of this union")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("variant shape mismatch: expected {expected}, got {got}")]
    #[diagnostic(code(wcl::eval::variant_shape_mismatch))]
    /// A variant was constructed with a payload shape its
    /// declaration does not accept — a record where it declares a unit,
    /// or the reverse.
    VariantShapeMismatch {
        /// How many were expected.
        expected: String,
        /// How many were supplied.
        got: String,
        #[label("argument shape does not match the variant body")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("error: {message}")]
    #[diagnostic(code(wcl::eval::user_error))]
    /// Raised by the `error` builtin: a document author reporting a
    /// domain failure in their own words.
    UserError {
        /// The rendered message.
        message: String,
        #[label("error raised here")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("union '{union}' has a cyclic 'extends' chain")]
    #[diagnostic(code(wcl::eval::union_cycle))]
    /// A union's `extends` chain loops back on itself, so its
    /// effective variant list cannot be built.
    UnionCycle {
        /// Fully-qualified name of the union.
        union: String,
        #[label("cyclic extends")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("operator '{op}' is not defined for {lhs_type} and {rhs_type}")]
    #[diagnostic(code(wcl::eval::type_mismatch))]
    /// A binary operator was applied to a pair of types it is not
    /// defined for, after numeric promotion has been tried.
    TypeMismatch {
        /// The operator that was applied.
        op: String,
        /// Type of the left operand, as WCL spells it.
        lhs_type: String,
        /// Type of the right operand, as WCL spells it.
        rhs_type: String,
        #[label("incompatible operands")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("operator '{op}' cannot {fault}")]
    #[diagnostic(code(wcl::eval::arithmetic))]
    /// The operator was defined for the operands but could not
    /// produce a result — a zero divisor, or an overflow.
    Arithmetic {
        /// The operator that was applied.
        op: String,
        /// Which fault, so tools can act on it without parsing the
        /// rendered message.
        fault: ArithmeticFault,
        #[label("no result for these operands")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("cannot evaluate {kind} as a leaf value")]
    #[diagnostic(code(wcl::eval::not_a_leaf))]
    /// A path resolved to a block or other container where a single
    /// value was required.
    NotALeaf {
        /// What was found instead, named as WCL spells it.
        kind: String,
        #[label("not a leaf")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("failed to import '{path}': {message}")]
    #[diagnostic(code(wcl::eval::import_failed))]
    /// An import could not be read, parsed or resolved.
    ImportFailed {
        /// The path, as written in the source.
        path: String,
        /// The rendered message.
        message: String,
        #[label("import error")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("{message}")]
    #[diagnostic(code(wcl::eval::schema_violation))]
    /// The document parsed but breaks its own schema. `kind`
    /// carries the machine-readable category so tools need not parse
    /// the message.
    SchemaViolation {
        /// What was found instead, named as WCL spells it.
        kind: SchemaViolationKind,
        /// The offending identifier (field / child block name) when the
        /// violation has one, so tools can act on it without parsing
        /// `message`. `None` for kinds that don't name a single token.
        detail: Option<String>,
        /// The rendered message.
        message: String,
        #[doc(hidden)]
        origin: Option<std::sync::Arc<SchemaDiagnosticSource>>,
        #[label("schema violation")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("unresolved reference '{path}'")]
    #[diagnostic(code(wcl::eval::unresolved_reference))]
    /// A reference-typed field names an id that no block in scope
    /// declares.
    UnresolvedReference {
        /// The path, as written in the source.
        path: String,
        #[label("does not resolve")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("expected a reference, got {kind}")]
    #[diagnostic(code(wcl::eval::not_a_reference))]
    /// A reference-typed field was given something that is not a
    /// reference.
    NotAReference {
        /// What was found instead, named as WCL spells it.
        kind: String,
        #[label("not a reference")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("'{unit}' is not a unit of type '{ty}'")]
    #[diagnostic(
        code(wcl::eval::unit_no_match),
        help(
            "declare it with `@unit(\"{unit}\", <factor>)` on the type alias, or use one of its declared units"
        )
    )]
    /// A unit-suffixed literal named a unit that the field's
    /// declared type does not declare via `@unit`.
    UnitNoMatch {
        /// The unit suffix that was written.
        unit: String,
        /// The type involved, as WCL spells it.
        ty: String,
        #[label("unknown unit for this type")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("unit literal '{unit}' has no declared type to resolve against")]
    #[diagnostic(
        code(wcl::eval::unit_without_type),
        help("assign it to a field or binding whose type carries `@unit(...)` declarations")
    )]
    /// A unit-suffixed literal appeared where no declared type is
    /// in context, so there is nothing to resolve the unit against.
    UnitWithoutType {
        /// The unit suffix that was written.
        unit: String,
        #[label("needs a unit-bearing type in context")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("no expander is registered for the `@contextual` block kind '{kind}'")]
    #[diagnostic(
        code(wcl::eval::missing_expander),
        help(
            "a `@contextual` block generates its children at expansion time; open the document \
             with the host environment that registers the expander (`Environment::set_expander`)"
        )
    )]
    /// A `@contextual` block's kind has no expander registered by
    /// the host, so its children cannot be produced.
    MissingExpander {
        /// What was found instead, named as WCL spells it.
        kind: String,
        #[label("this block's generated children were demanded")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },
}

impl EvalError {
    /// Build an [`EvalError::UnitNoMatch`], listing the units the type
    /// does declare so the message can suggest them.
    pub(crate) fn unit_no_match(
        unit: impl Into<String>,
        ty: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::UnitNoMatch {
            unit: unit.into(),
            ty: ty.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnitWithoutType`].
    pub(crate) fn unit_without_type(unit: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnitWithoutType {
            unit: unit.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::NotALeaf`].
    pub(crate) fn not_a_leaf(kind: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::NotALeaf {
            kind: kind.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::ImportFailed`].
    pub(crate) fn import_failed(
        path: impl Into<String>,
        message: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::ImportFailed {
            path: path.into(),
            message: message.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::SchemaViolation`] of the given kind.
    pub(crate) fn schema_violation(
        kind: SchemaViolationKind,
        message: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::SchemaViolation {
            kind,
            detail: None,
            message: message.into(),
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Like [`schema_violation`](Self::schema_violation) but records the
    /// offending identifier (`detail`) so consumers (e.g. LSP code
    /// actions) can act on it structurally.
    pub(crate) fn schema_violation_named(
        kind: SchemaViolationKind,
        message: impl Into<String>,
        name: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::SchemaViolation {
            kind,
            detail: Some(name.into()),
            message: message.into(),
            origin: None,
            span: span_to_miette(span),
        }
    }

    #[allow(dead_code)]
    /// Attach provenance to a schema violation so the diagnostic can
    /// render the offending source. A no-op on every other variant.
    pub(crate) fn with_schema_source(self, source: NamedSource<String>) -> Self {
        match self {
            Self::SchemaViolation {
                kind,
                detail,
                message,
                span,
                ..
            } => Self::SchemaViolation {
                kind,
                detail,
                message,
                origin: Some(std::sync::Arc::new(
                    SchemaDiagnosticSource::from_named_source(source),
                )),
                span,
            },
            other => other,
        }
    }

    #[allow(dead_code)]
    /// The provenance attached by [`Self::with_schema_source`], if any.
    pub(crate) fn schema_source(&self) -> Option<NamedSource<String>> {
        match self {
            Self::SchemaViolation {
                origin: Some(source),
                ..
            } => Some(source.named_source()),
            _ => None,
        }
    }

    /// Build a `SchemaViolation` and push it onto `out`. Tiny wrapper
    /// to keep `doc.rs` validators readable — every collection site
    /// was doing `out.push(EvalError::schema_violation(...))`.
    pub(crate) fn push_schema_violation(
        out: &mut Vec<EvalError>,
        kind: SchemaViolationKind,
        message: impl Into<String>,
        span: crate::ast::Span,
    ) {
        out.push(Self::schema_violation(kind, message, span));
    }

    /// Build an [`EvalError::UnknownBuiltin`].
    pub(crate) fn unknown_builtin(name: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnknownBuiltin {
            name: name.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::BuiltinArity`].
    pub(crate) fn builtin_arity(
        name: impl Into<String>,
        expected: usize,
        got: usize,
        span: crate::ast::Span,
    ) -> Self {
        Self::BuiltinArity {
            name: name.into(),
            expected,
            got,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::BuiltinTypeMismatch`].
    pub(crate) fn builtin_type(
        name: impl Into<String>,
        message: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::BuiltinTypeMismatch {
            name: name.into(),
            message: message.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::NonCallable`].
    pub(crate) fn non_callable(span: crate::ast::Span) -> Self {
        Self::NonCallable {
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::CallArity`].
    pub(crate) fn call_arity(expected: usize, got: usize, span: crate::ast::Span) -> Self {
        Self::CallArity {
            expected,
            got,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::CallDepthExceeded`].
    pub(crate) fn call_depth_exceeded(max: usize, span: crate::ast::Span) -> Self {
        Self::CallDepthExceeded {
            max,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::MatchNoArm`].
    pub(crate) fn match_no_arm(span: crate::ast::Span) -> Self {
        Self::MatchNoArm {
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::GuardNotBool`].
    pub(crate) fn guard_not_bool(kind: &'static str, span: crate::ast::Span) -> Self {
        Self::GuardNotBool {
            kind,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnknownUnion`].
    pub(crate) fn unknown_union(path: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnknownUnion {
            path: path.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnknownVariant`].
    pub(crate) fn unknown_variant(
        union: impl Into<String>,
        variant: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::UnknownVariant {
            union: union.into(),
            variant: variant.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::VariantShapeMismatch`].
    pub(crate) fn variant_shape_mismatch(
        expected: impl Into<String>,
        got: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::VariantShapeMismatch {
            expected: expected.into(),
            got: got.into(),
            span: span_to_miette(span),
        }
    }

    /// A host- or user-raised evaluation error (the `error()` builtin's
    /// shape). Public so hosts (e.g. the wdoc renderer) can record
    /// their own diagnostics through the same channel.
    pub fn user_error(message: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UserError {
            message: message.into(),
            span: span_to_miette(span),
        }
    }

    /// A `@contextual` block's generated children were demanded from a
    /// document opened without an [`Expander`](crate::Expander).
    pub(crate) fn missing_expander(kind: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::MissingExpander {
            kind: kind.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnionCycle`].
    pub(crate) fn union_cycle(union: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnionCycle {
            union: union.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnresolvedReference`].
    pub(crate) fn unresolved_reference(path: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnresolvedReference {
            path: path.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::NotAReference`].
    pub(crate) fn not_a_reference(kind: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::NotAReference {
            kind: kind.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::TypeMismatch`].
    pub(crate) fn type_mismatch(
        op: impl Into<String>,
        lhs_type: impl Into<String>,
        rhs_type: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::TypeMismatch {
            op: op.into(),
            lhs_type: lhs_type.into(),
            rhs_type: rhs_type.into(),
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::Arithmetic`] carrying the given fault.
    pub(crate) fn arithmetic(
        op: impl Into<String>,
        fault: ArithmeticFault,
        span: crate::ast::Span,
    ) -> Self {
        Self::Arithmetic {
            op: op.into(),
            fault,
            span: span_to_miette(span),
        }
    }
}

/// Convert a byte-range [`crate::ast::Span`] into the `miette`
/// equivalent used by every diagnostic in this module.
fn span_to_miette(span: crate::ast::Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.len().max(1))
}
