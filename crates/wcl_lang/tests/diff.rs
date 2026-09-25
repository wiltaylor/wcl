//! The semantic diff in `wcl_lang::diff`: `diff_values` on bare values,
//! and `diff_documents` on whole evaluated documents.

use std::sync::Arc;

use wcl_lang::Document;
use wcl_lang::Value;
use wcl_lang::diff::{
    ChangeOp, DOCUMENT_ENTITY, Diff, FieldKind, Side, Skipped, diff_documents, diff_values,
};

fn record(pairs: &[(&str, Value)]) -> Value {
    Value::Record {
        ty: Vec::new(),
        fields: Arc::new(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        ),
    }
}

fn open(src: &str) -> Document {
    Document::open(src, "doc.wcl").expect("fixture opens")
}

fn diff(old: &str, new: &str) -> Diff {
    diff_documents(&open(old), &open(new))
}

// ---------------------------------------------------------------------------
// diff_values
// ---------------------------------------------------------------------------

#[test]
fn equal_values_produce_no_change() {
    let a = record(&[("x", Value::I64(1))]);
    assert!(diff_values(&a, &a).is_empty());
}

#[test]
fn changed_leaf_carries_old_and_new() {
    let a = record(&[("x", Value::I64(1))]);
    let b = record(&[("x", Value::I64(2))]);
    // `FieldChange` is non_exhaustive, so outside the crate it is read
    // field by field rather than built as a literal to compare against.
    let d = diff_values(&a, &b);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, "x");
    assert_eq!(d[0].kind, FieldKind::Changed);
    assert_eq!(d[0].old, Some(Value::I64(1)));
    assert_eq!(d[0].new, Some(Value::I64(2)));
}

#[test]
fn unequal_scalars_at_the_root_have_an_empty_path() {
    let d = diff_values(&Value::I64(1), &Value::Utf8("1".into()));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, "");
    assert_eq!(d[0].kind, FieldKind::Changed);
}

#[test]
fn nested_added_and_removed_fields() {
    let a = record(&[("fields", record(&[("name", Value::Utf8("t".into()))]))]);
    let b = record(&[(
        "fields",
        record(&[
            ("name", Value::Utf8("t".into())),
            ("due_date", Value::Utf8("2026".into())),
        ]),
    )]);
    let d = diff_values(&a, &b);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, "fields.due_date");
    assert_eq!(d[0].kind, FieldKind::Added);
    assert_eq!(d[0].old, None);
    assert_eq!(d[0].new, Some(Value::Utf8("2026".into())));

    let r = diff_values(&b, &a);
    assert_eq!(r[0].kind, FieldKind::Removed);
    assert_eq!(r[0].old, Some(Value::Utf8("2026".into())));
    assert_eq!(r[0].new, None);
}

#[test]
fn none_to_value_reads_as_added() {
    let a = record(&[("due_date", Value::None)]);
    let b = record(&[("due_date", Value::Utf8("2026".into()))]);
    let d = diff_values(&a, &b);
    assert_eq!(d[0].path, "due_date");
    assert_eq!(d[0].kind, FieldKind::Added);
    assert_eq!(diff_values(&b, &a)[0].kind, FieldKind::Removed);
}

#[test]
fn lists_recurse_by_index() {
    // Element changed at index 1, added at index 2.
    let a = record(&[("xs", Value::list(vec![Value::I64(1), Value::I64(2)]))]);
    let b = record(&[(
        "xs",
        Value::list(vec![Value::I64(1), Value::I64(9), Value::I64(3)]),
    )]);
    let d = diff_values(&a, &b);
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].path, "xs[1]");
    assert_eq!(d[0].kind, FieldKind::Changed);
    assert_eq!(d[1].path, "xs[2]");
    assert_eq!(d[1].kind, FieldKind::Added);
    assert_eq!(d[1].new, Some(Value::I64(3)));
}

#[test]
fn list_shrink_reports_removed_tail() {
    let a = record(&[("xs", Value::list(vec![Value::I64(1), Value::I64(2)]))]);
    let b = record(&[("xs", Value::list(vec![Value::I64(1)]))]);
    let d = diff_values(&a, &b);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, "xs[1]");
    assert_eq!(d[0].kind, FieldKind::Removed);
    assert_eq!(d[0].old, Some(Value::I64(2)));
}

#[test]
fn record_type_names_are_ignored() {
    let mut typed = record(&[("x", Value::I64(1))]);
    if let Value::Record { ty, .. } = &mut typed {
        *ty = vec!["Entity".into()];
    }
    let bare = record(&[("x", Value::I64(2))]);
    let d = diff_values(&typed, &bare);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, "x");
}

// ---------------------------------------------------------------------------
// diff_documents
// ---------------------------------------------------------------------------

const OLD: &str = "@schemaless name = \"alpha\"\n\
@schemaless port = 8080\n\
server web {\n  host = \"x\"\n  port = 80\n}\n\
server db {\n  host = \"d\"\n}\n";

#[test]
fn identical_documents_have_no_changes() {
    let d = diff(OLD, OLD);
    assert!(d.is_empty());
    assert!(d.changes.is_empty());
    assert!(d.warnings.is_empty());
}

#[test]
fn a_reformat_alone_is_no_change() {
    let reformatted = "@schemaless   name   =  \"alpha\"\n@schemaless\nport = 8080\n\
        server web { host = \"x\"\n port = 80 }\n\
        server db { host = \"d\" }\n";
    assert!(diff(OLD, reformatted).is_empty());
}

#[test]
fn document_fields_diff_under_the_document_entity() {
    let new = OLD
        .replace("name = \"alpha\"", "name = \"beta\"")
        .replace("port = 8080\n", "debug = true\n");
    let d = diff(OLD, &new);
    assert_eq!(d.changes.len(), 1);
    let c = &d.changes[0];
    assert_eq!(c.op, ChangeOp::Modified);
    assert_eq!(c.entity, DOCUMENT_ENTITY);
    assert_eq!(c.entity_value, None);
    let summary: Vec<(&str, FieldKind)> = c.fields.iter().map(|f| (&*f.path, f.kind)).collect();
    assert_eq!(
        summary,
        vec![
            ("debug", FieldKind::Added),
            ("name", FieldKind::Changed),
            ("port", FieldKind::Removed),
        ]
    );
}

#[test]
fn blocks_are_entities_keyed_by_kind_and_label() {
    let new = OLD.replace("port = 80\n", "port = 81\n").replace(
        "server db {\n  host = \"d\"\n}\n",
        "server cache {\n  host = \"c\"\n}\n",
    );
    let d = diff(OLD, &new);
    let summary: Vec<(ChangeOp, &str)> = d.changes.iter().map(|c| (c.op, &*c.entity)).collect();
    // Sorted by entity key.
    assert_eq!(
        summary,
        vec![
            (ChangeOp::Added, "server:cache"),
            (ChangeOp::Removed, "server:db"),
            (ChangeOp::Modified, "server:web"),
        ]
    );

    let added = &d.changes[0];
    assert!(added.fields.is_empty());
    let Some(Value::Record { fields, .. }) = &added.entity_value else {
        panic!("an added entity carries its record");
    };
    assert_eq!(fields.get("host"), Some(&Value::Utf8("c".into())));

    let modified = &d.changes[2];
    assert_eq!(modified.fields.len(), 1);
    assert_eq!(modified.fields[0].path, "port");
    assert_eq!(modified.fields[0].old, Some(Value::I64(80)));
    assert_eq!(modified.fields[0].new, Some(Value::I64(81)));
}

#[test]
fn colliding_entity_keys_get_a_numbered_suffix() {
    let old = "thing {\n  v = 1\n}\nthing {\n  v = 2\n}\n";
    let new = "thing {\n  v = 1\n}\nthing {\n  v = 3\n}\n";
    let d = diff(old, new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].entity, "thing#2");
    assert_eq!(d.changes[0].fields[0].path, "v");
}

#[test]
fn nested_values_report_full_paths() {
    let old = "server web {\n  tags = [\"a\", \"b\"]\n  opts = { tls: true }\n}\n";
    let new = "server web {\n  tags = [\"a\"]\n  opts = { tls: false }\n}\n";
    let d = diff(old, new);
    let paths: Vec<&str> = d.changes[0].fields.iter().map(|f| &*f.path).collect();
    assert_eq!(paths, vec!["opts.tls", "tags[1]"]);
}

#[test]
fn unevaluable_items_are_skipped_with_warnings() {
    let old = "@schemaless name = \"a\"\n@schemaless broken = 1 / 0\n\
        server bad {\n  x = nowhere + 1\n}\n";
    let new = "@schemaless name = \"b\"\n@schemaless broken = missing\n";
    let d = diff(old, new);

    // What evaluated is still compared; what didn't is not reported as a
    // removal or change.
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].entity, DOCUMENT_ENTITY);
    assert_eq!(d.changes[0].fields.len(), 1);
    assert_eq!(d.changes[0].fields[0].path, "name");

    // Old side first (blocks, then fields), then the new side.
    let skipped: Vec<(Side, &Skipped)> = d.warnings.iter().map(|w| (w.side, &w.skipped)).collect();
    assert_eq!(
        skipped,
        vec![
            (Side::Old, &Skipped::Entity("server:bad".into())),
            (Side::Old, &Skipped::Field("broken".into())),
            (Side::New, &Skipped::Field("broken".into())),
        ]
    );
    let first = d.warnings[0].to_string();
    assert!(
        first.starts_with("entity 'server:bad' could not be evaluated, skipping: "),
        "got {first}"
    );
    assert!(
        d.warnings[1]
            .to_string()
            .starts_with("field 'broken' could not be evaluated, skipping: ")
    );
}

#[test]
fn warnings_alone_do_not_make_a_diff_non_empty() {
    let src = "@schemaless broken = 1 / 0\n";
    let d = diff(src, src);
    assert!(d.is_empty());
    assert_eq!(d.warnings.len(), 2);
}

#[test]
fn an_empty_document_against_a_full_one_adds_everything() {
    let d = diff("", OLD);
    assert!(d.changes.iter().all(|c| c.op == ChangeOp::Added));
    let entities: Vec<&str> = d.changes.iter().map(|c| &*c.entity).collect();
    assert_eq!(entities, vec![DOCUMENT_ENTITY, "server:db", "server:web"]);
}

#[test]
fn op_and_kind_names() {
    assert_eq!(ChangeOp::Added.as_str(), "added");
    assert_eq!(ChangeOp::Removed.as_str(), "removed");
    assert_eq!(ChangeOp::Modified.as_str(), "modified");
    assert_eq!(FieldKind::Added.as_str(), "added");
    assert_eq!(FieldKind::Removed.as_str(), "removed");
    assert_eq!(FieldKind::Changed.as_str(), "changed");
}
