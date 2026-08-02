#![allow(unused_assignments)] // miette/thiserror derive triggers spurious lints on variant fields

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiagnosticSource {
    name: String,
    text: String,
}

impl SchemaDiagnosticSource {
    fn from_named_source(source: NamedSource<String>) -> Self {
        Self {
            name: source.name().to_string(),
            text: source.inner().clone(),
        }
    }

    pub(crate) fn named_source(&self) -> NamedSource<String> {
        NamedSource::new(&self.name, self.text.clone())
    }
}

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

    #[error("operator '{op}' cannot {fault}")]
    #[diagnostic(code(wcl::eval::arithmetic))]
    Arithmetic {
        op: String,
        /// Which fault, so tools can act on it without parsing the
        /// rendered message.
        fault: ArithmeticFault,
        #[label("no result for these operands")]
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
        #[doc(hidden)]
        origin: Option<std::sync::Arc<SchemaDiagnosticSource>>,
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

    #[error("'{unit}' is not a unit of type '{ty}'")]
    #[diagnostic(
        code(wcl::eval::unit_no_match),
        help(
            "declare it with `@unit(\"{unit}\", <factor>)` on the type alias, or use one of its declared units"
        )
    )]
    UnitNoMatch {
        unit: String,
        ty: String,
        #[label("unknown unit for this type")]
        span: SourceSpan,
    },

    #[error("unit literal '{unit}' has no declared type to resolve against")]
    #[diagnostic(
        code(wcl::eval::unit_without_type),
        help("assign it to a field or binding whose type carries `@unit(...)` declarations")
    )]
    UnitWithoutType {
        unit: String,
        #[label("needs a unit-bearing type in context")]
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
    MissingExpander {
        kind: String,
        #[label("this block's generated children were demanded")]
        span: SourceSpan,
    },
}

/// Why an arithmetic operator had no answer for operands it *did* accept.
/// Distinct from a type mismatch: the operands were compatible, the result
/// simply isn't representable (or, for `/` and `%`, isn't defined at all).
///
/// Structured rather than pre-formatted so tools can act on it without
/// parsing the message — the same reason [`SchemaViolationKind`] exists.
/// `Display` is the phrasing that follows "cannot", and is the one copy of
/// that wording: `EvalError::Arithmetic` and the `sum` builtin both render
/// through it, so one fault reads the same wherever it surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArithmeticFault {
    /// `/` or `%` with a zero divisor — undefined, not merely
    /// unrepresentable, so no wider type would rescue it.
    DivideByZero,
    /// The result doesn't fit the numeric variant the operands share,
    /// carried in `ty` as WCL spells it (`i8`, `usize`).
    Overflow { ty: String },
}

impl ArithmeticFault {
    pub fn overflow(ty: impl Into<String>) -> Self {
        Self::Overflow { ty: ty.into() }
    }
}

impl std::fmt::Display for ArithmeticFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DivideByZero => f.write_str("divide by zero"),
            Self::Overflow { ty } => write!(f, "represent the result in {ty} (overflow)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaViolationKind {
    /// A decorator name that resolves to no type carrying a matching
    /// `@decorator("name")` declaration.
    UndeclaredDecorator,
    /// A declared decorator was written in a syntactic position excluded
    /// by its decorator schema's `@applies_to` declaration.
    DecoratorNotApplicable,
    /// An `@applies_to` declaration is internally inconsistent or attached
    /// to a type that declares no decorator schema.
    InvalidDecoratorApplicability,
    /// A non-repeatable decorator occurred more than once on one syntax
    /// node. The violation points at the repeated occurrence.
    DecoratorCardinality,
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
    /// Two sibling blocks of the same kind share an identity label —
    /// the kind's schema declares an `@inline(0) identifier` field, so
    /// the label is an id and a duplicate makes every reference to it
    /// ambiguous. Kinds whose label isn't identifier-typed (`code wcl`,
    /// `li`, …) repeat freely.
    DuplicateBlockId,
    /// A kind declared by a `@declares_kind` instance collides with a
    /// kind declared by a `@block`/`@table` type. The dispatch paths
    /// disagree on the winner (expansion prefers the declarer, schema
    /// lookup the declared type), so instances behave incoherently —
    /// the collision itself is the error.
    DeclaredKindCollision,
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
    /// A field declared with a `symbol_set` type was assigned a symbol
    /// that isn't one of the set's members.
    SymbolNotInSet,
    /// A field carrying `@ref("kind")` holds an id that doesn't name any
    /// existing block of that kind — a dangling reference.
    DanglingReference,
    /// Two `@document` schemas that co-govern a namespace declare the
    /// same field name and at least one side is a gather slot
    /// (`@child`/`@children`). The merged document schema resolves the
    /// name to only one declaration, so the other schema's gathered
    /// blocks silently vanish from templates iterating the field.
    /// Reported by [`Document::schema_warnings`], never by
    /// `schema_errors` — merging is a designed feature and existing
    /// documents must keep building.
    DocumentFieldShadow,
}

impl EvalError {
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

    pub(crate) fn unit_without_type(unit: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnitWithoutType {
            unit: unit.into(),
            span: span_to_miette(span),
        }
    }

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

fn span_to_miette(span: crate::ast::Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.len().max(1))
}
