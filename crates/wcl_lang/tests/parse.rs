use std::path::PathBuf;

use wcl_lang::{
    Document, Environment, EvalError, ResolvedType, SymbolKind, TypeRef, Value, VariantBodyView,
    from_fn,
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
        &Value::I64(8080)
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
    // Function literals are still inert — `double` is a Value::Function
    // but not callable.
    assert!(matches!(
        doc.field("double").unwrap().value().unwrap(),
        Value::Function(_)
    ));
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
        Value::List(vec![Value::I64(80), Value::I64(443), Value::I64(8080)])
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
