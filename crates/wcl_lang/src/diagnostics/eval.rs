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

use std::sync::Arc;

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use super::{ArithmeticFault, SchemaViolationKind};

/// The file an error was raised in: its name and full text, so the
/// diagnostic renders against that file rather than whichever source
/// the host happens to have open.
///
/// A document is a root source plus its imports, and a span alone does
/// not say which of them it indexes into. Carried on every
/// [`EvalError`] variant and read back through [`EvalError::origin`].
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct DiagnosticSource(NamedSource<Arc<str>>);

impl DiagnosticSource {
    /// Rebuild the `NamedSource` for rendering.
    pub(crate) fn named_source(&self) -> NamedSource<Arc<str>> {
        self.0.clone()
    }
}

/// Two provenances are equal when they name the same file with the
/// same text. The text is shared, so the pointer test settles almost
/// every comparison without reading it.
impl PartialEq for DiagnosticSource {
    fn eq(&self, other: &Self) -> bool {
        self.0.name() == other.0.name()
            && (Arc::ptr_eq(self.0.inner(), other.0.inner()) || self.0.inner() == other.0.inner())
    }
}

impl Eq for DiagnosticSource {}

impl miette::SourceCode for DiagnosticSource {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn miette::SpanContents<'a> + 'a>, miette::MietteError> {
        self.0
            .read_span(span, context_lines_before, context_lines_after)
    }
}

#[derive(Debug, Clone, PartialEq, Error, Diagnostic)]
/// A failure while evaluating a document: a bad expression, a broken
/// reference, or a schema violation.
///
/// Every variant carries a span so the diagnostic can point at the text
/// responsible. Errors are cached per field, so a field that fails
/// reports the same error on every later read rather than being retried.
#[non_exhaustive]
pub enum EvalError {
    #[error("cycle while evaluating '{field}'")]
    #[diagnostic(code(wcl::eval::cycle))]
    /// A field's evaluation depends on itself, directly or through a
    /// chain of other fields. The cycle poisons only its own loop.
    Cycle {
        /// Name of the field involved.
        field: String,
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
        #[label("invalid argument(s)")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("callee is not callable")]
    #[diagnostic(code(wcl::eval::non_callable))]
    /// The callee of a call expression evaluated to something that
    /// is not a function.
    NonCallable {
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
        #[label("wrong number of arguments")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("call depth limit exceeded (max {max})")]
    #[diagnostic(code(wcl::eval::call_depth_exceeded))]
    /// A `fn` call nested deeper than the evaluator's limit, which calls
    /// share with the fields and `let`s they force — the guard against
    /// unbounded recursion.
    CallDepthExceeded {
        /// The limit that was exceeded.
        max: usize,
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
        #[label("function call recurses too deeply")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("evaluation depth limit exceeded (max {max})")]
    #[diagnostic(
        code(wcl::eval::depth_exceeded),
        help(
            "a chain of references nests deeper than the evaluator allows; break it into shorter chains"
        )
    )]
    /// Field and `let` evaluations nested deeper than the evaluator's
    /// limit (which `fn` calls count towards too) — a long chain of
    /// references that never loops back, which would otherwise overflow
    /// the thread's stack.
    EvalDepthExceeded {
        /// The limit that was exceeded.
        max: usize,
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
        #[label("evaluation nests too deeply here")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("no match arm fits the value")]
    #[diagnostic(code(wcl::eval::match_no_arm))]
    /// No arm of a `match` matched the scrutinee, and no arm was a
    /// catch-all.
    MatchNoArm {
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[help]
        /// A likely fix, when the operands say what was meant: `2e-3`
        /// is the literal `2e` minus `3`, not scientific notation.
        help: Option<String>,
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
        #[label("incompatible operands")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("operator '{op}' is not defined for {operand_type}")]
    #[diagnostic(code(wcl::eval::type_mismatch))]
    /// An operator that reads one operand at a time — prefix `-` and `!`,
    /// or an operand of `&&` / `||` that is not a `bool` — was applied to
    /// a type it is not defined for. Shares the `type_mismatch` code with
    /// [`EvalError::TypeMismatch`], its two-operand counterpart.
    UnaryTypeMismatch {
        /// The operator that was applied.
        op: String,
        /// Type of the offending operand, as WCL spells it.
        operand_type: String,
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
        #[label("incompatible operand")]
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
        #[label("not a reference")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("'{unit}' is not a unit of type '{ty}'")]
    #[diagnostic(code(wcl::eval::unit_no_match))]
    /// A unit-suffixed literal named a unit that the field's
    /// declared type does not declare via `@unit`.
    UnitNoMatch {
        /// The unit suffix that was written.
        unit: String,
        /// The type involved, as WCL spells it.
        ty: String,
        #[help]
        /// What to do about it: declare the unit on the type, or, when
        /// the suffix is an exponent written without a decimal point
        /// (`1e39`), write the number as a float (`1.0e39`).
        help: String,
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
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
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
        #[label("this block's generated children were demanded")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },

    #[error("expanding the `@contextual` block '{kind}' exceeded {limit}")]
    #[diagnostic(
        code(wcl::eval::expansion_limit),
        help(
            "a `@contextual` block that expands into itself, or nested repetitions that \
             multiply, generate blocks without bound; make the recursion stop or shrink the data"
        )
    )]
    /// Nested `@contextual` expansion went past the nesting-depth cap
    /// or generated more blocks than the total cap.
    ExpansionLimit {
        /// The kind of the block whose expansion hit the limit.
        kind: String,
        /// Which limit, in words (`the nesting depth limit of 32`).
        limit: String,
        #[doc(hidden)]
        #[source_code]
        /// The file the error was raised in, when known. Read it
        /// through [`EvalError::origin`].
        origin: Option<Arc<DiagnosticSource>>,
        #[label("expansion stopped here")]
        /// Source span the diagnostic points at.
        span: SourceSpan,
    },
}

impl EvalError {
    /// Build an [`EvalError::UnitNoMatch`], listing the units the type
    /// does declare so the message can suggest them.
    ///
    /// `magnitude` is the number the unit was written after, when there
    /// is one: a suffix like `e39` after an integer is an exponent missing
    /// its decimal point, and the help says how to write it.
    pub(crate) fn unit_no_match(
        unit: impl Into<String>,
        ty: impl Into<String>,
        magnitude: Option<&crate::Value>,
        span: crate::ast::Span,
    ) -> Self {
        let unit = unit.into();
        let help = magnitude
            .and_then(|magnitude| scientific_notation_help(magnitude, &unit))
            .unwrap_or_else(|| {
                format!(
                    "declare it with `@unit(\"{unit}\", <factor>)` on the type alias, or use \
                     one of its declared units"
                )
            });
        Self::UnitNoMatch {
            unit,
            ty: ty.into(),
            help,
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnitWithoutType`].
    pub(crate) fn unit_without_type(unit: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnitWithoutType {
            unit: unit.into(),
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::NotALeaf`].
    pub(crate) fn not_a_leaf(kind: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::NotALeaf {
            kind: kind.into(),
            origin: None,
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
            origin: None,
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

    /// Attach the file this error was raised in, so the diagnostic
    /// renders against that file. A no-op on an error that already
    /// carries a source: the code that raised it knew its file, and an
    /// enclosing scope that passes it on (a function call returning its
    /// body's error, a parent block gathering its children's errors, a
    /// field whose expression read an erroring field) must not overwrite
    /// that with its own. The innermost origin wins.
    pub(crate) fn with_origin(mut self, source: &NamedSource<Arc<str>>) -> Self {
        self.attach_origin(source);
        self
    }

    /// In-place form of [`Self::with_origin`].
    pub(crate) fn attach_origin(&mut self, source: &NamedSource<Arc<str>>) {
        let slot = self.origin_slot();
        if slot.is_none() {
            *slot = Some(Arc::new(DiagnosticSource(source.clone())));
        }
    }

    /// The file this error was raised in — the root document or the
    /// imported file that holds the offending text — as the
    /// `NamedSource` its span indexes into. `None` when the library does
    /// not know it: an error against a declaration it synthesised rather
    /// than read from a file, or one a host built itself.
    ///
    /// `EvalError`'s [`Diagnostic::source_code`] returns the same
    /// source, so a `miette::Report` of the error renders its snippet
    /// without the host attaching one.
    pub fn origin(&self) -> Option<NamedSource<Arc<str>>> {
        self.origin_ref().map(|source| source.named_source())
    }

    /// The `origin` field, whichever variant this is.
    fn origin_ref(&self) -> Option<&DiagnosticSource> {
        match self {
            Self::Cycle { origin, .. }
            | Self::UnknownBuiltin { origin, .. }
            | Self::BuiltinArity { origin, .. }
            | Self::BuiltinTypeMismatch { origin, .. }
            | Self::NonCallable { origin, .. }
            | Self::CallArity { origin, .. }
            | Self::CallDepthExceeded { origin, .. }
            | Self::EvalDepthExceeded { origin, .. }
            | Self::MatchNoArm { origin, .. }
            | Self::GuardNotBool { origin, .. }
            | Self::UnknownUnion { origin, .. }
            | Self::UnknownVariant { origin, .. }
            | Self::VariantShapeMismatch { origin, .. }
            | Self::UserError { origin, .. }
            | Self::UnionCycle { origin, .. }
            | Self::TypeMismatch { origin, .. }
            | Self::UnaryTypeMismatch { origin, .. }
            | Self::Arithmetic { origin, .. }
            | Self::NotALeaf { origin, .. }
            | Self::ImportFailed { origin, .. }
            | Self::SchemaViolation { origin, .. }
            | Self::UnresolvedReference { origin, .. }
            | Self::NotAReference { origin, .. }
            | Self::UnitNoMatch { origin, .. }
            | Self::UnitWithoutType { origin, .. }
            | Self::MissingExpander { origin, .. }
            | Self::ExpansionLimit { origin, .. } => origin.as_deref(),
        }
    }

    /// Mutable access to the `origin` field, whichever variant this is.
    fn origin_slot(&mut self) -> &mut Option<Arc<DiagnosticSource>> {
        match self {
            Self::Cycle { origin, .. }
            | Self::UnknownBuiltin { origin, .. }
            | Self::BuiltinArity { origin, .. }
            | Self::BuiltinTypeMismatch { origin, .. }
            | Self::NonCallable { origin, .. }
            | Self::CallArity { origin, .. }
            | Self::CallDepthExceeded { origin, .. }
            | Self::EvalDepthExceeded { origin, .. }
            | Self::MatchNoArm { origin, .. }
            | Self::GuardNotBool { origin, .. }
            | Self::UnknownUnion { origin, .. }
            | Self::UnknownVariant { origin, .. }
            | Self::VariantShapeMismatch { origin, .. }
            | Self::UserError { origin, .. }
            | Self::UnionCycle { origin, .. }
            | Self::TypeMismatch { origin, .. }
            | Self::UnaryTypeMismatch { origin, .. }
            | Self::Arithmetic { origin, .. }
            | Self::NotALeaf { origin, .. }
            | Self::ImportFailed { origin, .. }
            | Self::SchemaViolation { origin, .. }
            | Self::UnresolvedReference { origin, .. }
            | Self::NotAReference { origin, .. }
            | Self::UnitNoMatch { origin, .. }
            | Self::UnitWithoutType { origin, .. }
            | Self::MissingExpander { origin, .. }
            | Self::ExpansionLimit { origin, .. } => origin,
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
            origin: None,
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
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::BuiltinTypeMismatch`].
    ///
    /// The rendered error already leads with `'{name}': `, so a message
    /// that opens with its own `name: ` (the convention builtin bodies
    /// follow, since they report through a bare `String`) has that prefix
    /// dropped rather than printed twice.
    pub(crate) fn builtin_type(
        name: impl Into<String>,
        message: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        let name = name.into();
        let mut message = message.into();
        if let Some(rest) = message
            .strip_prefix(name.as_str())
            .and_then(|rest| rest.strip_prefix(": "))
        {
            message = rest.to_string();
        }
        Self::BuiltinTypeMismatch {
            name,
            message,
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::NonCallable`].
    pub(crate) fn non_callable(span: crate::ast::Span) -> Self {
        Self::NonCallable {
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::CallArity`].
    pub(crate) fn call_arity(expected: usize, got: usize, span: crate::ast::Span) -> Self {
        Self::CallArity {
            expected,
            got,
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::CallDepthExceeded`].
    pub(crate) fn call_depth_exceeded(max: usize, span: crate::ast::Span) -> Self {
        Self::CallDepthExceeded {
            max,
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::EvalDepthExceeded`].
    pub(crate) fn eval_depth_exceeded(max: usize, span: crate::ast::Span) -> Self {
        Self::EvalDepthExceeded {
            max,
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::MatchNoArm`].
    pub(crate) fn match_no_arm(span: crate::ast::Span) -> Self {
        Self::MatchNoArm {
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::GuardNotBool`].
    pub(crate) fn guard_not_bool(kind: &'static str, span: crate::ast::Span) -> Self {
        Self::GuardNotBool {
            kind,
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnknownUnion`].
    pub(crate) fn unknown_union(path: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnknownUnion {
            path: path.into(),
            origin: None,
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
            origin: None,
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
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// A host- or user-raised evaluation error (the `error()` builtin's
    /// shape). Public so hosts (e.g. the wdoc renderer) can record
    /// their own diagnostics through the same channel.
    pub fn user_error(message: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UserError {
            message: message.into(),
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// A `@contextual` block's generated children were demanded from a
    /// document opened without an [`Expander`](crate::Expander).
    pub(crate) fn missing_expander(kind: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::MissingExpander {
            kind: kind.into(),
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::ExpansionLimit`]; `limit` names the cap in
    /// words.
    pub fn expansion_limit(
        kind: impl Into<String>,
        limit: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::ExpansionLimit {
            kind: kind.into(),
            limit: limit.into(),
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnionCycle`].
    pub(crate) fn union_cycle(union: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnionCycle {
            union: union.into(),
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnresolvedReference`].
    pub(crate) fn unresolved_reference(path: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnresolvedReference {
            path: path.into(),
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::NotAReference`].
    pub(crate) fn not_a_reference(kind: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::NotAReference {
            kind: kind.into(),
            origin: None,
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
            help: None,
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Build an [`EvalError::UnaryTypeMismatch`].
    pub(crate) fn unary_type_mismatch(
        op: impl Into<String>,
        operand_type: impl Into<String>,
        span: crate::ast::Span,
    ) -> Self {
        Self::UnaryTypeMismatch {
            op: op.into(),
            operand_type: operand_type.into(),
            origin: None,
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
            origin: None,
            span: span_to_miette(span),
        }
    }

    /// Add `text` as the help of a [`EvalError::TypeMismatch`]. Any other
    /// variant comes back as it is.
    pub(crate) fn with_type_mismatch_help(mut self, text: String) -> Self {
        if let Self::TypeMismatch { help, .. } = &mut self {
            *help = Some(text);
        }
        self
    }
}

/// The help for a number whose unit suffix is really an exponent: `e39`
/// after `1`, or `e-3` after `2`. WCL reads an exponent only after a
/// decimal point, so `1e39` is the number `1` with the unit `e39`; the
/// help spells the float the author meant. `None` when `exponent` is not
/// `e`/`E`, an optional sign and digits, or `magnitude` is not an
/// integer.
pub(crate) fn scientific_notation_help(magnitude: &crate::Value, exponent: &str) -> Option<String> {
    use crate::Value;
    let signed = exponent.strip_prefix(['e', 'E'])?;
    let digits = signed.strip_prefix(['+', '-']).unwrap_or(signed);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let whole = match magnitude {
        Value::I8(_)
        | Value::I16(_)
        | Value::I32(_)
        | Value::I64(_)
        | Value::I128(_)
        | Value::Isize(_)
        | Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_)
        | Value::Usize(_) => magnitude.to_string(),
        _ => return None,
    };
    Some(format!(
        "scientific notation needs a decimal point: write {whole}.0{exponent}"
    ))
}

/// Convert a byte-range [`crate::ast::Span`] into the `miette`
/// equivalent used by every diagnostic in this module.
fn span_to_miette(span: crate::ast::Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.len().max(1))
}
