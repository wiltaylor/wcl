//! TOML codec — decode and encode WCL values as TOML.

use crate::eval::value::Value;
use crate::transform::error::TransformError;
use indexmap::IndexMap;
use std::io::{Read, Write};

/// Convert a `toml::Value` to a WCL `Value`.
pub fn toml_value_to_wcl(val: &toml::Value) -> Value {
    match val {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(i) => Value::Int(*i),
        toml::Value::Float(f) => Value::Float(*f),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(arr) => Value::List(arr.iter().map(toml_value_to_wcl).collect()),
        toml::Value::Table(table) => {
            let mut m = IndexMap::new();
            for (k, v) in table {
                m.insert(k.clone(), toml_value_to_wcl(v));
            }
            Value::Map(m)
        }
    }
}

/// Convert a WCL `Value` to a `toml::Value`.
fn wcl_to_toml_value(val: &Value) -> toml::Value {
    match val {
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Int(i) => toml::Value::Integer(*i),
        Value::Float(f) => toml::Value::Float(*f),
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Null => toml::Value::String(String::new()),
        Value::List(arr) => toml::Value::Array(arr.iter().map(wcl_to_toml_value).collect()),
        Value::Map(map) => {
            let mut table = toml::map::Map::new();
            for (k, v) in map {
                table.insert(k.clone(), wcl_to_toml_value(v));
            }
            toml::Value::Table(table)
        }
        // For any other Value variants, convert to string representation.
        other => toml::Value::String(format!("{:?}", other)),
    }
}

/// Convenience: decode a TOML reader into a Vec of WCL Value records.
///
/// If the top-level is a table, it is treated as a single record.
/// If it contains an array key (first array-of-tables found), each element
/// becomes a separate record.
pub fn decode_toml_records(reader: impl Read) -> Result<Vec<Value>, TransformError> {
    let mut input = String::new();
    let mut reader = reader;
    reader
        .read_to_string(&mut input)
        .map_err(TransformError::Io)?;

    let toml_val: toml::Value = toml::from_str(&input)
        .map_err(|e| TransformError::Codec(format!("TOML parse error: {}", e)))?;

    match &toml_val {
        toml::Value::Table(table) => {
            // Look for a top-level key that holds an array of tables.
            for (_key, value) in table {
                if let toml::Value::Array(arr) = value {
                    // Check if it's an array of tables (not a plain array of scalars).
                    if arr.iter().all(|v| matches!(v, toml::Value::Table(_))) && !arr.is_empty() {
                        return Ok(arr.iter().map(toml_value_to_wcl).collect());
                    }
                }
            }
            // No array-of-tables found; treat the whole table as a single record.
            Ok(vec![toml_value_to_wcl(&toml_val)])
        }
        _ => Ok(vec![toml_value_to_wcl(&toml_val)]),
    }
}

/// Convenience: encode a Vec of WCL values as TOML.
///
/// For a single record, it is serialized as a top-level table.
/// For multiple records, they are wrapped in a `[[records]]` array of tables.
pub fn encode_toml_records(
    values: &[Value],
    writer: &mut dyn Write,
    pretty: bool,
) -> Result<(), TransformError> {
    let toml_val = if values.len() == 1 {
        wcl_to_toml_value(&values[0])
    } else {
        // Wrap multiple records under a "records" key as an array of tables.
        let arr: Vec<toml::Value> = values.iter().map(wcl_to_toml_value).collect();
        let mut table = toml::map::Map::new();
        table.insert("records".into(), toml::Value::Array(arr));
        toml::Value::Table(table)
    };

    let s = if pretty {
        toml::to_string_pretty(&toml_val)
    } else {
        toml::to_string(&toml_val)
    }
    .map_err(|e| TransformError::Codec(format!("TOML write error: {}", e)))?;

    writer.write_all(s.as_bytes()).map_err(TransformError::Io)?;
    writer.flush().map_err(TransformError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_toml_table() {
        let input = "[server]\nhost = \"localhost\"\nport = 8080\n";
        let records = decode_toml_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 1);

        if let Value::Map(ref m) = records[0] {
            if let Some(Value::Map(ref server)) = m.get("server") {
                assert_eq!(server.get("host"), Some(&Value::String("localhost".into())));
                assert_eq!(server.get("port"), Some(&Value::Int(8080)));
            } else {
                panic!("expected nested Map for 'server'");
            }
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn decode_toml_array_of_tables() {
        let input =
            "[[items]]\nname = \"Alice\"\nage = 30\n\n[[items]]\nname = \"Bob\"\nage = 25\n";
        let records = decode_toml_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 2);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::Int(30)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn encode_toml_roundtrip() {
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
        encode_toml_records(&records, &mut output, false).unwrap();
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Alice"));
        assert!(output_str.contains("Bob"));

        // The output wraps in [[records]], so decoding will produce the records.
        let decoded = decode_toml_records(output_str.as_bytes()).unwrap();
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn toml_value_conversion() {
        let toml_val: toml::Value = toml::from_str(
            r#"
            str = "hello"
            num = 42
            float = 3.14
            bool = true
            arr = [1, 2, 3]
            "#,
        )
        .unwrap();

        let wcl = toml_value_to_wcl(&toml_val);
        if let Value::Map(m) = wcl {
            assert_eq!(m.get("str"), Some(&Value::String("hello".into())));
            assert_eq!(m.get("num"), Some(&Value::Int(42)));
            assert_eq!(m.get("bool"), Some(&Value::Bool(true)));
        } else {
            panic!("expected Map");
        }
    }
}
