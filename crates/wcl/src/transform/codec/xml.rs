//! XML codec — decode and encode XML format.

use crate::eval::value::Value;
use crate::transform::error::TransformError;
use std::io::{Read, Write};

use super::json::json_value_to_wcl;

/// Convenience: decode an XML reader into a Vec of WCL Value records.
///
/// Uses `quick_xml` serde deserialization to convert XML into a
/// `serde_json::Value`, then converts to WCL values. XML has a single
/// root element, so the result is typically a single-element vec.
///
/// Note: `quick_xml` represents text-only elements as `{"$text": "value"}`.
/// This codec unwraps such maps to plain strings for cleaner WCL values.
pub fn decode_xml_records(reader: impl Read) -> Result<Vec<Value>, TransformError> {
    let mut buf = String::new();
    let mut reader = reader;
    reader
        .read_to_string(&mut buf)
        .map_err(TransformError::Io)?;

    let json_val: serde_json::Value = quick_xml::de::from_str(&buf)
        .map_err(|e| TransformError::Codec(format!("XML parse error: {}", e)))?;

    let wcl_val = simplify_xml_value(json_value_to_wcl(&json_val));

    match wcl_val {
        Value::List(items) => Ok(items),
        _ => Ok(vec![wcl_val]),
    }
}

/// Unwrap `{"$text": value}` maps produced by quick_xml into plain values.
fn simplify_xml_value(val: Value) -> Value {
    match val {
        Value::Map(m) => {
            if m.len() == 1 {
                if let Some(inner) = m.get("$text") {
                    return inner.clone();
                }
            }
            Value::Map(
                m.into_iter()
                    .map(|(k, v)| (k, simplify_xml_value(v)))
                    .collect(),
            )
        }
        Value::List(items) => Value::List(items.into_iter().map(simplify_xml_value).collect()),
        other => other,
    }
}

/// Convenience: encode a Vec of WCL values as XML.
///
/// Converts each value to XML elements. Maps become child elements,
/// scalars become text content. The output is wrapped in the specified
/// root element.
pub fn encode_xml_records(
    values: &[Value],
    writer: &mut dyn Write,
    root_element: &str,
) -> Result<(), TransformError> {
    writer
        .write_all(format!("<{}>", root_element).as_bytes())
        .map_err(TransformError::Io)?;
    for value in values {
        write_xml_value(writer, value)?;
    }
    writer
        .write_all(format!("</{}>", root_element).as_bytes())
        .map_err(TransformError::Io)?;
    writer.flush().map_err(TransformError::Io)?;
    Ok(())
}

/// Write a WCL value as XML elements.
fn write_xml_value(writer: &mut dyn Write, value: &Value) -> Result<(), TransformError> {
    match value {
        Value::Map(m) => {
            for (key, val) in m {
                writer
                    .write_all(format!("<{}>", key).as_bytes())
                    .map_err(TransformError::Io)?;
                write_xml_value(writer, val)?;
                writer
                    .write_all(format!("</{}>", key).as_bytes())
                    .map_err(TransformError::Io)?;
            }
        }
        Value::List(items) => {
            for item in items {
                write_xml_value(writer, item)?;
            }
        }
        Value::String(s) => {
            let escaped = quick_xml::escape::escape(s);
            writer
                .write_all(escaped.as_bytes())
                .map_err(TransformError::Io)?;
        }
        Value::Int(n) => {
            writer
                .write_all(n.to_string().as_bytes())
                .map_err(TransformError::Io)?;
        }
        Value::Float(f) => {
            writer
                .write_all(f.to_string().as_bytes())
                .map_err(TransformError::Io)?;
        }
        Value::Bool(b) => {
            writer
                .write_all(if *b { b"true" } else { b"false" })
                .map_err(TransformError::Io)?;
        }
        Value::Null => {
            // Write nothing for null
        }
        _ => {
            // Other value types: skip
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn decode_xml_simple() {
        let input = r#"<person><name>Alice</name><age>30</age></person>"#;
        let records = decode_xml_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 1);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::String("30".into())));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn decode_xml_nested() {
        let input = r#"<config><server><host>localhost</host><port>8080</port></server></config>"#;
        let records = decode_xml_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 1);

        if let Value::Map(ref m) = records[0] {
            assert!(m.contains_key("server"));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn encode_xml_single_record() {
        let records = vec![Value::Map({
            let mut m = IndexMap::new();
            m.insert("name".into(), Value::String("Alice".into()));
            m.insert("age".into(), Value::Int(30));
            m
        })];

        let mut output = Vec::new();
        encode_xml_records(&records, &mut output, "person").unwrap();
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("<person>"));
        assert!(output_str.contains("<name>Alice</name>"));
        assert!(output_str.contains("<age>30</age>"));
        assert!(output_str.contains("</person>"));
    }

    #[test]
    fn encode_xml_multiple_records() {
        let records = vec![
            Value::Map({
                let mut m = IndexMap::new();
                m.insert("name".into(), Value::String("Alice".into()));
                m
            }),
            Value::Map({
                let mut m = IndexMap::new();
                m.insert("name".into(), Value::String("Bob".into()));
                m
            }),
        ];

        let mut output = Vec::new();
        encode_xml_records(&records, &mut output, "people").unwrap();
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.starts_with("<people>"));
        assert!(output_str.ends_with("</people>"));
        assert!(output_str.contains("Alice"));
        assert!(output_str.contains("Bob"));
    }

    #[test]
    fn encode_xml_escaping() {
        let records = vec![Value::Map({
            let mut m = IndexMap::new();
            m.insert("text".into(), Value::String("a < b & c > d".into()));
            m
        })];

        let mut output = Vec::new();
        encode_xml_records(&records, &mut output, "root").unwrap();
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("&lt;"));
        assert!(output_str.contains("&amp;"));
        assert!(output_str.contains("&gt;"));
    }
}
