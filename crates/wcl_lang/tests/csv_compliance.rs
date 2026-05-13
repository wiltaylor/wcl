use indexmap::IndexMap;
use wcl_lang::eval::value::Value;
use wcl_lang::transform::codec::{custom, CodecOptions};
use wcl_lang::transform::TransformError;

fn csv_codec() -> custom::CustomCodec {
    custom::standard_registry()
        .expect("standard codecs")
        .get("csv")
        .expect("csv codec")
        .clone()
}

fn decode(input: &str, options: CodecOptions) -> Result<Vec<Value>, TransformError> {
    custom::decode_custom_records_with_options(input.as_bytes(), &csv_codec(), &options)
}

fn encode(records: &[Value], options: CodecOptions) -> Result<String, TransformError> {
    let mut output = Vec::new();
    custom::encode_custom_records(records, &csv_codec(), &options, &mut output)?;
    Ok(String::from_utf8(output).expect("utf8 output"))
}

fn options(pairs: &[(&str, Value)]) -> CodecOptions {
    let mut options = IndexMap::new();
    for (key, value) in pairs {
        options.insert((*key).to_string(), value.clone());
    }
    options
}

fn field<'a>(record: &'a Value, key: &str) -> &'a Value {
    let Value::Map(map) = record else {
        panic!("expected map record");
    };
    map.get(key).unwrap_or_else(|| panic!("missing key {key}"))
}

#[test]
fn decodes_header_records() {
    let records = decode("name,age\nAlice,30\nBob,25\n", CodecOptions::new()).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(field(&records[0], "name"), &Value::String("Alice".into()));
    assert_eq!(field(&records[1], "age"), &Value::String("25".into()));
}

#[test]
fn decodes_without_headers_as_col_keys() {
    let records = decode(
        "Alice,30\nBob,25\n",
        options(&[("has_header", Value::Bool(false))]),
    )
    .unwrap();
    assert_eq!(field(&records[0], "col0"), &Value::String("Alice".into()));
    assert_eq!(field(&records[0], "col1"), &Value::String("30".into()));
}

#[test]
fn decodes_quotes_escapes_and_multiline_fields() {
    let records = decode(
        "name,note\nAlice,\"hello, \"\"world\"\"\"\nBob,\"line 1\nline 2\"\n",
        CodecOptions::new(),
    )
    .unwrap();
    assert_eq!(
        field(&records[0], "note"),
        &Value::String("hello, \"world\"".into())
    );
    assert_eq!(
        field(&records[1], "note"),
        &Value::String("line 1\nline 2".into())
    );
}

#[test]
fn decodes_crlf_and_empty_fields() {
    let records = decode("a,b,c\r\n1,,3\r\n4,5,\r\n", CodecOptions::new()).unwrap();
    assert_eq!(field(&records[0], "b"), &Value::String(String::new()));
    assert_eq!(field(&records[1], "c"), &Value::String(String::new()));
}

#[test]
fn decodes_custom_separator() {
    let records = decode(
        "name\tport\napi\t9090\n",
        options(&[("separator", Value::String("\t".into()))]),
    )
    .unwrap();
    assert_eq!(field(&records[0], "name"), &Value::String("api".into()));
    assert_eq!(field(&records[0], "port"), &Value::String("9090".into()));
}

#[test]
fn rejects_malformed_csv() {
    assert!(decode("name,note\nAlice,\"unterminated\n", CodecOptions::new()).is_err());
    assert!(decode("name,note\nAlice,\"quoted\"x\n", CodecOptions::new()).is_err());
    assert!(decode("name,name\nAlice,1\n", CodecOptions::new()).is_err());
}

#[test]
fn encodes_with_rfc_4180_quoting() {
    let mut row = IndexMap::new();
    row.insert("name".into(), Value::String("Alice".into()));
    row.insert("note".into(), Value::String("hello, \"world\"".into()));

    let output = encode(&[Value::Map(row)], CodecOptions::new()).unwrap();
    assert_eq!(output, "name,note\nAlice,\"hello, \"\"world\"\"\"\n");

    let decoded = decode(&output, CodecOptions::new()).unwrap();
    assert_eq!(
        field(&decoded[0], "note"),
        &Value::String("hello, \"world\"".into())
    );
}
