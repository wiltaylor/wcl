//! Every schema violation knows which file it was raised in.
//!
//! A document is a root file plus its imports, and a span alone does not
//! say which text it indexes into. These tests put the offending text in
//! an imported file and check that each violation — whatever its kind,
//! on the strict path and on a lazy read — names that file, and that its
//! span lands on the offending text *in that file*.

use std::path::{Path, PathBuf};

use wcl_lang::{Document, EvalError, SchemaViolationKind};

/// Write `files` into a fresh directory and open `main.wcl` from it.
fn open_tree(files: &[(&str, &str)]) -> (tempfile::TempDir, Document) {
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, text) in files {
        std::fs::write(dir.path().join(name), text).expect("write fixture");
    }
    let doc = Document::from_file(&dir.path().join("main.wcl")).expect("document opens");
    (dir, doc)
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().expect("path exists")
}

/// The file a violation names and the text its span covers there.
fn located(error: &EvalError) -> (PathBuf, String) {
    let source = error
        .origin()
        .unwrap_or_else(|| panic!("`{error}` names no source"));
    let EvalError::SchemaViolation { span, .. } = error else {
        panic!("not a schema violation: {error}")
    };
    let text = &source.inner()[span.offset()..span.offset() + span.len()];
    (canonical(Path::new(source.name())), text.to_string())
}

const TYPES: &str = r#"
@block("server", required_fields = ["host"])
type Server {
  port: u16
  host: utf8
  @max(10) workers: i64
  @children("disk") disks: list<Disk>
}

@block("disk")
type Disk {
  size: i64
}

union Mode {
  Fast i32
  Slow utf8
}

union Other {
  Thing none
}

@document
type Root {
  mode: Mode
  @children("server") servers: list<Server>
}
"#;

const DATA: &str = r#"// data file, offsets differ from main.wcl
mode = Other::Thing

server web {
  port = "eighty"
  colour = "red"
  workers = 99
  disk { size = "big" }
  bogus { }
}
"#;

#[test]
fn every_strict_violation_in_an_imported_file_names_that_file() {
    let (dir, doc) = open_tree(&[
        ("types.wcl", TYPES),
        ("data.wcl", DATA),
        (
            "main.wcl",
            "import \"./types.wcl\"\nimport \"./data.wcl\"\n",
        ),
    ]);
    let data = canonical(&dir.path().join("data.wcl"));
    let diagnostics = doc.schema_diagnostics();

    let expect = [
        (SchemaViolationKind::FieldTypeMismatch, "port = \"eighty\""),
        (SchemaViolationKind::UnknownField, "colour = \"red\""),
        (SchemaViolationKind::ConstraintViolation, "workers = 99"),
        (SchemaViolationKind::FieldTypeMismatch, "size = \"big\""),
        (SchemaViolationKind::DisallowedChild, "bogus { }"),
        (
            SchemaViolationKind::VariantUnionMismatch,
            "mode = Other::Thing",
        ),
    ];
    for (kind, text) in expect {
        let found = diagnostics.iter().find(|(error, _)| {
            matches!(error, EvalError::SchemaViolation { kind: k, .. } if *k == kind)
                && located(error).1 == text
        });
        let (error, paired) = found
            .unwrap_or_else(|| panic!("no {kind:?} at `{text}` in data.wcl: {diagnostics:#?}"));
        assert_eq!(located(error).0, data, "{error}");
        let paired = paired.as_ref().expect("paired with a source");
        assert_eq!(canonical(Path::new(paired.name())), data, "{error}");
    }

    let missing = diagnostics
        .iter()
        .map(|(error, _)| error)
        .find(|error| error.to_string().contains("missing required field 'host'"))
        .expect("missing required field");
    let (file, text) = located(missing);
    assert_eq!(file, data);
    assert!(text.starts_with("server web {"), "{text}");

    // Nothing in this tree is left without a file.
    for (error, source) in &diagnostics {
        assert!(source.is_some(), "no source for `{error}`");
    }
}

#[test]
fn a_violation_in_the_root_names_the_root() {
    let (dir, doc) = open_tree(&[
        ("types.wcl", TYPES),
        (
            "main.wcl",
            "import \"./types.wcl\"\nmode = Mode::Fast(1)\nserver web {\n  port = \"x\"\n  host = \"h\"\n}\n",
        ),
    ]);
    let errors = doc.schema_errors();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    let (file, text) = located(&errors[0]);
    assert_eq!(file, canonical(&dir.path().join("main.wcl")));
    assert_eq!(text, "port = \"x\"");
}

#[test]
fn a_field_spliced_in_by_an_in_block_import_names_its_own_file() {
    let (dir, doc) = open_tree(&[
        ("types.wcl", TYPES),
        ("extra.wcl", "\n\ncolour = \"red\"\n"),
        (
            "main.wcl",
            "import \"./types.wcl\"\nmode = Mode::Fast(1)\nserver web {\n  host = \"h\"\n  port = 80\n  import \"./extra.wcl\"\n}\n",
        ),
    ]);
    let errors = doc.schema_errors();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    let (file, text) = located(&errors[0]);
    assert_eq!(file, canonical(&dir.path().join("extra.wcl")));
    assert_eq!(text, "colour = \"red\"");
}

#[test]
fn a_lazy_read_names_the_file_of_the_field_it_read() {
    let (dir, doc) = open_tree(&[
        ("types.wcl", TYPES),
        ("data.wcl", DATA),
        (
            "main.wcl",
            "import \"./types.wcl\"\nimport \"./data.wcl\"\n",
        ),
    ]);
    // No strict pass first: the read alone must know the file.
    let server = doc.block("server").expect("server block");
    let error = server
        .field("colour")
        .expect("colour field")
        .value()
        .expect_err("undeclared field fails to read");
    let (file, text) = located(error);
    assert_eq!(file, canonical(&dir.path().join("data.wcl")));
    assert_eq!(text, "colour = \"red\"");
}

#[test]
fn a_report_of_the_error_renders_against_its_own_file() {
    let (_dir, doc) = open_tree(&[
        ("types.wcl", TYPES),
        ("data.wcl", DATA),
        (
            "main.wcl",
            "import \"./types.wcl\"\nimport \"./data.wcl\"\n",
        ),
    ]);
    let error = doc
        .schema_errors()
        .into_iter()
        .find(|error| error.to_string().contains("'colour'"))
        .expect("unknown field");
    // No source is attached by the caller: the error supplies its own.
    let report = miette::Report::new(error);
    let mut rendered = String::new();
    miette::GraphicalReportHandler::new()
        .with_theme(miette::GraphicalTheme::unicode_nocolor())
        .render_report(&mut rendered, report.as_ref())
        .expect("render");
    assert!(rendered.contains("data.wcl:6:3]"), "{rendered}");
    assert!(rendered.contains("colour = \"red\""), "{rendered}");
}
