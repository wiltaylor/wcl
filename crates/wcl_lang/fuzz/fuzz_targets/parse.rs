#![no_main]

use libfuzzer_sys::fuzz_target;

// Drive the edit-path parser. Any panic on arbitrary bytes is a bug;
// returning `Err(ParseError)` is the expected behaviour and not a
// crash for fuzz purposes.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = wcl_lang::parse_for_edit(s, "fuzz");
    }
});
