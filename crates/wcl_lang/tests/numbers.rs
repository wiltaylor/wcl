//! Numeric correctness: how numbers fit declared types, compare across
//! widths, and print.
//!
//! - A number takes a declared numeric type only when it fits, and then
//!   reads back *as* that type.
//! - `==`, `!=`, `<`, `<=`, `>`, `>=` compare numbers of any two types
//!   exactly, and agree with `sort`.
//! - NaN is unordered: every ordering operator answers `false` for it.
//! - A printed float re-parses to the same float.

use wcl_lang::{Document, EvalError, SchemaViolationKind, Value};

fn eval(src: &str) -> Value {
    let doc = Document::open(src, "test").unwrap();
    doc.get("result")
        .expect("result field")
        .value()
        .expect("eval")
}

fn eval_err(src: &str) -> EvalError {
    let doc = Document::open(src, "test").unwrap();
    doc.get("result")
        .expect("result field")
        .value()
        .expect_err("expected an evaluation error")
}

fn truth(expr: &str) -> bool {
    match eval(&format!("@schemaless result = {expr}\n")) {
        Value::Bool(b) => b,
        other => panic!("{expr} gave {other:?}"),
    }
}

/// Every schema error message in `src`, in order.
fn schema_messages(src: &str) -> Vec<String> {
    Document::open(src, "test")
        .unwrap()
        .schema_errors()
        .iter()
        .map(|e| match e {
            EvalError::SchemaViolation { message, .. } => message.clone(),
            other => other.to_string(),
        })
        .collect()
}

// ── Numbers against declared types ───────────────────────────────────

#[test]
fn integers_that_do_not_fit_are_schema_errors() {
    // Regression: any number used to satisfy any numeric type.
    let messages = schema_messages(
        r#"
        @document type D { a: u8  b: u8  c: u8  d: list<u8>  e: Port }
        type Port = u16
        a = 300
        b = -1
        c = 2.5
        d = [1, 256]
        e = 70000
        "#,
    );
    assert_eq!(
        messages,
        vec![
            "field 'a' declared as u8 but value 300 is out of range for u8",
            "field 'b' declared as u8 but value -1 is out of range for u8",
            "field 'c' declared as u8 but value 2.5 is not a whole number, so it cannot be u8",
            "field 'd' declared as list<u8> but element [1] holds 256, which is out of range for u8",
            "field 'e' declared as Port but value 70000 is out of range for u16",
        ]
    );
}

#[test]
fn nested_block_fields_are_checked_the_same_way() {
    let doc = Document::open(
        r#"
        @document type D { @children("svc") svcs: list<Svc> }
        @block("svc") type Svc { port: u16 }
        svc web { port = 70000 }
        "#,
        "test",
    )
    .unwrap();
    let errors = doc.schema_errors();
    assert!(
        errors.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: SchemaViolationKind::FieldTypeMismatch,
                message,
                ..
            } if message.contains("70000 is out of range for u16")
        )),
        "{errors:#?}"
    );
}

#[test]
fn a_fitting_number_reads_back_as_its_declared_type() {
    let doc = Document::open(
        r#"
        @document type D { a: u8  b: f64  c: i32  d: f32  e: list<u16>  f: Port }
        type Port = u16
        a = 200
        b = 520
        c = 1.0
        d = 16777217
        e = [1, 2]
        f = 8080
        "#,
        "test",
    )
    .unwrap();
    assert!(doc.schema_errors().is_empty(), "{:#?}", doc.schema_errors());
    let get = |name: &str| doc.get(name).unwrap().value().unwrap();
    assert_eq!(get("a"), Value::U8(200));
    assert_eq!(get("b"), Value::F64(520.0));
    // A whole-valued float is accepted into an integer type.
    assert_eq!(get("c"), Value::I32(1));
    // An integer always fits a float type, rounding to the nearest
    // representable value: 2^24 + 1 has no f32.
    assert_eq!(get("d"), Value::F32(16_777_216.0));
    assert_eq!(get("e"), Value::list(vec![Value::U16(1), Value::U16(2)]));
    assert_eq!(get("f"), Value::U16(8080));
}

/// The message a lazy read of `name` fails with, which must be a
/// `FieldTypeMismatch` naming the field.
fn read_violation(doc: &Document, name: &str) -> String {
    match doc.get(name).expect("declared field").value() {
        Err(EvalError::SchemaViolation {
            kind: SchemaViolationKind::FieldTypeMismatch,
            detail,
            message,
            ..
        }) => {
            assert_eq!(detail.as_deref(), Some(name));
            message
        }
        other => panic!("reading `{name}` gave {other:?}"),
    }
}

#[test]
fn reading_a_misfit_fails_as_the_schema_check_does() {
    // Regression: a number that did not fit came back unconverted from a
    // read (`300` for a `u8`), so only `wcl check` ever saw the problem.
    let src = r#"
        @document type D { a: u8  b: u8  c: u8  d: list<u8>  e: Port  f: f32  @children("svc") svcs: list<Svc> }
        @block("svc") type Svc { weight: u8 }
        type Port = u16
        a = 300
        b = -1
        c = 2.5
        d = [1, 256]
        e = 70000
        f = 1.0e39
        svc web { weight = 256 }
    "#;
    let doc = Document::open(src, "test").unwrap();
    let reads: Vec<String> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| read_violation(&doc, name))
        .collect();
    assert_eq!(
        reads,
        vec![
            "field 'a' declared as u8 but value 300 is out of range for u8",
            "field 'b' declared as u8 but value -1 is out of range for u8",
            "field 'c' declared as u8 but value 2.5 is not a whole number, so it cannot be u8",
            "field 'd' declared as list<u8> but element [1] holds 256, which is out of range for u8",
            "field 'e' declared as Port but value 70000 is out of range for u16",
            "field 'f' declared as f32 but value 1.0e39 is out of range for f32",
        ]
    );
    let web = doc.blocks().next().expect("svc web");
    let weight = web.field("weight").unwrap();
    let error = weight.value().unwrap_err();
    assert_eq!(
        error.to_string(),
        "field 'weight' declared as u8 but value 256 is out of range for u8"
    );
    // The read tags the violation with the file the field was written in,
    // as every other schema violation a read raises.
    assert_eq!(
        error.origin().map(|s| s.name().to_string()),
        Some("test".into())
    );
    // The strict walk reports each once, with the same wording.
    assert_eq!(
        schema_messages(src),
        reads
            .iter()
            .map(String::as_str)
            .chain(["field 'weight' declared as u8 but value 256 is out of range for u8"])
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_misfit_read_is_exempt_where_the_schema_check_is() {
    // `@schemaless` on the field, on its block, or on the block field's
    // declaration opts out of the value-vs-type check on both paths, so
    // the number reads back as written.
    let src = r#"
        @document type D { @children("svc") svcs: list<Svc>  a: u8 }
        @block("svc") type Svc { @schemaless weight: u8  size: u8 }
        @schemaless a = 300
        svc web { weight = 256  @schemaless size = 256 }
        @schemaless svc db { size = 256 }
    "#;
    let doc = Document::open(src, "test").unwrap();
    assert_eq!(doc.get("a").unwrap().value().unwrap(), Value::I64(300));
    for block in doc.blocks() {
        for field in block.fields() {
            assert_eq!(field.value().unwrap(), &Value::I64(256), "{}", field.name());
        }
    }
    assert!(
        schema_messages(src).is_empty(),
        "{:#?}",
        schema_messages(src)
    );
}

#[test]
fn value_typed_rejects_a_misfit_of_the_named_type() {
    let doc = Document::open(
        "type Port = u16\n@schemaless block { port = 70000 }\n",
        "test",
    )
    .unwrap();
    let block = doc.blocks().next().expect("block");
    let error = block
        .field("port")
        .unwrap()
        .value_typed("Port")
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "field 'port' declared as Port but value 70000 is out of range for u16"
    );
}

#[test]
fn function_parameters_take_their_declared_numeric_type() {
    assert_eq!(
        eval("@schemaless result = (fn(x: u8) -> u8 x)(7)\n"),
        Value::U8(7)
    );
}

#[test]
fn decorator_arguments_must_fit_their_slot() {
    let messages = schema_messages(
        r#"
        @decorator("level") type Level { @inline(0) n: u8 }
        @document type D { @level(300) a: i64 }
        a = 1
        "#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("value 300 is out of range for u8")),
        "{messages:#?}"
    );
}

// ── Comparison ───────────────────────────────────────────────────────

#[test]
fn nan_is_unordered() {
    // Regression: NaN compared as Equal, so `<=` and `>=` were both true.
    for op in ["<", "<=", ">", ">="] {
        assert!(!truth(&format!("(0.0 / 0.0) {op} 1.0")), "NaN {op} 1.0");
        assert!(!truth(&format!("1 {op} (0.0 / 0.0)")), "1 {op} NaN");
    }
    assert!(!truth("(0.0 / 0.0) == (0.0 / 0.0)"));
    assert!(truth("(0.0 / 0.0) != (0.0 / 0.0)"));
}

#[test]
fn mixed_width_comparison_is_exact() {
    // Regression: `u128` above i128::MAX (here u128::MAX) could not be
    // compared with a signed integer at all.
    assert!(truth("340282366920938463463374607431768211455u128 > -1"));
    assert!(truth("-1 < 340282366920938463463374607431768211455u128"));
    // Regression: both sides rounded to the same f64 and compared equal.
    assert!(!truth("9007199254740993 == 9007199254740992.0"));
    assert!(truth("9007199254740993 != 9007199254740992.0"));
    assert!(truth("9007199254740993 > 9007199254740992.0"));
    assert!(truth("1u8 == 1.0"));
    assert!(truth("-0.0 == 0"));
    assert!(truth("2.5 > 2u64"));
    assert!(truth("-2.5 < -2i8"));
}

#[test]
fn comparison_agrees_with_sort() {
    let sorted =
        eval("@schemaless result = sort([9007199254740993, 9007199254740992.0, -1, 3u8])\n");
    let Value::List(items) = sorted else {
        panic!("sort gave {sorted:?}");
    };
    for pair in items.windows(2) {
        let src = format!("@schemaless result = {} <= {}\n", pair[0], pair[1]);
        assert_eq!(eval(&src), Value::Bool(true), "{src}");
    }
}

// ── Operator error messages ──────────────────────────────────────────

#[test]
fn ordering_errors_name_the_operator_written() {
    // Regression: every ordering failure reported operator '<>'.
    for op in ["<", "<=", ">", ">="] {
        let err = eval_err(&format!("@schemaless result = 1 {op} \"a\"\n"));
        assert_eq!(
            err.to_string(),
            format!("operator '{op}' is not defined for i64 and utf8")
        );
    }
}

#[test]
fn unary_errors_name_one_operand() {
    // Regression: unary errors printed "—" as a right-hand type.
    let err = eval_err("@schemaless result = -\"x\"\n");
    assert_eq!(err.to_string(), "operator '-' is not defined for utf8");
    let err = eval_err("@schemaless result = !1\n");
    assert_eq!(err.to_string(), "operator '!' is not defined for i64");
    let err = eval_err("@schemaless result = 1 && true\n");
    assert_eq!(err.to_string(), "operator '&&' is not defined for i64");
}

#[test]
fn builtin_errors_name_the_builtin_once() {
    // Regression: "'range': range: end ..." repeated the name.
    let err = eval_err("@schemaless result = range(5, 1)\n");
    let text = err.to_string();
    assert!(text.starts_with("'range': "), "{text}");
    assert!(!text.contains("range: "), "{text}");
}

// ── Printing ─────────────────────────────────────────────────────────

/// Print `v`, re-parse the printed form, and return what it evaluates to.
fn round_trip(v: &Value) -> Value {
    eval(&format!("@schemaless result = {v}\n"))
}

#[test]
fn floats_print_in_a_form_that_reparses() {
    let cases = [
        (Value::F64(1.0e300), "1.0e300"),
        (Value::F64(2.5e-7), "2.5e-7"),
        (Value::F64(-1.0e16), "-1.0e16"),
        (Value::F64(123.5), "123.5"),
        (Value::F64(3.0), "3.0"),
        (Value::F32(0.1), "0.1f32"),
        (Value::F32(3.0e38), "3.0e38f32"),
        (Value::F64(f64::INFINITY), "(1.0 / 0.0)"),
        (Value::F64(f64::NEG_INFINITY), "(-1.0 / 0.0)"),
        (Value::F32(f32::INFINITY), "(1.0f32 / 0.0f32)"),
    ];
    for (value, printed) in cases {
        assert_eq!(value.to_string(), printed);
        assert_eq!(round_trip(&value), value, "{printed} did not round-trip");
    }
    // NaN equals nothing, so check the round trip by kind.
    assert_eq!(Value::F64(f64::NAN).to_string(), "(0.0 / 0.0)");
    let nan = round_trip(&Value::F64(f64::NAN));
    assert!(matches!(nan, Value::F64(n) if n.is_nan()), "{nan:?}");
    let nan = round_trip(&Value::F32(f32::NAN));
    assert!(matches!(nan, Value::F32(n) if n.is_nan()), "{nan:?}");
}
