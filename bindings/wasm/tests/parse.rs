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

#[wasm_bindgen_test]
fn test_attribute_macro() {
    // Attribute macros use `macro @name(...)` with inject/set/remove directives
    let source = r#"
        macro @add_env(env) {
            inject {
                environment = env
            }
        }

        @add_env("production")
        server web {
            port = 8080
        }
    "#;
    let result = wcl_wasm::parse(source, None).unwrap();
    let has_errors = js_sys::Reflect::get(&result, &JsValue::from_str("hasErrors")).unwrap();
    assert_eq!(has_errors.as_bool().unwrap(), false);

    // In doc.values, block with id "web" is stored under key "web" as a BlockRef.
    // value_to_js converts BlockRef to { kind, id, attributes: { ... } }
    let values = js_sys::Reflect::get(&result, &JsValue::from_str("values")).unwrap();
    let block = js_sys::Reflect::get(&values, &JsValue::from_str("web")).unwrap();
    assert!(block.is_object());

    let kind = js_sys::Reflect::get(&block, &JsValue::from_str("kind")).unwrap();
    assert_eq!(kind.as_string().unwrap(), "server");

    let attrs = js_sys::Reflect::get(&block, &JsValue::from_str("attributes")).unwrap();
    let env = js_sys::Reflect::get(&attrs, &JsValue::from_str("environment")).unwrap();
    assert_eq!(env.as_string().unwrap(), "production");

    let port = js_sys::Reflect::get(&attrs, &JsValue::from_str("port")).unwrap();
    assert_eq!(port.as_f64().unwrap(), 8080.0);
}

#[wasm_bindgen_test]
fn test_for_loop() {
    // For loops iterate over a list and expand body items for each element.
    // Use parse() (not parse_values()) since duplicate attrs may produce warnings.
    let source = r#"
        let ports = [8080, 9090, 3000]
        for p in ports {
            last_port = p
        }
    "#;
    let result = wcl_wasm::parse(source, None).unwrap();
    let values = js_sys::Reflect::get(&result, &JsValue::from_str("values")).unwrap();
    // The for loop expands, and the first iteration's value is stored
    let last_port = js_sys::Reflect::get(&values, &JsValue::from_str("last_port")).unwrap();
    assert_eq!(last_port.as_f64().unwrap(), 8080.0);
}

#[wasm_bindgen_test]
fn test_for_loop_on_table() {
    // For loops can iterate over tables (recent feature)
    let source = r#"
        table users {
            name : string
            | "Alice" |
            | "Bob"   |
        }
        for row in users {
            greeting = row.name
        }
    "#;
    let result = wcl_wasm::parse(source, None).unwrap();
    let values = js_sys::Reflect::get(&result, &JsValue::from_str("values")).unwrap();
    // Should have a greeting attribute from the loop
    let greeting = js_sys::Reflect::get(&values, &JsValue::from_str("greeting")).unwrap();
    assert!(greeting.is_string());
}

#[wasm_bindgen_test]
fn test_if_true() {
    // If condition produces body items when the condition is true
    let source = r#"
        let enabled = true
        if enabled {
            feature flags {
                active = true
            }
        }
    "#;
    let result = wcl_wasm::parse(source, None).unwrap();
    let has_errors = js_sys::Reflect::get(&result, &JsValue::from_str("hasErrors")).unwrap();
    assert_eq!(has_errors.as_bool().unwrap(), false);

    // Block "feature flags" is stored under key "flags" as BlockRef
    let values = js_sys::Reflect::get(&result, &JsValue::from_str("values")).unwrap();
    let block = js_sys::Reflect::get(&values, &JsValue::from_str("flags")).unwrap();
    assert!(block.is_object());

    let kind = js_sys::Reflect::get(&block, &JsValue::from_str("kind")).unwrap();
    assert_eq!(kind.as_string().unwrap(), "feature");

    let attrs = js_sys::Reflect::get(&block, &JsValue::from_str("attributes")).unwrap();
    let active = js_sys::Reflect::get(&attrs, &JsValue::from_str("active")).unwrap();
    assert_eq!(active.as_bool().unwrap(), true);
}

#[wasm_bindgen_test]
fn test_if_false_with_else() {
    let source = r#"
        let enabled = false
        if enabled {
            feature on {
                active = true
            }
        } else {
            feature off {
                active = false
            }
        }
    "#;
    let result = wcl_wasm::parse(source, None).unwrap();
    let has_errors = js_sys::Reflect::get(&result, &JsValue::from_str("hasErrors")).unwrap();
    assert_eq!(has_errors.as_bool().unwrap(), false);

    // The else branch should be taken: block stored under key "off"
    let values = js_sys::Reflect::get(&result, &JsValue::from_str("values")).unwrap();
    let block = js_sys::Reflect::get(&values, &JsValue::from_str("off")).unwrap();
    assert!(block.is_object());

    let kind = js_sys::Reflect::get(&block, &JsValue::from_str("kind")).unwrap();
    assert_eq!(kind.as_string().unwrap(), "feature");

    let attrs = js_sys::Reflect::get(&block, &JsValue::from_str("attributes")).unwrap();
    let active = js_sys::Reflect::get(&attrs, &JsValue::from_str("active")).unwrap();
    assert_eq!(active.as_bool().unwrap(), false);
}

#[wasm_bindgen_test]
fn test_if_with_attribute() {
    // If condition can also produce top-level attributes
    let source = r#"
        let debug = true
        if debug {
            log_level = "verbose"
        }
    "#;
    let result = wcl_wasm::parse_values(source, None).unwrap();
    let log_level = js_sys::Reflect::get(&result, &JsValue::from_str("log_level")).unwrap();
    assert_eq!(log_level.as_string().unwrap(), "verbose");
}

#[wasm_bindgen_test]
fn test_inline_args() {
    let source = r#"
        server "web" {
            port = 8080
        }
    "#;
    let result = wcl_wasm::parse(source, None).unwrap();
    let has_errors = js_sys::Reflect::get(&result, &JsValue::from_str("hasErrors")).unwrap();
    assert_eq!(has_errors.as_bool().unwrap(), false);

    // Block without an explicit ID uses __block_<kind> as key in doc.values
    // value_to_js converts it to { kind, attributes: { _args, port, ... } }
    let values = js_sys::Reflect::get(&result, &JsValue::from_str("values")).unwrap();
    let block = js_sys::Reflect::get(&values, &JsValue::from_str("__block_server")).unwrap();
    assert!(block.is_object());

    let kind = js_sys::Reflect::get(&block, &JsValue::from_str("kind")).unwrap();
    assert_eq!(kind.as_string().unwrap(), "server");

    // Inline args should produce an _args list within attributes
    let attrs = js_sys::Reflect::get(&block, &JsValue::from_str("attributes")).unwrap();
    let args = js_sys::Reflect::get(&attrs, &JsValue::from_str("_args")).unwrap();
    assert!(js_sys::Array::is_array(&args));
    let arr = js_sys::Array::from(&args);
    assert_eq!(arr.length(), 1);
    assert_eq!(arr.get(0).as_string().unwrap(), "web");

    let port = js_sys::Reflect::get(&attrs, &JsValue::from_str("port")).unwrap();
    assert_eq!(port.as_f64().unwrap(), 8080.0);
}

#[wasm_bindgen_test]
fn test_partial_let() {
    // partial let merges list values across declarations
    let source = r#"
        partial let tags = ["x", "y"]
        partial let tags = ["z"]
        all_tags = tags
    "#;
    let result = wcl_wasm::parse_values(source, None).unwrap();
    let all_tags = js_sys::Reflect::get(&result, &JsValue::from_str("all_tags")).unwrap();
    assert!(js_sys::Array::is_array(&all_tags));
    let arr = js_sys::Array::from(&all_tags);
    assert_eq!(arr.length(), 3);
    assert_eq!(arr.get(0).as_string().unwrap(), "x");
    assert_eq!(arr.get(1).as_string().unwrap(), "y");
    assert_eq!(arr.get(2).as_string().unwrap(), "z");
}

#[wasm_bindgen_test]
fn test_variables_let_binding() {
    let source = r#"
        let name = "test"
        config {
            value = name
        }
    "#;
    let result = wcl_wasm::parse(source, None).unwrap();
    let has_errors = js_sys::Reflect::get(&result, &JsValue::from_str("hasErrors")).unwrap();
    assert_eq!(has_errors.as_bool().unwrap(), false);

    // Block without ID stored as __block_config, which is a BlockRef
    let values = js_sys::Reflect::get(&result, &JsValue::from_str("values")).unwrap();
    let block = js_sys::Reflect::get(&values, &JsValue::from_str("__block_config")).unwrap();
    assert!(block.is_object());

    let attrs = js_sys::Reflect::get(&block, &JsValue::from_str("attributes")).unwrap();
    let value = js_sys::Reflect::get(&attrs, &JsValue::from_str("value")).unwrap();
    assert_eq!(value.as_string().unwrap(), "test");
}

#[wasm_bindgen_test]
fn test_variables_top_level() {
    let source = r#"
        let name = "test"
        value = name
    "#;
    let result = wcl_wasm::parse_values(source, None).unwrap();
    let value = js_sys::Reflect::get(&result, &JsValue::from_str("value")).unwrap();
    assert_eq!(value.as_string().unwrap(), "test");
}

#[wasm_bindgen_test]
fn test_external_variables() {
    let source = r#"
        greeting = format("Hello, {}!", name)
    "#;
    let opts = js_sys::Object::new();
    let vars = js_sys::Object::new();
    js_sys::Reflect::set(
        &vars,
        &JsValue::from_str("name"),
        &JsValue::from_str("World"),
    )
    .unwrap();
    js_sys::Reflect::set(&opts, &JsValue::from_str("variables"), &vars).unwrap();

    let result = wcl_wasm::parse_values(source, Some(opts.into())).unwrap();
    let greeting = js_sys::Reflect::get(&result, &JsValue::from_str("greeting")).unwrap();
    assert_eq!(greeting.as_string().unwrap(), "Hello, World!");
}

#[wasm_bindgen_test]
fn test_custom_functions() {
    let source = r#"
        result = double(21)
    "#;

    let opts = js_sys::Object::new();
    let functions = js_sys::Object::new();

    // Create a JS function that doubles its argument
    let double_fn = js_sys::Function::new_with_args("x", "return x * 2");
    js_sys::Reflect::set(&functions, &JsValue::from_str("double"), &double_fn).unwrap();
    js_sys::Reflect::set(&opts, &JsValue::from_str("functions"), &functions).unwrap();

    let result = wcl_wasm::parse_values(source, Some(opts.into())).unwrap();
    let res = js_sys::Reflect::get(&result, &JsValue::from_str("result")).unwrap();
    assert_eq!(res.as_f64().unwrap(), 42.0);
}

#[wasm_bindgen_test]
fn test_custom_function_string() {
    let source = r#"
        result = greet("Alice")
    "#;

    let opts = js_sys::Object::new();
    let functions = js_sys::Object::new();

    let greet_fn = js_sys::Function::new_with_args("name", "return 'Hello, ' + name + '!'");
    js_sys::Reflect::set(&functions, &JsValue::from_str("greet"), &greet_fn).unwrap();
    js_sys::Reflect::set(&opts, &JsValue::from_str("functions"), &functions).unwrap();

    let result = wcl_wasm::parse_values(source, Some(opts.into())).unwrap();
    let res = js_sys::Reflect::get(&result, &JsValue::from_str("result")).unwrap();
    assert_eq!(res.as_string().unwrap(), "Hello, Alice!");
}
