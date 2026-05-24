//! End-to-end tests for the host-callable function API:
//! `Document::call_function` and `Document::call_value`.

use std::collections::BTreeMap;

use wcl_lang::{Document, EvalError, Value, VariantPayload};

#[test]
fn call_function_returning_variant_list() {
    let src = r#"
        union Shape {
          Rect   { x: f64  y: f64 }
          Circle { r: f64 }
        }

        @document
        type Doc { shapes_for: fn(utf8) -> list<Shape> }

        shapes_for = fn(label: utf8) -> list<Shape> [
          Shape::Rect   { x: 0.0, y: 0.0 },
          Shape::Circle { r: 5.0 },
        ]
    "#;
    let doc = Document::open(src, "test").expect("parse");
    let out = doc
        .call_function("shapes_for", &[Value::Utf8("hello".into())])
        .expect("call");

    let Value::List(items) = out else {
        panic!("expected Value::List, got {out:?}");
    };
    assert_eq!(items.len(), 2);

    let Value::Variant {
        variant: name0,
        payload: payload0,
        ..
    } = &items[0]
    else {
        panic!("item 0 not Variant: {:?}", items[0]);
    };
    assert_eq!(name0, "Rect");
    let VariantPayload::Record(fields0) = payload0 else {
        panic!("Rect payload is not record");
    };
    assert!(matches!(fields0.get("x"), Some(Value::F64(_))));
    assert!(matches!(fields0.get("y"), Some(Value::F64(_))));

    let Value::Variant { variant: name1, .. } = &items[1] else {
        panic!("item 1 not Variant: {:?}", items[1]);
    };
    assert_eq!(name1, "Circle");
}

#[test]
fn call_function_accepting_record_arg() {
    // A function that receives a record-typed param and reads its
    // fields. We pass in a `Value::Record` constructed in Rust.
    let src = r#"
        type Point { x: f64  y: f64 }

        @document
        type Doc { distance_sq: fn(Point) -> f64 }

        distance_sq = fn(p: Point) -> f64 p.x * p.x + p.y * p.y
    "#;
    let doc = Document::open(src, "test").expect("parse");

    let mut fields = BTreeMap::new();
    fields.insert("x".to_string(), Value::F64(3.0));
    fields.insert("y".to_string(), Value::F64(4.0));
    let arg = Value::Record {
        ty: vec!["Point".into()],
        fields,
    };

    let out = doc.call_function("distance_sq", &[arg]).expect("call");
    let Value::F64(d) = out else {
        panic!("expected F64, got {out:?}");
    };
    assert!((d - 25.0).abs() < 1e-9, "distance_sq = {d}");
}

#[test]
fn call_function_missing_name_is_user_error() {
    let doc = Document::open("", "test").expect("parse");
    let err = doc
        .call_function("not_here", &[])
        .expect_err("expected error");
    assert!(
        matches!(err, EvalError::UserError { .. }),
        "expected UserError, got {err:?}"
    );
}
