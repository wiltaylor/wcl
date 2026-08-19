//! Comments and blank lines: the formatting the tree preserves.
//!
//! The lexer collects these from the source between tokens and the
//! parser hands them to the node it is building; [`crate::format`]
//! re-emits them. Everything else about layout — indentation, brace
//! style, number radix, string-delimiter choice — is reformatted
//! canonically, so what these types carry is exactly what survives a
//! `parse → print` round trip.

/// Side-band formatting hints attached to each top-level [`Item`](super::Item) in
/// `leading_trivia`. The lexer collects these from the source between
/// tokens; the parser hands them to the next Item it builds. The
/// source printer re-emits them so comments and blank-line groupings
/// survive a round-trip. Other formatting (indentation, brace style,
/// number radix, string-delimiter choice) is reformatted canonically
/// — only what's in this enum is preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trivia {
    /// A line comment, payload-only (the leading `#` or `//` and the
    /// trailing newline are stripped). The printer re-adds the `#`
    /// prefix; original prefix style is not preserved.
    LineComment(String),
    /// One blank line break between items. Multiple consecutive blank
    /// lines collapse to a single marker — canonical output emits at
    /// most one blank line between any two items.
    BlankLine,
}

/// Comment trivia for one element of a comma-separated expression
/// collection whose elements are bare [`Expr`](super::Expr)s (list literals, call
/// arguments) and so have no struct of their own to hang trivia on. The
/// parser builds one entry per element, index-aligned with the element
/// vec; the evaluator ignores these entirely. `leading` holds comments
/// (and blank lines) printed above the element; `trailing` is a same-line
/// comment printed after it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ElemTrivia {
    /// Comments and blank lines printed above the element.
    pub leading: Vec<Trivia>,
    /// A same-line comment printed after the element.
    pub trailing: Option<String>,
}

impl ElemTrivia {
    /// True when this element carries a line comment in either position
    /// (blank lines alone don't count — they don't force a multi-line
    /// layout).
    pub fn has_comment(&self) -> bool {
        self.trailing.is_some()
            || self
                .leading
                .iter()
                .any(|t| matches!(t, Trivia::LineComment(_)))
    }
}
