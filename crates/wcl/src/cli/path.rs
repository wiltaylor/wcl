//! Span helpers shared between mutation commands. Used by `wcl remove` to
//! compute the byte range of a block or attribute (including any leading
//! decorator lines and the trailing newline) for in-place file rewriting.

use crate::lang::ast::*;
use crate::lang::span::Span;

/// Find the byte range of a line in the source that contains the given span,
/// extending to include the full line (from start-of-line to end-of-line including newline).
pub fn line_span_of(source: &str, span: Span) -> (usize, usize) {
    let bytes = source.as_bytes();

    // Find start of line
    let mut line_start = span.start;
    while line_start > 0 && bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }

    // Find end of line (including newline)
    let mut line_end = span.end;
    while line_end < bytes.len() && bytes[line_end] != b'\n' {
        line_end += 1;
    }
    if line_end < bytes.len() {
        line_end += 1; // include the newline
    }

    (line_start, line_end)
}

/// Find the full span of a block including any leading decorator lines.
pub fn block_full_span(source: &str, block: &Block) -> (usize, usize) {
    let start = if let Some(first_dec) = block.decorators.first() {
        line_span_of(source, first_dec.span).0
    } else {
        line_span_of(source, block.span).0
    };

    let (_, end) = line_span_of(source, block.span);
    (start, end)
}

/// Find the full span of an attribute including any leading decorator lines.
pub fn attr_full_span(source: &str, attr: &Attribute) -> (usize, usize) {
    let start = if let Some(first_dec) = attr.decorators.first() {
        line_span_of(source, first_dec.span).0
    } else {
        line_span_of(source, attr.span).0
    };

    let (_, end) = line_span_of(source, attr.span);
    (start, end)
}
