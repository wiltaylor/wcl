#![no_main]

use libfuzzer_sys::fuzz_target;

// Invariant: any source the parser accepts must survive a
// `parse_for_edit` → `format::to_source` → `parse_for_edit`
// round trip with a structurally equal AST. Catches parser /
// printer drift that escapes the example-driven round-trip
// tests in `crates/wcl_lang/tests/parse.rs`.
fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(ast1) = wcl_lang::parse_for_edit(src, "fuzz") else {
        return;
    };
    let printed = wcl_lang::format::to_source(&ast1);
    let ast2 = wcl_lang::parse_for_edit(&printed, "fuzz")
        .expect("printer output must reparse cleanly");
    assert_eq!(ast1, ast2, "round-trip changed AST");
});
