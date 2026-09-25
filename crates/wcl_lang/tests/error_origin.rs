//! Every evaluation error knows which file it was raised in.
//!
//! A document is a root file plus its imports, and a span alone does not
//! say which text it indexes into. `schema_provenance.rs` covers schema
//! violations; these tests cover the rest: an error raised inside a
//! function body names the file the body was written in, a failed read
//! names the file of the field or `let` it failed in, and the innermost of
//! those wins. Each also renders its snippet with no source supplied by
//! the host.

use std::path::Path;

use miette::Diagnostic;
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

/// The file an error names (its bare file name) and the text its primary
/// label covers in that file.
fn located(error: &EvalError) -> (String, String) {
    let source = error
        .origin()
        .unwrap_or_else(|| panic!("`{error}` names no source"));
    let span = error
        .labels()
        .and_then(|mut labels| labels.next())
        .unwrap_or_else(|| panic!("`{error}` has no label"));
    let text = &source.inner()[span.offset()..span.offset() + span.len()];
    let name = Path::new(source.name())
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.name().to_string());
    (name, text.to_string())
}

/// Render `error` with miette's plain handler and no host-supplied source.
fn rendered(error: &EvalError) -> String {
    let mut out = String::new();
    miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor())
        .render_report(&mut out, error)
        .expect("render");
    out
}

/// `main.wcl` calls a helper imported from `lib.wcl`, whose first lines are
/// padding so a span into it would land on different text in `main.wcl`.
const LIB: &str = "// padding line one\n\
                   // padding line two\n\
                   // padding line three\n\
                   let check = fn(x: i64) -> i64 { if x < 0 { error(\"negative!\") } else { x } }\n\
                   let mismatch = fn(x: i64) -> i64 { x + \"text\" }\n\
                   let divide = fn(x: i64) -> i64 { x / 0 }\n\
                   let broken = 7 / 0\n";

const MAIN: &str = "import \"./lib.wcl\"\n\
                    @document type Main { a: i64  b: i64  c: i64  d: i64  e: i64  f: i64 }\n\
                    a = check(-1)\n\
                    b = mismatch(1)\n\
                    c = divide(1)\n\
                    d = broken\n\
                    e = 1 / 0\n\
                    f = check(e)\n";

#[test]
fn an_error_inside_an_imported_function_names_the_function_file() {
    let (_dir, doc) = open_tree(&[("main.wcl", MAIN), ("lib.wcl", LIB)]);
    for (field, text) in [
        ("a", "error(\"negative!\")"),
        ("b", "x + \"text\""),
        ("c", "x / 0"),
    ] {
        let error = doc.get(field).unwrap().value().unwrap_err();
        assert_eq!(
            located(&error),
            ("lib.wcl".to_string(), text.to_string()),
            "{field}: {error:?}"
        );
        let report = rendered(&error);
        assert!(report.contains("lib.wcl:"), "{field}:\n{report}");
        assert!(report.contains(text), "{field}:\n{report}");
    }
}

#[test]
fn a_failed_let_names_the_file_the_let_was_written_in() {
    let (_dir, doc) = open_tree(&[("main.wcl", MAIN), ("lib.wcl", LIB)]);
    let error = doc.get("d").unwrap().value().unwrap_err();
    assert_eq!(
        located(&error),
        ("lib.wcl".to_string(), "7 / 0".to_string())
    );
}

#[test]
fn an_error_read_through_a_function_keeps_the_field_it_came_from() {
    // `f` passes the failing root field `e` to the imported helper. The
    // failure is `e`'s, raised in `main.wcl`, and the helper's file must
    // not claim it on the way out.
    let (_dir, doc) = open_tree(&[("main.wcl", MAIN), ("lib.wcl", LIB)]);
    let error = doc.get("f").unwrap().value().unwrap_err();
    assert_eq!(
        located(&error),
        ("main.wcl".to_string(), "1 / 0".to_string())
    );
}

#[test]
fn a_root_error_names_the_root() {
    let doc = Document::open("@document type D { a: i64 }\na = 1 / 0\n", "root.wcl").unwrap();
    let error = doc.get("a").unwrap().value().unwrap_err();
    assert_eq!(
        located(&error),
        ("root.wcl".to_string(), "1 / 0".to_string())
    );
}

#[test]
fn code_run_through_eval_is_its_own_source() {
    let doc = Document::open(
        "@document type D { a: i64 }\na = eval(\"2 / 0\")\n",
        "root.wcl",
    )
    .unwrap();
    let error = doc.get("a").unwrap().value().unwrap_err();
    assert_eq!(located(&error), ("<eval>".to_string(), "2 / 0".to_string()));
}

#[test]
fn an_exponent_without_a_decimal_point_says_how_to_write_it() {
    let doc = Document::open(
        "@document type D { a: f64  b: f64  c: u32  d: f64  e: u32 }\n\
         a = 1e39\n\
         b = 2e-3\n\
         c = 3E+5\n\
         d = 2e - 3\n\
         e = 5km\n",
        "sci.wcl",
    )
    .unwrap();
    let help = |field: &str| {
        doc.get(field)
            .unwrap()
            .value()
            .unwrap_err()
            .help()
            .map(|h| h.to_string())
    };
    assert_eq!(
        help("a").as_deref(),
        Some("scientific notation needs a decimal point: write 1.0e39")
    );
    assert_eq!(
        help("b").as_deref(),
        Some("scientific notation needs a decimal point: write 2.0e-3")
    );
    assert_eq!(
        help("c").as_deref(),
        Some("scientific notation needs a decimal point: write 3.0E+5")
    );
    // Spaced like the subtraction it is: no guess about an exponent.
    assert_eq!(help("d"), None);
    // A real unit keeps the advice about declaring it.
    assert_eq!(
        help("e").as_deref(),
        Some(
            "declare it with `@unit(\"km\", <factor>)` on the type alias, or use one of its \
             declared units"
        )
    );
    assert!(matches!(
        doc.get("b").unwrap().value().unwrap_err(),
        EvalError::TypeMismatch { .. }
    ));
}

/// A connection statement spliced into a block by an in-block `import`
/// is checked against the fragment's text, on both paths.
const GRAPH: &str = "symbol_set EdgeKind { uses  depends_on }\n\
                     connection DependsOn: Service -> Service : EdgeKind\n\
                     @block(\"service\") type Service { @inline(0) id: identifier }\n\
                     @block(\"graph\") type Graph {\n  \
                       @inline(0) id: identifier\n  \
                       @children(\"service\") services: list<Service>\n  \
                       @connections(DependsOn) edges: list<DependsOn>\n\
                     }\n\
                     @document type Config { @children(\"graph\") graphs: list<Graph> }\n\
                     graph g {\n  \
                       service web {}\n  \
                       service db {}\n  \
                       import \"./frag.wcl\"\n\
                     }\n";

const FRAGMENT: &str = "// fragment\n\
                        service cache {}\n\
                        web -> db :bogus\n\
                        web -> nowhere :uses\n";

#[test]
fn in_block_imported_connection_statements_name_the_fragment() {
    let (_dir, doc) = open_tree(&[("main.wcl", GRAPH), ("frag.wcl", FRAGMENT)]);
    let diagnostics = doc.schema_diagnostics();
    let connection: Vec<_> = diagnostics
        .iter()
        .filter(|(error, _)| {
            matches!(
                error,
                EvalError::SchemaViolation {
                    kind: SchemaViolationKind::UnknownConnectionKind
                        | SchemaViolationKind::UnknownConnectionOperand,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(connection.len(), 2, "{diagnostics:#?}");
    for (error, paired) in connection {
        let paired = paired.as_ref().expect("paired with a source");
        assert!(paired.name().ends_with("frag.wcl"), "{}", paired.name());
        assert_eq!(
            error.origin().map(|s| s.name().to_string()),
            Some(paired.name().to_string())
        );
    }
    let texts: Vec<String> = diagnostics.iter().map(|(e, _)| located(e).1).collect();
    assert!(texts.contains(&":bogus".to_string()), "{texts:?}");
    assert!(texts.contains(&"nowhere".to_string()), "{texts:?}");
}

#[test]
fn reading_bad_connection_statements_fails_with_the_check_violation() {
    let (_dir, doc) = open_tree(&[("main.wcl", GRAPH), ("frag.wcl", FRAGMENT)]);
    let graph = doc.blocks().next().expect("graph g");
    let edges = graph.typed_field("edges").expect("edges projects");
    let error = edges.value().expect_err("the projection fails");
    assert_eq!(
        error.to_string(),
        "connection kind ':bogus' is not a member of 'EdgeKind'"
    );
    assert_eq!(
        located(&error),
        ("frag.wcl".to_string(), ":bogus".to_string())
    );
    assert!(
        doc.schema_errors().contains(&error),
        "the read reports the check's violation: {:#?}",
        doc.schema_errors()
    );
}
