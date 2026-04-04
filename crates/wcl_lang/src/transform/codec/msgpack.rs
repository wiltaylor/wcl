//! MessagePack codec — decode and encode MessagePack binary format.

use crate::eval::value::Value;
use crate::transform::error::TransformError;
use std::io::{Read, Write};

use super::json::json_value_to_wcl;

/// Convenience: decode a MessagePack reader into a Vec of WCL Value records.
///
/// Deserializes MessagePack binary data via `rmp_serde` into a
/// `serde_json::Value`, then converts to WCL values. If the top-level
/// value is an array, each element becomes a separate record; otherwise
/// it is treated as a single record.
pub fn decode_msgpack_records(reader: impl Read) -> Result<Vec<Value>, TransformError> {
    let json_val: serde_json::Value = rmp_serde::from_read(reader)
        .map_err(|e| TransformError::Codec(format!("MessagePack parse error: {}", e)))?;

    match json_val {
        serde_json::Value::Array(arr) => Ok(arr.iter().map(json_value_to_wcl).collect()),
        serde_json::Value::Object(_) => Ok(vec![json_value_to_wcl(&json_val)]),
        other => Ok(vec![json_value_to_wcl(&other)]),
    }
}

/// Convenience: encode a Vec of WCL values as MessagePack binary.
///
/// Converts values to `serde_json::Value`, then serializes the array
/// with `rmp_serde`.
pub fn encode_msgpack_records(
    values: &[Value],
    writer: &mut dyn Write,
) -> Result<(), TransformError> {
    let json_values: Vec<serde_json::Value> =
        values.iter().map(crate::json::value_to_json).collect();
    rmp_serde::encode::write(writer, &json_values)
        .map_err(|e| TransformError::Codec(format!("MessagePack write error: {}", e)))?;
    writer.flush().map_err(TransformError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn msgpack_roundtrip() {
        let records = vec![
            Value::Map({
                let mut m = IndexMap::new();
                m.insert("name".into(), Value::String("Alice".into()));
                m.insert("age".into(), Value::Int(30));
                m
            }),
            Value::Map({
                let mut m = IndexMap::new();
                m.insert("name".into(), Value::String("Bob".into()));
                m.insert("age".into(), Value::Int(25));
                m
            }),
        ];

        // Encode
        let mut encoded = Vec::new();
        encode_msgpack_records(&records, &mut encoded).unwrap();
        assert!(!encoded.is_empty());

        // Decode back
        let decoded = decode_msgpack_records(encoded.as_slice()).unwrap();
        assert_eq!(decoded.len(), 2);

        if let Value::Map(ref m) = decoded[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::Int(30)));
        } else {
            panic!("expected Map");
        }

        if let Value::Map(ref m) = decoded[1] {
            assert_eq!(m.get("name"), Some(&Value::String("Bob".into())));
            assert_eq!(m.get("age"), Some(&Value::Int(25)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn decode_msgpack_single_object() {
        let json_val = serde_json::json!({"key": "value", "num": 42});
        let encoded = rmp_serde::to_vec(&json_val).unwrap();

        let records = decode_msgpack_records(encoded.as_slice()).unwrap();
        assert_eq!(records.len(), 1);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("key"), Some(&Value::String("value".into())));
            assert_eq!(m.get("num"), Some(&Value::Int(42)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn decode_msgpack_array() {
        let json_val = serde_json::json!([
            {"name": "Alice"},
            {"name": "Bob"}
        ]);
        let encoded = rmp_serde::to_vec(&json_val).unwrap();

        let records = decode_msgpack_records(encoded.as_slice()).unwrap();
        assert_eq!(records.len(), 2);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn msgpack_nested_values() {
        let records = vec![Value::Map({
            let mut m = IndexMap::new();
            m.insert("name".into(), Value::String("test".into()));
            m.insert("active".into(), Value::Bool(true));
            m.insert("score".into(), Value::Float(3.14));
            m.insert(
                "tags".into(),
                Value::List(vec![Value::String("a".into()), Value::String("b".into())]),
            );
            m.insert(
                "nested".into(),
                Value::Map({
                    let mut inner = IndexMap::new();
                    inner.insert("key".into(), Value::String("val".into()));
                    inner
                }),
            );
            m
        })];

        let mut encoded = Vec::new();
        encode_msgpack_records(&records, &mut encoded).unwrap();

        let decoded = decode_msgpack_records(encoded.as_slice()).unwrap();
        assert_eq!(decoded.len(), 1);

        if let Value::Map(ref m) = decoded[0] {
            assert_eq!(m.get("name"), Some(&Value::String("test".into())));
            assert_eq!(m.get("active"), Some(&Value::Bool(true)));
            assert!(m.contains_key("tags"));
            assert!(m.contains_key("nested"));
        } else {
            panic!("expected Map");
        }
    }
}
