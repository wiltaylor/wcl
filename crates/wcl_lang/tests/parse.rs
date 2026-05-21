use std::path::PathBuf;

use wcl_lang::{Value, parse, parse_file};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

#[test]
fn parses_basic_example_from_disk() {
    let doc = parse_file(&examples_dir().join("basic.wcl")).expect("basic example parses");
    assert_eq!(
        doc.field("name").unwrap().value,
        Value::String("alpha".into())
    );
    let svc = doc
        .blocks()
        .find(|b| b.kind == "service")
        .expect("service block");
    assert_eq!(svc.labels, vec!["web".to_string()]);
    assert_eq!(svc.field("port").unwrap().value, Value::Int(8080));
}

#[test]
fn document_round_trips_simple_fields() {
    let doc = parse(
        r#"
        name  = "alpha"
        count = 3
        # a comment
        // another comment
        flag  = false
        "#,
    )
    .expect("parses");
    assert_eq!(doc.fields().count(), 3);
    assert_eq!(doc.field("flag").unwrap().value, Value::Bool(false));
}

#[test]
fn nested_blocks_preserve_structure() {
    let doc = parse(
        r#"
        service "web" {
          port = 8080
          metadata {
            region = "us-east-1"
          }
        }
        "#,
    )
    .expect("parses");
    let svc = doc.blocks().next().unwrap();
    assert_eq!(svc.field("port").unwrap().value, Value::Int(8080));
    let meta = svc.blocks().next().unwrap();
    assert_eq!(meta.kind, "metadata");
    assert_eq!(
        meta.field("region").unwrap().value,
        Value::String("us-east-1".into())
    );
}

#[test]
fn parse_error_has_useful_span() {
    let err = parse("name = ").unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("expected value"), "rendered: {rendered}");
}
