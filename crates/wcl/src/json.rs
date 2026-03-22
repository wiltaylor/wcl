use indexmap::IndexMap;
use serde_json::json;

use crate::{BlockRef, Diagnostic, Severity, Value};

/// Convert a WCL Value to a serde_json::Value.
pub fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::String(s) => json!(s),
        Value::Int(i) => json!(i),
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
        Value::Set(items) => {
            json!({
                "__type": "set",
                "items": items.iter().map(value_to_json).collect::<Vec<_>>()
            })
        }
        Value::BlockRef(br) => block_ref_to_json(br),
        Value::Function(_) => serde_json::Value::Null,
    }
}

/// Convert a BlockRef to JSON.
pub fn block_ref_to_json(br: &BlockRef) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("kind".to_string(), json!(br.kind));
    if let Some(id) = &br.id {
        obj.insert("id".to_string(), json!(id));
    }
    if !br.attributes.is_empty() {
        let attrs: serde_json::Map<String, serde_json::Value> = br
            .attributes
            .iter()
            .map(|(k, v)| (k.clone(), value_to_json(v)))
            .collect();
        obj.insert("attributes".to_string(), serde_json::Value::Object(attrs));
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
        let br = BlockRef {
            kind: "server".to_string(),
            id: Some("main".to_string()),
            attributes: IndexMap::new(),
            children: vec![],
            decorators: vec![],
            span: Span::dummy(),
        };
        let json = block_ref_to_json(&br);
        assert_eq!(json["kind"], "server");
        assert_eq!(json["id"], "main");
    }
}
