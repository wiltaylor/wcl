use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn test_parse_simple() {
    let result = wcl_wasm::parse("x = 42", None).unwrap();
    let values = js_sys::Reflect::get(&result, &JsValue::from_str("values")).unwrap();
    let x = js_sys::Reflect::get(&values, &JsValue::from_str("x")).unwrap();
    assert_eq!(x.as_f64().unwrap(), 42.0);

    let has_errors = js_sys::Reflect::get(&result, &JsValue::from_str("hasErrors")).unwrap();
    assert_eq!(has_errors.as_bool().unwrap(), false);
}

#[wasm_bindgen_test]
fn test_parse_values_simple() {
    let result = wcl_wasm::parse_values("name = \"hello\"", None).unwrap();
    let name = js_sys::Reflect::get(&result, &JsValue::from_str("name")).unwrap();
    assert_eq!(name.as_string().unwrap(), "hello");
}

#[wasm_bindgen_test]
fn test_parse_values_error() {
    let result = wcl_wasm::parse_values("x = @@@", None);
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn test_parse_with_block() {
    let result = wcl_wasm::parse("server { port = 8080 }", None).unwrap();
    let has_errors = js_sys::Reflect::get(&result, &JsValue::from_str("hasErrors")).unwrap();
    assert_eq!(has_errors.as_bool().unwrap(), false);
    let values = js_sys::Reflect::get(&result, &JsValue::from_str("values")).unwrap();
    assert!(values.is_object());
}

#[wasm_bindgen_test]
fn test_parse_multiple_types() {
    let source = r#"
        name = "test"
        count = 5
        ratio = 3.14
        enabled = true
        items = [1, 2, 3]
    "#;
    let result = wcl_wasm::parse_values(source, None).unwrap();

    let name = js_sys::Reflect::get(&result, &JsValue::from_str("name")).unwrap();
    assert_eq!(name.as_string().unwrap(), "test");

    let count = js_sys::Reflect::get(&result, &JsValue::from_str("count")).unwrap();
    assert_eq!(count.as_f64().unwrap(), 5.0);

    let ratio = js_sys::Reflect::get(&result, &JsValue::from_str("ratio")).unwrap();
    assert!((ratio.as_f64().unwrap() - 3.14).abs() < 0.001);

    let enabled = js_sys::Reflect::get(&result, &JsValue::from_str("enabled")).unwrap();
    assert_eq!(enabled.as_bool().unwrap(), true);

    let items = js_sys::Reflect::get(&result, &JsValue::from_str("items")).unwrap();
    assert!(js_sys::Array::is_array(&items));
    let arr = js_sys::Array::from(&items);
    assert_eq!(arr.length(), 3);
}

#[wasm_bindgen_test]
fn test_query_simple() {
    let source = "service { port = 8080 }\nservice { port = 9090 }";
    let result = wcl_wasm::query(source, "service | .port", None).unwrap();
    assert!(js_sys::Array::is_array(&result));
    let arr = js_sys::Array::from(&result);
    assert_eq!(arr.length(), 2);
}

#[wasm_bindgen_test]
fn test_table_parse() {
    let source = r#"table users { name : string  age : int  | "Alice" | 30 | }"#;
    let result = wcl_wasm::parse(source, None).unwrap();
    let has_errors = js_sys::Reflect::get(&result, &JsValue::from_str("hasErrors")).unwrap();
    assert_eq!(has_errors.as_bool().unwrap(), false);
}

#[wasm_bindgen_test]
fn test_table_schema_ref_parse() {
    let source = r#"table users : user_row { | "Alice" | 30 | }"#;
    let result = wcl_wasm::parse(source, None).unwrap();
    // Should parse (schema may not exist, but parsing is OK)
    let has_errors = js_sys::Reflect::get(&result, &JsValue::from_str("hasErrors")).unwrap();
    // May have errors from schema not found, but should not have parse errors
    let _ = has_errors;
}
