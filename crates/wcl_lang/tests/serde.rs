//! `Value`'s `serde::Serialize` impl emits idiomatic JSON (scalars
//! as primitives, lists as arrays, records as objects). The impl is
//! one-way — see the `Value` doc comment for why. `TypeRef` and
//! friends are still round-trippable and exercised separately below.

use std::collections::BTreeMap;

use serde_json::json;
use wcl_lang::{BuiltinType, TensorDim, TypeRef, Value, VariantPayload};

fn assert_json_eq(v: &Value, expected: serde_json::Value) {
    let got = serde_json::to_value(v).expect("serialize");
    assert_eq!(got, expected, "value: {v:?}");
}

#[test]
fn scalar_values_serialize_as_primitives() {
    assert_json_eq(&Value::Bool(true), json!(true));
    assert_json_eq(&Value::I32(-7), json!(-7));
    assert_json_eq(&Value::U64(42), json!(42));
    assert_json_eq(&Value::F64(1.5), json!(1.5));
    assert_json_eq(&Value::Utf8("hello".into()), json!("hello"));
    assert_json_eq(&Value::Symbol("gold".into()), json!("gold"));
    assert_json_eq(&Value::None, json!(null));
}

#[test]
fn list_serializes_as_json_array() {
    assert_json_eq(
        &Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)]),
        json!([1, 2, 3]),
    );
}

#[test]
fn tensor_carries_shape_and_data() {
    assert_json_eq(
        &Value::Tensor {
            shape: vec![2, 2],
            data: vec![
                Value::F32(1.0),
                Value::F32(2.0),
                Value::F32(3.0),
                Value::F32(4.0),
            ],
        },
        json!({"shape": [2, 2], "data": [1.0, 2.0, 3.0, 4.0]}),
    );
}

#[test]
fn record_serializes_as_flat_object() {
    let mut fields = BTreeMap::new();
    fields.insert("host".to_string(), Value::Utf8("a".into()));
    fields.insert("port".to_string(), Value::U16(80));
    assert_json_eq(
        &Value::Record {
            ty: vec!["Conn".into()],
            fields,
        },
        json!({"host": "a", "port": 80}),
    );
}

#[test]
fn variant_payload_shapes() {
    // Unit variants serialize as a bare string.
    assert_json_eq(
        &Value::Variant {
            union: vec!["U".into()],
            variant: "Empty".into(),
            payload: VariantPayload::Unit,
        },
        json!("Empty"),
    );
    // Positional variants serialize as a single-key object.
    assert_json_eq(
        &Value::Variant {
            union: vec!["U".into()],
            variant: "Just".into(),
            payload: VariantPayload::Positional(Box::new(Value::I64(7))),
        },
        json!({"Just": 7}),
    );
    // Record variants serialize as a single-key object wrapping the fields.
    let mut record = BTreeMap::new();
    record.insert("name".to_string(), Value::Utf8("a".into()));
    assert_json_eq(
        &Value::Variant {
            union: vec!["U".into()],
            variant: "Named".into(),
            payload: VariantPayload::Record(record),
        },
        json!({"Named": {"name": "a"}}),
    );
}

#[test]
fn data_path_carries_kind_and_segments() {
    assert_json_eq(
        &Value::DataPath {
            kind: "type".into(),
            segments: vec!["company".into(), "User".into()],
        },
        json!({"kind": "type", "path": ["company", "User"]}),
    );
}

// ── TypeRef / Span still round-trip via derive ──────────────────────

fn round_trip<T>(v: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + PartialEq,
{
    let json = serde_json::to_value(v).expect("serialize");
    let back: T = serde_json::from_value(json).expect("deserialize");
    assert_eq!(*v, back);
}

#[test]
fn type_refs_round_trip() {
    round_trip(&TypeRef::Builtin(BuiltinType::Utf8));
    round_trip(&TypeRef::Named(vec!["company".into(), "User".into()]));
    round_trip(&TypeRef::List(Box::new(TypeRef::Builtin(BuiltinType::I32))));
    round_trip(&TypeRef::Tensor {
        element: Box::new(TypeRef::Builtin(BuiltinType::F32)),
        dims: vec![TensorDim::Fixed(3), TensorDim::Symbolic("N".into())],
    });
    round_trip(&TypeRef::Function {
        params: vec![TypeRef::Builtin(BuiltinType::I32)],
        return_ty: Box::new(TypeRef::Builtin(BuiltinType::Bool)),
    });
}
