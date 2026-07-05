//! String → identifier coercion on identifier-declared slots.
//!
//! A quoted ref (`system = "alpha"`) must evaluate to the same
//! `Value::Identifier` a bare ref (`system = alpha`) does, so template
//! joins (`c.system == s.id`) hold regardless of authoring style. The
//! motivating failure: a weak model authored an entire WAD with quoted
//! refs — `wcl check` passed, every derived C4 view silently rendered
//! empty.

use wcl_lang::{Document, Value};

const SRC: &str = r#"
    @block("sys") type Sys { @inline(0) id: identifier  name: utf8 }
    @block("cont") type Cont {
      @inline(0) id: identifier
      system: identifier
      name: utf8
      tags: list<identifier>
    }
    @document
    type D {
      @children("sys") systems: list<Sys>
      @children("cont") containers: list<Cont>
      matches: u64
      quoted_tag: identifier
      by_param: u64
    }

    sys alpha { name = "Alpha" }
    cont web { system = "alpha"  name = "Web"  tags = ["frontend", shared] }
    cont db  { system = alpha    name = "DB"   tags = [backend] }

    let s0 = at(systems, 0)
    let w0 = at(containers, 0)

    // Both the quoted and the bare ref must join against the system id.
    matches = len(filter(containers, fn(c: Cont) -> bool { c.system == s0.id }))

    // list<identifier> elements coerce too.
    quoted_tag = at(w0.tags, 0)

    // fn params declared `identifier` coerce their string arguments.
    let count_sys = fn(sid: identifier) -> u64 {
      len(filter(containers, fn(c: Cont) -> bool { c.system == sid }))
    }
    by_param = count_sys("alpha")
"#;

#[test]
fn quoted_refs_join_like_bare_ones() {
    let doc = Document::open(SRC, "test").expect("parse");
    assert_eq!(
        doc.get("matches").expect("path").value().expect("eval"),
        Value::I64(2)
    );
}

#[test]
fn quoted_list_elements_become_identifiers() {
    let doc = Document::open(SRC, "test").expect("parse");
    assert_eq!(
        doc.get("quoted_tag").expect("path").value().expect("eval"),
        Value::Identifier("frontend".into())
    );
}

#[test]
fn string_args_coerce_on_identifier_params() {
    let doc = Document::open(SRC, "test").expect("parse");
    assert_eq!(
        doc.get("by_param").expect("path").value().expect("eval"),
        Value::I64(2)
    );
}

#[test]
fn scalar_quoted_ref_field_evaluates_to_identifier() {
    let doc = Document::open(SRC, "test").expect("parse");
    // Read the quoted `system = "alpha"` field directly off the view.
    let v = doc
        .get("containers.web.system")
        .expect("path")
        .value()
        .expect("eval");
    assert_eq!(v, Value::Identifier("alpha".into()));
}
