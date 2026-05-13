use indexmap::IndexMap;
use wcl_lang::eval::value::Value;
use wcl_lang::transform::codec::{custom, CodecOptions};
use wcl_lang::transform::TransformError;

fn xml_codec() -> custom::CustomCodec {
    custom::standard_registry()
        .expect("standard codecs")
        .get("xml")
        .expect("xml codec")
        .clone()
}

fn decode(input: &str) -> Result<Vec<Value>, TransformError> {
    let input = input.to_string();
    std::thread::Builder::new()
        .name("xml-compliance-decode".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            custom::decode_custom_records_with_options(
                input.as_bytes(),
                &xml_codec(),
                &CodecOptions::new(),
            )
        })
        .expect("spawn XML decode thread")
        .join()
        .expect("XML decode thread")
}

fn encode(records: &[Value], options: CodecOptions) -> Result<String, TransformError> {
    let records = records.to_vec();
    std::thread::Builder::new()
        .name("xml-compliance-encode".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let mut output = Vec::new();
            custom::encode_custom_records(&records, &xml_codec(), &options, &mut output)?;
            Ok(String::from_utf8(output).expect("utf8 output"))
        })
        .expect("spawn XML encode thread")
        .join()
        .expect("XML encode thread")
}

fn map<'a>(value: &'a Value) -> &'a IndexMap<String, Value> {
    let Value::Map(map) = value else {
        panic!("expected map, got {value:?}");
    };
    map
}

fn string_field<'a>(value: &'a Value, key: &str) -> &'a str {
    let Value::String(text) = map(value).get(key).unwrap_or_else(|| panic!("missing {key}")) else {
        panic!("expected string field {key}");
    };
    text
}

fn list_field<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    let Value::List(items) = map(value).get(key).unwrap_or_else(|| panic!("missing {key}")) else {
        panic!("expected list field {key}");
    };
    items
}

#[test]
fn decodes_structured_root_with_attributes_text_and_entities() {
    let records = decode(
        r#"<?xml version="1.0"?><book id="b1" note="Tom &amp; Jerry">A&lt;B&#x21;</book>"#,
    )
    .unwrap();
    assert_eq!(records.len(), 1);
    let root = &records[0];
    assert_eq!(string_field(root, "type"), "element");
    assert_eq!(string_field(root, "name"), "book");
    assert_eq!(string_field(root, "local_name"), "book");
    assert_eq!(string_field(root, "text"), "A<B!");

    let attrs = list_field(root, "attributes");
    assert_eq!(attrs.len(), 2);
    assert_eq!(string_field(&attrs[0], "name"), "id");
    assert_eq!(string_field(&attrs[0], "value"), "b1");
    assert_eq!(string_field(&attrs[1], "value"), "Tom & Jerry");
}

#[test]
fn decodes_namespaces_comments_cdata_and_pi_in_order() {
    let records = decode(
        r#"<r xmlns="urn:default" xmlns:p="urn:p"><!--ok--><?go now?><p:c a="1"><![CDATA[x < y]]></p:c></r>"#,
    )
    .unwrap();
    let root = &records[0];
    assert_eq!(string_field(root, "namespace_uri"), "urn:default");
    let children = list_field(root, "children");
    assert_eq!(string_field(&children[0], "type"), "comment");
    assert_eq!(string_field(&children[1], "type"), "pi");
    assert_eq!(string_field(&children[2], "name"), "p:c");
    assert_eq!(string_field(&children[2], "namespace_uri"), "urn:p");
    assert_eq!(string_field(&children[2], "text"), "x < y");
}

#[test]
fn rejects_non_well_formed_documents() {
    let cases = [
        ("mismatched tags", "<a></b>"),
        ("duplicate attributes", "<a x=\"1\" x=\"2\"/>"),
        ("unknown entity", "<a>&custom;</a>"),
        ("external doctype", "<!DOCTYPE a SYSTEM \"a.dtd\"><a/>"),
        ("entity declaration", "<!DOCTYPE a [<!ENTITY x \"y\">]><a/>"),
        ("unbound prefix", "<p:a/>"),
        ("multiple roots", "<a/><b/>"),
        ("bad comment", "<a><!-- bad -- comment --></a>"),
    ];
    for (name, source) in cases {
        assert!(decode(source).is_err(), "{name} should fail");
    }
}

#[test]
fn encodes_structured_element_and_plain_values() {
    let records = decode(r#"<root><item flag="yes">a &amp; b</item></root>"#).unwrap();
    let output = encode(&records, CodecOptions::new()).unwrap();
    assert_eq!(output, r#"<root><item flag="yes">a &amp; b</item></root>"#.to_string() + "\n");

    let mut record = IndexMap::new();
    record.insert("name".into(), Value::String("Alice & Bob".into()));
    let mut options = CodecOptions::new();
    options.insert("root_name".into(), Value::String("person".into()));
    options.insert("xml_declaration".into(), Value::Bool(true));
    let output = encode(&[Value::Map(record)], options).unwrap();
    assert_eq!(
        output,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><person><name>Alice &amp; Bob</name></person>\n"
    );
}

#[test]
fn encode_rejects_invalid_names() {
    let mut record = IndexMap::new();
    record.insert("1bad".into(), Value::String("nope".into()));
    assert!(encode(&[Value::Map(record)], CodecOptions::new()).is_err());
}
