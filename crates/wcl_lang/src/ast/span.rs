//! Source positions.
//!
//! Every node in this tree carries a [`Span`], so any diagnostic raised
//! against a node can point at the bytes that produced it. Nodes with no
//! source behind them — the type declarations the evaluator fabricates,
//! see [`synthetic`](super::synthetic) — carry an empty span instead.

/// A half-open byte range `[start, end)` into the source text.
///
/// Every node carries one so a diagnostic can point at the text that
/// produced it. Nodes with no source behind them (schema types the
/// evaluator fabricates) carry an empty span instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    /// Byte offset of the first character.
    pub start: usize,
    /// Byte offset one past the last character.
    pub end: usize,
}

impl Span {
    /// Build a span covering `[start, end)`.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The span's width in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// True when the span covers no text — the shape of a synthetic node.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}
