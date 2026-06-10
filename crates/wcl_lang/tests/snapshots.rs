//! Snapshot baselines for output shapes the project doesn't want to
//! drift silently: formatter output, JSON serialization of a curated
//! `Value`, and rendered miette diagnostics for representative
//! errors. Update with `cargo insta review` after intentional changes.

use std::collections::BTreeMap;
use std::path::PathBuf;

use wcl_lang::{Document, Value, VariantPayload, format, parse_for_edit};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn read_fixture(rel: &str) -> String {
    let path = examples_dir().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn format_basic_example() {
    let src = read_fixture("basic.wcl");
    let ast = parse_for_edit(&src, "basic.wcl").expect("parse");
    let printed = format::to_source(&ast);
    insta::assert_snapshot!("format_basic", printed);
}

#[test]
fn format_connections_example() {
    let src = read_fixture("connections.wcl");
    let ast = parse_for_edit(&src, "connections.wcl").expect("parse");
    let printed = format::to_source(&ast);
    insta::assert_snapshot!("format_connections", printed);
}

#[test]
fn json_curated_value_covers_all_variant_kinds() {
    let mut record_fields = BTreeMap::new();
    record_fields.insert("host".into(), Value::Utf8("a".into()));
    record_fields.insert("port".into(), Value::U16(80));
    let mut variant_record = BTreeMap::new();
    variant_record.insert("x".into(), Value::F64(1.5));
    let value = Value::List(std::sync::Arc::new(vec![
        Value::Bool(true),
        Value::I32(-7),
        Value::U64(42),
        Value::F64(1.5),
        Value::Utf8("hi".into()),
        Value::Symbol("gold".into()),
        Value::None,
        Value::Tensor {
            shape: vec![2],
            data: std::sync::Arc::new(vec![Value::F32(1.0), Value::F32(2.0)]),
        },
        Value::Variant {
            union: vec!["Shape".into()],
            variant: "Empty".into(),
            payload: VariantPayload::Unit,
        },
        Value::Variant {
            union: vec!["Shape".into()],
            variant: "Hold".into(),
            payload: VariantPayload::Positional(Box::new(Value::I64(9))),
        },
        Value::Variant {
            union: vec!["Shape".into()],
            variant: "Circle".into(),
            payload: VariantPayload::Record(std::sync::Arc::new(variant_record)),
        },
        Value::Record {
            ty: vec!["Conn".into()],
            fields: std::sync::Arc::new(record_fields),
        },
        Value::DataPath {
            kind: "Type".into(),
            segments: vec!["pkg".into(), "Color".into()],
        },
    ]));
    let json = serde_json::to_string_pretty(&value).expect("serialize");
    insta::assert_snapshot!("value_json_all_kinds", json);
}

#[test]
fn diagnostic_unknown_field_renders_consistently() {
    let rendered = render_first_schema_error("errors/unknown_field.wcl");
    insta::assert_snapshot!("diagnostic_unknown_field", rendered);
}

#[test]
fn diagnostic_missing_required_renders_consistently() {
    let rendered = render_first_schema_error("errors/missing_required.wcl");
    insta::assert_snapshot!("diagnostic_missing_required", rendered);
}

fn render_first_schema_error(rel: &str) -> String {
    let src = read_fixture(rel);
    let doc = Document::open(&src, rel).expect("parse");
    let errs = doc.schema_errors();
    assert!(!errs.is_empty(), "expected schema error in {rel}");
    let report = miette::Report::new(errs.into_iter().next().unwrap()).with_source_code(src);
    // `{:?}` uses miette's installed handler. Build a no-colour handler
    // explicitly so snapshots don't depend on terminal detection.
    let mut buf = String::new();
    miette::GraphicalReportHandler::new()
        .with_theme(miette::GraphicalTheme::unicode_nocolor())
        .render_report(&mut buf, report.as_ref())
        .expect("render");
    buf
}
