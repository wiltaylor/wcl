//! JSON codec — streaming JSON decoder and encoder.

use crate::eval::value::Value;
use crate::transform::error::TransformError;
use crate::transform::event::Event;
use indexmap::IndexMap;
use std::io::{Read, Write};

/// JSON decoder that reads JSON and produces Events.
///
/// For arrays: streams one record per element.
/// For objects: treats the whole object as a single record.
pub struct JsonDecoder {
    /// Buffered events to emit.
    events: Vec<Event>,
    /// Current position in events.
    pos: usize,
}

impl JsonDecoder {
    pub fn new(mut reader: Box<dyn Read>) -> Self {
        let events = match Self::read_all(&mut *reader) {
            Ok(events) => events,
            Err(_) => vec![Event::Eof],
        };
        Self { events, pos: 0 }
    }

    fn read_all(reader: &mut dyn Read) -> Result<Vec<Event>, TransformError> {
        let val: serde_json::Value = serde_json::from_reader(reader)
            .map_err(|e| TransformError::Codec(format!("JSON parse error: {}", e)))?;

        let mut events = Vec::new();
        match &val {
            serde_json::Value::Array(arr) => {
                events.push(Event::EnterSeq(None));
                for item in arr {
                    json_to_events(item, None, &mut events);
                }
                events.push(Event::ExitSeq);
            }
            _ => {
                json_to_events(&val, None, &mut events);
            }
        }
        events.push(Event::Eof);
        Ok(events)
    }
}

impl super::Decoder for JsonDecoder {
    fn next_event(&mut self) -> Result<Event, TransformError> {
        if self.pos >= self.events.len() {
            return Ok(Event::Eof);
        }
        let event = self.events[self.pos].clone();
        self.pos += 1;
        Ok(event)
    }
}

fn json_to_events(val: &serde_json::Value, key: Option<String>, events: &mut Vec<Event>) {
    match val {
        serde_json::Value::Object(map) => {
            events.push(Event::EnterMap(key));
            for (k, v) in map {
                json_to_events(v, Some(k.clone()), events);
            }
            events.push(Event::ExitMap);
        }
        serde_json::Value::Array(arr) => {
            events.push(Event::EnterSeq(key));
            for item in arr {
                json_to_events(item, None, events);
            }
            events.push(Event::ExitSeq);
        }
        serde_json::Value::String(s) => {
            events.push(Event::Scalar(key, Value::String(s.clone())));
        }
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                events.push(Event::Scalar(key, Value::Int(i)));
            } else if let Some(f) = n.as_f64() {
                events.push(Event::Scalar(key, Value::Float(f)));
            }
        }
        serde_json::Value::Bool(b) => {
            events.push(Event::Scalar(key, Value::Bool(*b)));
        }
        serde_json::Value::Null => {
            events.push(Event::Scalar(key, Value::Null));
        }
    }
}

/// Convert a serde_json::Value to a WCL Value.
pub fn json_value_to_wcl(val: &serde_json::Value) -> Value {
    match val {
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Array(arr) => Value::List(arr.iter().map(json_value_to_wcl).collect()),
        serde_json::Value::Object(map) => {
            let mut m = IndexMap::new();
            for (k, v) in map {
                m.insert(k.clone(), json_value_to_wcl(v));
            }
            Value::Map(m)
        }
    }
}

/// JSON encoder that writes WCL values as JSON.
pub struct JsonEncoder {
    writer: Box<dyn Write>,
    pretty: bool,
    first_record: bool,
    in_array: bool,
}

impl JsonEncoder {
    pub fn new(writer: Box<dyn Write>, pretty: bool) -> Self {
        Self {
            writer,
            pretty,
            first_record: true,
            in_array: false,
        }
    }

    fn write_value(&mut self, value: &Value) -> Result<(), TransformError> {
        let json = crate::json::value_to_json(value);
        let s = if self.pretty {
            serde_json::to_string_pretty(&json)
        } else {
            serde_json::to_string(&json)
        }
        .map_err(|e| TransformError::Codec(format!("JSON write error: {}", e)))?;
        self.writer
            .write_all(s.as_bytes())
            .map_err(TransformError::Io)?;
        Ok(())
    }
}

impl super::Encoder for JsonEncoder {
    fn write_event(&mut self, event: &Event) -> Result<(), TransformError> {
        match event {
            Event::EnterSeq(_) => {
                self.in_array = true;
                self.writer.write_all(b"[").map_err(TransformError::Io)?;
                if self.pretty {
                    self.writer.write_all(b"\n").map_err(TransformError::Io)?;
                }
                Ok(())
            }
            Event::ExitSeq => {
                if self.pretty {
                    self.writer.write_all(b"\n]").map_err(TransformError::Io)?;
                } else {
                    self.writer.write_all(b"]").map_err(TransformError::Io)?;
                }
                self.in_array = false;
                Ok(())
            }
            Event::EnterMap(_) | Event::ExitMap => {
                // Handled by write_value at the record level
                Ok(())
            }
            Event::Scalar(_, value) => {
                if self.in_array && !self.first_record {
                    self.writer.write_all(b",").map_err(TransformError::Io)?;
                    if self.pretty {
                        self.writer.write_all(b"\n").map_err(TransformError::Io)?;
                    }
                }
                self.write_value(value)?;
                self.first_record = false;
                Ok(())
            }
            Event::Eof => Ok(()),
        }
    }

    fn finish(&mut self) -> Result<(), TransformError> {
        self.writer.write_all(b"\n").map_err(TransformError::Io)?;
        self.writer.flush().map_err(TransformError::Io)?;
        Ok(())
    }
}

/// Convenience: decode a JSON reader into a Vec of WCL Value::Map records.
pub fn decode_json_records(reader: impl Read) -> Result<Vec<Value>, TransformError> {
    let val: serde_json::Value = serde_json::from_reader(reader)
        .map_err(|e| TransformError::Codec(format!("JSON parse error: {}", e)))?;

    match val {
        serde_json::Value::Array(arr) => Ok(arr.iter().map(json_value_to_wcl).collect()),
        serde_json::Value::Object(_) => Ok(vec![json_value_to_wcl(&val)]),
        other => Ok(vec![json_value_to_wcl(&other)]),
    }
}

/// Convenience: encode a Vec of WCL values as a JSON array.
pub fn encode_json_records(
    values: &[Value],
    writer: &mut dyn Write,
    pretty: bool,
) -> Result<(), TransformError> {
    let json_values: Vec<serde_json::Value> =
        values.iter().map(crate::json::value_to_json).collect();
    let s = if pretty {
        serde_json::to_string_pretty(&json_values)
    } else {
        serde_json::to_string(&json_values)
    }
    .map_err(|e| TransformError::Codec(format!("JSON write error: {}", e)))?;
    writer.write_all(s.as_bytes()).map_err(TransformError::Io)?;
    writer.write_all(b"\n").map_err(TransformError::Io)?;
    writer.flush().map_err(TransformError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_json_array() {
        let input = r#"[{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]"#;
        let records = decode_json_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 2);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("age"), Some(&Value::Int(30)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn decode_json_object() {
        let input = r#"{"name": "Alice", "active": true}"#;
        let records = decode_json_records(input.as_bytes()).unwrap();
        assert_eq!(records.len(), 1);

        if let Value::Map(ref m) = records[0] {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("active"), Some(&Value::Bool(true)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn encode_json_roundtrip() {
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
        encode_json_records(&records, &mut output, false).unwrap();
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Alice"));
        assert!(output_str.contains("Bob"));

        // Round-trip: decode back
        let decoded = decode_json_records(output_str.as_bytes()).unwrap();
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn json_value_conversion() {
        let json: serde_json::Value = serde_json::json!({
            "str": "hello",
            "num": 42,
            "float": 3.14,
            "bool": true,
            "null": null,
            "arr": [1, 2, 3],
            "obj": {"nested": true}
        });
        let wcl = json_value_to_wcl(&json);
        if let Value::Map(m) = wcl {
            assert_eq!(m.get("str"), Some(&Value::String("hello".into())));
            assert_eq!(m.get("num"), Some(&Value::Int(42)));
            assert_eq!(m.get("bool"), Some(&Value::Bool(true)));
            assert_eq!(m.get("null"), Some(&Value::Null));
        } else {
            panic!("expected Map");
        }
    }
}
