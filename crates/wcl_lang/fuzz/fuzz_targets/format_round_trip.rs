#![no_main]

use libfuzzer_sys::fuzz_target;

// Invariant: the formatter is idempotent — `parse → format → parse →
// format` produces the same text on the second pass as the first. We
// can't compare ASTs directly because `Span` fields legitimately shift
// when whitespace is normalised; comparing the rendered text is the
// invariant we actually care about and catches genuine printer /
// parser drift.
fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(ast1) = wcl_lang::parse_for_edit(src, "fuzz") else {
        return;
    };
    let printed1 = wcl_lang::format::to_source(&ast1);
    let ast2 = wcl_lang::parse_for_edit(&printed1, "fuzz")
        .expect("printer output must reparse cleanly");
    let printed2 = wcl_lang::format::to_source(&ast2);
    assert_eq!(printed1, printed2, "formatter is not idempotent");
});
