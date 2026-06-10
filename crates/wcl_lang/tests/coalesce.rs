//! End-to-end tests for the `??` none-coalescing operator.

use wcl_lang::{Document, Value};

fn eval(src: &str) -> Value {
    let doc = Document::open(src, "test").unwrap();
    doc.get("result")
        .expect("result field")
        .value()
        .expect("eval")
}

#[test]
fn coalesce_takes_left_unless_none() {
    assert_eq!(eval("@schemaless result = 3 ?? 9\n"), Value::I64(3));
    assert_eq!(eval("@schemaless result = none ?? 9\n"), Value::I64(9));
}

#[test]
fn coalesce_chains_left_to_right() {
    assert_eq!(
        eval("@schemaless result = none ?? none ?? \"x\"\n"),
        Value::Utf8("x".into())
    );
    assert_eq!(eval("@schemaless result = none ?? 1 ?? 2\n"), Value::I64(1));
}

#[test]
fn coalesce_short_circuits_the_right_side() {
    // The rhs would abort evaluation if it ran; a non-none lhs must
    // skip it entirely.
    assert_eq!(
        eval("@schemaless result = 3 ?? error(\"must not evaluate\")\n"),
        Value::I64(3)
    );
}

#[test]
fn coalesce_binds_looser_than_logic_and_arithmetic() {
    // `1 + 2 ?? 9` is `(1 + 2) ?? 9`.
    assert_eq!(eval("@schemaless result = 1 + 2 ?? 9\n"), Value::I64(3));
    // `none ?? false || true` is `none ?? (false || true)`.
    assert_eq!(
        eval("@schemaless result = none ?? false || true\n"),
        Value::Bool(true)
    );
}

#[test]
fn coalesce_defaults_an_unset_optional_field() {
    let src = "@document type Cfg { @children(\"box\") boxes: list<BoxK> }\n\
               @block(\"box\") type BoxK { width: f64? }\n\
               @schemaless result = { let b = boxes.b; b.width ?? 480.0 }\n\
               box b { }\n";
    assert_eq!(eval(src), Value::F64(480.0));
}

#[test]
fn coalesce_formats_canonically_and_round_trips() {
    let src = "@schemaless a = none ?? 1 ?? 2\n@schemaless b = (1 ?? 2) * 3\n";
    let ast = wcl_lang::parse_for_edit(src, "t").expect("parse");
    let printed = wcl_lang::format::to_source(&ast);
    assert_eq!(printed, src);
    // Reparse of the printed form is identical.
    let ast2 = wcl_lang::parse_for_edit(&printed, "t").expect("reparse");
    assert_eq!(wcl_lang::format::to_source(&ast2), printed);
}
