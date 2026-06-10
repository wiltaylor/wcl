//! End-to-end tests for `try body catch name => handler`.

use wcl_lang::{Document, Value};

fn eval(src: &str) -> Value {
    let doc = Document::open(src, "test").unwrap();
    doc.get("result")
        .expect("result field")
        .value()
        .expect("eval")
}

#[test]
fn try_passes_through_a_successful_body() {
    assert_eq!(
        eval("@schemaless result = try 1 + 1 catch m => 0\n"),
        Value::I64(2)
    );
}

#[test]
fn catch_binds_the_error_message() {
    let v = eval("@schemaless result = try error(\"boom\") catch m => m\n");
    let Value::Utf8(msg) = v else {
        panic!("expected utf8, got {v:?}");
    };
    assert!(msg.contains("boom"), "{msg}");
}

#[test]
fn catch_handles_builtin_type_errors() {
    // `sum([])` aborts with "empty list" — catchable.
    assert_eq!(
        eval("@schemaless result = try sum([]) catch m => 0\n"),
        Value::I64(0)
    );
}

#[test]
fn block_forms_on_both_sides() {
    let src =
        "@schemaless result = try {\n  let x = error(\"inner\");\n  x\n} catch m { len(m) > 0 }\n";
    assert_eq!(eval(src), Value::Bool(true));
}

#[test]
fn try_catches_propagated_field_errors() {
    let src = "@schemaless broken = error(\"upstream\")\n\
               @schemaless result = try broken catch m => \"fallback\"\n";
    assert_eq!(eval(src), Value::Utf8("fallback".into()));
}

#[test]
fn nested_try_inner_catches_first() {
    let src = "@schemaless result = try (try error(\"a\") catch m => error(\"b\")) catch m => m\n";
    let Value::Utf8(msg) = eval(src) else {
        panic!("expected utf8");
    };
    assert!(
        msg.contains('b'),
        "outer catches the handler's error: {msg}"
    );
}

#[test]
fn binder_scopes_to_the_handler_only() {
    // `m` is undefined outside the handler.
    let doc = Document::open(
        "@schemaless result = { let a = try 1 catch m => 0; m }\n",
        "test",
    )
    .unwrap();
    assert!(
        doc.get("result").expect("field").value().is_err(),
        "catch binding must not leak"
    );
}

#[test]
fn fields_named_try_and_catch_still_parse() {
    assert_eq!(
        eval("@schemaless try = 1\n@schemaless catch = 2\n@schemaless result = catch\n"),
        Value::I64(2)
    );
}

#[test]
fn try_round_trips_through_the_formatter() {
    let src = "@schemaless a = try risky(1) catch m => 0\n\
               @schemaless b = (try f() catch m => 1) + 2\n";
    let ast = wcl_lang::parse_for_edit(src, "t").expect("parse");
    let printed = wcl_lang::format::to_source(&ast);
    assert_eq!(printed, src);
    let ast2 = wcl_lang::parse_for_edit(&printed, "t").expect("reparse");
    assert_eq!(wcl_lang::format::to_source(&ast2), printed);
}
