use std::path::PathBuf;

use wcl_lang::{Document, ResolvedType, Value};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn open(src: &str) -> Document {
    Document::open(src, "test").expect("open")
}

#[test]
fn parses_basic_example_from_disk() {
    let doc = Document::from_file(&examples_dir().join("basic.wcl")).expect("basic example parses");
    assert_eq!(
        doc.field("name").unwrap().value().unwrap(),
        &Value::Utf8("alpha".into())
    );
    let svc = doc.block("service").expect("service block");
    assert_eq!(svc.labels(), &["web".to_string()]);
    assert_eq!(
        svc.field("port").unwrap().value().unwrap(),
        &Value::I64(8080)
    );
}

#[test]
fn document_round_trips_simple_fields() {
    let doc = open(
        r#"
        name  = "alpha"
        count = 3
        # a comment
        // another comment
        flag  = false
        "#,
    );
    assert_eq!(doc.fields().count(), 3);
    assert_eq!(
        doc.field("flag").unwrap().value().unwrap(),
        &Value::Bool(false)
    );
}

#[test]
fn named_type_refs_resolve_in_fixture() {
    let doc = Document::from_file(&examples_dir().join("types.wcl")).expect("types fixture parses");
    let user = doc.type_decl("User").expect("User type");
    let parent = user.field("parent").expect("parent field");
    // parent is now &User?
    let ResolvedType::Reference(inner) = doc.resolve(parent.type_ref()) else {
        panic!("parent should resolve to a reference");
    };
    let ResolvedType::Named(decl) = *inner else {
        panic!("reference inner should be Named(User)");
    };
    assert_eq!(decl.name(), "User");
}

#[test]
fn typed_literals_resolve_from_disk() {
    let doc = Document::from_file(&examples_dir().join("types.wcl")).expect("types fixture parses");
    assert_eq!(doc.field("byte").unwrap().value().unwrap(), &Value::U8(200));
    assert_eq!(
        doc.field("small").unwrap().value().unwrap(),
        &Value::I8(-120)
    );
    assert_eq!(
        doc.field("ratio").unwrap().value().unwrap(),
        &Value::F32(1.5)
    );
    assert_eq!(
        doc.field("name").unwrap().value().unwrap(),
        &Value::Ascii("alpha".into())
    );
    assert_eq!(
        doc.field("hello16").unwrap().value().unwrap(),
        &Value::Utf16("hello".encode_utf16().collect())
    );
}

#[test]
fn nested_blocks_preserve_structure() {
    let doc = open(
        r#"
        service "web" {
          port = 8080
          metadata {
            region = "us-east-1"
          }
        }
        "#,
    );
    let svc = doc.block("service").unwrap();
    assert_eq!(
        svc.field("port").unwrap().value().unwrap(),
        &Value::I64(8080)
    );
    let meta = svc.block("metadata").unwrap();
    assert_eq!(
        meta.field("region").unwrap().value().unwrap(),
        &Value::Utf8("us-east-1".into())
    );
}

#[test]
fn parse_error_has_useful_span() {
    let err = Document::open("name = ", "input").unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("expected value"), "rendered: {rendered}");
}

#[test]
fn field_value_address_is_stable_across_accesses() {
    let doc = open(r#"name = "alpha""#);
    let f = doc.field("name").unwrap();
    let p1 = f.value().unwrap() as *const Value;
    let p2 = f.value().unwrap() as *const Value;
    assert_eq!(p1, p2);
}

#[test]
fn span_is_available_without_forcing_value() {
    let doc = open(r#"name = "alpha""#);
    let f = doc.field("name").unwrap();
    let span = f.span();
    assert!(span.start < span.end);
}
