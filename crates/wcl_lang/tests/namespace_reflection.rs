//! Reflection over namespaced declarations.
//!
//! Regression tests for the bug where `type_fields` / `child_types`
//! silently returned nothing for types declared under a `namespace`
//! (typically in an imported schema file): root resolution only tried
//! the root document's namespace, and `child_types` returned
//! unqualified segments. Reflection must see a namespaced type exactly
//! as it sees a root-namespace twin — via a bare reference (wildcard
//! from the import), a qualified `lib.Gizmo` path, and the refs that
//! `child_types` hands back.

use wcl_lang::{Document, Environment, Registry, Value, disk_loader};

const LIB_SCHEMA: &str = r#"
namespace lib

@block("gizmo")
type Gizmo {
  @inline(0) id: utf8
  name: utf8
}

@document
type LibModel {
  @children("gizmo") gizmos: list<Gizmo>
}
"#;

const ROOT_SCHEMA: &str = r#"
@block("root_gizmo")
type RootGizmo {
  @inline(0) id: utf8
  name: utf8
}

@document
type RootModel {
  @children("root_gizmo") root_gizmos: list<RootGizmo>
}
"#;

fn open(root: &str) -> Document {
    let mut reg = Registry::new();
    reg.register("schema.wcl", LIB_SCHEMA);
    reg.register("schema_root.wcl", ROOT_SCHEMA);
    let loader = reg.loader(disk_loader());
    Document::open_at_with_loader(root, "main.wcl", None, &Environment::new(), loader)
        .expect("document opens")
}

fn eval(doc: &Document, field: &str) -> Value {
    doc.get(field)
        .unwrap_or_else(|| panic!("field '{field}' present"))
        .value()
        .unwrap_or_else(|e| panic!("field '{field}' evaluates: {e:?}"))
}

#[test]
fn namespaced_type_fields_match_root_twin_via_bare_and_qualified_refs() {
    let doc = open(
        r#"
        import <schema.wcl>
        import <schema_root.wcl>
        @schemaless bare = type_fields(Gizmo)
        @schemaless qualified = type_fields(lib.Gizmo)
        @schemaless root = type_fields(RootGizmo)
        "#,
    );
    let bare = eval(&doc, "bare");
    let qualified = eval(&doc, "qualified");
    let root = eval(&doc, "root");
    let Value::List(items) = &bare else {
        panic!("expected list, got {bare:?}");
    };
    assert_eq!(items.len(), 2, "Gizmo reflects both fields: {items:?}");
    assert_eq!(bare, qualified, "bare and qualified refs agree");
    assert_eq!(bare, root, "namespaced reflection matches the root twin");
}

#[test]
fn namespaced_child_types_match_root_twin_and_chain() {
    let doc = open(
        r#"
        import <schema.wcl>
        import <schema_root.wcl>
        @schemaless ns_kids = child_types(LibModel)
        @schemaless qualified_kids = child_types(lib.LibModel)
        @schemaless root_kids = child_types(RootModel)
        @schemaless chained = type_fields(at(child_types(LibModel), 0))
        "#,
    );
    assert_eq!(
        eval(&doc, "ns_kids"),
        Value::List(vec![Value::DataPath {
            kind: "type".into(),
            segments: vec!["lib".into(), "Gizmo".into()],
        }]),
        "child slot refs carry the FQN"
    );
    assert_eq!(eval(&doc, "ns_kids"), eval(&doc, "qualified_kids"));
    assert_eq!(
        eval(&doc, "root_kids"),
        Value::List(vec![Value::DataPath {
            kind: "type".into(),
            segments: vec!["RootGizmo".into()],
        }])
    );
    let chained = eval(&doc, "chained");
    let Value::List(fields) = &chained else {
        panic!("expected list, got {chained:?}");
    };
    assert_eq!(
        fields.len(),
        2,
        "refs from child_types chain into type_fields: {fields:?}"
    );
}

#[test]
fn unresolvable_reflection_target_errors_instead_of_empty() {
    let doc = open(
        r#"
        import <schema.wcl>
        @schemaless out = type_fields(NoSuchType)
        "#,
    );
    let err = doc
        .get("out")
        .expect("field present")
        .value()
        .expect_err("unresolvable type reference is an error, not silence");
    let msg = format!("{err:?}");
    assert!(msg.contains("NoSuchType"), "{msg}");
}
