use std::path::PathBuf;

use wcl_lang::{Document, ResolvedType, TypeRef, Value, VariantBodyView};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn open(src: &str) -> Document {
    Document::open(src, "test").expect("open")
}

#[test]
fn parses_basic_example_from_disk() {
    let doc = Document::from_file(&examples_dir().join("basic.wcl")).expect("basic example parses");
    assert_eq!(
        doc.field("name").unwrap().value().unwrap(),
        &Value::Utf8("alpha".into())
    );
    let svc = doc.block("service").expect("service block");
    assert_eq!(svc.labels(), vec![Value::Utf8("web".into())]);
    assert_eq!(
        svc.field("port").unwrap().value().unwrap(),
        &Value::I64(8080)
    );
}

#[test]
fn document_round_trips_simple_fields() {
    let doc = open(
        r#"
        name  = "alpha"
        count = 3
        # a comment
        // another comment
        flag  = false
        "#,
    );
    assert_eq!(doc.fields().count(), 3);
    assert_eq!(
        doc.field("flag").unwrap().value().unwrap(),
        &Value::Bool(false)
    );
}

#[test]
fn fixture_union_shape_resolves() {
    let doc = Document::from_file(&examples_dir().join("types.wcl")).expect("types fixture parses");
    let shape = doc.union_decl("company.Shape").expect("Shape union");
    assert_eq!(shape.variants().count(), 4);
    let polygon = shape.variant("Polygon").expect("Polygon variant");
    // Polygon's body is `P` (alias) — source form unresolved; resolve follows.
    match polygon.body() {
        VariantBodyView::TypeRef(t) => {
            assert_eq!(*t, TypeRef::Named(vec!["P".into()]));
            match doc.resolve(t) {
                ResolvedType::Named(d) => assert_eq!(d.full_name(), "company.utils.Point"),
                _ => panic!("expected Named after resolve"),
            }
        }
        _ => panic!("Polygon body should be TypeRef"),
    }
    let empty = shape.variant("Empty").expect("Empty variant");
    assert!(matches!(empty.body(), VariantBodyView::Unit));
}

#[test]
fn fixture_brush_block_schema_resolves() {
    let doc = Document::from_file(&examples_dir().join("types.wcl")).expect("types fixture parses");
    let schema = doc.block_schema("brush").expect("brush block schema");
    assert_eq!(schema.name(), "Brush");
    // @inline(0) -> id (identifier)
    let id = schema.field("id").unwrap();
    assert_eq!(id.inline_slot(), Some(0));
    // @default(8080) -> port
    let port = schema.field("port").unwrap();
    assert_eq!(port.default_value(), Some(Value::I64(8080)));
}

#[test]
fn fixture_brush_block_has_mixed_labels() {
    let doc = Document::from_file(&examples_dir().join("types.wcl")).expect("types fixture parses");
    let b = doc.block("brush").expect("brush block");
    let labels = b.labels();
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0], Value::Identifier("primary".into()));
    assert_eq!(labels[1], Value::Utf8("matte".into()));
}

#[test]
fn named_type_refs_resolve_in_fixture() {
    let doc = Document::from_file(&examples_dir().join("types.wcl")).expect("types fixture parses");
    let user = doc.type_decl("company.User").expect("User type");
    let parent = user.field("parent").expect("parent field");
    let ResolvedType::Reference(inner) = doc.resolve(parent.type_ref()) else {
        panic!("parent should resolve to a reference");
    };
    let ResolvedType::Named(decl) = *inner else {
        panic!("reference inner should be Named(company.User)");
    };
    assert_eq!(decl.full_name(), "company.User");
}

#[test]
fn fixture_namespace_and_uses_round_trip_through_api() {
    let doc = Document::from_file(&examples_dir().join("types.wcl")).expect("types fixture parses");
    assert_eq!(doc.namespace(), &["company".to_string()]);
    assert!(doc.uses().count() >= 3);
    let user = doc.type_decl("company.User").unwrap();
    // Item alias P → company.utils.Point
    match doc.resolve(user.field("pos").unwrap().type_ref()) {
        ResolvedType::Named(d) => assert_eq!(d.full_name(), "company.utils.Point"),
        _ => panic!("pos should resolve via alias"),
    }
    // Wildcard import `use company.utils` makes bare `Address` resolve.
    match doc.resolve(user.field("home").unwrap().type_ref()) {
        ResolvedType::Named(d) => assert_eq!(d.full_name(), "company.utils.Address"),
        _ => panic!("home should resolve via wildcard"),
    }
    // Brace alias Sq → company.shapes.Square
    match doc.resolve(user.field("avatar").unwrap().type_ref()) {
        ResolvedType::Named(d) => assert_eq!(d.full_name(), "company.shapes.Square"),
        _ => panic!("avatar should resolve via brace alias"),
    }
}

#[test]
fn typed_literals_resolve_from_disk() {
    let doc = Document::from_file(&examples_dir().join("types.wcl")).expect("types fixture parses");
    assert_eq!(doc.field("byte").unwrap().value().unwrap(), &Value::U8(200));
    assert_eq!(
        doc.field("small").unwrap().value().unwrap(),
        &Value::I8(-120)
    );
    assert_eq!(
        doc.field("ratio").unwrap().value().unwrap(),
        &Value::F32(1.5)
    );
    assert_eq!(
        doc.field("name").unwrap().value().unwrap(),
        &Value::Ascii("alpha".into())
    );
    assert_eq!(
        doc.field("hello16").unwrap().value().unwrap(),
        &Value::Utf16("hello".encode_utf16().collect())
    );
}

#[test]
fn nested_blocks_preserve_structure() {
    let doc = open(
        r#"
        service "web" {
          port = 8080
          metadata {
            region = "us-east-1"
          }
        }
        "#,
    );
    let svc = doc.block("service").unwrap();
    assert_eq!(
        svc.field("port").unwrap().value().unwrap(),
        &Value::I64(8080)
    );
    let meta = svc.block("metadata").unwrap();
    assert_eq!(
        meta.field("region").unwrap().value().unwrap(),
        &Value::Utf8("us-east-1".into())
    );
}

#[test]
fn parse_error_has_useful_span() {
    let err = Document::open("name = ", "input").unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("expected value"), "rendered: {rendered}");
}

#[test]
fn field_value_address_is_stable_across_accesses() {
    let doc = open(r#"name = "alpha""#);
    let f = doc.field("name").unwrap();
    let p1 = f.value().unwrap() as *const Value;
    let p2 = f.value().unwrap() as *const Value;
    assert_eq!(p1, p2);
}

#[test]
fn span_is_available_without_forcing_value() {
    let doc = open(r#"name = "alpha""#);
    let f = doc.field("name").unwrap();
    let span = f.span();
    assert!(span.start < span.end);
}

#[test]
fn parses_functions_example_from_disk() {
    use wcl_lang::BuiltinType;

    let doc = Document::from_file(&examples_dir().join("functions.wcl"))
        .expect("functions example parses");

    let double = doc.field("double").unwrap();
    let Value::Function(f) = double.value().unwrap() else {
        panic!("expected function value")
    };
    assert_eq!(f.params().len(), 1);
    assert_eq!(f.params()[0].name(), "x");
    assert_eq!(f.params()[0].ty(), &TypeRef::Builtin(BuiltinType::I32));
    assert_eq!(f.return_ty(), &TypeRef::Builtin(BuiltinType::I32));

    let Value::Function(f) = doc.field("sum_squared").unwrap().value().unwrap() else {
        panic!("expected function value")
    };
    assert_eq!(f.params().len(), 2);
    assert_eq!(f.params()[1].name(), "y");

    let handler = doc.type_decl("Handler").expect("type Handler");
    let on_click = handler.field("on_click").unwrap();
    let TypeRef::Function { params, return_ty } = on_click.type_ref() else {
        panic!("on_click should be a fn type")
    };
    assert_eq!(params.len(), 1);
    assert_eq!(**return_ty, TypeRef::Builtin(BuiltinType::Bool));
    let thunk = handler.field("thunk").unwrap();
    let TypeRef::Function { params, .. } = thunk.type_ref() else {
        panic!("thunk should be a fn type")
    };
    assert!(params.is_empty());

    let Value::Function(f) = doc.field("adder").unwrap().value().unwrap() else {
        panic!("expected function value")
    };
    assert!(matches!(f.return_ty(), TypeRef::Function { .. }));
}
