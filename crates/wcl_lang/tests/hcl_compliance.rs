use indexmap::IndexMap;
use wcl_lang::eval::value::Value;
use wcl_lang::transform::codec::{custom, CodecOptions};
use wcl_lang::transform::TransformError;

fn hcl_codec() -> custom::CustomCodec {
    custom::standard_registry()
        .expect("standard codecs")
        .get("hcl")
        .expect("hcl codec")
        .clone()
}

fn decode(input: &str, options: CodecOptions) -> Result<Vec<Value>, TransformError> {
    let input = input.to_string();
    std::thread::Builder::new()
        .name("hcl-compliance-decode".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            custom::decode_custom_records_with_options(input.as_bytes(), &hcl_codec(), &options)
        })
        .expect("spawn hcl decode")
        .join()
        .expect("join hcl decode")
}

fn field<'a>(record: &'a Value, key: &str) -> &'a Value {
    let Value::Map(map) = record else {
        panic!("expected map record");
    };
    map.get(key).unwrap_or_else(|| panic!("missing key {key}"))
}

fn options(pairs: &[(&str, Value)]) -> CodecOptions {
    let mut options = IndexMap::new();
    for (key, value) in pairs {
        options.insert((*key).to_string(), value.clone());
    }
    options
}

#[test]
fn decodes_attributes_and_literal_values() {
    let records = decode(
        r#"
name = "Alice"
age = 30
active = true
items = [1, "two", false]
meta = { role = "admin", score = 9 }
"#,
        CodecOptions::new(),
    )
    .unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(field(&records[0], "name"), &Value::String("Alice".into()));
    assert_eq!(field(&records[0], "age"), &Value::Int(30));
    assert_eq!(field(&records[0], "active"), &Value::Bool(true));
    let Value::List(items) = field(&records[0], "items") else {
        panic!("expected list");
    };
    assert_eq!(items[1], Value::String("two".into()));
}

#[test]
fn decodes_blocks_and_labels_as_nested_maps() {
    let records = decode(
        r#"
server "web" "prod" {
  host = "localhost"
  port = 8080
}
server "api" { host = "api.local" }
"#,
        CodecOptions::new(),
    )
    .unwrap();

    let Value::Map(server) = field(&records[0], "server") else {
        panic!("expected server map");
    };
    let Value::Map(web) = server.get("web").expect("web") else {
        panic!("expected web map");
    };
    let Value::Map(prod) = web.get("prod").expect("prod") else {
        panic!("expected prod map");
    };
    assert_eq!(prod.get("port"), Some(&Value::Int(8080)));

    let Value::Map(api) = server.get("api").expect("api") else {
        panic!("expected api map");
    };
    assert_eq!(api.get("host"), Some(&Value::String("api.local".into())));
}

#[test]
fn evaluates_basic_expressions_with_variables_and_functions() {
    let mut vars = IndexMap::new();
    vars.insert("env".into(), Value::String("prod".into()));
    vars.insert("replicas".into(), Value::Int(3));

    let records = decode(
        r#"
count = 1 + replicas * 2
enabled = replicas > 1 && env == "prod"
name = upper(env)
"#,
        options(&[("variables", Value::Map(vars))]),
    )
    .unwrap();

    assert_eq!(field(&records[0], "count"), &Value::Int(7));
    assert_eq!(field(&records[0], "enabled"), &Value::Bool(true));
    assert_eq!(field(&records[0], "name"), &Value::String("PROD".into()));
}

#[test]
fn decodes_heredocs_and_comments() {
    let records = decode(
        r#"
// line comment
name = "Alice" # trailing comment
script = <<EOF
hello
world
EOF
"#,
        CodecOptions::new(),
    )
    .unwrap();

    assert_eq!(field(&records[0], "name"), &Value::String("Alice".into()));
    assert_eq!(
        field(&records[0], "script"),
        &Value::String("hello\nworld".into())
    );
}

#[test]
fn rejects_errors() {
    assert!(decode("name = 1\nname = 2\n", CodecOptions::new()).is_err());
    assert!(decode("name = missing\n", CodecOptions::new()).is_err());
}
