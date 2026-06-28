//! End-to-end tests for literal units (`5MiB`): the attached-suffix
//! syntax, type-scoped resolution against `@unit(name, factor)`
//! decorators, the always-on `std.*` unit types, the error paths, and the
//! `format_unit` / `format_unit_value` builtins.

use wcl_lang::{Document, EvalError, Value};

/// Open a doc whose root `@document` declares one field `v` of type `ty`,
/// authored as `expr`, and return the resolved value of `v`.
fn eval_field(ty: &str, expr: &str) -> Result<Value, EvalError> {
    let src = format!("@document\ntype C {{ v: {ty} }}\nv = {expr}\n");
    let doc = Document::open(&src, "test").unwrap();
    doc.get("v").expect("field v").value()
}

#[test]
fn iec_byte_units_resolve_against_byte_size() {
    assert_eq!(eval_field("std.ByteSize", "1B").unwrap(), Value::I64(1));
    assert_eq!(
        eval_field("std.ByteSize", "1KiB").unwrap(),
        Value::I64(1024)
    );
    assert_eq!(
        eval_field("std.ByteSize", "5MiB").unwrap(),
        Value::I64(5 * 1024 * 1024)
    );
    assert_eq!(
        eval_field("std.ByteSize", "2GiB").unwrap(),
        Value::I64(2 * 1024 * 1024 * 1024)
    );
}

#[test]
fn si_byte_units_use_powers_of_1000() {
    assert_eq!(eval_field("std.ByteSize", "5kB").unwrap(), Value::I64(5000));
    assert_eq!(
        eval_field("std.ByteSize", "5MB").unwrap(),
        Value::I64(5_000_000)
    );
    // IEC and SI of the "same" magnitude differ — the whole point.
    assert_ne!(
        eval_field("std.ByteSize", "1KiB").unwrap(),
        eval_field("std.ByteSize", "1kB").unwrap()
    );
}

#[test]
fn distance_units_resolve_in_millimetres() {
    assert_eq!(eval_field("std.Distance", "1mm").unwrap(), Value::I64(1));
    assert_eq!(eval_field("std.Distance", "1cm").unwrap(), Value::I64(10));
    assert_eq!(eval_field("std.Distance", "1m").unwrap(), Value::I64(1000));
    assert_eq!(
        eval_field("std.Distance", "3km").unwrap(),
        Value::I64(3_000_000)
    );
}

#[test]
fn duration_units_resolve_in_nanoseconds() {
    assert_eq!(eval_field("std.Duration", "1ns").unwrap(), Value::I64(1));
    assert_eq!(
        eval_field("std.Duration", "1ms").unwrap(),
        Value::I64(1_000_000)
    );
    assert_eq!(
        eval_field("std.Duration", "30s").unwrap(),
        Value::I64(30_000_000_000)
    );
    assert_eq!(
        eval_field("std.Duration", "2min").unwrap(),
        Value::I64(120_000_000_000)
    );
}

#[test]
fn float_magnitude_with_whole_product_is_an_integer() {
    // 1.5 MiB = 1572864 bytes, an exact integer.
    assert_eq!(
        eval_field("std.ByteSize", "1.5MiB").unwrap(),
        Value::I64(1_572_864)
    );
}

#[test]
fn float_magnitude_with_fractional_product_errors() {
    // 1.5 bytes is not an integer number of bytes.
    let err = eval_field("std.ByteSize", "1.5B").unwrap_err();
    assert!(
        matches!(err, EvalError::SchemaViolation { .. }),
        "expected fractional-value error, got {err:?}"
    );
}

#[test]
fn list_of_unit_typed_elements_resolves_each() {
    let v = eval_field("list<std.ByteSize>", "[1KiB, 1MiB, 2KiB]").unwrap();
    assert_eq!(
        v,
        Value::list(vec![
            Value::I64(1024),
            Value::I64(1024 * 1024),
            Value::I64(2048),
        ])
    );
}

#[test]
fn unit_not_declared_on_the_type_errors() {
    let err = eval_field("std.ByteSize", "3km").unwrap_err();
    match err {
        EvalError::UnitNoMatch { unit, ty, .. } => {
            assert_eq!(unit, "km");
            assert_eq!(ty, "std.ByteSize");
        }
        other => panic!("expected UnitNoMatch, got {other:?}"),
    }
}

#[test]
fn unit_on_a_plain_numeric_type_errors() {
    let err = eval_field("i64", "5MiB").unwrap_err();
    assert!(
        matches!(err, EvalError::UnitNoMatch { .. }),
        "expected UnitNoMatch, got {err:?}"
    );
}

#[test]
fn unit_literal_without_a_type_context_errors() {
    let doc = Document::open("@schemaless x = 5MiB\n", "test").unwrap();
    let err = doc.get("x").expect("field x").value().unwrap_err();
    match err {
        EvalError::UnitWithoutType { unit, .. } => assert_eq!(unit, "MiB"),
        other => panic!("expected UnitWithoutType, got {other:?}"),
    }
}

#[test]
fn arithmetic_on_unresolved_units_errors() {
    // PendingUnit is non-numeric, so it can't participate in arithmetic.
    let err = eval_field("std.ByteSize", "5MiB + 1KiB").unwrap_err();
    assert!(
        matches!(err, EvalError::TypeMismatch { .. }),
        "expected TypeMismatch, got {err:?}"
    );
}

#[test]
fn user_defined_unit_type_works_like_the_builtins() {
    // The mechanism is not special to `std.*` — any numeric alias with
    // `@unit` decorators carries units.
    let src = "@unit(\"kg\", 1000) @unit(\"g\", 1)\ntype Mass = i64\n\
               @document type C { m: Mass }\nm = 2kg\n";
    let doc = Document::open(src, "test").unwrap();
    assert_eq!(doc.get("m").expect("m").value().unwrap(), Value::I64(2000));
}

#[test]
fn format_unit_value_renders_with_an_explicit_factor() {
    let src = "@document type C { label: utf8 }\n\
               label = format_unit_value(5242880, 1048576, \"MiB\")\n";
    let doc = Document::open(src, "test").unwrap();
    assert_eq!(
        doc.get("label").expect("label").value().unwrap(),
        Value::Utf8("5 MiB".to_string())
    );
}

#[test]
fn format_unit_resolves_the_factor_from_the_type() {
    let src = "@document type C { size: std.ByteSize  label: utf8 }\n\
               size = 5MiB\n\
               label = format_unit(size, \"std.ByteSize\", \"MiB\")\n";
    let doc = Document::open(src, "test").unwrap();
    assert_eq!(
        doc.get("label").expect("label").value().unwrap(),
        Value::Utf8("5 MiB".to_string())
    );
}
