//! The field-edit API in `wcl_lang::edit`: `set_field`, and the
//! `locate_field` / `replace_field` pair it is built from.

use wcl_lang::edit::{self, EditError};
use wcl_lang::{Document, Span, Value, parse_expr};

/// Reopen an edited source and evaluate one path, to prove the edit
/// landed where it was aimed.
fn value_at(source: &str, path: &str) -> Value {
    let doc = Document::open(source, "edited.wcl").expect("edited source opens");
    doc.get(path)
        .expect("path still resolves")
        .value()
        .expect("path evaluates")
}

#[test]
fn set_field_replaces_a_top_level_field() {
    let src = "@schemaless name = \"alpha\"\n@schemaless port = 80\n";
    let out = edit::set_field(src, "site.wcl", "port", "8080").unwrap();
    assert_eq!(value_at(&out, "port"), Value::I64(8080));
    assert_eq!(value_at(&out, "name"), Value::Utf8("alpha".into()));
}

#[test]
fn set_field_replaces_a_field_inside_a_block() {
    let src = "server {\n  port = 80\n  host = \"x\"\n}\n";
    let out = edit::set_field(src, "site.wcl", "server.port", "9090u32").unwrap();
    assert!(out.contains("port = 9090u32"), "got:\n{out}");
    assert!(
        out.contains("host = \"x\""),
        "sibling untouched; got:\n{out}"
    );
}

#[test]
fn set_field_keeps_comments() {
    let src = "# the listening port\n@schemaless port = 80\n";
    let out = edit::set_field(src, "site.wcl", "port", "81").unwrap();
    assert!(out.contains("# the listening port"), "got:\n{out}");
    assert!(out.contains("port = 81"), "got:\n{out}");
}

#[test]
fn set_field_accepts_any_expression() {
    let src = "@schemaless tags = []\n";
    let out = edit::set_field(src, "site.wcl", "tags", "[:a, :b]").unwrap();
    assert_eq!(
        value_at(&out, "tags"),
        Value::list(vec![Value::Symbol("a".into()), Value::Symbol("b".into())])
    );
}

#[test]
fn missing_path_suggests_a_near_name() {
    let src = "@schemaless port = 80\n";
    let err = edit::set_field(src, "site.wcl", "prot", "1").unwrap_err();
    match err {
        EditError::NoSuchPath { path, suggestion } => {
            assert_eq!(path, "prot");
            assert_eq!(suggestion.as_deref(), Some("port"));
        }
        other => panic!("expected NoSuchPath, got {other:?}"),
    }
}

#[test]
fn missing_path_with_nothing_close_has_no_suggestion() {
    let src = "@schemaless port = 80\n";
    let err = edit::set_field(src, "site.wcl", "completely_different", "1").unwrap_err();
    assert!(matches!(
        err,
        EditError::NoSuchPath {
            suggestion: None,
            ..
        }
    ));
}

#[test]
fn a_block_path_is_not_a_field() {
    let src = "server {\n  port = 80\n}\n";
    let err = edit::set_field(src, "site.wcl", "server", "1").unwrap_err();
    assert!(matches!(err, EditError::NotAField { kind: "block", .. }));
    assert_eq!(err.to_string(), "`server` resolved to a block, not a field");
}

#[test]
fn an_unparseable_value_is_invalid_value() {
    let src = "@schemaless port = 80\n";
    let err = edit::set_field(src, "site.wcl", "port", "1 +").unwrap_err();
    assert!(matches!(err, EditError::InvalidValue(_)), "got {err:?}");
}

#[test]
fn an_unparseable_source_is_invalid_source() {
    let err = edit::set_field("port = =\n", "site.wcl", "port", "1").unwrap_err();
    assert!(matches!(err, EditError::InvalidSource(_)), "got {err:?}");
}

#[test]
fn set_field_refuses_a_field_from_an_import() {
    let dir = tempfile::tempdir().unwrap();
    let shared_path = dir.path().join("shared.wcl");
    std::fs::write(
        &shared_path,
        "namespace shared\n@schemaless brand = \"wcl\"\n",
    )
    .unwrap();
    // An absolute import resolves without a base directory, so the
    // in-memory source sees the field, and `set_field` declines to edit a
    // file it was not given.
    let main = format!("import \"{}\"\n", shared_path.display());
    match edit::set_field(&main, "main.wcl", "shared.brand", "\"x\"").unwrap_err() {
        EditError::Imported { path, file } => {
            assert_eq!(path, "shared.brand");
            assert_eq!(file.file_name().unwrap(), "shared.wcl");
        }
        other => panic!("expected Imported, got {other:?}"),
    }
}

#[test]
fn locate_then_replace_edits_the_imported_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("shared.wcl"),
        "namespace shared\n@schemaless brand = \"wcl\"\n",
    )
    .unwrap();
    let main_path = dir.path().join("main.wcl");
    std::fs::write(&main_path, "import \"./shared.wcl\"\n").unwrap();

    // Opened from disk, the import resolves and `locate_field` names the
    // file that declares the field.
    let doc = Document::from_file(&main_path).unwrap();
    let target = edit::locate_field(&doc, "shared.brand").unwrap();
    let home = target.source_path.clone().expect("declared in the import");
    assert_eq!(home.file_name().unwrap(), "shared.wcl");
    assert_eq!(target.file(&main_path), home.as_path());

    let shared_src = std::fs::read_to_string(&home).unwrap();
    let value = parse_expr("\"renamed\"", "<value>").unwrap();
    let out = edit::replace_field(&shared_src, "shared.wcl", target.span, value).unwrap();
    assert!(out.contains("brand = \"renamed\""), "got:\n{out}");
}

#[test]
fn locate_field_in_the_entry_source_has_no_source_path() {
    let doc = Document::open("@schemaless port = 80\n", "site.wcl").unwrap();
    let target = edit::locate_field(&doc, "port").unwrap();
    assert_eq!(target.source_path, None);
    let entry = std::path::Path::new("site.wcl");
    assert_eq!(target.file(entry), entry);
}

#[test]
fn replace_field_at_a_span_with_no_field_is_field_not_found() {
    let src = "@schemaless port = 80\n";
    let bogus = Span::new(1000, 1001);
    let err =
        edit::replace_field(src, "site.wcl", bogus, parse_expr("1", "<v>").unwrap()).unwrap_err();
    match err {
        EditError::FieldNotFound { span, name } => {
            assert_eq!(span, bogus);
            assert_eq!(name, "site.wcl");
        }
        other => panic!("expected FieldNotFound, got {other:?}"),
    }
}

#[test]
fn suggest_path_matches_block_kinds_and_ignores_exact_names() {
    let doc = Document::open("server {\n  port = 80\n}\n", "s.wcl").unwrap();
    assert_eq!(
        edit::suggest_path(&doc, "sever.port").as_deref(),
        Some("server")
    );
    assert_eq!(edit::suggest_path(&doc, "server.port"), None);
}
