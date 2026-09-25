//! Failures that stop a source becoming a syntax tree.
//!
//! [`ParseError`] is what [`Document::open`](crate::Document::open) and
//! [`parse_for_edit`](crate::parse_for_edit) return: the file could not
//! be read, or it did not parse. [`SyntaxError`] is the rendered form of
//! a parse failure, carrying the source and the span so `miette` can
//! print it with the offending line underneath.
//!
//! The parser does not stop at the first mistake. It recovers at the
//! next item and keeps going, so one [`ParseError::Syntax`] carries every
//! syntax error in the file: the first as the error itself, the rest in
//! [`SyntaxError::others`], which `miette` renders as related
//! diagnostics. [`ParseError::syntax_errors`] walks them all in source
//! order.
//!
//! These are the errors raised *before* a document exists. Everything
//! that can go wrong once it does is an
//! [`EvalError`](super::EvalError).

#![allow(unused_assignments)] // miette/thiserror derive triggers spurious lints on variant fields

use std::sync::Arc;

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
/// A failure to turn source text into a syntax tree: either the file
/// could not be read, or it did not parse.
#[non_exhaustive]
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
        src: NamedSource<Arc<str>>,
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
            others: Vec::new(),
        }))
    }

    /// Like [`Self::syntax`] but attaches a secondary `related` label
    /// pointing at a prior occurrence (e.g. the original site of a
    /// duplicate declaration).
    pub(crate) fn syntax_with_related(
        message: String,
        src: NamedSource<Arc<str>>,
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
            others: Vec::new(),
        }))
    }

    /// Fold the errors one parse collected into a single
    /// [`ParseError::Syntax`]: the first becomes the error, the rest its
    /// [`SyntaxError::others`]. `None` when the parse was clean.
    pub(crate) fn from_syntax_errors(errors: Vec<SyntaxError>) -> Option<Self> {
        let mut errors = errors.into_iter();
        let mut first = errors.next()?;
        first.others.extend(errors);
        Some(Self::Syntax(Box::new(first)))
    }

    /// Every syntax error this failure reports, in source order: the
    /// error itself, then its [`SyntaxError::others`]. Empty for
    /// [`ParseError::Io`].
    pub fn syntax_errors(&self) -> impl Iterator<Item = &SyntaxError> {
        let first = match self {
            Self::Syntax(syntax) => Some(syntax.as_ref()),
            Self::Io(_) => None,
        };
        first
            .into_iter()
            .flat_map(|syntax| std::iter::once(syntax).chain(&syntax.others))
    }
}

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(wcl::parse))]
/// One parse failure, carrying enough context for `miette` to render
/// the offending source with a caret and, optionally, a second label
/// pointing at a related site. The first failure of a parse also holds
/// the ones after it, in [`others`](Self::others).
#[non_exhaustive]
pub struct SyntaxError {
    /// The rendered message.
    pub message: String,
    #[source_code]
    /// The source text the span indexes into, for rendering.
    pub src: NamedSource<Arc<str>>,
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
    #[related]
    /// The syntax errors found after this one in the same parse, in
    /// source order, rendered by `miette` beneath it. Empty on each of
    /// those errors, and on the first when it was the only one. The
    /// parser stops collecting at [`MAX_SYNTAX_ERRORS`].
    pub others: Vec<SyntaxError>,
}

/// The most syntax errors one parse reports. Past this the parser stops
/// and returns what it has, so a file that is not WCL at all yields a
/// bounded report rather than one error per token.
pub const MAX_SYNTAX_ERRORS: usize = 100;
