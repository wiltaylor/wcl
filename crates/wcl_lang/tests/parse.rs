use std::path::PathBuf;

use wcl_lang::{
    DeclName, Document, Environment, EvalError, ProfileKey, Registry, ResolvedType, SymbolKind,
    TypeRef, Value, VariantBodyView, VariantPayload, ast, disk_loader, format, from_fn,
    parse_for_edit,
};

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
    assert_eq!(svc.labels().unwrap(), vec![Value::Utf8("web".into())]);
    assert_eq!(
        svc.field("port").unwrap().value().unwrap(),
        &Value::U32(8080)
    );
}

#[test]
fn document_round_trips_simple_fields() {
    let doc = open(
        r#"
        @schemaless name  = "alpha"
        @schemaless count = 3
        # a comment
        // another comment
        @schemaless flag  = false
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
    let labels = b.labels().unwrap();
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0], Value::Identifier("primary".into()));
    assert_eq!(labels[1], Value::Utf8("matte".into()));
}

#[test]
fn bare_kebab_and_path_labels_resolve_to_identifier() {
    // Issue #11: a bare label may contain `-`/`/` connectors, so kebab
    // class names and path-like page names need no quoting. Each stitches
    // into a single `Value::Identifier` and validates against the
    // `identifier`-typed @inline(0) field.
    let doc = open(
        r#"
        @document type Cfg { @children("svc") svcs: list<Svc> }
        @block("svc") type Svc { @inline(0) name: identifier }
        svc dgm-box {}
        svc wdoc-series-1 {}
        svc reference/intro {}
        svc api/v1/users {}
        "#,
    );
    let labels: Vec<Value> = doc
        .blocks()
        .filter(|b| b.kind() == "svc")
        .map(|b| b.labels().unwrap().into_iter().next().unwrap())
        .collect();
    assert_eq!(
        labels,
        vec![
            Value::Identifier("dgm-box".into()),
            Value::Identifier("wdoc-series-1".into()),
            Value::Identifier("reference/intro".into()),
            Value::Identifier("api/v1/users".into()),
        ]
    );
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn quoted_kebab_label_stays_utf8_and_still_valid() {
    // Quoting remains valid and is NOT auto-converted; the value stays a
    // string, which an `identifier` field also accepts.
    let doc = open(
        r#"
        @document type Cfg { @children("svc") svcs: list<Svc> }
        @block("svc") type Svc { @inline(0) name: identifier }
        svc "dgm-box" {}
        "#,
    );
    let b = doc.block("svc").unwrap();
    assert_eq!(b.labels().unwrap()[0], Value::Utf8("dgm-box".into()));
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn spaced_dash_label_does_not_stitch_and_subtraction_survives() {
    // A connector only joins when byte-adjacent — `a-b` stitches, but
    // arithmetic `7 - 3` / `12 / 4` in a field RHS is untouched.
    let doc = open(
        r#"
        @schemaless diff = 7 - 3
        @schemaless quot = 12 / 4
        "#,
    );
    assert_eq!(doc.field("diff").unwrap().value().unwrap(), &Value::I64(4));
    assert_eq!(doc.field("quot").unwrap().value().unwrap(), &Value::I64(3));
}

#[test]
fn dangling_connector_label_does_not_mis_parse() {
    // `svc foo- {}` must not silently produce a `foo-`/`foo` body with a
    // swallowed connector — it fails cleanly (no panic, no wrong AST).
    assert!(
        Document::open(
            r#"
            @document type Cfg { @children("svc") svcs: list<Svc> }
            @block("svc") type Svc { @inline(0) name: identifier }
            svc foo- {}
            "#,
            "test",
        )
        .is_err()
    );
}

#[test]
fn dotted_label_still_errors() {
    // `.` is intentionally not a connector (member access in a label is an
    // error by convention — use an interpolated string instead).
    assert!(
        Document::open(
            r#"
            @document type Cfg { @children("svc") svcs: list<Svc> }
            @block("svc") type Svc { @inline(0) name: identifier }
            svc foo.bar {}
            "#,
            "test",
        )
        .is_err()
    );
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
    let doc = open(r#"@schemaless name = "alpha""#);
    let f = doc.field("name").unwrap();
    let p1 = f.value().unwrap() as *const Value;
    let p2 = f.value().unwrap() as *const Value;
    assert_eq!(p1, p2);
}

#[test]
fn span_is_available_without_forcing_value() {
    let doc = open(r#"@schemaless name = "alpha""#);
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

#[test]
fn document_exposes_symbol_index() {
    let doc = open(
        r#"
        namespace foo
        type Bar { x: i32 }
        union Maybe { Some i32 None none }
        symbol_set Color { red blue }
        port = 8080
        service "web" { region = "us" }
        "#,
    );
    let idx = doc.symbols();

    // Top-level decls indexed under the file-ns-qualified FQN.
    assert!(matches!(
        idx.lookup("foo.Bar").unwrap().kind,
        SymbolKind::TypeDecl
    ));
    assert!(matches!(
        idx.lookup("foo.Bar.x").unwrap().kind,
        SymbolKind::TypeField { .. }
    ));
    assert!(matches!(
        idx.lookup("foo.Maybe").unwrap().kind,
        SymbolKind::UnionDecl
    ));
    assert!(matches!(
        idx.lookup("foo.Maybe.None").unwrap().kind,
        SymbolKind::UnionVariant { .. }
    ));
    assert!(matches!(
        idx.lookup("foo.Color").unwrap().kind,
        SymbolKind::SymbolSetDecl
    ));
    assert!(matches!(
        idx.lookup("foo.Color.blue").unwrap().kind,
        SymbolKind::SymbolEntry { .. }
    ));
    assert!(matches!(
        idx.lookup("foo.port").unwrap().kind,
        SymbolKind::Field
    ));
    assert_eq!(idx.blocks_with_kind("service").len(), 1);

    // Document::type_decl now routes through the index — same FQN works.
    assert!(doc.type_decl("foo.Bar").is_some());
    assert!(doc.union_decl("foo.Maybe").is_some());
    assert!(doc.symbol_set("foo.Color").is_some());
}

#[test]
fn builtins_example_evaluates_end_to_end() {
    let mut env = Environment::new();
    env.add_builtin("upper", from_fn(|s: String| s.to_uppercase()));
    env.add_builtin("len", from_fn(|s: String| s.len() as i64));
    env.add_builtin("add", from_fn(|a: i64, b: i64| a + b));

    let path = examples_dir().join("builtins.wcl");
    let source = std::fs::read_to_string(&path).expect("read builtins.wcl");
    let doc = Document::open_with(&source, &path.display().to_string(), &env)
        .expect("builtins fixture parses");

    assert_eq!(
        doc.field("greeting").unwrap().value().unwrap(),
        &Value::Utf8("HELLO".into())
    );
    assert_eq!(
        doc.field("shouted").unwrap().value().unwrap(),
        &Value::Utf8("ALPHA".into())
    );
    assert_eq!(doc.field("total").unwrap().value().unwrap(), &Value::I64(3));
    assert_eq!(
        doc.field("ranking").unwrap().value().unwrap(),
        &Value::I64(13)
    );
    assert_eq!(
        doc.field("is_big").unwrap().value().unwrap(),
        &Value::Bool(true)
    );
    assert_eq!(
        doc.field("combined").unwrap().value().unwrap(),
        &Value::I64(2)
    );
    assert!(matches!(
        doc.field("double").unwrap().value().unwrap(),
        Value::Function(_)
    ));
    assert_eq!(
        doc.field("doubled4").unwrap().value().unwrap(),
        &Value::I64(8)
    );
}

#[test]
fn collection_builtins_evaluate_end_to_end() {
    let env = Environment::new();
    let path = examples_dir().join("builtins_collections.wcl");
    let source = std::fs::read_to_string(&path).expect("read builtins_collections.wcl");
    let doc = Document::open_with(&source, &path.display().to_string(), &env)
        .expect("collection fixture parses");

    let i64s = |xs: &[i64]| {
        Value::List(
            xs.iter()
                .copied()
                .map(Value::I64)
                .collect::<Vec<_>>()
                .into(),
        )
    };

    assert_eq!(
        doc.field("doubled").unwrap().value().unwrap(),
        &i64s(&[2, 4, 6, 8])
    );
    assert_eq!(doc.field("evens").unwrap().value().unwrap(), &i64s(&[2, 4]));
    assert_eq!(
        doc.field("total").unwrap().value().unwrap(),
        &Value::I64(10)
    );
    assert_eq!(doc.field("count").unwrap().value().unwrap(), &Value::I64(4));
    assert_eq!(
        doc.field("quick_sum").unwrap().value().unwrap(),
        &Value::I64(10)
    );
    assert_eq!(doc.field("first").unwrap().value().unwrap(), &Value::I64(1));
    assert_eq!(
        doc.field("rest").unwrap().value().unwrap(),
        &i64s(&[2, 3, 4])
    );
    assert_eq!(
        doc.field("r").unwrap().value().unwrap(),
        &i64s(&[0, 1, 2, 3, 4])
    );

    // Tensor round-trip preserves shape.
    let t_mapped = doc.field("t_mapped").unwrap().value().unwrap();
    let Value::Tensor { shape, data } = t_mapped else {
        panic!("expected tensor value, got {t_mapped:?}");
    };
    assert_eq!(shape, &vec![2u64, 3]);
    assert_eq!(
        data.as_ref(),
        &vec![
            Value::I64(11),
            Value::I64(12),
            Value::I64(13),
            Value::I64(14),
            Value::I64(15),
            Value::I64(16),
        ]
    );
    assert_eq!(
        doc.field("t_shape").unwrap().value().unwrap(),
        &i64s(&[2, 3])
    );
    assert_eq!(
        doc.field("t_data").unwrap().value().unwrap(),
        &i64s(&[11, 12, 13, 14, 15, 16])
    );
}

#[test]
fn closures_capture_definition_site_locals() {
    let path = examples_dir().join("closures.wcl");
    let doc = Document::from_file(&path).expect("closures fixture parses");
    let val = |name: &str| doc.field(name).unwrap().value().unwrap();
    assert_eq!(val("captured"), &Value::I64(8));
    assert_eq!(val("fifteen"), &Value::I64(15));
    assert_eq!(val("seven"), &Value::I64(7));
    assert_eq!(val("shadowing"), &Value::I64(7));
}

#[test]
fn strict_value_vs_type_field_mismatch() {
    let src = r#"
        @document type Cfg { port: u16  host: utf8 }
        port = "oops"
        host = 42
    "#;
    let doc = Document::open(src, "test").unwrap();
    let errs = doc.schema_errors();
    let count = errs
        .iter()
        .filter(|e| {
            matches!(
                e,
                EvalError::SchemaViolation {
                    kind: wcl_lang::SchemaViolationKind::FieldTypeMismatch,
                    ..
                }
            )
        })
        .count();
    assert_eq!(
        count, 2,
        "expected 2 FieldTypeMismatch errors, got: {errs:#?}"
    );
}

#[test]
fn strict_value_vs_type_list_element_mismatch() {
    let src = r#"
        @document type Cfg { flags: list<bool> }
        flags = [1, 2, 3]
    "#;
    let doc = Document::open(src, "test").unwrap();
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::FieldTypeMismatch,
                ..
            }
        )),
        "expected FieldTypeMismatch on list element, got: {errs:#?}"
    );
}

#[test]
fn unqualified_variant_patterns_match() {
    let src = r#"
        union Shape { Circle { radius: f64 } Square { side: f64 } }
        @schemaless c = Shape::Circle { radius: 2.5 }
        @schemaless got = match c {
          Circle { radius } => radius,
          Square { side } => side,
          _ => 0.0,
        }
    "#;
    let doc = Document::open(src, "test").unwrap();
    assert_eq!(doc.field("got").unwrap().value().unwrap(), &Value::F64(2.5));
}

#[test]
fn multi_level_union_extends_inherits_all_variants() {
    let src = r#"
        union A { Foo { a: i64 } }
        union B extends A { Bar { b: utf8 } }
        union C extends B { Baz { c: bool } }
        @schemaless v1 = C::Foo { a: 1 }
        @schemaless v2 = C::Bar { b: "hi" }
        @schemaless v3 = C::Baz { c: true }
    "#;
    let doc = Document::open(src, "test").unwrap();
    let names: Vec<&str> = ["v1", "v2", "v3"]
        .iter()
        .map(|n| {
            let Value::Variant { variant, .. } = doc.field(n).unwrap().value().unwrap() else {
                panic!("{n} should be a variant")
            };
            variant.as_str()
        })
        .collect();
    assert_eq!(names, vec!["Foo", "Bar", "Baz"]);
}

#[test]
fn new_builtins_smoke() {
    let src = r#"
        @schemaless greeting  = concat("hello, ", "world")
        @schemaless formatted = format("x = {}", 42)
        @schemaless flat      = flatten([[1, 2], [3]])
        @schemaless pairs     = zip([1, 2, 3], [:a, :b])
        @schemaless reshaped  = tensor_shape(tensor_reshape(tensor([1, 2, 3, 4], [2, 2]), [4, 1]))
        @schemaless ok        = assert(true, "fine")
    "#;
    let doc = Document::open(src, "test").unwrap();
    assert_eq!(
        doc.field("greeting").unwrap().value().unwrap(),
        &Value::Utf8("hello, world".into())
    );
    assert_eq!(
        doc.field("formatted").unwrap().value().unwrap(),
        &Value::Utf8("x = 42".into())
    );
    assert_eq!(
        doc.field("flat").unwrap().value().unwrap(),
        &Value::List(std::sync::Arc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3)
        ]))
    );
    // zip with shorter `[:a, :b]` truncates to length 2.
    let pairs = doc.field("pairs").unwrap().value().unwrap();
    let Value::List(rows) = pairs else {
        panic!("zip should be list")
    };
    assert_eq!(rows.len(), 2);
    // reshape: [4, 1].
    assert_eq!(
        doc.field("reshaped").unwrap().value().unwrap(),
        &Value::List(std::sync::Arc::new(vec![Value::I64(4), Value::I64(1)]))
    );
    assert_eq!(doc.field("ok").unwrap().value().unwrap(), &Value::None);
}

#[test]
fn assert_failure_surfaces_user_error() {
    let src = r#"
        @schemaless boom = assert(1 > 2, "math is broken")
    "#;
    let doc = Document::open(src, "test").unwrap();
    let err = doc.field("boom").unwrap().value().unwrap_err();
    let EvalError::UserError { message, .. } = err else {
        panic!("expected UserError, got {err:?}")
    };
    assert_eq!(message, "math is broken");
}

#[test]
fn union_dispatch_block_to_variant() {
    let path = examples_dir().join("union_dispatch.wcl");
    let doc = Document::from_file(&path).expect("union_dispatch fixture parses");
    let shapes = doc
        .get("scene.shapes")
        .expect("scene.shapes")
        .value()
        .unwrap();
    let Value::List(items) = shapes else {
        panic!("scene.shapes should be a list");
    };
    assert_eq!(items.len(), 2);
    let Value::Variant { variant, .. } = &items[0] else {
        panic!("[0] should be a variant")
    };
    assert_eq!(variant, "Circle");
    let Value::Variant { variant, .. } = &items[1] else {
        panic!("[1] should be a variant")
    };
    assert_eq!(variant, "Square");
}

#[test]
fn union_dispatch_table_rows_to_variants() {
    let path = examples_dir().join("union_dispatch.wcl");
    let doc = Document::from_file(&path).expect("union_dispatch fixture parses");
    let steps = doc.get("flow.steps").expect("flow.steps").value().unwrap();
    let Value::List(items) = steps else {
        panic!("flow.steps should be a list");
    };
    assert_eq!(items.len(), 3);
    let names: Vec<&str> = items
        .iter()
        .map(|v| {
            let Value::Variant { variant, .. } = v else {
                panic!("expected variant")
            };
            variant.as_str()
        })
        .collect();
    assert_eq!(names, vec!["Greet", "Delay", "Greet"]);
}

#[test]
fn union_dispatch_no_match_surfaces_schema_error() {
    let src = r#"
        union Shape { Circle { radius: f64 } Square { side: f64 } }
        @block("scene")
        type Scene { @children(Shape) shapes: list<Shape> }
        scene "x" { weird { thing = 1 } }
    "#;
    let doc = Document::open(src, "test").unwrap();
    let scene = doc.block("scene").unwrap();
    let errs = scene.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::VariantNoMatch,
                ..
            }
        )),
        "expected VariantNoMatch, got: {errs:#?}"
    );
}

#[test]
fn union_extends_inherits_variants() {
    let path = examples_dir().join("union_extends.wcl");
    let doc = Document::from_file(&path).expect("union_extends fixture parses");

    let val = |name: &str| doc.field(name).unwrap().value().unwrap();

    // Inherited from Result, constructed via Extended path → union FQN is Extended.
    let Value::Variant { union, variant, .. } = val("e_ok") else {
        panic!("e_ok should be a variant")
    };
    assert_eq!(union, &vec!["Extended".to_string()]);
    assert_eq!(variant, "Ok");

    // Own variant.
    let Value::Variant { variant, .. } = val("e_pend") else {
        panic!("e_pend should be a variant")
    };
    assert_eq!(variant, "Pending");

    // Original Result path still works, untouched.
    let Value::Variant { union, variant, .. } = val("r_ok") else {
        panic!("r_ok should be a variant")
    };
    assert_eq!(union, &vec!["Result".to_string()]);
    assert_eq!(variant, "Ok");

    // InterfaceRef variant wraps a value that satisfies the interface.
    let Value::Variant {
        variant, payload, ..
    } = val("wrapped")
    else {
        panic!("wrapped should be a variant");
    };
    assert_eq!(variant, "Wrapped");
    let VariantPayload::Positional(inner) = payload else {
        panic!("Wrapped is positional")
    };
    let Value::Variant { variant, .. } = &**inner else {
        panic!("inner should be a variant")
    };
    assert_eq!(variant, "AsValue");
}

#[test]
fn value_of_wrong_union_is_schema_violation() {
    let src = r#"
        union Result { Ok { value: i64 } }
        union Maybe  { Some { value: i64 } Nothing none }
        @document type Doc { r: Result }
        r = Maybe::Some { value: 1 }
    "#;
    let doc = Document::open(src, "test").unwrap();
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::VariantUnionMismatch,
                ..
            }
        )),
        "expected VariantUnionMismatch, got: {errs:#?}"
    );
}

#[test]
fn union_extends_cycle_is_caught() {
    let src = r#"
        union A extends B { Foo {x: i64} }
        union B extends A { Bar {y: utf8} }
    "#;
    let doc = Document::open(src, "test").unwrap();
    let errs = doc.schema_errors();
    assert!(
        errs.iter()
            .any(|e| matches!(e, EvalError::UnionCycle { .. })),
        "expected UnionCycle, got: {errs:#?}"
    );
}

#[test]
fn duplicate_variant_across_extends_is_caught() {
    let src = r#"
        union A { Foo {x: i64} }
        union B extends A { Foo {y: utf8} }
    "#;
    let doc = Document::open(src, "test").unwrap();
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::DuplicateVariant,
                ..
            }
        )),
        "expected DuplicateVariant, got: {errs:#?}"
    );
}

#[test]
fn variant_shape_collision_is_caught() {
    let src = r#"
        union U { Foo {x: i64} Bar {x: i64} }
    "#;
    let doc = Document::open(src, "test").unwrap();
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::VariantShapeCollision,
                ..
            }
        )),
        "expected VariantShapeCollision, got: {errs:#?}"
    );
}

#[test]
fn branching_fixture_covers_if_match_and_variants() {
    let path = examples_dir().join("branching.wcl");
    let doc = Document::from_file(&path).expect("branching fixture parses");

    let val = |name: &str| doc.field(name).unwrap().value().unwrap();

    assert_eq!(val("cmp_lt"), &Value::Symbol("less".into()));
    assert_eq!(val("cmp_cat"), &Value::Symbol("zero".into()));
    assert_eq!(val("light"), &Value::Symbol("slow".into()));
    assert_eq!(val("sign"), &Value::Symbol("neg".into()));
    assert_eq!(val("poly_n"), &Value::I64(7));
    assert_eq!(val("empty_ok"), &Value::Bool(true));
    assert_eq!(val("c1_area"), &Value::F64(75.0));
    assert_eq!(val("whole_r"), &Value::F64(5.0));
    assert_eq!(val("if_let_r"), &Value::F64(5.0));
    assert_eq!(val("if_let_no"), &Value::Symbol("something_else".into()));
    assert_eq!(val("tag_for"), &Value::Symbol("circle".into()));

    // Variant construction shapes round-trip through evaluation.
    let Value::Variant {
        variant, payload, ..
    } = val("c1")
    else {
        panic!("c1 should be a variant");
    };
    assert_eq!(variant, "Circle");
    let VariantPayload::Record(map) = payload else {
        panic!("Circle has a record body")
    };
    assert_eq!(map.get("radius"), Some(&Value::F64(5.0)));
    assert_eq!(map.get("stroke"), Some(&Value::F64(0.5)));

    let Value::Variant {
        variant, payload, ..
    } = val("poly")
    else {
        panic!("poly should be a variant");
    };
    assert_eq!(variant, "Polygon");
    let VariantPayload::Positional(inner) = payload else {
        panic!("Polygon is positional")
    };
    assert_eq!(**inner, Value::I64(7));

    let Value::Variant {
        variant, payload, ..
    } = val("empty")
    else {
        panic!("empty should be a variant");
    };
    assert_eq!(variant, "Empty");
    assert!(matches!(payload, VariantPayload::Unit));

    // `whole` uses an @-binding to return the full Circle variant.
    let Value::Variant { variant, .. } = val("whole") else {
        panic!("whole should be a variant");
    };
    assert_eq!(variant, "Circle");
}

#[test]
fn error_builtin_raises_user_error() {
    let src = r#"
        @schemaless boom = error("oops")
    "#;
    let doc = Document::open(src, "test").unwrap();
    let err = doc.field("boom").unwrap().value().unwrap_err();
    let EvalError::UserError { message, .. } = err else {
        panic!("expected UserError, got {err:?}");
    };
    assert_eq!(message, "oops");
}

#[test]
fn match_without_wildcard_is_parse_error() {
    let src = r#"
        @schemaless x = match :red { :red => 1, :green => 2 }
    "#;
    let err = Document::open(src, "test").unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(
        rendered.contains("wildcard"),
        "expected wildcard error, got: {rendered}"
    );
}

#[test]
fn profile_records_call_tree_for_map_over_user_fn() {
    use std::time::Duration;

    let path = examples_dir().join("builtins_collections.wcl");
    let doc = Document::from_file_profiled(&path).expect("profiled fixture parses");
    let _ = doc
        .field("doubled")
        .unwrap()
        .value()
        .expect("force doubled");

    let p = doc.profile().expect("profile enabled");
    let doubled = p
        .root()
        .children
        .get(&ProfileKey::Field {
            path: "doubled".into(),
        })
        .expect("doubled was profiled");
    assert_eq!(doubled.count, 1);
    assert!(doubled.total > Duration::ZERO);

    let map_node = doubled
        .children
        .get(&ProfileKey::Builtin { name: "map".into() })
        .expect("map call recorded under doubled");
    assert_eq!(map_node.count, 1);

    let fn_node = map_node
        .children
        .get(&ProfileKey::UserFn {
            name: String::new(),
        })
        .expect("user fn invocations recorded under map");
    assert_eq!(fn_node.count, 4, "map ran the callback once per element");
    assert!(fn_node.min <= fn_node.max);
}

#[test]
fn profile_is_none_for_unprofiled_document() {
    let path = examples_dir().join("builtins_collections.wcl");
    let doc = Document::from_file(&path).expect("fixture parses");
    let _ = doc.field("doubled").unwrap().value().unwrap();
    assert!(doc.profile().is_none());
}

#[test]
fn data_ref_walks_data_access_fixture() {
    let path = examples_dir().join("data_access.wcl");
    let doc = Document::from_file(&path).expect("data_access fixture parses");

    // Top-level field via dotted-path API.
    assert_eq!(doc.get("port").unwrap().value().unwrap(), Value::I64(8080));
    assert_eq!(
        doc.get("name").unwrap().value().unwrap(),
        Value::Utf8("alpha".into())
    );

    // Nested block path.
    assert_eq!(
        doc.get("service.port").unwrap().value().unwrap(),
        Value::I64(9090)
    );
    assert_eq!(
        doc.get("service.region").unwrap().value().unwrap(),
        Value::Utf8("us-east-1".into())
    );

    // Doubly-nested.
    assert_eq!(
        doc.get("service.metadata.owner").unwrap().value().unwrap(),
        Value::Utf8("platform-team".into())
    );
    assert_eq!(
        doc.get("service.metadata.tier").unwrap().value().unwrap(),
        Value::Symbol("gold".into())
    );

    // Missing segment yields None.
    assert!(doc.get("service.does_not_exist").is_none());
    assert!(doc.get("not_a_thing").is_none());

    // Intermediate node isn't a leaf.
    let svc = doc.get("service").unwrap();
    assert_eq!(svc.kind(), "block");
    let err = svc.value().unwrap_err();
    assert!(matches!(err, EvalError::NotALeaf { .. }));

    // Schema decl reachable via the same API.
    let user = doc.get("User").unwrap();
    assert_eq!(user.kind(), "type");
    let name_field = doc.get("User.name").unwrap();
    assert_eq!(name_field.kind(), "type_field");
}

#[test]
fn imports_example_resolves_paths() {
    let path = examples_dir().join("imports").join("main.wcl");
    let doc = Document::from_file(&path).expect("imports/main.wcl parses");

    // Top-level eager import: `brand` lives under namespace `shared`,
    // so it's only reachable via the FQN.
    assert_eq!(
        doc.get("shared.brand").unwrap().value().unwrap(),
        Value::Utf8("wcl".into())
    );

    // The imported type is reachable through the unified index.
    let color = doc.type_decl("shared.Color").expect("shared.Color");
    assert_eq!(color.name(), "Color");

    // Block-level lazy import: `service.region` is in
    // `web-defaults.wcl`, loaded only when we touch the block.
    assert_eq!(
        doc.get("service.region").unwrap().value().unwrap(),
        Value::Utf8("us-east-1".into())
    );
    assert_eq!(
        doc.get("service.tier").unwrap().value().unwrap(),
        Value::Symbol("gold".into())
    );

    // The importer's own field still wins inside the block.
    assert_eq!(
        doc.get("service.port").unwrap().value().unwrap(),
        Value::I64(9090)
    );
}

#[test]
fn import_requires_file_path_open_string_fails() {
    let err = Document::open(r#"import "./missing.wcl""#, "test").unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(
        rendered.contains("base directory") || rendered.contains("failed to import"),
        "rendered: {rendered}"
    );
}

#[test]
fn top_level_import_cycle_detected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = dir.path().join("a.wcl");
    let b = dir.path().join("b.wcl");
    std::fs::write(&a, r#"import "./b.wcl""#).unwrap();
    std::fs::write(&b, r#"import "./a.wcl""#).unwrap();

    let err = Document::from_file(&a).unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("cycle"), "rendered: {rendered}");
}

#[test]
fn system_import_round_trips() {
    // Both import forms survive a print → re-parse → print cycle, with
    // angle brackets for system imports and quotes for disk imports.
    let src = "import <wdoc/core.wcl>\nimport \"./local.wcl\"\n";
    let ast = parse_for_edit(src, "test".to_string()).expect("parse");
    let printed = format::to_source(&ast);
    assert!(printed.contains("import <wdoc/core.wcl>"), "{printed}");
    assert!(printed.contains("import \"./local.wcl\""), "{printed}");
    let ast2 = parse_for_edit(&printed, "test".to_string()).expect("re-parse");
    assert_eq!(printed, format::to_source(&ast2), "printer not idempotent");
}

#[test]
fn registry_resolves_system_import() {
    let mut reg = Registry::new();
    reg.register("wdoc/prelude.wcl", "@schemaless\nanswer = 42\n");
    let loader = reg.loader(disk_loader());
    let doc = Document::open_at_with_loader(
        "import <wdoc/prelude.wcl>\n",
        "test",
        None,
        &Environment::new(),
        loader,
    )
    .expect("open with registry loader");
    assert_eq!(doc.get("answer").unwrap().value().unwrap(), Value::I64(42));
}

#[test]
fn registry_system_imports_are_importer_relative() {
    // The user imports the full path; the prelude reaches a sibling with
    // a relative path that resolves within the same registry directory.
    let mut reg = Registry::new();
    reg.register("wdoc/prelude.wcl", "import <core.wcl>\n");
    reg.register("wdoc/core.wcl", "@schemaless\ncore_val = 7\n");
    let loader = reg.loader(disk_loader());
    let doc = Document::open_at_with_loader(
        "import <wdoc/prelude.wcl>\n",
        "test",
        None,
        &Environment::new(),
        loader,
    )
    .expect("open with registry loader");
    assert_eq!(doc.get("core_val").unwrap().value().unwrap(), Value::I64(7));
}

#[test]
fn registry_miss_is_reported() {
    let loader = Registry::new().loader(disk_loader());
    let err = Document::open_at_with_loader(
        "import <wdoc/missing.wcl>\n",
        "test",
        None,
        &Environment::new(),
        loader,
    )
    .unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    // miette word-wraps the message, so assert on an unbroken fragment.
    assert!(
        rendered.contains("no system import"),
        "rendered: {rendered}"
    );
}

#[test]
fn system_import_cycle_detected() {
    let mut reg = Registry::new();
    reg.register("a.wcl", "import <b.wcl>\n");
    reg.register("b.wcl", "import <a.wcl>\n");
    let loader = reg.loader(disk_loader());
    let err = Document::open_at_with_loader(
        "import <a.wcl>\n",
        "test",
        None,
        &Environment::new(),
        loader,
    )
    .unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("cycle"), "rendered: {rendered}");
}

#[test]
fn lazy_import_not_loaded_until_block_accessed() {
    // Block-level import to a non-existent file shouldn't surface on
    // `Document::open` / `from_file` — only on first read of the block.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    std::fs::write(
        &main,
        r#"
service "web" {
  import "./does-not-exist.wcl"
}
"#,
    )
    .unwrap();
    let doc = Document::from_file(&main).expect("opens despite lazy bad import");

    // `kind` and `span` don't read items — load not triggered yet.
    let svc = doc.block("service").unwrap();
    assert_eq!(svc.kind(), "service");

    // Reading items triggers the failed load.
    let res = svc.fields().count();
    let _ = res; // iteration completes; the import error is silently dropped here
    let errs = svc.import_errors();
    assert!(!errs.is_empty(), "expected at least one import error");
    assert!(matches!(errs[0], EvalError::ImportFailed { .. }));
}

#[test]
fn transitive_top_level_imports_form_a_chain() {
    // a -> b -> c, where c declares a type that a's field consumes.
    let dir = tempfile::tempdir().expect("tempdir");
    let a = dir.path().join("a.wcl");
    let b = dir.path().join("b.wcl");
    let c = dir.path().join("c.wcl");
    std::fs::write(
        &c,
        r#"
type Inner { name: utf8 }
"#,
    )
    .unwrap();
    std::fs::write(
        &b,
        r#"
import "./c.wcl"
type Middle { inner: Inner }
"#,
    )
    .unwrap();
    std::fs::write(
        &a,
        r#"
import "./b.wcl"
@schemaless
name = "alpha"
"#,
    )
    .unwrap();

    let doc = Document::from_file(&a).expect("transitive imports load");
    // Both types should be visible after the transitive chain expands.
    assert!(doc.type_decl("Inner").is_some(), "Inner from c.wcl");
    assert!(doc.type_decl("Middle").is_some(), "Middle from b.wcl");
}

#[test]
fn lazy_block_import_brings_in_declared_items_on_first_read() {
    // Lazy (block-level) imports defer resolution until the host
    // block's items are read. Once read, items declared in the
    // imported file become visible *inside* the block's scope.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let extras = dir.path().join("extras.wcl");
    std::fs::write(
        &extras,
        r#"
@schemaless
extra_port = 9090
"#,
    )
    .unwrap();
    std::fs::write(
        &main,
        r#"
service "web" {
  import "./extras.wcl"
}
"#,
    )
    .unwrap();

    let doc = Document::from_file(&main).expect("opens");
    let svc = doc.block("service").unwrap();
    let names: Vec<String> = svc.fields().map(|f| f.name().to_string()).collect();
    assert!(
        names.iter().any(|n| n == "extra_port"),
        "extras.wcl's field should be visible inside the host block; got {names:?}",
    );
    assert!(
        svc.import_errors().is_empty(),
        "no import errors expected for a well-formed lazy import",
    );
}

#[test]
fn lazy_block_import_nests_child_instances() {
    // An `import` inside a block splices the imported file's top-level
    // block instances into the enclosing block as children, subject to
    // the same `@children` projection as if written inline.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let kids = dir.path().join("kids.wcl");
    std::fs::write(
        &kids,
        r#"
kid "a" {}
kid "b" {}
"#,
    )
    .unwrap();
    std::fs::write(
        &main,
        r#"
@block("kid")    type Kid    { @inline(0) id: utf8 }
@block("parent") type Parent { @inline(0) id: utf8  @children("kid") kids: list<Kid> }
@document type Doc { @children("parent") parents: list<Parent> }

parent "p" {
  import "./kids.wcl"
}
"#,
    )
    .unwrap();

    let doc = Document::from_file(&main).expect("opens");
    assert!(
        doc.schema_errors().is_empty(),
        "expected no schema errors, got: {:#?}",
        doc.schema_errors(),
    );
    let parent = doc.block("parent").expect("parent block");
    // The fixed `@children` projection (children_projection) must surface
    // the imported `child` instances, not just `blocks()`.
    let kids = parent.typed_field("kids").expect("kids slot");
    assert_eq!(
        kids.len(),
        Some(2),
        "imported child instances should nest as the parent's kids",
    );
}

#[test]
fn lazy_block_import_connection_projects_into_block() {
    // A connection statement in an in-block import is block-scoped: it
    // projects into the enclosing block's `@connections` slot, and its
    // operands resolve against both the imported fragment (sibling) and
    // the enclosing block's own children (ancestor).
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let wires = dir.path().join("wires.wcl");
    std::fs::write(
        &wires,
        r#"
shape { id = a }
a -> root_shape
"#,
    )
    .unwrap();
    std::fs::write(
        &main,
        r#"
symbol_set Kind { default }
connection Edge: Shape -> Shape : Kind
@block("shape") type Shape { id: identifier }
@block("group") type Group {
  @children("shape") shapes: list<Shape>
  @connections(Edge) edges: list<Edge>
}
@document type Site { @children("group") groups: list<Group> }

group {
  shape { id = root_shape }
  import "./wires.wcl"
}
"#,
    )
    .unwrap();

    let doc = Document::from_file(&main).expect("opens");
    assert!(
        doc.schema_errors().is_empty(),
        "expected no schema errors, got: {:#?}",
        doc.schema_errors(),
    );
    let group = doc.block("group").expect("group block");
    let edges = group
        .typed_field("edges")
        .expect("edges slot")
        .value()
        .expect("eval edges");
    let Value::List(items) = edges else {
        panic!("edges should be a list, got {edges:?}");
    };
    assert_eq!(items.len(), 1, "imported connection should project");
    let Value::Record { fields, .. } = &items[0] else {
        panic!("expected record");
    };
    assert_eq!(fields.get("source"), Some(&Value::Identifier("a".into())));
    assert_eq!(
        fields.get("destination"),
        Some(&Value::Identifier("root_shape".into())),
    );
}

#[test]
fn document_root_connection_resolves_in_block_imported_endpoint() {
    // A connection at the document root can name a block that was nested
    // into the tree via an in-block import: operand resolution descends
    // into the lazy import.
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.wcl");
    let shapes = dir.path().join("shapes.wcl");
    std::fs::write(
        &shapes,
        r#"
shape { id = a }
shape { id = b }
"#,
    )
    .unwrap();
    std::fs::write(
        &main,
        r#"
symbol_set Kind { default }
connection Edge: Shape -> Shape : Kind
@block("shape") type Shape { id: identifier }
@block("group") type Group { @children("shape") shapes: list<Shape> }
@document type Site {
  @children("group") groups: list<Group>
  @connections(Edge) edges: list<Edge>
}

group {
  import "./shapes.wcl"
}

a -> b
"#,
    )
    .unwrap();

    let doc = Document::from_file(&main).expect("opens");
    assert!(
        doc.schema_errors().is_empty(),
        "expected no schema errors, got: {:#?}",
        doc.schema_errors(),
    );
    let edges = doc.get("edges").expect("edges").value().expect("eval");
    let Value::List(items) = edges else {
        panic!("edges should be a list, got {edges:?}");
    };
    assert_eq!(
        items.len(),
        1,
        "root connection should resolve imported endpoints",
    );
}

#[test]
fn nested_blocks_example_resolves_via_schema() {
    let path = examples_dir().join("nested_blocks.wcl");
    let doc = Document::from_file(&path).expect("nested_blocks fixture parses");

    // Singleton @child — `config` field resolves to a Block.
    let cfg = doc.get("service.config").expect("service.config");
    assert_eq!(cfg.kind(), "block");
    assert_eq!(
        cfg.get("region").unwrap().value().unwrap(),
        Value::Utf8("us-east-1".into())
    );
    assert_eq!(
        cfg.get("tier").unwrap().value().unwrap(),
        Value::Symbol("gold".into())
    );

    // Plural @children — `routes` field resolves to a BlockList.
    let routes = doc.get("service.routes").expect("service.routes");
    assert_eq!(routes.kind(), "block_list");
    assert_eq!(routes.len(), Some(2));
    let mut methods: Vec<Value> = routes
        .children()
        .map(|r| r.get("method").unwrap().value().unwrap())
        .collect();
    methods.sort_by_key(|v| match v {
        Value::Utf8(s) => s.clone(),
        _ => String::new(),
    });
    assert_eq!(methods, vec![Value::Utf8("GET".into()); 2]);

    // No schema violations on the well-formed fixture.
    let svc = doc.block("service").expect("service block");
    assert!(svc.schema_errors().is_empty());
}

#[test]
fn nested_blocks_fixture_top_level_list_field() {
    let path = examples_dir().join("nested_blocks.wcl");
    let doc = Document::from_file(&path).expect("nested_blocks fixture parses");

    let ports = doc.get("ports").expect("ports").value().unwrap();
    assert_eq!(
        ports,
        Value::List(std::sync::Arc::new(vec![
            Value::I64(80),
            Value::I64(443),
            Value::I64(8080)
        ]))
    );
}

#[test]
fn nested_blocks_fixture_required_children_clean() {
    let path = examples_dir().join("nested_blocks.wcl");
    let doc = Document::from_file(&path).expect("fixture parses");
    let svc = doc.block("service").expect("service block");
    // required_children = ["config"] is satisfied by the fixture's
    // `config { ... }` child.
    assert!(svc.schema_errors().is_empty(), "{:?}", svc.schema_errors());
}

#[test]
fn nested_blocks_missing_required_surfaces_error() {
    let doc = Document::open(
        r#"
        @block("service", required_children = ["config"])
        type Service {
          @child("config") config: Config?
        }
        @block("config") type Config {}
        service web {}
        "#,
        "test",
    )
    .expect("parses (validation is lazy)");
    let svc = doc.block("service").unwrap();
    let errs = svc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::MissingRequired,
                ..
            }
        )),
        "expected MissingRequired, got {errs:?}"
    );
}

#[test]
fn tables_example_resolves_via_schema() {
    let path = examples_dir().join("tables.wcl");
    let doc = Document::from_file(&path).expect("fixture parses");
    let users = doc.get("db.users").expect("db.users present");
    assert_eq!(users.kind(), "table");
    assert_eq!(users.row_count(), Some(3));

    let alice = users.row(0).unwrap();
    let alice_block = alice.as_block().expect("row is a block");
    let labels = alice_block.labels().unwrap();
    assert_eq!(
        labels,
        vec![
            Value::Utf8("alice".into()),
            Value::I64(30),
            Value::Bool(true)
        ]
    );

    let names = users.column("name").unwrap();
    assert_eq!(
        names,
        vec![
            Value::Utf8("alice".into()),
            Value::Utf8("bob".into()),
            Value::Utf8("cara".into()),
        ]
    );
    let ages = users.column("age").unwrap();
    assert_eq!(ages, vec![Value::I64(30), Value::I64(25), Value::I64(42)]);
}

#[test]
fn references_example_resolves_via_scope() {
    let path = examples_dir().join("references.wcl");
    let doc = Document::from_file(&path).expect("fixture parses");

    // `active = users.alice` → row block whose first label is "alice".
    let active_ref = doc
        .get("db.active")
        .expect("db.active present")
        .reference()
        .expect("&User field exposes reference()")
        .expect("active resolves");
    let labels = active_ref.as_block().unwrap().labels().unwrap();
    assert_eq!(labels.first(), Some(&Value::Utf8("alice".into())));

    // `pinned = users.bob` → row whose age column is 25.
    let pinned_ref = doc
        .get("db.pinned")
        .expect("db.pinned present")
        .reference()
        .expect("&User field exposes reference()")
        .expect("pinned resolves");
    let age = pinned_ref
        .as_block()
        .unwrap()
        .labels()
        .unwrap()
        .get(1)
        .cloned()
        .unwrap();
    assert_eq!(age, Value::I64(25));
}

#[test]
fn interfaces_example_parses_clean() {
    let path = examples_dir().join("interfaces.wcl");
    let doc = Document::from_file(&path).expect("fixture parses");
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    let drawable = doc
        .interface("Drawable")
        .expect("Drawable interface present");
    let field_names: Vec<_> = drawable.fields().map(|f| f.name().to_string()).collect();
    assert_eq!(field_names, vec!["bounds", "z_index"]);

    // `Pet extends Dog extends Animal` — effective_fields includes
    // every ancestor field in source order.
    let pet = doc.type_decl("Pet").expect("Pet type present");
    let pet_fields: Vec<_> = pet
        .effective_fields()
        .into_iter()
        .map(|f| f.name().to_string())
        .collect();
    assert_eq!(
        pet_fields,
        vec!["name", "age", "breed", "bounds", "z_index"]
    );
    assert!(pet.is_descendant_of("Animal"));
    assert!(pet.is_descendant_of("Dog"));
}

// -----------------------------------------------------------------------------
// Negative fixtures: every file under examples/errors/ should fail (either at
// open time or via schema_errors after open). The asserted substring is a
// human-readable summary of *which* error path the fixture exercises — it's
// what would degrade silently if the diagnostic copy ever drifted.
// -----------------------------------------------------------------------------

fn errors_dir() -> PathBuf {
    examples_dir().join("errors")
}

/// Try `Document::from_file` for syntax-style errors. Some fixtures parse
/// successfully but emit schema violations; for those, the caller falls back
/// to `schema_errors`. Returns the rendered error string for substring checks.
fn open_or_schema_error(name: &str) -> String {
    let path = errors_dir().join(name);
    match Document::from_file(&path) {
        Err(e) => format!("{e:?}"),
        Ok(doc) => doc
            .schema_errors()
            .iter()
            .map(|e| format!("{e:?}"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[test]
fn syntax_unclosed_brace_fixture_reports_eof_inside_block() {
    let rendered = open_or_schema_error("syntax_unclosed_brace.wcl");
    assert!(
        rendered.contains("unexpected end of file"),
        "expected EOF-inside-block error, got: {rendered}",
    );
}

#[test]
fn unknown_type_fixture_reports_unknown_type() {
    let rendered = open_or_schema_error("unknown_type.wcl");
    assert!(
        rendered.contains("unknown type") && rendered.contains("Missing"),
        "expected unknown-type error, got: {rendered}",
    );
}

#[test]
fn cyclic_extends_fixture_reports_cycle() {
    let rendered = open_or_schema_error("cyclic_extends.wcl");
    assert!(
        rendered.contains("cyclic extends"),
        "expected cyclic extends error, got: {rendered}",
    );
}

#[test]
fn extends_self_fixture_reports_self_extension() {
    let rendered = open_or_schema_error("extends_self.wcl");
    assert!(
        rendered.contains("cannot extend itself"),
        "expected self-extension error, got: {rendered}",
    );
}

#[test]
fn duplicate_namespace_fixture_reports_duplicate() {
    let rendered = open_or_schema_error("duplicate_namespace.wcl");
    assert!(
        rendered.contains("duplicate namespace"),
        "expected duplicate namespace error, got: {rendered}",
    );
}

#[test]
fn unknown_field_fixture_reports_unknown_field() {
    let rendered = open_or_schema_error("unknown_field.wcl");
    assert!(
        rendered.contains("not declared by schema") && rendered.contains("unexpected"),
        "expected unknown-field violation, got: {rendered}",
    );
}

#[test]
fn disallowed_child_fixture_reports_disallowed_child() {
    let rendered = open_or_schema_error("disallowed_child.wcl");
    assert!(
        rendered.contains("not allowed inside") && rendered.contains("route"),
        "expected disallowed-child violation, got: {rendered}",
    );
}

#[test]
fn missing_required_fixture_reports_missing_required_child() {
    let rendered = open_or_schema_error("missing_required.wcl");
    assert!(
        rendered.contains("missing required child"),
        "expected missing-required-child violation, got: {rendered}",
    );
}

// -----------------------------------------------------------------------------
// Parser robustness: feed a deterministic stream of "random-ish" inputs derived
// from real fixture corpora into Document::open and assert nothing panics. Two
// strategies are used: (1) bit-flip mutations of valid fixtures, and (2) raw
// printable-ASCII junk. Each input either parses or returns an `Err`; either
// outcome is acceptable, but a panic / abort is a regression.
// -----------------------------------------------------------------------------

/// Linear-congruential PRNG so the test is deterministic on every machine.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next() as usize) % n.max(1)
    }
}

#[test]
fn parser_does_not_panic_on_bitflipped_fixtures() {
    let corpus: Vec<String> = [
        "basic.wcl",
        "types.wcl",
        "functions.wcl",
        "nested_blocks.wcl",
        "tables.wcl",
        "references.wcl",
        "interfaces.wcl",
    ]
    .into_iter()
    .filter_map(|name| std::fs::read_to_string(examples_dir().join(name)).ok())
    .collect();
    assert!(!corpus.is_empty(), "no corpus files loaded");

    let mut rng = Lcg(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..200 {
        let src = &corpus[rng.range(corpus.len())];
        let mut bytes = src.as_bytes().to_vec();
        if bytes.is_empty() {
            continue;
        }
        // Apply 1..=4 single-byte mutations at random offsets.
        for _ in 0..(1 + rng.range(4)) {
            let idx = rng.range(bytes.len());
            bytes[idx] = (rng.next() & 0x7f) as u8;
        }
        // Skip invalid UTF-8 — Document::open requires &str. The lexer/parser
        // robustness story is "valid UTF-8 in, parse error out".
        let Ok(s) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let _ = Document::open(s, "fuzz");
    }
}

#[test]
fn parser_does_not_panic_on_printable_junk() {
    let mut rng = Lcg(0x1234_5678_9ABC_DEF0);
    let charset: &[u8] = b"abcdefghij0123 \t\n{}[]()=,.:;|@#&*+-/<>?!\"'\\_TypeBlockFnList";
    for _ in 0..200 {
        let len = 1 + rng.range(256);
        let bytes: Vec<u8> = (0..len)
            .map(|_| charset[rng.range(charset.len())])
            .collect();
        // charset is ASCII-only, so this is always valid UTF-8.
        let s = std::str::from_utf8(&bytes).expect("ASCII-only charset");
        let _ = Document::open(s, "fuzz");
    }
}

// ---- connections -----------------------------------------------------

#[test]
fn connections_fixture_parses_and_validates() {
    let doc =
        Document::from_file(&examples_dir().join("connections.wcl")).expect("connections.wcl");
    assert!(
        doc.schema_errors().is_empty(),
        "expected no schema errors, got: {:#?}",
        doc.schema_errors()
    );

    let decls: Vec<_> = doc.connection_decls().collect();
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0].name(), "DependsOn");

    let stmts: Vec<_> = doc.connection_stmts().collect();
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[0].source(), "web");
    assert_eq!(stmts[0].destination(), "db");
    assert_eq!(stmts[0].kind(), None);
    assert_eq!(stmts[1].kind(), Some("uses"));
}

#[test]
fn connections_field_decomposes_into_records() {
    let doc =
        Document::from_file(&examples_dir().join("connections.wcl")).expect("connections.wcl");
    let deps = doc
        .get("deps")
        .expect("deps field present")
        .value()
        .unwrap();
    let Value::List(items) = deps else {
        panic!("deps should be a list, got {deps:?}");
    };
    assert_eq!(items.len(), 2);
    for item in items.iter() {
        let Value::Record { ty, fields } = item else {
            panic!("expected Value::Record, got {item:?}");
        };
        assert_eq!(ty, &vec!["DependsOn".to_string()]);
        assert!(fields.contains_key("source"));
        assert!(fields.contains_key("destination"));
        assert!(fields.contains_key("kind"));
    }
    // Defaulted kind: first symbol in EdgeKind is `uses`.
    let Value::Record { fields, .. } = &items[0] else {
        unreachable!()
    };
    assert_eq!(fields.get("source"), Some(&Value::Utf8("web".into())));
    assert_eq!(fields.get("destination"), Some(&Value::Utf8("db".into())));
    assert_eq!(fields.get("kind"), Some(&Value::Symbol("uses".into())));
}

#[test]
fn connection_record_member_access_works() {
    let doc =
        Document::from_file(&examples_dir().join("connections.wcl")).expect("connections.wcl");
    let dest = doc.get("deps.destination").and_then(|d| d.value().ok());
    // `deps` is a list; member access on a list doesn't descend, so
    // this should be None — we use index-free addressing on records
    // by accessing the field directly when working with a single
    // record value (covered below).
    assert!(dest.is_none(), "list-level member access should not work");
}

#[test]
fn connection_unknown_operand_fixture_reports_violation() {
    let rendered = open_or_schema_error("connection_unknown_operand.wcl");
    assert!(
        rendered.contains("does not name a block in scope"),
        "expected unknown operand error, got: {rendered}",
    );
}

#[test]
fn connection_type_mismatch_fixture_reports_violation() {
    let rendered = open_or_schema_error("connection_type_mismatch.wcl");
    assert!(
        rendered.contains("no connection schema accepts"),
        "expected type-mismatch error, got: {rendered}",
    );
}

#[test]
fn connection_unknown_kind_fixture_reports_violation() {
    let rendered = open_or_schema_error("connection_unknown_kind.wcl");
    assert!(
        rendered.contains("is not a member of"),
        "expected unknown-kind error, got: {rendered}",
    );
}

#[test]
fn connection_resolves_id_field_inside_nested_blocks() {
    // Connection operands are identified by an `id` field (not just
    // first-label), and the resolver recursively descends into
    // nested blocks. The two `Shape` blocks live inside two
    // `Group` blocks; the connection statement at the document root
    // must still find them.
    let doc = open(
        r#"
        symbol_set Kind { default }
        connection Edge: Shape -> Shape : Kind

        @block("group")
        type Group {
          @children("shape") shapes: list<Shape>
        }
        @block("shape")
        type Shape {
          id: identifier
        }

        @document
        type Site {
          @children("group") groups: list<Group>
          @connections(Edge) edges: list<Edge>
        }

        group {
          shape { id = a }
        }
        group {
          shape { id = b }
        }

        a -> b
        "#,
    );
    assert!(
        doc.schema_errors().is_empty(),
        "expected no schema errors, got: {:#?}",
        doc.schema_errors(),
    );
    let edges = doc
        .get("edges")
        .expect("edges field")
        .value()
        .expect("eval edges");
    let Value::List(items) = edges else {
        panic!("edges should be a list, got {edges:?}");
    };
    assert_eq!(items.len(), 1);
    let Value::Record { fields, .. } = &items[0] else {
        panic!("expected record");
    };
    assert_eq!(fields.get("source"), Some(&Value::Identifier("a".into())));
    assert_eq!(
        fields.get("destination"),
        Some(&Value::Identifier("b".into())),
    );
}

#[test]
fn connection_accepts_interface_typed_endpoints() {
    // A connection schema declared against an interface (here as
    // `&Shape`, since interfaces in non-reference position are
    // rejected) admits any block whose type implements the
    // interface.
    let doc = open(
        r#"
        symbol_set Kind { default }
        connection Edge: &Shape -> &Shape : Kind

        interface Shape {
          id: identifier?
        }
        @block("rect_shape")
        type RectShape extends Shape {
          id: identifier?
        }
        @block("circle_shape")
        type CircleShape extends Shape {
          id: identifier?
        }

        @document
        type Site {
          @children("rect_shape")   rects:   list<RectShape>
          @children("circle_shape") circles: list<CircleShape>
          @connections(Edge)        edges:   list<Edge>
        }

        rect_shape { id = r1 }
        circle_shape { id = c1 }
        r1 -> c1
        "#,
    );
    assert!(
        doc.schema_errors().is_empty(),
        "expected no schema errors, got: {:#?}",
        doc.schema_errors(),
    );
    let edges = doc
        .get("edges")
        .expect("edges field")
        .value()
        .expect("eval edges");
    let Value::List(items) = edges else {
        panic!("edges should be a list, got {edges:?}");
    };
    assert_eq!(items.len(), 1);
}

// ---- heredocs --------------------------------------------------------

#[test]
fn heredoc_fixture_field_values_match_expected() {
    let doc = Document::from_file(&examples_dir().join("heredoc.wcl")).expect("heredoc.wcl");
    assert_eq!(
        doc.field("message").unwrap().value().unwrap(),
        &Value::Utf8("Hello, world.\n\nGoodbye, world.\n".into()),
    );
    assert_eq!(
        doc.field("plain").unwrap().value().unwrap(),
        &Value::Ascii("plain ASCII only\nspans two lines\n".into()),
    );
    let note = doc.block("note").expect("note block");
    assert_eq!(
        note.field("body").unwrap().value().unwrap(),
        &Value::Utf8("line one\nline two\n".into()),
    );
}

#[test]
fn heredoc_unterminated_fixture_reports_error() {
    let rendered = open_or_schema_error("heredoc_unterminated.wcl");
    assert!(
        rendered.contains("unterminated heredoc"),
        "expected unterminated heredoc error, got: {rendered}",
    );
}

#[test]
fn heredoc_non_ascii_fixture_reports_error() {
    let rendered = open_or_schema_error("heredoc_non_ascii.wcl");
    assert!(
        rendered.contains("non-ASCII"),
        "expected non-ASCII error, got: {rendered}",
    );
}

// ---- interpolation ---------------------------------------------------

#[test]
fn interpolation_fixture_evaluates_each_form() {
    let doc =
        Document::from_file(&examples_dir().join("interpolation.wcl")).expect("interpolation.wcl");
    assert_eq!(
        doc.field("greeting").unwrap().value().unwrap(),
        &Value::Utf8("Hello, alice! You have 4 item(s).".into()),
    );
    assert_eq!(
        doc.field("ascii_id").unwrap().value().unwrap(),
        &Value::Ascii("id=alice".into()),
    );
    assert_eq!(
        doc.field("body").unwrap().value().unwrap(),
        &Value::Utf8("name=alice\ncount=30\n".into()),
    );
}

#[test]
fn interpolation_unterminated_slot_reports_error() {
    let rendered = open_or_schema_error("interp_unterminated_slot.wcl");
    assert!(
        rendered.contains("interpolation slot") || rendered.contains("multiple lines"),
        "expected slot-spans-lines error, got: {rendered}",
    );
}

#[test]
fn interpolation_ascii_violation_reports_at_eval() {
    let doc = Document::from_file(&examples_dir().join("errors/interp_ascii_violation.wcl"))
        .expect("file parses");
    let err = doc
        .field("oops")
        .expect("oops field")
        .value()
        .unwrap_err()
        .clone();
    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("non-ASCII"),
        "expected non-ASCII eval error, got: {rendered}",
    );
}

#[test]
fn parse_for_edit_captures_leading_trivia() {
    // Two items, separated by a blank line; the second item carries a
    // leading comment. The lexer-side trivia harvester should attach
    // both signals to the second item's `leading_trivia` (a single
    // BlankLine marker followed by the LineComment).
    let src = "\
@schemaless first = 1

# explains second
@schemaless second = 2
";
    let ast = parse_for_edit(src, "test").expect("parse");
    assert_eq!(ast.items.len(), 2);

    let first = match &ast.items[0] {
        ast::Item::Field(f) => f,
        other => panic!("expected first item to be a Field, got {other:?}"),
    };
    // First item has no preceding source → empty leading_trivia.
    assert!(
        first.leading_trivia.is_empty(),
        "{:?}",
        first.leading_trivia
    );

    let second = match &ast.items[1] {
        ast::Item::Field(f) => f,
        other => panic!("expected second item to be a Field, got {other:?}"),
    };
    assert_eq!(
        second.leading_trivia,
        vec![
            ast::Trivia::BlankLine,
            ast::Trivia::LineComment("explains second".into()),
        ],
    );
}

#[test]
fn parse_for_edit_exposes_mutable_ast() {
    // The edit-path entry point hands back an owned `ast::Source` with
    // public fields. A host can walk and mutate it directly without
    // going through the Document view layer.
    let src = r#"
        @schemaless name  = "alpha"
        @schemaless count = 3
    "#;
    let mut ast = parse_for_edit(src, "test").expect("parse");
    assert_eq!(ast.items.len(), 2);

    // Find the `name` field and overwrite its expression.
    for item in &mut ast.items {
        if let ast::Item::Field(f) = item
            && f.name == "name"
        {
            f.expr = ast::Expr::Utf8("beta".into());
        }
    }

    // Confirm the mutation took.
    let mutated = match &ast.items[0] {
        ast::Item::Field(f) => &f.expr,
        other => panic!("expected Field, got {other:?}"),
    };
    assert!(matches!(mutated, ast::Expr::Utf8(s) if s == "beta"));
}

#[test]
fn edit_path_full_loop_writes_then_reevaluates() {
    // End-to-end exercise of the round-trip API: parse for edit,
    // mutate the AST, write the formatted source to disk, reopen as a
    // Document, and evaluate. This is the loop hosts actually use —
    // earlier tests cover the pieces in isolation.
    let tmp = tempfile::TempDir::new().expect("mkdir tempdir");
    let path = tmp.path().join("doc.wcl");

    // Seed file. `@schemaless` is the only decorator the open-Document
    // path doesn't insist on a schema for, which keeps this test
    // independent of the schema machinery.
    std::fs::write(
        &path,
        "@schemaless greeting = \"hello\"\n@schemaless count = 1\n",
    )
    .expect("seed write");

    // EDIT MODE.
    let mut ast = parse_for_edit(
        &std::fs::read_to_string(&path).expect("read"),
        path.display().to_string(),
    )
    .expect("parse_for_edit");
    for item in &mut ast.items {
        if let ast::Item::Field(f) = item
            && f.name == "greeting"
        {
            f.expr = ast::Expr::Utf8("goodbye".into());
        }
    }
    std::fs::write(&path, wcl_lang::format::to_source(&ast)).expect("write back");

    // EVAL MODE — reopen from disk, assert the mutation is visible
    // through the Document layer.
    let doc = Document::from_file(&path).expect("reopen");
    assert_eq!(
        doc.field("greeting").unwrap().value().unwrap(),
        &Value::Utf8("goodbye".into())
    );
    // The untouched field still reads its original value.
    assert_eq!(doc.field("count").unwrap().value().unwrap(), &Value::I64(1));
}

#[test]
fn field_source_path_attributes_eager_imports() {
    // `examples/imports/main.wcl` does `import "./shared.wcl"`, and
    // shared.wcl declares `brand` under its own `@document` schema.
    // Opening main.wcl and walking to `shared.brand` should report
    // shared.wcl as the home file; a field declared directly in
    // main.wcl reports None.
    let main = examples_dir().join("imports").join("main.wcl");
    let doc = Document::from_file(&main).expect("open imports/main.wcl");

    let imported = doc
        .get("shared.brand")
        .expect("brand resolves through import");
    let imported = imported.as_field().expect("brand is a field");
    let imported_home = imported
        .source_path()
        .expect("brand lives in an imported file, not main");
    assert!(
        imported_home.ends_with("shared.wcl"),
        "brand should be home in shared.wcl, got {}",
        imported_home.display()
    );

    // Compare against a field declared directly in main.wcl. The
    // `service` block lives in main.wcl, so any of its fields should
    // report `None` (= main source).
    let local = doc
        .get("service.port")
        .expect("service.port resolves locally");
    let local = local.as_field().expect("port is a field");
    assert!(
        local.source_path().is_none(),
        "service.port lives in main.wcl, should report None; got {:?}",
        local.source_path()
    );
}

/// Verify the source printer is a fixed point on its own output:
/// `print(parse(print(parse(src)))) == print(parse(src))`. This is
/// strictly stronger than structural-equality of the AST after one
/// round-trip (which fails on byte-spans of reformatted output), and
/// catches both formatting drift and structural information loss.
fn assert_roundtrip(path: &std::path::Path) {
    let src =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let ast1 = parse_for_edit(&src, path.display().to_string())
        .unwrap_or_else(|e| panic!("first parse of {}: {e:?}", path.display()));
    let printed = format::to_source(&ast1);
    let ast2 = parse_for_edit(&printed, path.display().to_string()).unwrap_or_else(|e| {
        panic!(
            "re-parse of printed {} failed: {e:?}\n--- printed ---\n{printed}",
            path.display()
        )
    });
    let printed2 = format::to_source(&ast2);
    assert_eq!(
        printed,
        printed2,
        "printer is not idempotent on {}\n--- first ---\n{printed}\n--- second ---\n{printed2}",
        path.display()
    );
}

#[test]
fn implicit_coercion_int_plus_float() {
    // `1 + 2.0` should evaluate cleanly with promotion.
    let src = "@schemaless mixed = 1 + 2.0\n";
    let doc = Document::open(src, "test").unwrap();
    let v = doc
        .get("mixed")
        .expect("mixed exists")
        .value()
        .expect("eval");
    assert_eq!(v, Value::F64(3.0));
}

#[test]
fn implicit_coercion_signed_plus_unsigned() {
    let src = "@schemaless mixed = 100u32 + 1i64\n";
    let doc = Document::open(src, "test").unwrap();
    let v = doc.get("mixed").unwrap().value().unwrap();
    // Promotion ladder routes mixed-integer pairs through i128.
    assert_eq!(v, Value::I128(101));
}

#[test]
fn implicit_coercion_in_comparison_and_equality() {
    let src = "@schemaless cmp = 1u32 == 1i64\n@schemaless lt = 1i32 < 2.0\n";
    let doc = Document::open(src, "test").unwrap();
    assert_eq!(doc.get("cmp").unwrap().value().unwrap(), Value::Bool(true));
    assert_eq!(doc.get("lt").unwrap().value().unwrap(), Value::Bool(true));
}

#[test]
fn round_trip_all_examples() {
    let dir = examples_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read examples/")
        .filter_map(|r| r.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wcl"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no .wcl examples found in {dir:?}");
    for path in entries {
        assert_roundtrip(&path);
    }
}

#[test]
fn all_symbols_spans_the_import_graph_with_source_paths() {
    let doc = Document::from_file(&examples_dir().join("imports").join("main.wcl"))
        .expect("imports example parses");
    let symbols: Vec<_> = doc.all_symbols().collect();
    // A root-document symbol carries no path.
    assert!(
        symbols
            .iter()
            .any(|(path, rec)| path.is_none() && rec.fqn == "app.Service"),
        "root symbol with no path"
    );
    // An imported symbol carries the declaring file's path.
    let (path, _) = symbols
        .iter()
        .find(|(_, rec)| rec.fqn == "shared.Color")
        .expect("imported shared.Color is visible");
    assert!(
        path.is_some_and(|p| p.ends_with("shared.wcl")),
        "imported symbol names its file, got {path:?}"
    );
}
