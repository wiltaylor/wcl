#![no_main]

use libfuzzer_sys::fuzz_target;
use wcl_lang::Span;
use wcl_lang::ast::{Expr, Item};
use wcl_lang::edit;

/// How many fields of one input are rewritten. Each rewrite re-parses the
/// whole source twice, so an input with thousands of fields would
/// otherwise dominate the run.
const MAX_FIELDS: usize = 8;

// Invariant: for any source that parses, replacing any field's RHS
// through `edit::replace_field` — the library call behind `wcl set` —
// succeeds. The span comes from a parse of the same bytes, so the field
// must be found, and the reprinted source must re-parse
// (`EditError::Unprintable` is a printer bug). Catches AST shapes the
// printer can't re-emit after mutation, at any nesting depth.
fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(ast) = wcl_lang::parse_for_edit(src, "fuzz") else {
        return;
    };
    let mut spans = Vec::new();
    collect_field_spans(&ast.items, &mut spans);
    for span in spans.into_iter().take(MAX_FIELDS) {
        if let Err(e) = edit::replace_field(src, "fuzz", span, Expr::I64(0)) {
            panic!("editing a field of a parsing source must succeed: {e}");
        }
    }
});

/// Every field's span, depth first, descending into block bodies.
fn collect_field_spans(items: &[Item], out: &mut Vec<Span>) {
    for item in items {
        match item {
            Item::Field(f) => out.push(f.span),
            Item::Block(b) => collect_field_spans(&b.items, out),
            _ => {}
        }
    }
}
