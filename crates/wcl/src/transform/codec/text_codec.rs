//! Text codec — simple line-oriented text format (TSV, CSV-like, space-separated).

use crate::eval::value::Value;
use crate::transform::error::TransformError;
use indexmap::IndexMap;
use std::io::{Read, Write};

/// Decode text records from a reader using the given separator.
///
/// If `has_header` is true, the first line provides field names.
/// Otherwise, fields are named "col0", "col1", etc.
/// Empty lines are skipped.
pub fn decode_text_records(
    mut reader: impl Read,
    separator: &str,
    has_header: bool,
) -> Result<Vec<Value>, TransformError> {
    let mut content = String::new();
    reader
        .read_to_string(&mut content)
        .map_err(TransformError::Io)?;

    let mut lines = content.lines().filter(|l| !l.is_empty());
    let mut records = Vec::new();

    let headers: Vec<String> = if has_header {
        match lines.next() {
            Some(header_line) => header_line
                .split(separator)
                .map(|s| s.to_string())
                .collect(),
            None => return Ok(records),
        }
    } else {
        Vec::new()
    };

    for line in lines {
        let fields: Vec<&str> = line.split(separator).collect();

        let mut map = IndexMap::new();
        for (i, field) in fields.iter().enumerate() {
            let key = if has_header && i < headers.len() {
                headers[i].clone()
            } else {
                format!("col{}", i)
            };
            map.insert(key, Value::String(field.to_string()));
        }
        records.push(Value::Map(map));
    }

    Ok(records)
}

/// Encode records as text lines with the given separator.
///
/// If `header` is true and there is at least one record, writes a header line
/// with field names from the first record's keys.
pub fn encode_text_records(
    records: &[Value],
    writer: &mut dyn Write,
    separator: &str,
    header: bool,
) -> Result<(), TransformError> {
    if records.is_empty() {
        return Ok(());
    }

    // Collect keys from first record for header
    let keys: Vec<String> = if let Value::Map(m) = &records[0] {
        m.keys().cloned().collect()
    } else {
        return Err(TransformError::TypeMismatch {
            expected: "map".into(),
            got: records[0].type_name().to_string(),
        });
    };

    if header {
        let header_line = keys.join(separator);
        writeln!(writer, "{}", header_line).map_err(TransformError::Io)?;
    }

    for record in records {
        if let Value::Map(m) = record {
            let values: Vec<String> = keys
                .iter()
                .map(|k| m.get(k).map(value_to_text).unwrap_or_default())
                .collect();
            writeln!(writer, "{}", values.join(separator)).map_err(TransformError::Io)?;
        } else {
            return Err(TransformError::TypeMismatch {
                expected: "map".into(),
                got: record.type_name().to_string(),
            });
        }
    }

    Ok(())
}

/// Convert a Value to its text representation for output.
fn value_to_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::BigInt(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        _ => format!("{:?}", v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_tab_separated_with_header() {
        let input = "name\tage\nAlice\t30\nBob\t25\n";
        let records = decode_text_records(input.as_bytes(), "\t", true).unwrap();

        assert_eq!(records.len(), 2);
        if let Value::Map(m) = &records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::String("30".into())));
        } else {
            panic!("expected map");
        }
        if let Value::Map(m) = &records[1] {
            assert_eq!(m.get("name"), Some(&Value::String("Bob".into())));
        }
    }

    #[test]
    fn decode_space_separated_no_header() {
        let input = "Alice 30\nBob 25\n";
        let records = decode_text_records(input.as_bytes(), " ", false).unwrap();

        assert_eq!(records.len(), 2);
        if let Value::Map(m) = &records[0] {
            assert_eq!(m.get("col0"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("col1"), Some(&Value::String("30".into())));
        } else {
            panic!("expected map");
        }
    }

    #[test]
    fn skip_empty_lines() {
        let input = "a\tb\n\n1\t2\n\n3\t4\n";
        let records = decode_text_records(input.as_bytes(), "\t", true).unwrap();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn encode_with_header() {
        let mut records = Vec::new();
        let mut m = IndexMap::new();
        m.insert("name".to_string(), Value::String("Alice".into()));
        m.insert("age".to_string(), Value::Int(30));
        records.push(Value::Map(m));

        let mut m = IndexMap::new();
        m.insert("name".to_string(), Value::String("Bob".into()));
        m.insert("age".to_string(), Value::Int(25));
        records.push(Value::Map(m));

        let mut buf = Vec::new();
        encode_text_records(&records, &mut buf, "\t", true).unwrap();

        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines[0], "name\tage");
        assert_eq!(lines[1], "Alice\t30");
        assert_eq!(lines[2], "Bob\t25");
    }

    #[test]
    fn round_trip_text() {
        let mut records = Vec::new();
        let mut m = IndexMap::new();
        m.insert("x".to_string(), Value::String("hello".into()));
        m.insert("y".to_string(), Value::String("world".into()));
        records.push(Value::Map(m));

        let mut buf = Vec::new();
        encode_text_records(&records, &mut buf, "\t", true).unwrap();

        let decoded = decode_text_records(buf.as_slice(), "\t", true).unwrap();
        assert_eq!(decoded.len(), 1);
        if let Value::Map(m) = &decoded[0] {
            assert_eq!(m.get("x"), Some(&Value::String("hello".into())));
            assert_eq!(m.get("y"), Some(&Value::String("world".into())));
        }
    }
}
