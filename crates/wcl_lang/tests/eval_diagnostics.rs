//! `Document::eval_diagnostics` forces every value and reports each
//! failure once.
//!
//! Fields evaluate lazily, so a failing field nobody reads stays silent.
//! These tests check that the forced pass finds failures wherever data
//! lives — top-level fields, block fields, nested blocks, imported files —
//! skips template bodies a host expands later, and leaves out what the
//! schema pass already reports.

use std::path::{Path, PathBuf};

use miette::Diagnostic;
use wcl_lang::{Document, EvalError};

fn open(src: &str) -> Document {
    Document::open(src, "test.wcl").expect("document opens")
}

/// Each failure's message, in the order reported.
fn messages(doc: &Document) -> Vec<String> {
    doc.eval_errors().iter().map(ToString::to_string).collect()
}

/// The text a failure's primary span covers in the source it is paired
/// with, and that source's name.
fn located(error: &EvalError, source: &miette::NamedSource<std::sync::Arc<str>>) -> String {
    let label = error
        .labels()
        .and_then(|mut labels| labels.next())
        .unwrap_or_else(|| panic!("`{error}` has no span"));
    source.inner()[label.offset()..label.offset() + label.len()].to_string()
}

const INSN: &str = r#"
type F { name: utf8  hi: u8  lo: u8 }
@block("insn") type I {
  @inline(0) id: identifier
  width: u16
  enc: list<F>
  @schemaless ok: bool?
}
@document type D { @children("insn") insns: list<I> }
insn add {
  width = 32u16
  enc = [ { name: "a", hi: 31u8, lo: 8u8 }, { name: "b", hi: 6u8, lo: 0u8 } ]
  ok = assert(fold(map(self.enc, fn(f: F) -> i64 f.hi - f.lo + 1), 0,
    fn(a: i64, b: i64) -> i64 a + b) == self.width, "fields do not cover width") == none
}
"#;

#[test]
fn a_failing_assert_in_a_block_field_is_reported() {
    let doc = open(INSN);
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    let diagnostics = doc.eval_diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let (error, source) = &diagnostics[0];
    assert_eq!(error.to_string(), "error: fields do not cover width");
    let source = source.as_ref().expect("the failure names its source");
    assert!(located(error, source).starts_with("assert("));
}

#[test]
fn a_document_with_nothing_failing_reports_nothing() {
    let doc = open(&INSN.replace("lo: 8u8", "lo: 7u8"));
    assert_eq!(messages(&doc), Vec::<String>::new());
}

#[test]
fn a_failing_top_level_field_is_reported() {
    let doc = open(
        r#"
        @document type D { a: i64  b: i64 }
        a = 1
        b = 1 / 0
        "#,
    );
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    assert_eq!(messages(&doc), ["operator '/' cannot divide by zero"]);
}

#[test]
fn failures_in_nested_blocks_are_reported_in_source_order() {
    let doc = open(
        r#"
        @block("outer") type Outer {
          x: i64
          @children("inner") inners: list<Inner>
        }
        @block("inner") type Inner {
          y: i64
          @children("inner") inners: list<Inner>
        }
        @document type D { @children("outer") outers: list<Outer> }
        outer {
          x = assert(false, "outer x")
          inner {
            y = assert(false, "inner y")
            inner { y = assert(false, "innermost y") }
          }
        }
        "#,
    );
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    assert_eq!(
        messages(&doc),
        ["error: outer x", "error: inner y", "error: innermost y"]
    );
}

#[test]
fn a_failure_read_through_another_field_is_reported_once() {
    let doc = open(
        r#"
        @document type D { a: i64  b: i64 }
        a = 1 / 0
        b = a
        "#,
    );
    assert_eq!(messages(&doc), ["operator '/' cannot divide by zero"]);
}

#[test]
fn a_contextual_template_body_is_not_forced() {
    // No expander is registered: the body of `loop` is never expanded,
    // and evaluated in place `it` resolves to nothing. The loop's own
    // field is not a template, so its failure is reported.
    let doc = open(
        r#"
        @block("deck") type Deck {
          @inline(0) name: identifier
          @children("card") cards: list<Card>
        }
        @block("card") type Card { title: utf8 }
        @block("loop") @contextual type Loop {
          @schemaless each: list<utf8>
          @children("card") cards: list<Card>
        }
        @document type D { @children("deck") decks: list<Deck> }
        deck main {
          card { title = "literal" }
          loop { each = [assert(false, "each fails")]
            card { title = it }
          }
        }
        "#,
    );
    let deck = doc.blocks().next().expect("deck");
    let lp = deck.blocks().find(|b| b.kind() == "loop").expect("loop");
    let body = lp.blocks().next().expect("loop body");
    assert!(lp.is_contextual());
    assert!(lp.is_template_child(&body));
    assert!(
        body.field("title").expect("title").value().is_err(),
        "the template body fails when read in place"
    );
    assert_eq!(messages(&doc), ["error: each fails"]);
}

#[test]
fn a_declares_kind_body_is_not_forced() {
    let doc = open(
        r#"
        @document("d") type D {
          @children("widget") widgets: list<Widget>
        }
        @block("widget") @declares_kind(name = 0, params = "params", body = "body")
        type Widget {
          @inline(0) name: identifier
          @children("param") params: list<Param>
          @child("body") body: WidgetBody
        }
        @block("param") type Param { @inline(0) name: identifier }
        @block("body") @schemaless type WidgetBody {}
        widget card {
          param label
          body { text = label }
        }
        "#,
    );
    let widget = doc.blocks().next().expect("widget");
    let body = widget.blocks().find(|b| b.kind() == "body").expect("body");
    let param = widget
        .blocks()
        .find(|b| b.kind() == "param")
        .expect("param");
    assert!(widget.is_template_child(&body));
    assert!(!widget.is_template_child(&param));
    assert_eq!(messages(&doc), Vec::<String>::new());
}

#[test]
fn a_schema_violation_is_not_reported_twice() {
    let doc = open(
        r#"
        @block("server") type Server { port: u16 }
        @document type D { @children("server") servers: list<Server> }
        server { port = 80u16  colour = "red" }
        "#,
    );
    let server = doc.blocks().next().expect("server");
    let read = server
        .field("colour")
        .expect("colour")
        .value()
        .expect_err("a lazy read reports the violation")
        .clone();
    let schema = doc.schema_errors();
    assert!(
        schema.iter().any(|e| e.to_string() == read.to_string()),
        "the schema pass reports the same violation: {schema:?}"
    );
    assert_eq!(
        messages(&doc),
        Vec::<String>::new(),
        "the forced pass leaves the violation to the schema pass"
    );
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().expect("path exists")
}

#[test]
fn a_failure_in_an_imported_file_carries_that_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("main.wcl"),
        "@document type D { a: i64  b: i64 }\nimport \"./data.wcl\"\na = 1\n",
    )
    .expect("write main");
    std::fs::write(
        dir.path().join("data.wcl"),
        "// data, so the offsets differ from main.wcl\nb = assert(false, \"in data\")\n",
    )
    .expect("write data");
    let doc = Document::from_file(&dir.path().join("main.wcl")).expect("opens");
    let diagnostics = doc.eval_diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let (error, source) = &diagnostics[0];
    let source = source.as_ref().expect("the failure names its source");
    assert_eq!(
        canonical(Path::new(source.name())),
        canonical(&dir.path().join("data.wcl"))
    );
    assert_eq!(located(error, source), "assert(false, \"in data\")");
}
