//! End-to-end tests for type aliases (`type Port = u16`) and the
//! constraint decorators (`@min` / `@max` / `@non_empty`).

use wcl_lang::{Document, SchemaViolationKind, Value};

fn violations(src: &str) -> Vec<(SchemaViolationKind, String)> {
    let doc = Document::open(src, "test").unwrap();
    doc.schema_errors()
        .iter()
        .filter_map(|e| match e {
            wcl_lang::EvalError::SchemaViolation { kind, message, .. } => {
                Some((*kind, message.clone()))
            }
            _ => None,
        })
        .collect()
}

const SCHEMA: &str = "@min(1) @max(65535)\ntype Port = u16\n\
                      @non_empty type Name = utf8\n\
                      @document type Cfg { @children(\"svc\") svcs: list<Svc> }\n\
                      @block(\"svc\") type Svc { port: Port  name: Name }\n";

#[test]
fn alias_accepts_values_of_the_target_type() {
    let src = format!("{SCHEMA}svc web {{\n  port = 9090u16\n  name = \"web\"\n}}\n");
    assert!(violations(&src).is_empty(), "{:?}", violations(&src));
}

#[test]
fn alias_rejects_values_of_the_wrong_type() {
    let src = format!("{SCHEMA}svc web {{\n  port = \"oops\"\n  name = \"web\"\n}}\n");
    let v = violations(&src);
    assert!(
        v.iter()
            .any(|(k, _)| matches!(k, SchemaViolationKind::FieldTypeMismatch)),
        "{v:?}"
    );
}

#[test]
fn min_max_constraints_flag_out_of_range_values() {
    let src = format!("{SCHEMA}svc web {{\n  port = 0u16\n  name = \"web\"\n}}\n");
    let v = violations(&src);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(matches!(v[0].0, SchemaViolationKind::ConstraintViolation));
    assert!(v[0].1.contains("below @min(1)"), "{}", v[0].1);
}

#[test]
fn non_empty_flags_empty_strings() {
    let src = format!("{SCHEMA}svc web {{\n  port = 80u16\n  name = \"\"\n}}\n");
    let v = violations(&src);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].1.contains("@non_empty"), "{}", v[0].1);
}

#[test]
fn constraints_on_the_field_declaration_apply_directly() {
    let src = "@document type Cfg { @children(\"g\") gs: list<G> }\n\
               @block(\"g\") type G { @min(0) @max(1) gain: f64 }\n\
               g a { gain = 1.5 }\n";
    let v = violations(src);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].1.contains("above @max(1)"), "{}", v[0].1);
}

#[test]
fn constraints_check_root_fields_too() {
    let src = "@min(1) type Port = u16\n\
               @document type Cfg { port: Port }\n\
               port = 0u16\n";
    let v = violations(src);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(matches!(v[0].0, SchemaViolationKind::ConstraintViolation));
}

#[test]
fn alias_chains_resolve_transitively() {
    let src = "type Port = u16\ntype WebPort = Port\n\
               @document type Cfg { p: WebPort }\n\
               p = 8080u16\n";
    assert!(violations(src).is_empty(), "{:?}", violations(src));
}

#[test]
fn alias_of_list_type_resolves_elements() {
    let src = "@non_empty type Tags = list<utf8>\n\
               @document type Cfg { tags: Tags }\n\
               tags = []\n";
    let v = violations(src);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].1.contains("@non_empty"), "{}", v[0].1);
}

#[test]
fn alias_cycle_stays_permissive_instead_of_hanging() {
    let src = "type A = B\ntype B = A\n\
               @document type Cfg { x: A }\n\
               x = 1\n";
    // Must terminate; the unresolvable alias falls back to permissive.
    let doc = Document::open(src, "test").unwrap();
    let _ = doc.schema_errors();
}

#[test]
fn alias_formats_and_round_trips() {
    let src = "@min(1)\ntype Port = u16\n@schemaless p = 1\n";
    let ast = wcl_lang::parse_for_edit(src, "t").expect("parse");
    let printed = wcl_lang::format::to_source(&ast);
    assert_eq!(printed, src);
}

#[test]
fn alias_with_extends_is_a_parse_error() {
    let src = "type A { x: i64 }\ntype B extends A = u16\n";
    let doc = Document::open(src, "test");
    assert!(doc.is_err(), "alias with extends must not parse");
}

#[test]
fn eval_through_alias_typed_field_works() {
    let src = "type Port = u16\n\
               @document type Cfg { p: Port }\n\
               p = 8080u16\n";
    let doc = Document::open(src, "test").unwrap();
    assert_eq!(
        doc.get("p").expect("p").value().expect("eval"),
        Value::U16(8080)
    );
}
