//! CSV codec — decode and encode WCL values as CSV.

use crate::eval::value::Value;
use crate::transform::error::TransformError;
use indexmap::IndexMap;
use std::io::{Read, Write};

/// Convenience: decode a CSV reader into a Vec of WCL Value::Map records.
///
/// If `has_header` is true, the first row is used as map keys.
/// Otherwise, keys are generated as "col0", "col1", etc.
pub fn decode_csv_records(
    reader: impl Read,
    has_header: bool,
    separator: u8,
) -> Result<Vec<Value>, TransformError> {
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(has_header)
        .delimiter(separator)
        .from_reader(reader);

    let headers: Vec<String> = if has_header {
        csv_reader
            .headers()
            .map_err(|e| TransformError::Codec(format!("CSV header error: {}", e)))?
            .iter()
            .map(|h| h.to_string())
            .collect()
    } else {
        Vec::new()
    };

    let mut records = Vec::new();

    for result in csv_reader.records() {
        let record =
            result.map_err(|e| TransformError::Codec(format!("CSV record error: {}", e)))?;

        let mut map = IndexMap::new();
        for (i, field) in record.iter().enumerate() {
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

/// Convenience: encode a Vec of WCL values as CSV.
///
/// Headers are extracted from the keys of the first record.
/// Values are converted to their string representation.
pub fn encode_csv_records(
    values: &[Value],
    writer: &mut dyn Write,
    separator: u8,
) -> Result<(), TransformError> {
    if values.is_empty() {
        return Ok(());
    }

    let mut csv_writer = csv::WriterBuilder::new()
        .delimiter(separator)
        .from_writer(writer);

    // Extract headers from the first record's keys.
    let headers: Vec<String> = if let Value::Map(ref m) = values[0] {
        m.keys().cloned().collect()
    } else {
        return Err(TransformError::Codec(
            "CSV encode expects Value::Map records".into(),
        ));
    };

    csv_writer
        .write_record(&headers)
        .map_err(|e| TransformError::Codec(format!("CSV write error: {}", e)))?;

    for value in values {
        if let Value::Map(ref m) = value {
            let row: Vec<String> = headers
                .iter()
                .map(|h| match m.get(h) {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Int(i)) => i.to_string(),
                    Some(Value::Float(f)) => f.to_string(),
                    Some(Value::Bool(b)) => b.to_string(),
                    Some(Value::Null) => String::new(),
                    Some(other) => format!("{:?}", other),
                    None => String::new(),
                })
                .collect();
            csv_writer
                .write_record(&row)
                .map_err(|e| TransformError::Codec(format!("CSV write error: {}", e)))?;
        } else {
            return Err(TransformError::Codec(
                "CSV encode expects Value::Map records".into(),
            ));
        }
    }

    csv_writer
        .flush()
        .map_err(|e| TransformError::Codec(format!("CSV flush error: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_csv_with_headers() {
        let input = "name,age\nAlice,30\nBob,25\n";
        let records = decode_csv_records(input.as_bytes(), true, b',').unwrap();
        assert_eq!(records.len(), 2);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::String("30".into())));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn decode_csv_without_headers() {
        let input = "Alice,30\nBob,25\n";
        let records = decode_csv_records(input.as_bytes(), false, b',').unwrap();
        assert_eq!(records.len(), 2);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("col0"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("col1"), Some(&Value::String("30".into())));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn encode_csv_roundtrip() {
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
        encode_csv_records(&records, &mut output, b',').unwrap();
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Alice"));
        assert!(output_str.contains("Bob"));

        // Round-trip: decode back
        let decoded = decode_csv_records(output_str.as_bytes(), true, b',').unwrap();
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn custom_separator() {
        let input = "name\tage\nAlice\t30\nBob\t25\n";
        let records = decode_csv_records(input.as_bytes(), true, b'\t').unwrap();
        assert_eq!(records.len(), 2);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::String("30".into())));
        } else {
            panic!("expected Map");
        }
    }
}
