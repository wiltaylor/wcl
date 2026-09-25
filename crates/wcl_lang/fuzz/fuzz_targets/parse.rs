#![no_main]

use libfuzzer_sys::fuzz_target;

// Drive the edit-path parser, strict and recovering. Any panic or hang
// on arbitrary bytes is a bug; syntax errors are the expected outcome.
// The two entry points must agree: the recovering parse is clean
// exactly when the strict one succeeds, with the same tree, and
// otherwise reports the same errors.
fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let recovered = wcl_lang::parse_for_edit_recovering(s, "fuzz");
    assert!(recovered.errors.len() <= wcl_lang::MAX_SYNTAX_ERRORS);
    match wcl_lang::parse_for_edit(s, "fuzz") {
        Ok(tree) => {
            assert!(recovered.errors.is_empty());
            assert!(recovered.source == tree);
        }
        Err(err) => {
            let strict: Vec<_> = err.syntax_errors().map(|e| (&e.message, e.span)).collect();
            let partial: Vec<_> = recovered.errors.iter().map(|e| (&e.message, e.span)).collect();
            assert_eq!(strict, partial);
        }
    }
});
