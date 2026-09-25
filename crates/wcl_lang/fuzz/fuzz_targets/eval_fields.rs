#![no_main]

use libfuzzer_sys::fuzz_target;
use wcl_lang::Document;
use wcl_lang::ast::{Expr, Item};

// Force every expression in the document through the evaluator, schema
// or no schema. `eval` stops at `schema_errors()`, and a field with no
// `@document` schema reports a violation instead of evaluating, so
// arbitrary input rarely reaches a builtin there. Here each field, `let`
// and block-body field expression is evaluated directly against the
// document's root scope with `Document::eval_expr`, and every top-level
// field is also read through `Field::value`. Evaluation errors are
// expected; panics, aborts and hangs are the bugs.
fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(doc) = Document::open(src, "fuzz") else {
        return;
    };
    for field in doc.fields() {
        let _ = field.value();
    }
    let Ok(ast) = wcl_lang::parse_for_edit(src, "fuzz") else {
        return;
    };
    let mut exprs = Vec::new();
    collect_exprs(&ast.items, &mut exprs);
    for expr in exprs {
        let _ = doc.eval_expr(expr);
    }
});

/// Every field and `let` right-hand side in `items`, descending into
/// block bodies.
fn collect_exprs<'a>(items: &'a [Item], out: &mut Vec<&'a Expr>) {
    for item in items {
        match item {
            Item::Field(f) => out.push(&f.expr),
            Item::Let(l) => out.push(&l.value),
            Item::Block(b) => collect_exprs(&b.items, out),
            _ => {}
        }
    }
}
