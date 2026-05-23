#![no_main]

use libfuzzer_sys::fuzz_target;

// Invariant: any value produced by evaluating a top-level field of a
// schemaless document must serialize to JSON that re-parses (via
// serde_json) and then re-serializes to byte-identical text. Catches
// drift in the hand-rolled `Value` serializer — non-canonical key
// ordering, float formatting bugs, or escapes that round-trip lossily.
fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let prefixed = format!("@schemaless\nfuzz = {src}\n");
    let Ok(doc) = wcl_lang::Document::open(&prefixed, "fuzz") else {
        return;
    };
    let Some(field) = doc.field("fuzz") else {
        return;
    };
    let Ok(value) = field.value() else {
        return;
    };
    let Ok(s1) = serde_json::to_string(value) else {
        return;
    };
    let parsed: serde_json::Value = match serde_json::from_str(&s1) {
        Ok(v) => v,
        Err(_) => return,
    };
    let s2 = serde_json::to_string(&parsed).expect("re-serialize");
    assert_eq!(s1, s2, "JSON round-trip drift");
});
