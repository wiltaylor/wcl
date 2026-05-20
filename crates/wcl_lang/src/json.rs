use indexmap::IndexMap;
use serde_json::json;

#[cfg(test)]
use crate::BlockRefData;
use crate::{BlockRef, Diagnostic, Severity, Value};

/// Convert a WCL Value to a serde_json::Value.
pub fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::String(s) => json!(s),
        Value::Int(i) => json!(i),
        Value::BigInt(i) => {
            // i128 may not fit in JSON number; serialize as string if too large
            if *i >= i64::MIN as i128 && *i <= i64::MAX as i128 {
                json!(*i as i64)
            } else {
                json!(i.to_string())
            }
        }
        Value::Float(f) => json!(f),
        Value::Bool(b) => json!(b),
        Value::Null => serde_json::Value::Null,
        Value::Identifier(s) => json!(s),
        Value::Symbol(s) => json!(s),
        Value::List(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Object(object) => {
            let mut obj: serde_json::Map<String, serde_json::Value> = object
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            obj.insert("__type".to_string(), json!(object.type_name));
            serde_json::Value::Object(obj)
        }
        Value::Bytes(bytes) => {
            json!({
                "__type": "bytes",
                "bytes": bytes,
            })
        }
        Value::MsgPackExt { type_id, data } => {
            json!({
                "__type": "msgpack_ext",
                "type_id": type_id,
                "data": data,
            })
        }
        Value::MsgPackTimestamp {
            seconds,
            nanoseconds,
        } => {
            json!({
                "__type": "msgpack_timestamp",
                "seconds": seconds,
                "nanoseconds": nanoseconds,
            })
        }
        Value::Set(items) => {
            json!({
                "__type": "set",
                "items": items.iter().map(value_to_json).collect::<Vec<_>>()
            })
        }
        Value::BlockRef(br) => block_ref_to_json(br),
        Value::Function(_) => serde_json::Value::Null,
        Value::Lazy(_) => serde_json::Value::Null,
        Value::Stream(_) => serde_json::Value::Null,
        Value::NativeStream(_) => serde_json::Value::Null,
        Value::StateHandle(_) => serde_json::Value::Null,
        Value::Date(s) => json!(s),
        Value::OffsetDateTime(s) => json!(s),
        Value::LocalDateTime(s) => json!(s),
        Value::LocalTime(s) => json!(s),
        Value::Duration(s) => json!(s),
        Value::Pattern(s) => json!(s),
    }
}

/// Convert a BlockRef to JSON.
pub fn block_ref_to_json(br: &BlockRef) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("kind".to_string(), json!(br.kind));
    if let Some(id) = &br.id {
        obj.insert("id".to_string(), json!(id));
    }
    if let Some(qid) = &br.qualified_id {
        obj.insert("qualified_id".to_string(), json!(qid));
    }
    if !br.attributes.is_empty() {
        let attrs: serde_json::Map<String, serde_json::Value> = br
            .attributes
            .iter()
            .map(|(k, v)| (k.clone(), value_to_json(v)))
            .collect();
        obj.insert("attributes".to_string(), serde_json::Value::Object(attrs));
    }
    if !br.attribute_decorators.is_empty() {
        let attr_decorators: serde_json::Map<String, serde_json::Value> = br
            .attribute_decorators
            .iter()
            .map(|(attr, decorators)| {
                let decorators: Vec<serde_json::Value> = decorators
                    .iter()
                    .map(|d| {
                        let args: serde_json::Map<String, serde_json::Value> = d
                            .args
                            .iter()
                            .map(|(k, v)| (k.clone(), value_to_json(v)))
                            .collect();
                        json!({ "name": d.name, "args": args })
                    })
                    .collect();
                (attr.clone(), serde_json::Value::Array(decorators))
            })
            .collect();
        obj.insert(
            "attribute_decorators".to_string(),
            serde_json::Value::Object(attr_decorators),
        );
    }
    if !br.children.is_empty() {
        let children: Vec<serde_json::Value> = br.children.iter().map(block_ref_to_json).collect();
        obj.insert("children".to_string(), serde_json::Value::Array(children));
    }
    if !br.decorators.is_empty() {
        let decorators: Vec<serde_json::Value> = br
            .decorators
            .iter()
            .map(|d| {
                let args: serde_json::Map<String, serde_json::Value> = d
                    .args
                    .iter()
                    .map(|(k, v)| (k.clone(), value_to_json(v)))
                    .collect();
                json!({ "name": d.name, "args": args })
            })
            .collect();
        obj.insert(
            "decorators".to_string(),
            serde_json::Value::Array(decorators),
        );
    }
    serde_json::Value::Object(obj)
}

/// Convert a JSON value to a WCL Value.
pub fn json_to_value(json: &serde_json::Value) -> Result<Value, String> {
    match json {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Float(f))
            } else {
                Err(format!("unsupported number: {}", n))
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(s.clone())),
        serde_json::Value::Array(items) => {
            let values: Result<Vec<Value>, String> = items.iter().map(json_to_value).collect();
            Ok(Value::List(values?))
        }
        serde_json::Value::Object(map) => {
            let mut result = IndexMap::new();
            for (k, v) in map {
                result.insert(k.clone(), json_to_value(v)?);
            }
            Ok(Value::Map(result))
        }
    }
}

/// Convert a JSON value to a WCL Value without fallible validation.
///
/// This is used by transform codecs that deserialize through serde-compatible
/// formats and need the same JSON-to-WCL shape as `json_to_value`.
pub fn json_value_to_wcl(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(items) => {
            Value::List(items.iter().map(json_value_to_wcl).collect())
        }
        serde_json::Value::Object(map) => {
            let mut result = IndexMap::new();
            for (k, v) in map {
                result.insert(k.clone(), json_value_to_wcl(v));
            }
            Value::Map(result)
        }
    }
}

/// Convert an IndexMap of WCL Values to a JSON value.
pub fn values_to_json(values: &IndexMap<String, Value>) -> serde_json::Value {
    let obj: serde_json::Map<String, serde_json::Value> = values
        .iter()
        .map(|(k, v)| (k.clone(), value_to_json(v)))
        .collect();
    serde_json::Value::Object(obj)
}

/// Convert a Diagnostic to JSON.
pub fn diagnostic_to_json(d: &Diagnostic) -> serde_json::Value {
    let severity = match d.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    };
    let mut obj = serde_json::Map::new();
    obj.insert("severity".to_string(), json!(severity));
    obj.insert("message".to_string(), json!(d.message));
    if let Some(code) = &d.code {
        obj.insert("code".to_string(), json!(code));
    }
    serde_json::Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Span;

    #[test]
    fn test_value_roundtrip_primitives() {
        let cases = vec![
            Value::String("hello".into()),
            Value::Int(42),
            Value::Float(2.72),
            Value::Bool(true),
            Value::Null,
        ];
        for val in cases {
            let json = value_to_json(&val);
            let back = json_to_value(&json).unwrap();
            assert_eq!(val, back);
        }
    }

    #[test]
    fn test_value_roundtrip_list() {
        let val = Value::List(vec![Value::Int(1), Value::String("two".into())]);
        let json = value_to_json(&val);
        let back = json_to_value(&json).unwrap();
        assert_eq!(val, back);
    }

    #[test]
    fn test_value_roundtrip_map() {
        let mut map = IndexMap::new();
        map.insert("key".to_string(), Value::Int(42));
        let val = Value::Map(map);
        let json = value_to_json(&val);
        let back = json_to_value(&json).unwrap();
        assert_eq!(val, back);
    }

    #[test]
    fn test_block_ref_to_json() {
        let br = BlockRef::new(BlockRefData {
            kind: "server".to_string(),
            id: Some("main".to_string()),
            qualified_id: None,
            attributes: IndexMap::new(),
            attribute_decorators: IndexMap::new(),
            children: vec![],
            decorators: vec![],
            span: Span::dummy(),
        });
        let json = block_ref_to_json(&br);
        assert_eq!(json["kind"], "server");
        assert_eq!(json["id"], "main");
    }
}
