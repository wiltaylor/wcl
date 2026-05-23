#![no_main]

use libfuzzer_sys::fuzz_target;
use wcl_lang::ast::{Expr, Item};

// Invariant: parse → mutate every field's RHS to a fixed literal →
// format → reparse, and the second parse must succeed. Exercises the
// edit-path mutation API plus the printer's interaction with
// arbitrary surrounding trivia. Catches AST shapes the printer can't
// re-emit after mutation.
fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(mut ast) = wcl_lang::parse_for_edit(src, "fuzz") else {
        return;
    };
    for item in &mut ast.items {
        if let Item::Field(f) = item {
            f.expr = Expr::I64(0);
        }
    }
    let printed = wcl_lang::format::to_source(&ast);
    wcl_lang::parse_for_edit(&printed, "fuzz")
        .expect("mutated AST must re-print into a parseable source");
});
