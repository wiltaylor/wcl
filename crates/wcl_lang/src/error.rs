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

    #[error("{message}")]
    #[diagnostic(code(wcl::eval::unsupported))]
    Unsupported {
        message: String,
        #[label("not evaluable yet")]
        span: SourceSpan,
    },

    #[error("unbound identifier '{name}'")]
    #[diagnostic(code(wcl::eval::unbound))]
    UnboundIdentifier {
        name: String,
        #[label("not in scope")]
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

    #[error("callee is not a built-in function")]
    #[diagnostic(code(wcl::eval::non_callable))]
    NonCallable {
        #[label("not callable")]
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
}

impl EvalError {
    #[allow(dead_code)] // reserved for future evaluator gaps
    pub(crate) fn new(message: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::Unsupported {
            message: message.into(),
            span: span_to_miette(span),
        }
    }

    #[allow(dead_code)] // reserved for strict unbound-id mode
    pub(crate) fn unbound(name: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::UnboundIdentifier {
            name: name.into(),
            span: span_to_miette(span),
        }
    }

    pub(crate) fn not_a_leaf(kind: impl Into<String>, span: crate::ast::Span) -> Self {
        Self::NotALeaf {
            kind: kind.into(),
            span: span_to_miette(span),
        }
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
