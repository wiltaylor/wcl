#![allow(unused_assignments)] // miette/thiserror derive triggers spurious lints on variant fields

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum ParseError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    #[diagnostic(transparent)]
    Syntax(Box<SyntaxError>),
}

impl ParseError {
    pub(crate) fn syntax(
        message: String,
        src: NamedSource<String>,
        span: SourceSpan,
        label: String,
    ) -> Self {
        Self::Syntax(Box::new(SyntaxError {
            message,
            src,
            span,
            label,
            related_span: None,
            related_label: String::new(),
        }))
    }

    /// Like [`Self::syntax`] but attaches a secondary `related` label
    /// pointing at a prior occurrence (e.g. the original site of a
    /// duplicate declaration).
    pub(crate) fn syntax_with_related(
        message: String,
        src: NamedSource<String>,
        span: SourceSpan,
        label: String,
        related_span: SourceSpan,
        related_label: String,
    ) -> Self {
        Self::Syntax(Box::new(SyntaxError {
            message,
            src,
            span,
            label,
            related_span: Some(related_span),
            related_label,
        }))
    }
}

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(wcl::parse))]
pub struct SyntaxError {
    pub message: String,
    #[source_code]
    pub src: NamedSource<String>,
    #[label("{label}")]
    pub span: SourceSpan,
    pub label: String,
    #[label("{related_label}")]
    pub related_span: Option<SourceSpan>,
    pub related_label: String,
}

#[derive(Debug, Clone, PartialEq, Error, Diagnostic)]
pub enum EvalError {
    #[error("cycle while evaluating '{field}'")]
    #[diagnostic(code(wcl::eval::cycle))]
    Cycle {
        field: String,
        #[label("evaluated recursively")]
        span: SourceSpan,
    },

    #[error("unknown built-in '{name}'")]
    #[diagnostic(code(wcl::eval::unknown_builtin))]
    UnknownBuiltin {
        name: String,
        #[label("no builtin with this name")]
        span: SourceSpan,
    },

    #[error("'{name}' expects {expected} argument(s), got {got}")]
    #[diagnostic(code(wcl::eval::builtin_arity))]
    BuiltinArity {
        name: String,
        expected: usize,
        got: usize,
        #[label("wrong number of arguments")]
        span: SourceSpan,
    },

    #[error("'{name}': {message}")]
    #[diagnostic(code(wcl::eval::builtin_type))]
    BuiltinTypeMismatch {
        name: String,
        message: String,
        #[label("invalid argument(s)")]
        span: SourceSpan,
    },

    #[error("callee is not callable")]
    #[diagnostic(code(wcl::eval::non_callable))]
    NonCallable {
        #[label("not callable")]
        span: SourceSpan,
    },

    #[error("call expected {expected} argument(s), got {got}")]
    #[diagnostic(code(wcl::eval::call_arity))]
    CallArity {
        expected: usize,
        got: usize,
        #[label("wrong number of arguments")]
        span: SourceSpan,
    },

    #[error("call depth limit exceeded (max {max})")]
    #[diagnostic(code(wcl::eval::call_depth_exceeded))]
    CallDepthExceeded {
        max: usize,
        #[label("function call recurses too deeply")]
        span: SourceSpan,
    },

    #[error("no match arm fits the value")]
    #[diagnostic(code(wcl::eval::match_no_arm))]
    MatchNoArm {
        #[label("no arm matched")]
        span: SourceSpan,
    },

    #[error("match guard must return bool, got {kind}")]
    #[diagnostic(code(wcl::eval::guard_not_bool))]
    GuardNotBool {
        kind: &'static str,
        #[label("guard expression is not a bool")]
        span: SourceSpan,
    },

    #[error("unknown union '{path}'")]
    #[diagnostic(code(wcl::eval::unknown_union))]
    UnknownUnion {
        path: String,
        #[label("no union with this name in scope")]
        span: SourceSpan,
    },

    #[error("union '{union}' has no variant named '{variant}'")]
    #[diagnostic(code(wcl::eval::unknown_variant))]
    UnknownVariant {
        union: String,
        variant: String,
        #[label("not a variant of this union")]
        span: SourceSpan,
    },

    #[error("variant shape mismatch: expected {expected}, got {got}")]
    #[diagnostic(code(wcl::eval::variant_shape_mismatch))]
    VariantShapeMismatch {
        expected: String,
        got: String,
        #[label("argument shape does not match the variant body")]
        span: SourceSpan,
    },

    #[error("error: {message}")]
    #[diagnostic(code(wcl::eval::user_error))]
    UserError {
        message: String,
        #[label("error raised here")]
        span: SourceSpan,
    },

    #[error("union '{union}' has a cyclic 'extends' chain")]
    #[diagnostic(code(wcl::eval::union_cycle))]
    UnionCycle {
        union: String,
        #[label("cyclic extends")]
        span: SourceSpan,
    },

    #[error("operator '{op}' is not defined for {lhs_type} and {rhs_type}")]
    #[diagnostic(code(wcl::eval::type_mismatch))]
    TypeMismatch {
        op: String,
        lhs_type: String,
        rhs_type: String,
        #[label("incompatible operands")]
        span: SourceSpan,
    },

    #[error("cannot evaluate {kind} as a leaf value")]
    #[diagnostic(code(wcl::eval::not_a_leaf))]
    NotALeaf {
        kind: String,
        #[label("not a leaf")]
        span: SourceSpan,
    },

    #[error("failed to import '{path}': {message}")]
    #[diagnostic(code(wcl::eval::import_failed))]
    ImportFailed {
        path: String,
        message: String,
        #[label("import error")]
        span: SourceSpan,
    },

    #[error("{message}")]
    #[diagnostic(code(wcl::eval::schema_violation))]
    SchemaViolation {
        kind: SchemaViolationKind,
        /// The offending identifier (field / child block name) when the
        /// violation has one, so tools can act on it without parsing
        /// `message`. `None` for kinds that don't name a single token.
        detail: Option<String>,
        message: String,
        #[label("schema violation")]
        span: SourceSpan,
    },

    #[error("unresolved reference '{path}'")]
    #[diagnostic(code(wcl::eval::unresolved_reference))]
    UnresolvedReference {
        path: String,
        #[label("does not resolve")]
        span: SourceSpan,
    },

    #[error("expected a reference, got {kind}")]
    #[diagnostic(code(wcl::eval::not_a_reference))]
    NotAReference {
        kind: String,
        #[label("not a reference")]
        span: SourceSpan,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaViolationKind {
    DisallowedChild,
    MissingRequired,
    ChildrenTooFew,
    ChildrenTooMany,
    BlockChildrenOverflow,
    UnexpectedExtraChild,
    /// Block whose `kind` has no corresponding `@block`/`@table` type
    /// declaration anywhere in the document.
    UnregisteredKind,
    /// Field whose `name` isn't declared by the parent schema (or by
    /// the document schema, for top-level fields).
    UnknownField,
    /// Top-level value (field or block) but no `@document`-decorated
    /// type exists.
    NoDocumentSchema,
    /// More than one `@document`-decorated type declared in the
    /// document.
    MultipleDocumentSchemas,
    /// Two `@block`/`@table`/`@decorator` declarations carry the same
    /// kind string within the same namespace, so a reference to that
    /// kind is ambiguous. Declarations in *different* namespaces are
    /// fine — they're disambiguated by a `::` qualifier.
    DuplicateBlockKind,
    /// A `&Interface` reference field's target doesn't implement
    /// the interface (missing or differently-typed field), or a
    /// `&T` reference field's target isn't `T` and isn't a
    /// descendant of `T` via the `extends` chain.
    InterfaceNotImplemented,
    /// A field declared `: SomeUnion` was assigned a variant whose
    /// constructing union FQN differs.
    VariantUnionMismatch,
    /// A field's evaluated value doesn't match its declared
    /// `TypeRef` under the conservative `value_matches_type_ref` check
    /// (scalar / string / list-element / variant FQN).
    FieldTypeMismatch,
    /// A field's value violates a constraint decorator (`@min` /
    /// `@max` / `@non_empty`) declared on the field or on a type alias
    /// its declared type goes through.
    ConstraintViolation,
    /// Two variants in a union's effective list share a name (across
    /// the `extends` chain).
    DuplicateVariant,
    /// Two variants in the same effective list have identical bodies,
    /// making structural dispatch ambiguous.
    VariantShapeCollision,
    /// A block / decorator / table-row didn't match any variant of
    /// the declared union via structural dispatch.
    VariantNoMatch,
    /// Defensive: structural dispatch matched more than one variant.
    /// Should be unreachable after `VariantShapeCollision` declaration
    /// checks land.
    VariantAmbiguous,
    /// A connection statement's lhs or rhs identifier doesn't resolve
    /// to a block in scope.
    UnknownConnectionOperand,
    /// No declared `connection` schema accepts the resolved operand
    /// types of a connection statement.
    UnknownConnection,
    /// More than one declared `connection` schema matches the operand
    /// types of a connection statement.
    AmbiguousConnection,
    /// A connection statement's `:kind` symbol isn't a member of the
    /// matched schema's `kind_set`.
    UnknownConnectionKind,
}

impl EvalError {
    pub(crate) fn not_a_leaf(kind: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::NotALeaf {
            kind: kind.into(),
            span: span_to_miette(span),
        }
    }

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

    pub(crate) fn schema_violation(
        kind: SchemaViolationKind,
        message: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::SchemaViolation {
            kind,
            detail: None,
            message: message.into(),
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
            span: span_to_miette(span),
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

    pub(crate) fn unknown_builtin(name: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnknownBuiltin {
            name: name.into(),
            span: span_to_miette(span),
        }
    }

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

    pub(crate) fn non_callable(span: crate::ast::Span) -> Self {
        Self::NonCallable {
            span: span_to_miette(span),
        }
    }

    pub(crate) fn call_arity(expected: usize, got: usize, span: crate::ast::Span) -> Self {
        Self::CallArity {
            expected,
            got,
            span: span_to_miette(span),
        }
    }

    pub(crate) fn call_depth_exceeded(max: usize, span: crate::ast::Span) -> Self {
        Self::CallDepthExceeded {
            max,
            span: span_to_miette(span),
        }
    }

    pub(crate) fn match_no_arm(span: crate::ast::Span) -> Self {
        Self::MatchNoArm {
            span: span_to_miette(span),
        }
    }

    pub(crate) fn guard_not_bool(kind: &'static str, span: crate::ast::Span) -> Self {
        Self::GuardNotBool {
            kind,
            span: span_to_miette(span),
        }
    }

    pub(crate) fn unknown_union(path: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnknownUnion {
            path: path.into(),
            span: span_to_miette(span),
        }
    }

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

    pub(crate) fn user_error(message: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UserError {
            message: message.into(),
            span: span_to_miette(span),
        }
    }

    pub(crate) fn union_cycle(union: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnionCycle {
            union: union.into(),
            span: span_to_miette(span),
        }
    }

    pub(crate) fn unresolved_reference(path: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnresolvedReference {
            path: path.into(),
            span: span_to_miette(span),
        }
    }

    pub(crate) fn not_a_reference(kind: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::NotAReference {
            kind: kind.into(),
            span: span_to_miette(span),
        }
    }

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
}

fn span_to_miette(span: crate::ast::Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.len().max(1))
}
