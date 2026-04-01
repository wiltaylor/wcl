//! HCL codec — decode and encode HCL (HashiCorp Configuration Language) format.

use crate::eval::value::Value;
use crate::transform::error::TransformError;
use std::io::{Read, Write};

use super::json::json_value_to_wcl;

/// Convenience: decode an HCL reader into a Vec of WCL Value records.
///
/// Parses HCL text via serde deserialization into `serde_json::Value`,
/// then converts to WCL values. The result is always a single-element
/// vec since HCL bodies represent one top-level configuration object.
pub fn decode_hcl_records(reader: impl Read) -> Result<Vec<Value>, TransformError> {
    let mut buf = String::new();
    let mut reader = reader;
    reader
        .read_to_string(&mut buf)
        .map_err(TransformError::Io)?;

    let json_val: serde_json::Value = hcl::from_str(&buf)
        .map_err(|e| TransformError::Codec(format!("HCL parse error: {}", e)))?;

    match json_val {
        serde_json::Value::Array(arr) => Ok(arr.iter().map(json_value_to_wcl).collect()),
        serde_json::Value::Object(_) => Ok(vec![json_value_to_wcl(&json_val)]),
        other => Ok(vec![json_value_to_wcl(&other)]),
    }
}

/// Convenience: encode a Vec of WCL values as HCL text.
///
/// Converts each value to `serde_json::Value`, then serializes via `hcl::to_string`.
pub fn encode_hcl_records(values: &[Value], writer: &mut dyn Write) -> Result<(), TransformError> {
    for value in values {
        let json_val = crate::json::value_to_json(value);
        let hcl_str = hcl::to_string(&json_val)
            .map_err(|e| TransformError::Codec(format!("HCL write error: {}", e)))?;
        writer
            .write_all(hcl_str.as_bytes())
            .map_err(TransformError::Io)?;
    }
    writer.flush().map_err(TransformError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn decode_hcl_simple() {
        let input = r#"
            name = "Alice"
            age  = 30
            active = true
        "#;
        let records = decode_hcl_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 1);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::Int(30)));
            assert_eq!(m.get("active"), Some(&Value::Bool(true)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn encode_hcl_roundtrip() {
        let records = vec![Value::Map({
            let mut m = IndexMap::new();
            m.insert("name".into(), Value::String("Alice".into()));
            m.insert("age".into(), Value::Int(30));
            m
        })];

        let mut output = Vec::new();
        encode_hcl_records(&records, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Alice"));
        assert!(output_str.contains("age"));

        // Round-trip: decode back
        let decoded = decode_hcl_records(output_str.as_bytes()).unwrap();
        assert_eq!(decoded.len(), 1);
        if let Value::Map(ref m) = decoded[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::Int(30)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn decode_hcl_nested_block() {
        let input = r#"
            server {
                host = "localhost"
                port = 8080
            }
        "#;
        let records = decode_hcl_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 1);

        if let Value::Map(ref m) = records[0] {
            assert!(m.contains_key("server"));
        } else {
            panic!("expected Map");
        }
    }
}
