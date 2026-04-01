//! YAML codec — decode and encode WCL values as YAML.

use crate::eval::value::Value;
use crate::transform::error::TransformError;
#[cfg(test)]
use indexmap::IndexMap;
use std::io::{Read, Write};

/// Convenience: decode a YAML reader into a Vec of WCL Value records.
///
/// If the top-level YAML is a sequence, each element becomes a record.
/// If it is a mapping, it is treated as a single record.
pub fn decode_yaml_records(reader: impl Read) -> Result<Vec<Value>, TransformError> {
    let yaml_val: serde_yaml_ng::Value = serde_yaml_ng::from_reader(reader)
        .map_err(|e| TransformError::Codec(format!("YAML parse error: {}", e)))?;

    // Convert YAML -> serde_json::Value as intermediary, then use json_value_to_wcl.
    let json_val = serde_json::to_value(&yaml_val)
        .map_err(|e| TransformError::Codec(format!("YAML->JSON conversion error: {}", e)))?;

    match json_val {
        serde_json::Value::Array(arr) => Ok(arr
            .iter()
            .map(crate::transform::codec::json::json_value_to_wcl)
            .collect()),
        serde_json::Value::Object(_) => Ok(vec![crate::transform::codec::json::json_value_to_wcl(
            &json_val,
        )]),
        other => Ok(vec![crate::transform::codec::json::json_value_to_wcl(
            &other,
        )]),
    }
}

/// Convenience: encode a Vec of WCL values as YAML.
pub fn encode_yaml_records(
    values: &[Value],
    writer: &mut dyn Write,
    _pretty: bool,
) -> Result<(), TransformError> {
    let json_values: Vec<serde_json::Value> =
        values.iter().map(crate::json::value_to_json).collect();
    serde_yaml_ng::to_writer(writer, &json_values)
        .map_err(|e| TransformError::Codec(format!("YAML write error: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_yaml_array() {
        let input = "- name: Alice\n  age: 30\n- name: Bob\n  age: 25\n";
        let records = decode_yaml_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 2);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::Int(30)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn decode_yaml_single_object() {
        let input = "name: Alice\nactive: true\n";
        let records = decode_yaml_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 1);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("active"), Some(&Value::Bool(true)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn encode_yaml_roundtrip() {
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

        let mut output = Vec::new();
        encode_yaml_records(&records, &mut output, false).unwrap();
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Alice"));
        assert!(output_str.contains("Bob"));

        // Round-trip: decode back
        let decoded = decode_yaml_records(output_str.as_bytes()).unwrap();
        assert_eq!(decoded.len(), 2);
    }
}
