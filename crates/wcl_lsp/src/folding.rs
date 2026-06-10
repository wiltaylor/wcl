//! Folding ranges: every multi-line block, declaration and table folds.
//!
//! Works on the raw single-file AST (`parse_for_edit`) rather than a
//! `Document`, so no import resolution runs and every span is guaranteed
//! to be a byte range in *this* buffer.

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};
use wcl_lang::ast::Item;
use wcl_lang::{Span, parse_for_edit};

use crate::convert::offset_to_position;

/// Compute folding ranges for `source`. Returns an empty vec on parse
/// failure (the diagnostics path already surfaces the parse error).
pub(crate) fn compute(source: &str, uri: &str) -> Vec<FoldingRange> {
    let Ok(ast) = parse_for_edit(source, uri.to_string()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk_items(&ast.items, source, &mut out);
    out
}

fn walk_items(items: &[Item], source: &str, out: &mut Vec<FoldingRange>) {
    for item in items {
        match item {
            Item::Block(b) => {
                push_span(b.span, source, out);
                walk_items(&b.items, source, out);
            }
            Item::TypeDecl(t) => push_span(t.span, source, out),
            Item::InterfaceDecl(i) => push_span(i.span, source, out),
            Item::UnionDecl(u) => push_span(u.span, source, out),
            Item::SymbolSetDecl(s) => push_span(s.span, source, out),
            Item::Table(t) => push_span(t.span, source, out),
            _ => {}
        }
    }
}

/// Emit a folding range when the span covers more than one line. The
/// end position lands on the closing brace's line (span end is
/// exclusive), so editors keep the `}` visible when collapsed.
fn push_span(span: Span, source: &str, out: &mut Vec<FoldingRange>) {
    let start = offset_to_position(source, span.start);
    let end = offset_to_position(source, span.end.saturating_sub(1));
    if end.line > start.line {
        out.push(FoldingRange {
            start_line: start.line,
            start_character: None,
            end_line: end.line,
            end_character: None,
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: None,
        });
    }
}
