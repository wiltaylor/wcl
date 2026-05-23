//! Round-trip Value, TypeRef and friends through `serde_json` to
//! confirm the public serialization surface stays intact.

use std::collections::BTreeMap;

use wcl_lang::{BuiltinType, TensorDim, TypeRef, Value, VariantPayload};

fn round_trip<T>(v: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + PartialEq,
{
    let json = serde_json::to_value(v).expect("serialize");
    let back: T = serde_json::from_value(json).expect("deserialize");
    assert_eq!(*v, back);
}

#[test]
fn scalar_values_round_trip() {
    round_trip(&Value::Bool(true));
    round_trip(&Value::I32(-7));
    round_trip(&Value::U64(42));
    round_trip(&Value::F64(1.5));
    round_trip(&Value::Utf8("hello".into()));
    round_trip(&Value::Symbol("gold".into()));
    round_trip(&Value::None);
}

#[test]
fn collection_values_round_trip() {
    round_trip(&Value::List(vec![Value::I64(1), Value::I64(2)]));
    round_trip(&Value::Tensor {
        shape: vec![2, 2],
        data: vec![
            Value::F32(1.0),
            Value::F32(2.0),
            Value::F32(3.0),
            Value::F32(4.0),
        ],
    });
    let mut fields = BTreeMap::new();
    fields.insert("name".to_string(), Value::Utf8("alpha".into()));
    round_trip(&Value::Record {
        ty: vec!["Conn".into()],
        fields,
    });
}

#[test]
fn variant_payloads_round_trip() {
    round_trip(&Value::Variant {
        union: vec!["company".into(), "Shape".into()],
        variant: "Circle".into(),
        payload: VariantPayload::Positional(Box::new(Value::F64(1.0))),
    });
    let mut record = BTreeMap::new();
    record.insert("name".to_string(), Value::Utf8("a".into()));
    round_trip(&Value::Variant {
        union: vec!["U".into()],
        variant: "R".into(),
        payload: VariantPayload::Record(record),
    });
    round_trip(&Value::Variant {
        union: vec!["U".into()],
        variant: "Unit".into(),
        payload: VariantPayload::Unit,
    });
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
