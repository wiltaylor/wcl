//! End-to-end tests for syntax-only type arguments (`content<SvgBlock>`).
//!
//! The arguments are metadata: they parse, they print, and they are
//! readable off the `TypeRef`. Nothing checks their arity and nothing
//! substitutes them — a named type still resolves by path alone, which
//! is what lets a document carry them without the language having
//! generics.

use wcl_lang::{Document, SchemaViolationKind, TypeRef};

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

fn formatted(src: &str) -> String {
    let ast = wcl_lang::parse_for_edit(src, "t").expect("parse");
    wcl_lang::format::to_source(&ast)
}

#[test]
fn formatter_round_trips_type_arguments() {
    let src = "type Page {\n}\ntype Q {\n  f: Slot<Page>\n}\n@schemaless x = 1\n";
    assert_eq!(formatted(src), src);
}

#[test]
fn formatter_is_stable_over_type_arguments() {
    // `wcl fmt` normalises spacing on the first pass and is a fixpoint
    // from there — the shape `wcl fmt --check` depends on.
    let src = "type Q { f: a.b.Path<x.Y,u32> }\n";
    let once = formatted(src);
    assert_eq!(once, "type Q {\n  f: a.b.Path<x.Y, u32>\n}\n");
    assert_eq!(formatted(&once), once);
}

#[test]
fn type_arguments_are_readable_as_metadata() {
    let src = "type Page {}\ntype Slot {}\ntype Q { f: Slot<Page> }\n@schemaless x = 1\n";
    let doc = Document::open(src, "test").unwrap();
    let decl = doc.type_decl("Q").expect("type Q");
    let ty = decl.field("f").expect("field f").type_ref().clone();
    assert_eq!(
        ty.type_args(),
        &[TypeRef::named(vec!["Page".into()])],
        "the argument is on the TypeRef"
    );
    assert_eq!(ty.to_string(), "Slot<Page>");
    // Every other shape reports no arguments.
    assert!(TypeRef::named(vec!["Slot".into()]).type_args().is_empty());
    assert!(
        TypeRef::List(Box::new(TypeRef::named(vec!["Page".into()])))
            .type_args()
            .is_empty()
    );
}

#[test]
fn a_type_argument_does_not_change_how_the_named_type_resolves() {
    // `Port<Whatever>` is the type `Port`: the same value passes and the
    // same value fails as without the argument.
    let ok = "type Port = u16\n\
              @document type Cfg { @children(\"svc\") svcs: list<Svc> }\n\
              @block(\"svc\") type Svc { port: Port<Whatever> }\n\
              svc web {\n  port = 9090u16\n}\n";
    assert!(violations(ok).is_empty(), "{:?}", violations(ok));

    let bad = ok.replace("port = 9090u16", "port = \"oops\"");
    assert!(
        violations(&bad)
            .iter()
            .any(|(k, _)| matches!(k, SchemaViolationKind::FieldTypeMismatch)),
        "{:?}",
        violations(&bad)
    );
}

#[test]
fn arity_is_not_checked() {
    // No declaration form carries type parameters, so there is no arity
    // to check against: any number of arguments parses and opens.
    for args in ["<Page>", "<Page, Page>", "<Page, Page, Page>"] {
        let src = format!(
            "type Page {{}}\n\
             @document type Cfg {{ @children(\"svc\") svcs: list<Svc> }}\n\
             @block(\"svc\") type Svc {{ p: Page{args} }}\n\
             svc web {{\n  p = 1\n}}\n"
        );
        Document::open(&src, "test").unwrap_or_else(|e| panic!("`Page{args}` must open: {e:?}"));
    }
}

#[test]
fn arguments_do_not_distinguish_two_types_that_extends_must_agree_on() {
    // `extends` compares the declared field types. Arguments are
    // metadata, so an override differing only in them is not a
    // conflict — otherwise `S<A>` and `S<B>` would be distinct types by
    // the back door. (The interface path compares resolved types, which
    // carry no arguments at all; the two must agree.)
    let src = "type X {}\ntype Y {}\ntype S {}\n\
               type P { f: S<X> }\n\
               type C extends P { f: S<Y> }\n";
    Document::open(src, "test").expect("arguments alone are not an extends conflict");

    let conflict = "type S {}\ntype T {}\n\
                    type P { f: S<X> }\n\
                    type C extends P { f: T<X> }\n";
    assert!(
        Document::open(conflict, "test").is_err(),
        "a genuinely different head type is still a conflict"
    );
}

#[test]
fn arguments_do_not_make_two_union_variants_distinguishable() {
    // Dispatch resolves a named type by path, so `A S<X>` and `B S<Y>`
    // are as indistinguishable as `A S` and `B S`.
    let src = "type S {}\n\
               union U { A S<X> B S<Y> }\n\
               @document type Cfg { u: U }\n\
               @schemaless x = 1\n";
    let doc = Document::open(src, "test").unwrap();
    assert!(
        doc.schema_errors().iter().any(|e| matches!(
            e,
            wcl_lang::EvalError::SchemaViolation {
                kind: SchemaViolationKind::VariantShapeCollision,
                ..
            }
        )),
        "colliding variant bodies are still reported: {:?}",
        doc.schema_errors()
    );
}

#[test]
fn an_unknown_type_with_arguments_is_still_an_unknown_type() {
    // The accepted cost: `content<Nonsense>` parses, and the *head* is
    // reported by the usual resolution pass. Arguments are not resolved.
    let err = Document::open("type Q { f: Missing<Page> }\n", "test")
        .expect_err("unknown head type is rejected");
    assert!(
        format!("{err:?}").contains("Missing"),
        "resolution reports the head: {err:?}"
    );
    Document::open("type Page {}\ntype Q { f: Page<Nonsense> }\n", "test")
        .expect("an unresolvable argument is not checked");
}
