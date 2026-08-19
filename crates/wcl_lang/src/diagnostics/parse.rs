//! Failures that stop a source becoming a syntax tree.
//!
//! [`ParseError`] is what [`Document::open`](crate::Document::open) and
//! [`parse_for_edit`](crate::parse_for_edit) return: the file could not
//! be read, or it did not parse. [`SyntaxError`] is the rendered form of
//! a single parse failure, carrying the source and the span so `miette`
//! can print it with the offending line underneath.
//!
//! These are the errors raised *before* a document exists. Everything
//! that can go wrong once it does is an
//! [`EvalError`](super::EvalError).

#![allow(unused_assignments)] // miette/thiserror derive triggers spurious lints on variant fields

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
/// A failure to turn source text into a syntax tree: either the file
/// could not be read, or it did not parse.
pub enum ParseError {
    #[error("io error: {0}")]
    /// The source could not be read from disk.
    Io(#[from] std::io::Error),

    #[error("{0}")]
    #[diagnostic(transparent)]
    /// The source was read but did not parse. Boxed to keep
    /// `ParseError` small, since the syntax case carries its source text.
    Syntax(Box<SyntaxError>),
}

impl ParseError {
    /// Build a [`ParseError::Syntax`] with a single primary label.
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
/// One parse failure, carrying enough context for `miette` to render
/// the offending source with a caret and, optionally, a second label
/// pointing at a related site.
pub struct SyntaxError {
    /// The rendered message.
    pub message: String,
    #[source_code]
    /// The source text the span indexes into, for rendering.
    pub src: NamedSource<String>,
    #[label("{label}")]
    /// Source span the diagnostic points at.
    pub span: SourceSpan,
    /// Text of the primary label.
    pub label: String,
    #[label("{related_label}")]
    /// Optional secondary span — a prior occurrence.
    pub related_span: Option<SourceSpan>,
    /// Text of the secondary label.
    pub related_label: String,
}
