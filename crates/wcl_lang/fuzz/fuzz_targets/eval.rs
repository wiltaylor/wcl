#![no_main]

use libfuzzer_sys::fuzz_target;

// Open as a Document (parse + bind + schema) and then walk schema
// errors. Catches panics anywhere along the evaluation path; valid
// parse + schema errors are not a crash.
fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };
    if let Ok(doc) = wcl_lang::Document::open(s, "fuzz") {
        let _ = doc.schema_errors();
    }
});
