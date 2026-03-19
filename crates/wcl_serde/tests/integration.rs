use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wcl_eval::value::Value;
use wcl_serde::{from_value, to_string, to_string_pretty, Error};

// ── Deserialize Value::String to String ──────────────────────────────────────

#[test]
fn deser_string_to_string() {
    let val = Value::String("hello world".to_string());
    let result: String = from_value(val).unwrap();
    assert_eq!(result, "hello world");
}

#[test]
fn deser_identifier_to_string() {
    // Value::Identifier can also be deserialized as a String
    let val = Value::Identifier("svc-auth".to_string());
    let result: String = from_value(val).unwrap();
    assert_eq!(result, "svc-auth");
}

// ── Deserialize Value::Int to i64 ────────────────────────────────────────────

#[test]
fn deser_int_to_i64() {
    let val = Value::Int(99);
    let result: i64 = from_value(val).unwrap();
    assert_eq!(result, 99i64);
}

#[test]
fn deser_int_zero_to_i64() {
    let val = Value::Int(0);
    let result: i64 = from_value(val).unwrap();
    assert_eq!(result, 0i64);
}

#[test]
fn deser_negative_int_to_i64() {
    let val = Value::Int(-42);
    let result: i64 = from_value(val).unwrap();
    assert_eq!(result, -42i64);
}

// ── Deserialize Value::Float to f64 ──────────────────────────────────────────

#[test]
fn deser_float_to_f64() {
    let val = Value::Float(3.14);
    let result: f64 = from_value(val).unwrap();
    assert!((result - 3.14f64).abs() < 1e-10);
}

#[test]
fn deser_int_coerces_to_f64() {
    // The deserializer allows Value::Int to deserialize as f64
    let val = Value::Int(7);
    let result: f64 = from_value(val).unwrap();
    assert!((result - 7.0f64).abs() < f64::EPSILON);
}

// ── Deserialize Value::Bool to bool ──────────────────────────────────────────

#[test]
fn deser_bool_true() {
    let val = Value::Bool(true);
    let result: bool = from_value(val).unwrap();
    assert!(result);
}

#[test]
fn deser_bool_false() {
    let val = Value::Bool(false);
    let result: bool = from_value(val).unwrap();
    assert!(!result);
}

// ── Deserialize Value::List to Vec<T> ────────────────────────────────────────

#[test]
fn deser_list_of_ints_to_vec_i64() {
    let val = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    let result: Vec<i64> = from_value(val).unwrap();
    assert_eq!(result, vec![1i64, 2, 3]);
}

#[test]
fn deser_list_of_strings_to_vec_string() {
    let val = Value::List(vec![
        Value::String("a".to_string()),
        Value::String("b".to_string()),
        Value::String("c".to_string()),
    ]);
    let result: Vec<String> = from_value(val).unwrap();
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
fn deser_empty_list_to_empty_vec() {
    let val = Value::List(vec![]);
    let result: Vec<i64> = from_value(val).unwrap();
    assert!(result.is_empty());
}

#[test]
fn deser_set_to_vec() {
    // Value::Set also deserializes as a sequence
    let val = Value::Set(vec![Value::Int(10), Value::Int(20)]);
    let result: Vec<i64> = from_value(val).unwrap();
    assert_eq!(result, vec![10i64, 20]);
}

// ── Deserialize Value::Map to struct ─────────────────────────────────────────

#[derive(Debug, Deserialize, PartialEq)]
struct Config {
    port: i64,
    enabled: bool,
    name: String,
}

#[test]
fn deser_map_to_struct() {
    let mut map = IndexMap::new();
    map.insert("port".to_string(), Value::Int(8080));
    map.insert("enabled".to_string(), Value::Bool(true));
    map.insert("name".to_string(), Value::String("my-service".to_string()));

    let val = Value::Map(map);
    let result: Config = from_value(val).unwrap();
    assert_eq!(result.port, 8080);
    assert!(result.enabled);
    assert_eq!(result.name, "my-service");
}

#[derive(Debug, Deserialize, PartialEq)]
struct Inner {
    value: i64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Outer {
    label: String,
    inner: Inner,
}

#[test]
fn deser_nested_map_to_nested_struct() {
    let mut inner_map = IndexMap::new();
    inner_map.insert("value".to_string(), Value::Int(42));

    let mut outer_map = IndexMap::new();
    outer_map.insert("label".to_string(), Value::String("top".to_string()));
    outer_map.insert("inner".to_string(), Value::Map(inner_map));

    let val = Value::Map(outer_map);
    let result: Outer = from_value(val).unwrap();
    assert_eq!(result.label, "top");
    assert_eq!(result.inner.value, 42);
}

// ── Deserialize Value::Map to HashMap ────────────────────────────────────────

#[test]
fn deser_map_to_hashmap_string_int() {
    let mut map = IndexMap::new();
    map.insert("a".to_string(), Value::Int(1));
    map.insert("b".to_string(), Value::Int(2));
    map.insert("c".to_string(), Value::Int(3));

    let val = Value::Map(map);
    let result: HashMap<String, i64> = from_value(val).unwrap();
    assert_eq!(result.get("a"), Some(&1i64));
    assert_eq!(result.get("b"), Some(&2i64));
    assert_eq!(result.get("c"), Some(&3i64));
    assert_eq!(result.len(), 3);
}

#[test]
fn deser_map_to_hashmap_string_string() {
    let mut map = IndexMap::new();
    map.insert("host".to_string(), Value::String("localhost".to_string()));
    map.insert("env".to_string(), Value::String("prod".to_string()));

    let val = Value::Map(map);
    let result: HashMap<String, String> = from_value(val).unwrap();
    assert_eq!(result.get("host").map(String::as_str), Some("localhost"));
    assert_eq!(result.get("env").map(String::as_str), Some("prod"));
}

// ── Type mismatch errors ──────────────────────────────────────────────────────

#[test]
fn deser_type_mismatch_string_to_i64_is_err() {
    let val = Value::String("not a number".to_string());
    let result: Result<i64, Error> = from_value(val);
    assert!(result.is_err());
}

#[test]
fn deser_type_mismatch_int_to_bool_is_err() {
    let val = Value::Int(1);
    let result: Result<bool, Error> = from_value(val);
    assert!(result.is_err());
}

#[test]
fn deser_type_mismatch_bool_to_string_is_err() {
    let val = Value::Bool(true);
    let result: Result<String, Error> = from_value(val);
    assert!(result.is_err());
}

// ── Serialize string / int / float / bool to WCL text ────────────────────────

#[test]
fn ser_string_produces_quoted_output() {
    let result = to_string(&"hello").unwrap();
    assert_eq!(result, "\"hello\"");
}

#[test]
fn ser_string_escapes_quotes_and_newlines() {
    let result = to_string(&"say \"hi\"\nnext").unwrap();
    assert_eq!(result, r#""say \"hi\"\nnext""#);
}

#[test]
fn ser_i64_produces_decimal() {
    assert_eq!(to_string(&42i64).unwrap(), "42");
    assert_eq!(to_string(&-7i64).unwrap(), "-7");
    assert_eq!(to_string(&0i64).unwrap(), "0");
}

#[test]
fn ser_f64_produces_decimal() {
    let result = to_string(&1.5f64).unwrap();
    assert_eq!(result, "1.5");
}

#[test]
fn ser_bool_true() {
    assert_eq!(to_string(&true).unwrap(), "true");
}

#[test]
fn ser_bool_false() {
    assert_eq!(to_string(&false).unwrap(), "false");
}

#[test]
fn ser_none_produces_null() {
    let v: Option<i64> = None;
    assert_eq!(to_string(&v).unwrap(), "null");
}

#[test]
fn ser_some_value() {
    let v: Option<i64> = Some(99);
    assert_eq!(to_string(&v).unwrap(), "99");
}

// ── Serialize Vec<T> ──────────────────────────────────────────────────────────

#[test]
fn ser_vec_of_ints() {
    let v = vec![1i32, 2, 3];
    assert_eq!(to_string(&v).unwrap(), "[1, 2, 3]");
}

#[test]
fn ser_empty_vec() {
    let v: Vec<i32> = vec![];
    assert_eq!(to_string(&v).unwrap(), "[]");
}

#[test]
fn ser_vec_of_strings() {
    let v = vec!["a", "b"];
    assert_eq!(to_string(&v).unwrap(), "[\"a\", \"b\"]");
}

// ── Serialize struct to WCL text (compact) ───────────────────────────────────

#[derive(Serialize)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn ser_struct_compact_no_spaces_between_fields() {
    let p = Point { x: 1, y: 2 };
    let result = to_string(&p).unwrap();
    // Compact mode: no newlines
    assert_eq!(result, "{x = 1y = 2}");
}

// ── Serialize struct to WCL text (pretty) ────────────────────────────────────

#[test]
fn ser_struct_pretty_with_indentation() {
    let p = Point { x: 10, y: 20 };
    let result = to_string_pretty(&p).unwrap();
    assert_eq!(result, "{\n    x = 10\n    y = 20\n}");
}

#[derive(Serialize)]
struct ServerConfig {
    host: String,
    port: i32,
    tls: bool,
}

#[test]
fn ser_struct_pretty_all_field_types() {
    let cfg = ServerConfig {
        host: "localhost".to_string(),
        port: 8443,
        tls: true,
    };
    let result = to_string_pretty(&cfg).unwrap();
    assert!(result.contains("host = \"localhost\""));
    assert!(result.contains("port = 8443"));
    assert!(result.contains("tls = true"));
}

// ── Round-trip: serialize then deserialize ────────────────────────────────────

#[test]
fn roundtrip_i64_via_value() {
    let original = 12345i64;
    // Serialization produces text representation
    let text = to_string(&original).unwrap();
    assert_eq!(text, "12345");
    // Deserialization from Value
    let deserialized: i64 = from_value(Value::Int(original)).unwrap();
    assert_eq!(deserialized, original);
}

#[test]
fn roundtrip_string_via_value() {
    let original = "round-trip string".to_string();
    let text = to_string(&original).unwrap();
    assert_eq!(text, "\"round-trip string\"");
    let deserialized: String = from_value(Value::String(original.clone())).unwrap();
    assert_eq!(deserialized, original);
}

#[test]
fn roundtrip_float_via_value() {
    let original = 2.718f64;
    let text = to_string(&original).unwrap();
    assert_eq!(text, "2.718");
    let deserialized: f64 = from_value(Value::Float(original)).unwrap();
    assert!((deserialized - original).abs() < 1e-10);
}

#[test]
fn roundtrip_bool_via_value() {
    for b in [true, false] {
        let text = to_string(&b).unwrap();
        let deserialized: bool = from_value(Value::Bool(b)).unwrap();
        assert_eq!(deserialized, b);
        assert_eq!(text, if b { "true" } else { "false" });
    }
}

#[test]
fn roundtrip_vec_via_value() {
    let original = vec![10i64, 20, 30];
    let text = to_string(&original).unwrap();
    assert_eq!(text, "[10, 20, 30]");

    let val = Value::List(original.iter().copied().map(Value::Int).collect());
    let deserialized: Vec<i64> = from_value(val).unwrap();
    assert_eq!(deserialized, original);
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct RoundTripStruct {
    count: i64,
    active: bool,
    label: String,
}

#[test]
fn roundtrip_struct_serialize_then_deserialize() {
    let original = RoundTripStruct {
        count: 7,
        active: true,
        label: "test".to_string(),
    };

    // Serialize to WCL text and verify field presence
    let text = to_string_pretty(&original).unwrap();
    assert!(text.contains("count = 7"));
    assert!(text.contains("active = true"));
    assert!(text.contains("label = \"test\""));

    // Deserialize from a Value::Map (simulating what the evaluator would produce)
    let mut map = IndexMap::new();
    map.insert("count".to_string(), Value::Int(7));
    map.insert("active".to_string(), Value::Bool(true));
    map.insert("label".to_string(), Value::String("test".to_string()));

    let deserialized: RoundTripStruct = from_value(Value::Map(map)).unwrap();
    assert_eq!(deserialized, original);
}

#[test]
fn roundtrip_option_none_and_some() {
    // None
    let none_text = to_string::<Option<i64>>(&None).unwrap();
    assert_eq!(none_text, "null");
    let none_deser: Option<i64> = from_value(Value::Null).unwrap();
    assert_eq!(none_deser, None);

    // Some
    let some_text = to_string::<Option<i64>>(&Some(42)).unwrap();
    assert_eq!(some_text, "42");
    let some_deser: Option<i64> = from_value(Value::Int(42)).unwrap();
    assert_eq!(some_deser, Some(42));
}
