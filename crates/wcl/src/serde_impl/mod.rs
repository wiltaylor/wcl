//! WCL Serde — Serializer and Deserializer for WCL

pub mod de;
pub mod error;
pub mod ser;

pub use error::Error;

use crate::eval::value::Value;
use serde::{Deserialize, Serialize};

/// Deserialize a WCL Value into a Rust type
pub fn from_value<'de, T: Deserialize<'de>>(value: Value) -> Result<T, Error> {
    T::deserialize(de::Deserializer::from_value(value))
}

/// Serialize a Rust type to WCL text
pub fn to_string<T: Serialize>(value: &T) -> Result<String, Error> {
    let mut serializer = ser::Serializer::new(false);
    value.serialize(&mut serializer)?;
    Ok(serializer.into_output())
}

/// Serialize a Rust type to pretty-printed WCL text
pub fn to_string_pretty<T: Serialize>(value: &T) -> Result<String, Error> {
    let mut serializer = ser::Serializer::new(true);
    value.serialize(&mut serializer)?;
    Ok(serializer.into_output())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::value::Value;
    use indexmap::IndexMap;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    // ── Deserializer tests ────────────────────────────────────────────────────

    #[test]
    fn deser_int_to_i64() {
        let val = Value::Int(42);
        let result: i64 = from_value(val).unwrap();
        assert_eq!(result, 42i64);
    }

    #[test]
    fn deser_string_to_string() {
        let val = Value::String("hello".to_string());
        let result: String = from_value(val).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn deser_list_to_vec_i32() {
        let val = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result: Vec<i32> = from_value(val).unwrap();
        assert_eq!(result, vec![1i32, 2, 3]);
    }

    #[test]
    fn deser_map_to_hashmap() {
        let mut map = IndexMap::new();
        map.insert("a".to_string(), Value::Int(1));
        map.insert("b".to_string(), Value::Int(2));
        let val = Value::Map(map);
        let result: HashMap<String, i32> = from_value(val).unwrap();
        assert_eq!(result.get("a"), Some(&1i32));
        assert_eq!(result.get("b"), Some(&2i32));
    }

    #[test]
    fn deser_bool_to_bool() {
        let val = Value::Bool(true);
        let result: bool = from_value(val).unwrap();
        assert!(result);

        let val2 = Value::Bool(false);
        let result2: bool = from_value(val2).unwrap();
        assert!(!result2);
    }

    #[test]
    fn deser_null_to_option_none() {
        let val = Value::Null;
        let result: Option<i32> = from_value(val).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn deser_some_to_option_some() {
        let val = Value::Int(7);
        let result: Option<i32> = from_value(val).unwrap();
        assert_eq!(result, Some(7i32));
    }

    // ── Serializer tests ──────────────────────────────────────────────────────

    #[test]
    fn ser_i64_to_string() {
        let result = to_string(&42i64).unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn ser_str_to_quoted_string() {
        let result = to_string(&"hello").unwrap();
        assert_eq!(result, "\"hello\"");
    }

    #[test]
    fn ser_str_escapes_special_chars() {
        let result = to_string(&"say \"hi\"\nnewline").unwrap();
        assert_eq!(result, r#""say \"hi\"\nnewline""#);
    }

    #[test]
    fn ser_vec_i32() {
        let v = vec![1i32, 2, 3];
        let result = to_string(&v).unwrap();
        assert_eq!(result, "[1, 2, 3]");
    }

    #[test]
    fn ser_empty_vec() {
        let v: Vec<i32> = vec![];
        let result = to_string(&v).unwrap();
        assert_eq!(result, "[]");
    }

    #[test]
    fn ser_bool() {
        assert_eq!(to_string(&true).unwrap(), "true");
        assert_eq!(to_string(&false).unwrap(), "false");
    }

    #[test]
    fn ser_none() {
        let v: Option<i32> = None;
        let result = to_string(&v).unwrap();
        assert_eq!(result, "null");
    }

    #[test]
    fn ser_some() {
        let v: Option<i32> = Some(5);
        let result = to_string(&v).unwrap();
        assert_eq!(result, "5");
    }

    #[test]
    fn ser_struct_compact() {
        #[derive(Serialize)]
        struct Point {
            x: i32,
            y: i32,
        }
        let p = Point { x: 1, y: 2 };
        let result = to_string(&p).unwrap();
        assert_eq!(result, "{x = 1\ny = 2\n}");
    }

    #[test]
    fn ser_struct_pretty() {
        #[derive(Serialize)]
        struct Point {
            x: i32,
            y: i32,
        }
        let p = Point { x: 10, y: 20 };
        let result = to_string_pretty(&p).unwrap();
        assert_eq!(result, "{\n    x = 10\n    y = 20\n}");
    }

    #[test]
    fn ser_nested_struct_pretty() {
        #[derive(Serialize)]
        struct Inner {
            value: i32,
        }
        #[derive(Serialize)]
        struct Outer {
            name: String,
            inner: Inner,
        }
        let o = Outer {
            name: "test".to_string(),
            inner: Inner { value: 42 },
        };
        let result = to_string_pretty(&o).unwrap();
        assert_eq!(
            result,
            "{\n    name = \"test\"\n    inner = {\n        value = 42\n    }\n}"
        );
    }

    // ── Round-trip tests ──────────────────────────────────────────────────────

    #[test]
    fn roundtrip_i64() {
        let original = 99i64;
        let serialized = to_string(&original).unwrap();
        // Parse back manually — serializer produces "99", deserializer needs a Value
        // We test the round-trip via Value directly.
        let val = Value::Int(original);
        let deserialized: i64 = from_value(val.clone()).unwrap();
        assert_eq!(deserialized, original);
        assert_eq!(serialized, "99");
    }

    #[test]
    fn roundtrip_string() {
        let original = "round trip".to_string();
        let val = Value::String(original.clone());
        let deserialized: String = from_value(val).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn roundtrip_vec() {
        let original = vec![10i32, 20, 30];
        // Serialize to WCL text
        let serialized = to_string(&original).unwrap();
        assert_eq!(serialized, "[10, 20, 30]");
        // Deserialize from Value
        let val = Value::List(vec![Value::Int(10), Value::Int(20), Value::Int(30)]);
        let deserialized: Vec<i32> = from_value(val).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn roundtrip_struct() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct Config {
            port: i64,
            enabled: bool,
        }

        let original = Config {
            port: 8080,
            enabled: true,
        };

        // Serialize to WCL text
        let serialized = to_string_pretty(&original).unwrap();
        assert!(serialized.contains("port = 8080"));
        assert!(serialized.contains("enabled = true"));

        // Deserialize from Value::Map
        let mut map = IndexMap::new();
        map.insert("port".to_string(), Value::Int(8080));
        map.insert("enabled".to_string(), Value::Bool(true));
        let val = Value::Map(map);
        let deserialized: Config = from_value(val).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn roundtrip_option_none_some() {
        // None
        let none_val = Value::Null;
        let deser_none: Option<i32> = from_value(none_val).unwrap();
        assert_eq!(deser_none, None);

        // Some
        let some_val = Value::Int(42);
        let deser_some: Option<i32> = from_value(some_val).unwrap();
        assert_eq!(deser_some, Some(42));
    }

    #[test]
    fn deser_identifier_as_string() {
        let val = Value::Identifier("svc-auth".to_string());
        let result: String = from_value(val).unwrap();
        assert_eq!(result, "svc-auth");
    }

    #[test]
    fn deser_float_from_int() {
        let val = Value::Int(5);
        let result: f64 = from_value(val).unwrap();
        assert!((result - 5.0f64).abs() < f64::EPSILON);
    }

    #[test]
    fn deser_float_from_float() {
        let val = Value::Float(3.14);
        let result: f64 = from_value(val).unwrap();
        assert!((result - 3.14f64).abs() < 1e-10);
    }

    #[test]
    fn deser_set_to_vec() {
        let val = Value::Set(vec![Value::Int(1), Value::Int(2)]);
        let result: Vec<i32> = from_value(val).unwrap();
        assert_eq!(result, vec![1i32, 2]);
    }

    #[test]
    fn deser_type_mismatch_error() {
        let val = Value::String("not a number".to_string());
        let result: Result<i64, Error> = from_value(val);
        assert!(result.is_err());
    }

    #[test]
    fn ser_struct_compact_fields_separated() {
        #[derive(Debug, Serialize)]
        struct Point {
            x: i32,
            y: i32,
            z: i32,
        }
        let p = Point { x: 1, y: 2, z: 3 };
        let result = to_string(&p).unwrap();
        // Each field must be on its own line, not concatenated
        assert_eq!(result, "{x = 1\ny = 2\nz = 3\n}");
    }

    #[test]
    fn ser_vec_of_structs_compact() {
        #[derive(Debug, Serialize)]
        struct Tool {
            name: String,
            enabled: bool,
        }
        let tools = vec![
            Tool {
                name: "deploy".into(),
                enabled: true,
            },
            Tool {
                name: "test".into(),
                enabled: false,
            },
        ];
        let result = to_string(&tools).unwrap();
        // Fields within each struct must be separated, not concatenated
        assert!(result.contains("name = \"deploy\"\nenabled = true"));
        assert!(result.contains("name = \"test\"\nenabled = false"));
    }
}
