//! Duplicate-id detection (issue #17).
//!
//! Two sibling blocks of one kind sharing an identity label — a label the
//! schema declares as `@inline(0) id: identifier` — make every reference
//! to that id ambiguous, and gathered lists silently carry both. `wcl
//! check` flags each repeat. Kinds whose label isn't an id (`code wcl`'s
//! language, a `name: identifier` label) repeat freely, and the same id
//! under different parents is legal (step ids repeat across procedures).

use wcl_lang::Document;

fn violations(src: &str) -> Vec<String> {
    let doc = Document::open(src, "test").expect("parse");
    doc.schema_errors()
        .iter()
        .map(|e| e.to_string())
        .filter(|m| m.contains("duplicate id"))
        .collect()
}

const SCHEMA: &str = r#"
    @block("comp") type Comp { @inline(0) id: identifier  name: utf8 }
    @block("note") type Note { @inline(0) tag: utf8  text: utf8 }
    @block("lang") type Lang { @inline(0) name: identifier  text: utf8 }
    @block("step") type Step { @inline(0) id: identifier  text: utf8 }
    @block("proc") type Proc { @inline(0) id: identifier  @children("step") steps: list<Step> }
    @document type D {
      @children("comp") comps: list<Comp>
      @children("note") notes: list<Note>
      @children("lang") langs: list<Lang>
      @children("proc") procs: list<Proc>
    }
"#;

#[test]
fn duplicate_top_level_ids_are_flagged() {
    let src =
        format!("{SCHEMA}\ncomp engine {{ name = \"one\" }}\ncomp engine {{ name = \"two\" }}\n");
    let v = violations(&src);
    assert_eq!(v.len(), 1, "one violation for the repeat: {v:?}");
    assert!(v[0].contains("'comp' block 'engine'"));
}

#[test]
fn non_id_labels_repeat_freely() {
    // `tag: utf8` and `name: identifier` labels are parameters, not ids.
    let src = format!(
        "{SCHEMA}\nnote wcl {{ text = \"a\" }}\nnote wcl {{ text = \"b\" }}\n\
         lang rust {{ text = \"a\" }}\nlang rust {{ text = \"b\" }}\n"
    );
    assert!(violations(&src).is_empty());
}

#[test]
fn nested_duplicates_flagged_but_cross_parent_repeats_allowed() {
    let src = format!(
        "{SCHEMA}\n\
         proc a {{\n  step verify {{ text = \"x\" }}\n  step verify {{ text = \"y\" }}\n}}\n\
         proc b {{\n  step verify {{ text = \"z\" }}\n}}\n"
    );
    let v = violations(&src);
    assert_eq!(v.len(), 1, "only proc a's repeat: {v:?}");
    assert!(v[0].contains("'step' block 'verify'"));
}

#[test]
fn distinct_ids_pass() {
    let src =
        format!("{SCHEMA}\ncomp engine {{ name = \"one\" }}\ncomp video {{ name = \"two\" }}\n");
    assert!(violations(&src).is_empty());
}
