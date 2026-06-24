use super::*;

fn open(src: &str) -> Document {
    // The strict-validation default rejects any top-level field
    // or block without a `@document` schema. Most tests in this
    // module assert parser/eval behaviour, not validation, so
    // we wrap each top-level field/block with `@schemaless`
    // here. Tests that exercise validation use
    // `Document::open(_with)` directly with explicit schemas.
    let lax = laxify_for_tests(src);
    // Use an empty registry so existing tests aren't polluted with the
    // four built-in decorator schemas. Explicit `Document::open` /
    // `open_with` behaviour is tested separately.
    Document::open_with(&lax, "test", &Environment::empty()).expect("open")
}

#[test]
fn document_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Document>();
}

#[test]
fn default_scalars_resolve_to_default_types() {
    let doc = open(
        r#"
        s = "alpha"
        i = 3
        f = 2.5
        b = true
        "#,
    );
    assert_eq!(
        doc.field("s").unwrap().value().unwrap(),
        &Value::Utf8("alpha".into())
    );
    assert_eq!(doc.field("i").unwrap().value().unwrap(), &Value::I64(3));
    assert_eq!(doc.field("f").unwrap().value().unwrap(), &Value::F64(2.5));
    assert_eq!(doc.field("b").unwrap().value().unwrap(), &Value::Bool(true));
}

#[test]
fn every_typed_literal_evaluates() {
    let doc = open(
        r#"
        a = 1i8
        b = 2i16
        c = 3i32
        d = 4i64
        e = 5i128
        f = 6isize
        g = 7u8
        h = 8u16
        i = 9u32
        j = 10u64
        k = 11u128
        l = 12usize
        m = 1.5f32
        n = 2.5f64
        s_utf8  = utf8"alpha"
        s_ascii = ascii"beta"
        s_utf16 = utf16"gamma"
        s_utf32 = utf32"delta"
        "#,
    );
    assert_eq!(doc.field("a").unwrap().value().unwrap(), &Value::I8(1));
    assert_eq!(doc.field("b").unwrap().value().unwrap(), &Value::I16(2));
    assert_eq!(doc.field("c").unwrap().value().unwrap(), &Value::I32(3));
    assert_eq!(doc.field("d").unwrap().value().unwrap(), &Value::I64(4));
    assert_eq!(doc.field("e").unwrap().value().unwrap(), &Value::I128(5));
    assert_eq!(doc.field("f").unwrap().value().unwrap(), &Value::Isize(6));
    assert_eq!(doc.field("g").unwrap().value().unwrap(), &Value::U8(7));
    assert_eq!(doc.field("h").unwrap().value().unwrap(), &Value::U16(8));
    assert_eq!(doc.field("i").unwrap().value().unwrap(), &Value::U32(9));
    assert_eq!(doc.field("j").unwrap().value().unwrap(), &Value::U64(10));
    assert_eq!(doc.field("k").unwrap().value().unwrap(), &Value::U128(11));
    assert_eq!(doc.field("l").unwrap().value().unwrap(), &Value::Usize(12));
    assert_eq!(doc.field("m").unwrap().value().unwrap(), &Value::F32(1.5));
    assert_eq!(doc.field("n").unwrap().value().unwrap(), &Value::F64(2.5));
    assert_eq!(
        doc.field("s_utf8").unwrap().value().unwrap(),
        &Value::Utf8("alpha".into())
    );
    assert_eq!(
        doc.field("s_ascii").unwrap().value().unwrap(),
        &Value::Ascii("beta".into())
    );
    assert_eq!(
        doc.field("s_utf16").unwrap().value().unwrap(),
        &Value::Utf16("gamma".encode_utf16().collect())
    );
    assert_eq!(
        doc.field("s_utf32").unwrap().value().unwrap(),
        &Value::Utf32("delta".chars().collect())
    );
}

#[test]
fn strict_typing_distinguishes_widths() {
    let doc = open("a = 42i32\nb = 42i64\n");
    let a = doc.field("a").unwrap().value().unwrap();
    let b = doc.field("b").unwrap().value().unwrap();
    assert_ne!(a, b);
}

#[test]
fn field_value_caches_on_first_access() {
    let doc = open(r#"name = "alpha""#);
    let f = doc.field("name").unwrap();
    let first = f.value().unwrap() as *const Value;
    let second = f.value().unwrap() as *const Value;
    assert_eq!(first, second);
}

#[test]
fn span_is_available_without_forcing_eval() {
    let doc = open(r#"name = "alpha""#);
    let f = doc.field("name").unwrap();
    assert!(!f.span().is_empty());
    // value() not called; the OnceLock should still be empty.
    assert!(f.field_cell().value.get().is_none());
}

#[test]
fn block_field_resolves() {
    let doc = open(r#"service "web" { port = 8080 }"#);
    let b = doc.block("service").unwrap();
    assert_eq!(b.labels().unwrap(), vec![Value::Utf8("web".into())]);
    assert_eq!(b.field("port").unwrap().value().unwrap(), &Value::I64(8080));
}

#[test]
fn nested_block_field_resolves() {
    let doc = open(
        r#"
        service "web" {
          metadata {
            region = "us-east-1"
          }
        }
        "#,
    );
    let svc = doc.block("service").unwrap();
    let meta = svc.block("metadata").unwrap();
    assert_eq!(
        meta.field("region").unwrap().value().unwrap(),
        &Value::Utf8("us-east-1".into())
    );
}

#[test]
fn unknown_field_returns_none() {
    let doc = open(r#"name = "alpha""#);
    assert!(doc.field("missing").is_none());
}

#[test]
fn unknown_block_returns_none() {
    let doc = open(r#"name = "alpha""#);
    assert!(doc.block("missing").is_none());
}

#[test]
fn fields_iter_yields_only_root_fields() {
    let doc = open(
        r#"a = 1
                      b "label" { c = 2 }
                      d = 3"#,
    );
    let names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
    assert_eq!(names, vec!["a".to_string(), "d".to_string()]);
}

#[test]
fn blocks_iter_yields_only_root_blocks() {
    let doc = open(
        r#"a = 1
                      b "label" { c = 2 }
                      d {}"#,
    );
    let kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
    assert_eq!(kinds, vec!["b".to_string(), "d".to_string()]);
}

#[test]
fn unresolved_bare_identifier_in_field_rhs_errors() {
    // Bare identifiers in expression position must resolve via
    // the scope chain or the document root, otherwise we surface
    // an UnresolvedReference. This replaces the old
    // Value::Identifier pass-through behaviour.
    let doc = open("owner = wil_taylor");
    let err = doc.field("owner").unwrap().value().unwrap_err();
    assert!(
        matches!(err, EvalError::UnresolvedReference { .. }),
        "{err:?}"
    );
}

#[test]
fn none_value_resolves() {
    let doc = open("maybe = none");
    assert_eq!(doc.field("maybe").unwrap().value().unwrap(), &Value::None);
}

#[test]
fn type_decls_are_queryable() {
    use crate::value::BuiltinType;
    let doc = open(
        r#"
        type User {
          name: utf8
          bio:  utf8?
          link: &User?
        }
        type Empty {}
        "#,
    );
    assert_eq!(doc.type_decls().count(), 2);
    let user = doc.type_decl("User").expect("User type");
    let fields: Vec<_> = user.fields().collect();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].name(), "name");
    assert_eq!(fields[0].type_ref(), &TypeRef::Builtin(BuiltinType::Utf8));
    assert!(!fields[0].optional());
    assert_eq!(fields[1].name(), "bio");
    assert!(fields[1].optional());
    assert_eq!(fields[2].name(), "link");
    assert_eq!(
        fields[2].type_ref(),
        &TypeRef::Reference(Box::new(TypeRef::Named(vec!["User".into()])))
    );
    assert!(fields[2].optional());
    assert!(doc.type_decl("Empty").unwrap().fields().count() == 0);
}

#[test]
fn type_decl_named_field_lookup() {
    let doc = open("type Point { x: i32 y: i32 }");
    let t = doc.type_decl("Point").unwrap();
    assert!(t.field("x").is_some());
    assert!(t.field("y").is_some());
    assert!(t.field("z").is_none());
}

#[test]
fn type_decls_dont_appear_in_field_or_block_iteration() {
    let doc = open(
        r#"
        type User { name: utf8 }
        count = 1
        svc {}
        "#,
    );
    let field_names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
    assert_eq!(field_names, vec!["count".to_string()]);
    let block_kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
    assert_eq!(block_kinds, vec!["svc".to_string()]);
}

#[test]
fn forward_ref_resolves_at_open() {
    let doc = open("type A { b: B }\ntype B { x: i32 }");
    let a = doc.type_decl("A").unwrap();
    let b_field = a.field("b").unwrap();
    match doc.resolve(b_field.type_ref()) {
        ResolvedType::Named(decl) => assert_eq!(decl.name(), "B"),
        _ => panic!("expected named ResolvedType::Named"),
    }
}

#[test]
fn self_ref_resolves_at_open() {
    let doc = open("type Tree { parent: Tree? }");
    let t = doc.type_decl("Tree").unwrap();
    let parent = t.field("parent").unwrap();
    match doc.resolve(parent.type_ref()) {
        ResolvedType::Named(decl) => assert_eq!(decl.name(), "Tree"),
        _ => panic!("expected ResolvedType::Named"),
    }
}

#[test]
fn unknown_type_ref_errors_at_open() {
    let err = Document::open("type X { y: NotDecl }", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(
                e.message.contains("unknown type 'NotDecl'"),
                "message: {}",
                e.message
            );
            // Span covers exactly the type ident `NotDecl`.
            assert_eq!(e.span.offset(), "type X { y: ".len());
            assert_eq!(e.span.len(), "NotDecl".len());
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn duplicate_type_decl_errors_at_open() {
    let err = Document::open("type Foo { x: i32 }\ntype Foo { y: i32 }", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(
                e.message.contains("duplicate declaration 'Foo'"),
                "message: {}",
                e.message
            );
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn type_and_union_with_same_name_errors() {
    let err = Document::open("type Foo {}\nunion Foo {}", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(
                e.message.contains("duplicate declaration 'Foo'"),
                "message: {}",
                e.message
            );
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn duplicate_variant_name_errors() {
    let err = Document::open("union X { A none A none }", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(
                e.message.contains("duplicate variant 'A' in union 'X'"),
                "message: {}",
                e.message
            );
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn union_decls_are_queryable() {
    let doc = open(
        r#"
        type Point { x: f64 y: f64 }
        union Shape {
          Circle { radius: f64 }
          Polygon Point
          Empty none
        }
        union Maybe { Some { v: i32 } Nothing none }
        "#,
    );
    assert_eq!(doc.union_decls().count(), 2);
    let shape = doc.union_decl("Shape").expect("Shape union");
    assert_eq!(shape.variants().count(), 3);
    assert!(shape.variant("Circle").is_some());
    assert!(shape.variant("missing").is_none());
}

#[test]
fn variant_record_fields_iterate() {
    let doc =
        open("type Point { x: f64 y: f64 }\nunion Shape { Circle { radius: f64 center: Point } }");
    let shape = doc.union_decl("Shape").unwrap();
    let circle = shape.variant("Circle").unwrap();
    assert!(matches!(circle.body(), VariantBodyView::Record));
    let names: Vec<_> = circle.fields().map(|f| f.name().to_string()).collect();
    assert_eq!(names, vec!["radius".to_string(), "center".to_string()]);
}

#[test]
fn variant_type_ref_body_resolves() {
    let doc = open("type Point { x: f64 y: f64 }\nunion Shape { Polygon Point }");
    let shape = doc.union_decl("Shape").unwrap();
    let v = shape.variant("Polygon").unwrap();
    match v.body() {
        VariantBodyView::TypeRef(t) => assert_eq!(*t, TypeRef::Named(vec!["Point".into()])),
        _ => panic!("expected TypeRef body"),
    }
}

#[test]
fn variant_unit_body() {
    let doc = open("union Maybe { Nothing none }");
    let m = doc.union_decl("Maybe").unwrap();
    let n = m.variant("Nothing").unwrap();
    assert!(matches!(n.body(), VariantBodyView::Unit));
    assert_eq!(n.fields().count(), 0);
}

#[test]
fn resolve_union_returns_union() {
    let doc = open("union Shape { Empty none }\ntype Box { contents: Shape }");
    let b = doc.type_decl("Box").unwrap();
    let contents = b.field("contents").unwrap();
    match doc.resolve(contents.type_ref()) {
        ResolvedType::Union(u) => assert_eq!(u.name(), "Shape"),
        _ => panic!("expected union"),
    }
}

#[test]
fn unknown_variant_body_type_errors() {
    let err = Document::open("union X { V NotDecl }", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(
                e.message.contains("unknown type 'NotDecl'"),
                "message: {}",
                e.message
            );
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn namespace_sets_file_ns_and_qualifies_decls() {
    let doc = open("namespace foo.bar\ntype X {}");
    assert_eq!(doc.namespace(), &["foo".to_string(), "bar".to_string()]);
    let x = doc.type_decl("foo.bar.X").expect("found");
    assert_eq!(x.full_name(), "foo.bar.X");
    assert_eq!(x.name(), "X");
}

#[test]
fn dotted_decl_name_extends_file_ns() {
    let doc = open("namespace foo\ntype a.b.X {}");
    assert!(doc.type_decl("foo.a.b.X").is_some());
    let x = doc.type_decl("foo.a.b.X").unwrap();
    assert_eq!(x.name(), "X");
    assert_eq!(x.full_name(), "foo.a.b.X");
    assert_eq!(
        x.namespace(),
        vec!["foo".to_string(), "a".to_string(), "b".to_string()]
    );
}

#[test]
fn local_name_resolves_within_file_ns() {
    let doc = open(
        r#"
        namespace app
        type X { name: utf8 }
        type Y { f: X }
        "#,
    );
    let y = doc.type_decl("app.Y").unwrap();
    match doc.resolve(y.field("f").unwrap().type_ref()) {
        ResolvedType::Named(d) => assert_eq!(d.full_name(), "app.X"),
        _ => panic!("expected Named"),
    }
}

#[test]
fn item_alias_with_as_resolves() {
    let doc = open(
        r#"
        type a.b.Baz {}
        use a.b.Baz as MB
        type Q { f: MB }
        "#,
    );
    let q = doc.type_decl("Q").unwrap();
    match doc.resolve(q.field("f").unwrap().type_ref()) {
        ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Baz"),
        _ => panic!("expected Named"),
    }
}

#[test]
fn wildcard_import_resolves_bare_name() {
    let doc = open(
        r#"
        type a.b.Baz {}
        use a.b
        type Q { f: Baz }
        "#,
    );
    let q = doc.type_decl("Q").unwrap();
    match doc.resolve(q.field("f").unwrap().type_ref()) {
        ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Baz"),
        _ => panic!("expected Named"),
    }
}

#[test]
fn namespace_alias_with_as_resolves() {
    let doc = open(
        r#"
        type a.b.Baz {}
        use a.b as B
        type Q { f: B.Baz }
        "#,
    );
    let q = doc.type_decl("Q").unwrap();
    match doc.resolve(q.field("f").unwrap().type_ref()) {
        ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Baz"),
        _ => panic!("expected Named"),
    }
}

#[test]
fn brace_list_imports_each_item() {
    let doc = open(
        r#"
        type a.b.X {}
        type a.b.Y {}
        use a.b.{X, Y as Z}
        type Q { f1: X f2: Z }
        "#,
    );
    let q = doc.type_decl("Q").unwrap();
    match doc.resolve(q.field("f1").unwrap().type_ref()) {
        ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.X"),
        _ => panic!("f1"),
    }
    match doc.resolve(q.field("f2").unwrap().type_ref()) {
        ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Y"),
        _ => panic!("f2"),
    }
}

#[test]
fn namespace_must_be_first_item() {
    let err = Document::open("type X {}\nnamespace foo", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("must be the first item"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn duplicate_namespace_errors() {
    let err = Document::open("namespace foo\nnamespace bar", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(e.message.contains("duplicate namespace"), "{}", e.message)
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn unknown_use_target_errors() {
    let err = Document::open("use foo.bar.Nope", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(e.message.contains("unknown use target"), "{}", e.message)
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn brace_list_unknown_item_errors() {
    let err = Document::open("type a.b.X {}\nuse a.b.{Nope}", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(e.message.contains("unknown use target"), "{}", e.message)
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn brace_list_on_leaf_errors() {
    // foo.bar.X is a leaf; can't do use foo.bar.X.{Y}
    let err = Document::open("type foo.bar.X {}\nuse foo.bar.X.{Y}", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(e.message.contains("expected namespace"), "{}", e.message)
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn duplicate_alias_errors() {
    let err = Document::open(
        r#"
        type a.X {}
        type b.X {}
        use a.X
        use b.X
        "#,
        "test",
    )
    .unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(e.message.contains("duplicate use alias"), "{}", e.message)
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn unions_dont_appear_in_field_or_block_iteration() {
    let doc = open(
        r#"
        union Shape { Empty none }
        count = 1
        svc {}
        "#,
    );
    let field_names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
    assert_eq!(field_names, vec!["count".to_string()]);
    let block_kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
    assert_eq!(block_kinds, vec!["svc".to_string()]);
}

#[test]
fn resolve_reference_unwraps_to_named() {
    let doc = open("type User { name: utf8 }\ntype Post { author: &User }");
    let post = doc.type_decl("Post").unwrap();
    let author = post.field("author").unwrap();
    let resolved = doc.resolve(author.type_ref());
    let ResolvedType::Reference(inner) = resolved else {
        panic!("expected reference");
    };
    let ResolvedType::Named(decl) = *inner else {
        panic!("expected inner Named");
    };
    assert_eq!(decl.name(), "User");
}

#[test]
fn resolve_reference_to_builtin() {
    let doc = open("type Score { value: &i32 }");
    let s = doc.type_decl("Score").unwrap();
    let v = s.field("value").unwrap();
    let resolved = doc.resolve(v.type_ref());
    let ResolvedType::Reference(inner) = resolved else {
        panic!("expected reference");
    };
    let ResolvedType::Builtin(b) = *inner else {
        panic!("expected inner builtin");
    };
    assert_eq!(b, BuiltinType::I32);
}

#[test]
fn unknown_reference_target_errors() {
    let err = Document::open("type X { y: &NotDecl }", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => {
            assert!(
                e.message.contains("unknown type 'NotDecl'"),
                "message: {}",
                e.message
            );
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn resolve_builtin_returns_builtin() {
    let doc = open("");
    match doc.resolve(&TypeRef::Builtin(BuiltinType::I32)) {
        ResolvedType::Builtin(b) => assert_eq!(b, BuiltinType::I32),
        _ => panic!("expected builtin"),
    }
}

#[test]
fn resolve_lets_caller_walk_named_decl_fields() {
    let doc = open(
        r#"
        type Inner { a: i32 b: utf8 }
        type Outer { inner: Inner }
        "#,
    );
    let outer = doc.type_decl("Outer").unwrap();
    let inner_field = outer.field("inner").unwrap();
    let ResolvedType::Named(inner_decl) = doc.resolve(inner_field.type_ref()) else {
        panic!("expected named");
    };
    let sub_names: Vec<_> = inner_decl.fields().map(|f| f.name().to_string()).collect();
    assert_eq!(sub_names, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn resolve_list_of_named() {
    let doc = open("type User { name: utf8 }\ntype Q { items: list<User> }");
    let q = doc.type_decl("Q").unwrap();
    let items = q.field("items").unwrap();
    let resolved = doc.resolve(items.type_ref());
    let ResolvedType::List(inner) = resolved else {
        panic!("expected List");
    };
    let ResolvedType::Named(decl) = *inner else {
        panic!("expected Named inner");
    };
    assert_eq!(decl.name(), "User");
}

#[test]
fn resolve_tensor_keeps_dims() {
    let doc = open("type Q { w: tensor<f32, [3, N, 5]> }");
    let q = doc.type_decl("Q").unwrap();
    let w = q.field("w").unwrap();
    let resolved = doc.resolve(w.type_ref());
    let ResolvedType::Tensor { element, dims } = resolved else {
        panic!("expected Tensor");
    };
    let ResolvedType::Builtin(b) = *element else {
        panic!("expected Builtin element");
    };
    assert_eq!(b, BuiltinType::F32);
    assert_eq!(
        dims,
        &[
            TensorDim::Fixed(3),
            TensorDim::Symbolic("N".into()),
            TensorDim::Fixed(5),
        ]
    );
}

#[test]
fn unknown_list_element_type_errors() {
    let err = Document::open("type Q { items: list<NotDecl> }", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("unknown type 'NotDecl'"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn unknown_tensor_element_type_errors() {
    let err = Document::open("type Q { w: tensor<NotDecl, [4]> }", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("unknown type 'NotDecl'"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn list_of_reference_resolves() {
    let doc = open("type T { x: i32 }\ntype Q { items: list<&T> }");
    let q = doc.type_decl("Q").unwrap();
    let resolved = doc.resolve(q.field("items").unwrap().type_ref());
    let ResolvedType::List(inner) = resolved else {
        panic!("expected List");
    };
    let ResolvedType::Reference(inner2) = *inner else {
        panic!("expected Reference inner");
    };
    let ResolvedType::Named(d) = *inner2 else {
        panic!("expected Named");
    };
    assert_eq!(d.name(), "T");
}

#[test]
fn symbol_sets_are_queryable() {
    let doc = open(
        r#"
        symbol_set Color { red green blue }
        symbol_set Mood { warm cool }
        "#,
    );
    assert_eq!(doc.symbol_sets().count(), 2);
    let color = doc.symbol_set("Color").expect("Color");
    assert_eq!(color.symbols().count(), 3);
    assert!(color.has("red"));
    assert!(!color.has("missing"));
}

#[test]
fn resolve_named_symbol_set() {
    let doc = open("symbol_set Color { red green }\ntype Q { f: Color }");
    let q = doc.type_decl("Q").unwrap();
    let f = q.field("f").unwrap();
    match doc.resolve(f.type_ref()) {
        ResolvedType::SymbolSet(s) => assert_eq!(s.name(), "Color"),
        _ => panic!("expected SymbolSet"),
    }
}

#[test]
fn duplicate_symbol_in_set_errors() {
    let err = Document::open("symbol_set X { a a }", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("duplicate symbol 'a' in symbol_set 'X'"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn symbol_set_collides_with_type_name() {
    let err = Document::open("type Foo {}\nsymbol_set Foo {}", "test").unwrap_err();
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("duplicate declaration 'Foo'"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn symbol_value_evaluates() {
    let doc = open("tag = :wide");
    assert_eq!(
        doc.field("tag").unwrap().value().unwrap(),
        &Value::Symbol("wide".into())
    );
}

#[test]
fn symbol_sets_dont_appear_in_field_or_block_iteration() {
    let doc = open(
        r#"
        symbol_set Color { red green }
        count = 1
        svc {}
        "#,
    );
    let field_names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
    assert_eq!(field_names, vec!["count".to_string()]);
    let block_kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
    assert_eq!(block_kinds, vec!["svc".to_string()]);
}

#[test]
fn decorator_iterator_on_type() {
    let doc = open(r#"@deprecated("X") type Foo {}"#);
    let t = doc.type_decl("Foo").unwrap();
    let decs: Vec<_> = t.decorators().collect();
    assert_eq!(decs.len(), 1);
    assert_eq!(decs[0].name(), "deprecated");
    assert_eq!(decs[0].positional().unwrap(), vec![Value::Utf8("X".into())]);
}

#[test]
fn decorator_iterator_on_field() {
    let doc = open("type T { @max(64) name: utf8 }");
    let t = doc.type_decl("T").unwrap();
    let f = t.field("name").unwrap();
    let decs: Vec<_> = f.decorators().collect();
    assert_eq!(decs.len(), 1);
    assert_eq!(decs[0].name(), "max");
    assert_eq!(decs[0].positional().unwrap(), vec![Value::I64(64)]);
}

#[test]
fn decorator_iterator_on_variant() {
    let doc = open("union U { @hidden Circle { radius: f64 } }");
    let u = doc.union_decl("U").unwrap();
    let v = u.variant("Circle").unwrap();
    let decs: Vec<_> = v.decorators().collect();
    assert_eq!(decs.len(), 1);
    assert_eq!(decs[0].name(), "hidden");
}

#[test]
fn decorator_iterator_on_symbol_entry() {
    let doc = open("symbol_set C { @default red green }");
    let s = doc.symbol_set("C").unwrap();
    let entries: Vec<_> = s.symbols().collect();
    assert_eq!(entries[0].decorators().count(), 1);
    assert_eq!(entries[1].decorators().count(), 0);
}

#[test]
fn decorator_named_args_via_helper() {
    let doc = open("@v(min = 1, max = 10) type X {}");
    let x = doc.type_decl("X").unwrap();
    let d = x.decorators().next().unwrap();
    assert_eq!(d.named_arg("min").unwrap().unwrap(), Value::I64(1));
    assert_eq!(d.named_arg("max").unwrap().unwrap(), Value::I64(10));
    assert!(d.named_arg("missing").is_none());
}

#[test]
fn decorator_with_symbol_arg() {
    let doc = open("@tagged(:enabled) type X {}");
    let x = doc.type_decl("X").unwrap();
    let d = x.decorators().next().unwrap();
    assert_eq!(
        d.positional().unwrap(),
        vec![Value::Symbol("enabled".into())]
    );
}

#[test]
fn decorator_with_none_arg() {
    let doc = open("@default(none) type X {}");
    let x = doc.type_decl("X").unwrap();
    let d = x.decorators().next().unwrap();
    assert_eq!(d.positional().unwrap(), vec![Value::None]);
}

#[test]
fn decorator_dotted_name_full_name() {
    let doc = open("@a.b.c type X {}");
    let x = doc.type_decl("X").unwrap();
    let d = x.decorators().next().unwrap();
    assert_eq!(d.full_name(), "a.b.c");
    assert_eq!(d.name(), "c");
}

#[test]
fn block_schema_lookup() {
    let doc = open(r#"@block("service") type Service {}"#);
    let s = doc.block_schema("service").expect("Service schema");
    assert_eq!(s.name(), "Service");
    assert!(doc.block_schema("nope").is_none());
}

#[test]
fn decorator_schema_lookup() {
    let doc = open(r#"@decorator("max") type MaxDec { value: i64 }"#);
    let s = doc.decorator_schema("max").expect("max schema");
    assert_eq!(s.name(), "MaxDec");
}

#[test]
fn inline_slot_helper() {
    let doc = open("type Q { @inline(2) f: utf8 }");
    let q = doc.type_decl("Q").unwrap();
    assert_eq!(q.field("f").unwrap().inline_slot(), Some(2));
}

#[test]
fn inline_slot_returns_none_when_decorator_absent() {
    let doc = open("type Q { f: utf8 }");
    let q = doc.type_decl("Q").unwrap();
    assert_eq!(q.field("f").unwrap().inline_slot(), None);
}

#[test]
fn default_value_helper() {
    let doc = open("type Q { @default(8080) port: u32? }");
    let q = doc.type_decl("Q").unwrap();
    assert_eq!(
        q.field("port").unwrap().default_value(),
        Some(Value::I64(8080))
    );
}

#[test]
fn default_value_with_symbol_arg() {
    let doc = open("type Q { @default(:enabled) mode: symbol }");
    let q = doc.type_decl("Q").unwrap();
    assert_eq!(
        q.field("mode").unwrap().default_value(),
        Some(Value::Symbol("enabled".into()))
    );
}

#[test]
fn mixed_block_labels_round_trip() {
    let doc = open(r#"service web "prod" { port = 1 }"#);
    let b = doc.block("service").unwrap();
    let labels = b.labels().unwrap();
    assert_eq!(
        labels,
        vec![Value::Identifier("web".into()), Value::Utf8("prod".into())]
    );
}

#[test]
fn block_label_can_be_any_value() {
    let doc = open(r#"slot 0 :enabled 1.5 { x = 1 }"#);
    let b = doc.block("slot").unwrap();
    let labels = b.labels().unwrap();
    assert_eq!(labels.len(), 3);
    assert_eq!(labels[0], Value::I64(0));
    assert_eq!(labels[1], Value::Symbol("enabled".into()));
    assert_eq!(labels[2], Value::F64(1.5));
}

#[test]
fn cycle_error_renders() {
    let err = EvalError::Cycle {
        field: "x".into(),
        span: SourceSpan::new(0.into(), 1),
    };
    let s = format!("{}", err);
    assert!(s.contains("cycle"));
}

// ─── Evaluator (builtins + operators + identifier resolution) ────

fn env_with_test_builtins() -> Environment {
    use crate::builtins::from_fn;
    let mut env = Environment::empty();
    env.add_builtin("upper", from_fn(|s: String| s.to_uppercase()));
    env.add_builtin("len", from_fn(|s: String| s.len() as i64));
    env.add_builtin("add", from_fn(|a: i64, b: i64| a + b));
    env.add_builtin(
        "die",
        from_fn(|s: String| -> Result<i64, String> { Err(s) }),
    );
    env
}

fn open_with_builtins(src: &str) -> Document {
    // Same lax wrap as `open()` so the strict-validation default
    // doesn't fail every eval test on `NoDocumentSchema`.
    let lax = laxify_for_tests(src);
    Document::open_with(&lax, "test", &env_with_test_builtins()).expect("open")
}

#[test]
fn eval_call_literal_arg() {
    let doc = open_with_builtins(r#"out = upper("hi")"#);
    assert_eq!(
        doc.field("out").unwrap().value().unwrap(),
        &Value::Utf8("HI".into())
    );
}

#[test]
fn eval_call_identifier_arg_resolves_via_field_lookup() {
    let doc = open_with_builtins(
        r#"
        name = "alpha"
        out  = upper(name)
        "#,
    );
    assert_eq!(
        doc.field("out").unwrap().value().unwrap(),
        &Value::Utf8("ALPHA".into())
    );
}

#[test]
fn eval_call_nested() {
    let doc = open_with_builtins(r#"n = add(len("ab"), 1)"#);
    assert_eq!(doc.field("n").unwrap().value().unwrap(), &Value::I64(3));
}

#[test]
fn eval_unknown_builtin_errors() {
    let doc = open_with_builtins("x = nope()");
    let err = doc.field("x").unwrap().value().unwrap_err();
    assert!(matches!(err, EvalError::UnknownBuiltin { .. }));
}

#[test]
fn eval_arity_mismatch_errors() {
    let doc = open_with_builtins(r#"x = upper("a", "b")"#);
    let err = doc.field("x").unwrap().value().unwrap_err();
    assert!(matches!(err, EvalError::BuiltinArity { .. }));
}

#[test]
fn eval_type_mismatch_errors_at_builtin() {
    let doc = open_with_builtins("x = upper(42)");
    let err = doc.field("x").unwrap().value().unwrap_err();
    assert!(matches!(err, EvalError::BuiltinTypeMismatch { .. }));
}

#[test]
fn eval_fallible_builtin_propagates_error() {
    let doc = open_with_builtins(r#"x = die("boom")"#);
    let err = doc.field("x").unwrap().value().unwrap_err();
    let EvalError::BuiltinTypeMismatch { message, .. } = err else {
        panic!("expected BuiltinTypeMismatch, got {err:?}");
    };
    assert!(message.contains("boom"), "{message}");
}

#[test]
fn eval_arithmetic_precedence() {
    let doc = open_with_builtins("x = 1 + 2 * 3");
    assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::I64(7));
}

#[test]
fn eval_unary_neg_and_paren() {
    let doc = open_with_builtins("x = -(1 + 2)");
    assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::I64(-3));
}

#[test]
fn eval_comparison_returns_bool() {
    let doc = open_with_builtins("a = 2 > 1\nb = 2 == 1\nc = 2 != 1");
    assert_eq!(doc.field("a").unwrap().value().unwrap(), &Value::Bool(true));
    assert_eq!(
        doc.field("b").unwrap().value().unwrap(),
        &Value::Bool(false)
    );
    assert_eq!(doc.field("c").unwrap().value().unwrap(), &Value::Bool(true));
}

#[test]
fn eval_short_circuits_logical_and() {
    // `false && nope()` must not invoke the unknown builtin.
    let doc = open_with_builtins("x = false && nope()");
    assert_eq!(
        doc.field("x").unwrap().value().unwrap(),
        &Value::Bool(false)
    );
}

#[test]
fn eval_short_circuits_logical_or() {
    let doc = open_with_builtins("x = true || nope()");
    assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::Bool(true));
}

#[test]
fn eval_block_with_let_bindings() {
    let doc = open_with_builtins("x = { let a = 2; let b = 3; a + b }");
    assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::I64(5));
}

#[test]
fn eval_block_inner_let_shadows_field() {
    let doc = open_with_builtins(
        r#"
        n = 100
        x = { let n = 1; n + 2 }
        "#,
    );
    assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::I64(3));
}

#[test]
fn eval_decorator_positional_arg_evaluates() {
    let doc = open_with_builtins("@logged(add(1, 2)) type X {}");
    let t = doc.type_decl("X").unwrap();
    let d = t.decorators().next().unwrap();
    let pos = d.positional().unwrap();
    assert_eq!(pos, vec![Value::I64(3)]);
}

#[test]
fn eval_user_function_call_returns_body_value() {
    // Function literals are first-class: bind one to a field and call
    // it by name; the body sees the parameter as a local.
    let doc = open_with_builtins(
        r#"
        f = fn(x: i32) -> i32 x
        y = f(3)
        "#,
    );
    assert_eq!(*doc.field("y").unwrap().value().unwrap(), Value::I64(3));
}

#[test]
fn eval_user_function_arity_mismatch() {
    let doc = open_with_builtins(
        r#"
        f = fn(x: i32) -> i32 x
        y = f(1, 2)
        "#,
    );
    let err = doc.field("y").unwrap().value().unwrap_err();
    assert!(matches!(err, EvalError::BuiltinArity { .. }));
}

// ─── Lazy data access (DataRef / Document::get) ───────────────────

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

#[test]
fn get_resolves_top_level_field() {
    let doc = open("port = 8080");
    let r = doc.get("port").expect("port should resolve");
    assert_eq!(r.value().unwrap(), Value::I64(8080));
}

#[test]
fn get_resolves_nested_block_path() {
    let doc = open(r#"service "web" { port = 9090 }"#);
    let r = doc
        .get("service.port")
        .expect("service.port should resolve");
    assert_eq!(r.value().unwrap(), Value::I64(9090));
}

#[test]
fn get_resolves_deeply_nested_path() {
    let doc = open(
        r#"
        service "web" {
          metadata {
            region = "us-east-1"
          }
        }
        "#,
    );
    let r = doc
        .get("service.metadata.region")
        .expect("path should resolve");
    assert_eq!(r.value().unwrap(), Value::Utf8("us-east-1".into()));
}

#[test]
fn get_returns_none_for_missing_segment() {
    let doc = open(r#"service "web" { port = 1 }"#);
    assert!(doc.get("service.missing").is_none());
    assert!(doc.get("nonexistent").is_none());
}

#[test]
fn get_intermediate_node_is_not_a_leaf() {
    let doc = open(r#"service "web" { port = 1 }"#);
    let svc = doc.get("service").expect("service block");
    let err = svc.value().unwrap_err();
    assert!(matches!(err, EvalError::NotALeaf { .. }));
}

#[test]
fn get_descends_into_type_decl_field() {
    let doc = open("type User { name: utf8 age: u32 }");
    let f = doc.get("User.name").expect("User.name");
    assert_eq!(f.kind(), "type_field");
}

#[test]
fn get_descends_into_union_variant() {
    let doc = open("union Shape { Circle { r: f64 } Square none }");
    let v = doc.get("Shape.Circle").expect("variant");
    assert_eq!(v.kind(), "variant");
    let r = doc.get("Shape.Circle.r").expect("variant field");
    assert_eq!(r.kind(), "type_field");
}

#[test]
fn get_descends_into_symbol_set_entry() {
    let doc = open("symbol_set Color { red green }");
    let s = doc.get("Color.red").expect("symbol entry");
    assert_eq!(s.kind(), "symbol_entry");
}

#[test]
fn block_labels_cached_across_calls() {
    // Inspect the labels OnceLock directly: empty before first call,
    // populated after.
    let doc = open(r#"my_block first "two" 3 { x = 1 }"#);
    let b = doc.block("my_block").unwrap();
    let labels_cell = match &b.cells.kind {
        ItemCellKind::Block { labels, .. } => labels,
        _ => unreachable!(),
    };
    assert!(labels_cell.get().is_none());
    let v1 = b.labels().unwrap();
    assert!(labels_cell.get().is_some());
    let v2 = b.labels().unwrap();
    assert_eq!(v1, v2);
}

#[test]
fn decorator_positional_cached_across_calls() {
    let counter = Arc::new(AtomicUsize::new(0));
    let bumper = {
        let c = counter.clone();
        crate::builtins::from_fn(move || {
            c.fetch_add(1, AtomicOrdering::Relaxed);
            7i64
        })
    };
    let mut env = Environment::empty();
    env.add_builtin("bump", bumper);

    let doc = Document::open_with(r#"@x(bump()) type T {}"#, "test", &env).expect("open");
    let t = doc.type_decl("T").unwrap();
    let d = t.decorators().next().unwrap();
    let _ = d.positional().unwrap();
    let _ = d.positional().unwrap();
    let _ = d.positional().unwrap();
    assert_eq!(counter.load(AtomicOrdering::Relaxed), 1);
}

#[test]
fn decorator_named_arg_cached_across_calls() {
    let counter = Arc::new(AtomicUsize::new(0));
    let bumper = {
        let c = counter.clone();
        crate::builtins::from_fn(move || {
            c.fetch_add(1, AtomicOrdering::Relaxed);
            3i64
        })
    };
    let mut env = Environment::empty();
    env.add_builtin("bump", bumper);

    let doc = Document::open_with(r#"@x(amount = bump()) type T {}"#, "test", &env).expect("open");
    let t = doc.type_decl("T").unwrap();
    let d = t.decorators().next().unwrap();
    let _ = d.named_arg("amount").unwrap().unwrap();
    let _ = d.named_arg("amount").unwrap().unwrap();
    let _ = d.named_arg("amount").unwrap().unwrap();
    assert_eq!(counter.load(AtomicOrdering::Relaxed), 1);
}

// ─── Nested-block schema (`@child` / `@children`) ─────────────────

fn open_nested() -> Document {
    Document::open(
        r#"
        @block("service", max_children = 50)
        type Service {
          @inline(0) id: identifier
          @child("config")             config:  Config
          @children("route", max = 32) routes:  list<Route>
        }

        @block("config")
        type Config { region: utf8  tier: symbol }

        @block("route")
        type Route {
          @inline(0) path: utf8
          method: utf8
        }

        service web {
          config { region = "us-east-1"  tier = :gold }
          route "/api"     { method = "GET" }
          route "/healthz" { method = "GET" }
        }
        "#,
        "test",
    )
    .expect("open")
}

#[test]
fn typefield_reports_child_kind() {
    let doc = open_nested();
    let svc = doc.type_decl("Service").unwrap();
    assert_eq!(
        svc.field("config").unwrap().child_block_kind().as_deref(),
        Some("config")
    );
    assert_eq!(
        svc.field("routes")
            .unwrap()
            .children_block_kind()
            .as_deref(),
        Some("route")
    );
    assert_eq!(svc.field("routes").unwrap().children_max(), Some(32));
}

#[test]
fn typedecl_reports_max_children_and_allowed_set() {
    let doc = open_nested();
    let svc = doc.type_decl("Service").unwrap();
    assert_eq!(svc.max_children(), Some(50));
    let mut allowed = svc.allowed_child_kinds();
    allowed.sort();
    assert_eq!(allowed, vec!["config".to_string(), "route".to_string()]);
}

#[test]
fn data_ref_resolves_child_field_to_nested_block() {
    let doc = open_nested();
    let cfg = doc.get("service.config").expect("service.config");
    assert_eq!(cfg.kind(), "block");
    let region = cfg.get("region").expect("region");
    assert_eq!(region.value().unwrap(), Value::Utf8("us-east-1".into()));
}

#[test]
fn data_ref_resolves_children_field_to_block_list() {
    let doc = open_nested();
    let routes = doc.get("service.routes").expect("service.routes");
    assert_eq!(routes.kind(), "block_list");
    assert_eq!(routes.len(), Some(2));
    let first = routes.children().next().unwrap();
    let method = first.get("method").unwrap().value().unwrap();
    assert_eq!(method, Value::Utf8("GET".into()));
}

#[test]
fn raw_ast_view_still_works() {
    // Block::blocks() / Block::field() (raw AST) keep their
    // structural semantics regardless of schema.
    let doc = open_nested();
    let svc = doc.block("service").unwrap();
    let raw_kinds: Vec<&str> = svc.blocks().map(|b| b.kind()).collect();
    assert_eq!(raw_kinds, vec!["config", "route", "route"]);
}

#[test]
fn schema_errors_empty_for_clean_block() {
    let doc = open_nested();
    let svc = doc.block("service").unwrap();
    assert!(svc.schema_errors().is_empty());
}

#[test]
fn schema_errors_missing_required_child() {
    let doc = Document::open(
        r#"
        @block("service") type Service {
          @child("config") config: Config
        }
        @block("config") type Config {}
        service web {}
        "#,
        "test",
    )
    .expect("open");
    let svc = doc.block("service").unwrap();
    let errs = svc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::MissingRequired,
                ..
            }
        )),
        "expected MissingRequired, got {errs:?}"
    );
}

#[test]
fn schema_errors_disallowed_child_kind() {
    let doc = Document::open(
        r#"
        @block("service") type Service {
          @child("config") config: Config?
        }
        @block("config") type Config {}
        service web { config {}  rogue {} }
        "#,
        "test",
    )
    .expect("open");
    let svc = doc.block("service").unwrap();
    let errs = svc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::DisallowedChild,
                ..
            }
        )),
        "expected DisallowedChild, got {errs:?}"
    );
}

#[test]
fn schema_errors_children_max_violated() {
    let doc = Document::open(
        r#"
        @block("service") type Service {
          @children("route", max = 1) routes: list<Route>
        }
        @block("route") type Route {}
        service web { route {} route {} }
        "#,
        "test",
    )
    .expect("open");
    let svc = doc.block("service").unwrap();
    let errs = svc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::ChildrenTooMany,
                ..
            }
        )),
        "expected ChildrenTooMany, got {errs:?}"
    );
}

#[test]
fn schema_errors_block_max_children_violated() {
    let doc = Document::open(
        r#"
        @block("service", max_children = 1) type Service {
          @children("route") routes: list<Route>
        }
        @block("route") type Route {}
        service web { route {} route {} }
        "#,
        "test",
    )
    .expect("open");
    let svc = doc.block("service").unwrap();
    let errs = svc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::BlockChildrenOverflow,
                ..
            }
        )),
        "expected BlockChildrenOverflow, got {errs:?}"
    );
}

#[test]
fn schema_errors_cached_after_first_call() {
    let doc = open_nested();
    let svc = doc.block("service").unwrap();
    let p1 = svc.schema_errors().as_ptr();
    let p2 = svc.schema_errors().as_ptr();
    assert_eq!(p1, p2, "schema_errors should be cached (same Vec address)");
}

// ─── Computed @children (splice) ──────────────────────────────────
//
// A `@children(kind)` slot may be authored as a value expression
// (`field = map(data, …)`) instead of nested block literals; each list
// element is materialised into a value-backed synthetic child block.

fn open_splice() -> Document {
    Document::open(
        r#"
        @document
        type Doc { @children("service") services: list<Service> }

        @block("service")
        type Service {
          @inline(0) id: identifier
          @children("route") routes: list<Route>
        }

        @block("route")
        type Route {
          @inline(0) path: utf8
          method: utf8?
        }

        let paths = ["/api", "/healthz", "/metrics"]

        service web {
          routes = map(paths, fn(p: utf8) -> Route { { path: p } })
        }
        "#,
        "test",
    )
    .expect("open")
}

#[test]
fn computed_children_project_to_block_list() {
    let doc = open_splice();
    let routes = doc.get("service.routes").expect("service.routes");
    assert_eq!(routes.kind(), "block_list");
    assert_eq!(routes.len(), Some(3), "three spliced children");
}

#[test]
fn computed_children_are_label_addressable_and_schema_completed() {
    let doc = open_splice();
    // Addressable by first label (the @inline(0) `path`), like a static
    // nested block.
    let route = doc.get("service.routes./healthz").expect("/healthz route");
    assert_eq!(route.kind(), "block");
    // A synthetic child behaves exactly like a statically-nested one: its
    // `@inline(0)` value lands in the label, and an omitted optional field
    // is simply absent at the AST/view layer (schema-completion to `none`
    // happens later, at lowering time).
    let b = route.as_block().expect("block view");
    let label = b.labels().unwrap();
    assert_eq!(label.first(), Some(&Value::Utf8("/healthz".into())));
    assert!(
        b.field("method").is_none(),
        "an omitted optional is absent, like a static block"
    );
}

#[test]
fn computed_children_appear_in_blocks_after_static() {
    // `Block::blocks()` (the render-path walk) surfaces spliced children
    // after any literal nested blocks, in source order.
    let doc = Document::open(
        r#"
        @document
        type Doc { @children("service") services: list<Service> }
        @block("service")
        type Service { @children("route") routes: list<Route> }
        @block("route")
        type Route { @inline(0) path: utf8 }
        let extra = ["/b", "/c"]
        service {
          route "/a"
          routes = map(extra, fn(p: utf8) -> Route { { path: p } })
        }
        "#,
        "test",
    )
    .expect("open");
    let svc = doc.block("service").unwrap();
    let labels: Vec<String> = svc
        .blocks()
        .filter(|b| b.kind() == "route")
        .map(|b| match &b.labels().unwrap()[0] {
            Value::Utf8(s) => s.clone(),
            other => panic!("expected utf8 label, got {other:?}"),
        })
        .collect();
    assert_eq!(labels, vec!["/a", "/b", "/c"]);
}

#[test]
fn inline_field_set_by_name_resolves_like_the_label() {
    // An `@inline(N)` field may be written either as the positional label
    // (`shape "x"`) or as an explicit `text = "x"` field; both must resolve to
    // the same value (a diagram `node { text = … }` relied on this).
    let doc = Document::open(
        r#"
        @document
        type Doc { @children("shape") shapes: list<Shape> }
        @block("shape")
        type Shape { @inline(0) text: utf8  id: identifier? }
        shape { id = a  text = "Named" }
        shape "Inline" { id = b }
        "#,
        "test",
    )
    .expect("open");
    let text_of = |b: &crate::doc::Block<'_>| match b.to_record_value().unwrap() {
        Value::Record { fields, .. } => fields.get("text").cloned().unwrap(),
        other => panic!("expected record, got {other:?}"),
    };
    let shapes: Vec<_> = doc.blocks().filter(|b| b.kind() == "shape").collect();
    assert_eq!(text_of(&shapes[0]), Value::Utf8("Named".into()));
    assert_eq!(text_of(&shapes[1]), Value::Utf8("Inline".into()));
}

#[test]
fn computed_children_non_list_is_schema_error() {
    let doc = Document::open(
        r#"
        @document
        type Doc { @children("service") services: list<Service> }
        @block("service")
        type Service { @children("route") routes: list<Route> }
        @block("route")
        type Route { @inline(0) path: utf8 }
        service { routes = 42 }
        "#,
        "test",
    )
    .expect("open");
    let errs = doc.block("service").unwrap().schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::FieldTypeMismatch,
                ..
            }
        )),
        "expected FieldTypeMismatch for a non-list @children splice, got {errs:?}"
    );
}

#[test]
fn computed_children_clean_splice_has_no_schema_errors() {
    let doc = open_splice();
    assert!(
        doc.block("service").unwrap().schema_errors().is_empty(),
        "a well-formed splice should not flag a value-vs-type mismatch"
    );
}

#[test]
fn computed_children_union_slot_infers_variants_by_shape() {
    // A `@children(SomeUnion)` slot spliced from bare records coerces
    // each record to its matching variant by shape (the same matcher the
    // static union-children path uses).
    let doc = Document::open(
        r#"
        @document
        type Doc { @children("chart") charts: list<Chart> }
        union Series { Of { name: utf8  values: list<f64> } }
        @block("chart")
        type Chart { @children(Series) series: list<Series> }
        let data = [ { name: "a", values: [1.0, 2.0] }, { name: "b", values: [3.0] } ]
        chart { series = data }
        "#,
        "test",
    )
    .expect("open");
    let series = doc.get("chart.series").expect("chart.series");
    let vals = series.value().expect("variant list value");
    let Value::List(items) = vals else {
        panic!("expected a list, got {vals:?}");
    };
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .all(|v| matches!(v, Value::Variant { variant, .. } if variant == "Of")),
        "each record should infer the `Of` variant, got {items:?}"
    );
}

// ─── Value-binding scope frame (component slots / repeater loop var) ──

#[test]
fn binding_scope_frame_resolves_injected_name() {
    use std::sync::Arc;
    let doc = Document::open(
        r#"
        @schemaless outer {
          inner { greeting = label }
        }
        "#,
        "test",
    )
    .expect("open");
    let outer = doc.block("outer").unwrap();
    let bindings = Arc::new(vec![("label".to_string(), Value::Utf8("hi".into()))]);
    let groups = outer.expand_bodies(&outer, vec![bindings]);
    let inner = groups[0].iter().find(|b| b.kind() == "inner").unwrap();
    assert_eq!(
        inner.field("greeting").unwrap().value().unwrap(),
        &Value::Utf8("hi".into()),
        "an injected binding resolves in a child block's field expression"
    );
}

#[test]
fn binding_scope_frame_is_read_by_interpolation() {
    use std::sync::Arc;
    let doc = Document::open(
        r#"
        @schemaless outer {
          inner { msg = $"value is ${v}" }
        }
        "#,
        "test",
    )
    .expect("open");
    let outer = doc.block("outer").unwrap();
    let bindings = Arc::new(vec![("v".to_string(), Value::I64(42))]);
    let groups = outer.expand_bodies(&outer, vec![bindings]);
    let inner = groups[0].iter().find(|b| b.kind() == "inner").unwrap();
    assert_eq!(
        inner.field("msg").unwrap().value().unwrap(),
        &Value::Utf8("value is 42".into()),
        "${{…}} interpolation reads the injected binding"
    );
}

#[test]
fn binding_scope_frame_shadows_outer_field() {
    use std::sync::Arc;
    // A binding named like an ancestor field shadows it (innermost wins),
    // matching `let` shadowing semantics.
    let doc = Document::open(
        r#"
        @schemaless outer {
          x = "from-field"
          inner { picked = x }
        }
        "#,
        "test",
    )
    .expect("open");
    let outer = doc.block("outer").unwrap();
    // Without bindings, `x` resolves to the outer field.
    let plain = outer.block("inner").unwrap();
    assert_eq!(
        plain.field("picked").unwrap().value().unwrap(),
        &Value::Utf8("from-field".into())
    );
    // With a binding, the binding wins.
    let bindings = Arc::new(vec![("x".to_string(), Value::Utf8("from-binding".into()))]);
    let groups = outer.expand_bodies(&outer, vec![bindings]);
    let inner = groups[0].iter().find(|b| b.kind() == "inner").unwrap();
    assert_eq!(
        inner.field("picked").unwrap().value().unwrap(),
        &Value::Utf8("from-binding".into())
    );
}

#[test]
fn expand_bodies_gives_each_set_independent_caches() {
    use std::sync::Arc;
    // Two binding sets over the same body must NOT collide in the
    // field-value cache (the bug fresh per-expansion cells fix).
    let doc = Document::open(
        r#"
        @schemaless outer {
          inner { v = label }
        }
        "#,
        "test",
    )
    .expect("open");
    let outer = doc.block("outer").unwrap();
    let sets = vec![
        Arc::new(vec![("label".to_string(), Value::Utf8("one".into()))]),
        Arc::new(vec![("label".to_string(), Value::Utf8("two".into()))]),
    ];
    let groups = outer.expand_bodies(&outer, sets);
    let v0 = groups[0]
        .iter()
        .find(|b| b.kind() == "inner")
        .unwrap()
        .field("v")
        .unwrap()
        .value()
        .unwrap()
        .clone();
    let v1 = groups[1]
        .iter()
        .find(|b| b.kind() == "inner")
        .unwrap()
        .field("v")
        .unwrap()
        .value()
        .unwrap()
        .clone();
    assert_eq!(v0, Value::Utf8("one".into()));
    assert_eq!(
        v1,
        Value::Utf8("two".into()),
        "second set must not see the first's cached value"
    );
}

#[test]
fn schemaless_block_passes_validation() {
    // Strict mode: un-schema'd kinds normally error
    // `UnregisteredKind`, but `@schemaless` opts out.
    let doc = Document::open(r#"@schemaless random "label" { x = 1 }"#, "test").expect("open");
    let b = doc.block("random").unwrap();
    assert!(b.schema_errors().is_empty());
}

#[test]
fn unschemad_block_surfaces_unregistered_kind() {
    // The opposite of `schemaless_block_passes_validation` —
    // without `@schemaless`, the un-registered kind itself is
    // an error.
    let doc = Document::open(r#"random "label" { x = 1 }"#, "test").expect("open");
    let b = doc.block("random").unwrap();
    let errs = b.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::UnregisteredKind,
                ..
            }
        )),
        "expected UnregisteredKind, got {errs:?}"
    );
}

// ─── List literals + required_children ───────────────────────────

#[test]
fn eval_list_literal_to_value_list() {
    let doc = open("x = [1, 2, 3]");
    assert_eq!(
        doc.field("x").unwrap().value().unwrap(),
        &Value::List(std::sync::Arc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3)
        ]))
    );
}

#[test]
fn eval_empty_list_literal() {
    let doc = open("x = []");
    assert_eq!(
        doc.field("x").unwrap().value().unwrap(),
        &Value::List(std::sync::Arc::new(vec![]))
    );
}

#[test]
fn eval_nested_list_literal() {
    let doc = open("x = [[1, 2], [3, 4]]");
    let v = doc.field("x").unwrap().value().unwrap();
    let Value::List(outer) = v else {
        panic!("expected outer list")
    };
    assert_eq!(outer.len(), 2);
    assert_eq!(
        outer[0],
        Value::List(std::sync::Arc::new(vec![Value::I64(1), Value::I64(2)]))
    );
}

#[test]
fn eval_list_literal_resolves_identifiers() {
    let doc = open("a = 1\nb = 2\nx = [a, b, 3]");
    assert_eq!(
        doc.field("x").unwrap().value().unwrap(),
        &Value::List(std::sync::Arc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3)
        ]))
    );
}

#[test]
fn eval_identifier_list_field_keeps_bare_names_opaque() {
    // A `list<identifier>`-typed field is the element-wise lift of a
    // scalar `identifier` field: bare-id elements stay opaque names
    // (so they can reference shapes/blocks by id) instead of resolving
    // as bindings. Without the declared type they'd error as unresolved
    // references — see `eval_list_literal_resolves_identifiers`.
    let src = r#"
@document
type Root { members: list<identifier> }
members = [shop, stripe]
"#;
    let doc = Document::open(src, "test").expect("open");
    assert_eq!(
        doc.field("members").unwrap().value().unwrap(),
        &Value::List(std::sync::Arc::new(vec![
            Value::Identifier("shop".into()),
            Value::Identifier("stripe".into()),
        ]))
    );
}

#[test]
fn eval_decorator_arg_with_list_literal() {
    let doc = open(r#"@v(items = [1, 2, 3]) type T {}"#);
    let t = doc.type_decl("T").unwrap();
    let d = t.decorators().next().unwrap();
    let arg = d.named_arg("items").unwrap().unwrap();
    assert_eq!(
        arg,
        Value::List(std::sync::Arc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3)
        ]))
    );
}

#[test]
fn required_children_reads_list_arg() {
    let doc = open(
        r#"
        @block("service", required_children = ["config", "audit"])
        type Service {
          @child("config")  config:  Config?
          @child("audit")   audit:   Audit?
        }
        @block("config") type Config {}
        @block("audit")  type Audit {}
        "#,
    );
    let svc = doc.type_decl("Service").unwrap();
    assert_eq!(
        svc.required_children(),
        vec!["config".to_string(), "audit".to_string()]
    );
}

#[test]
fn required_children_present_no_error() {
    let doc = open(
        r#"
        @block("service", required_children = ["config"])
        type Service {
          @child("config") config: Config?
        }
        @block("config") type Config {}
        service web { config {} }
        "#,
    );
    let svc = doc.block("service").unwrap();
    assert!(svc.schema_errors().is_empty(), "{:?}", svc.schema_errors());
}

#[test]
fn required_children_missing_errors() {
    let doc = open(
        r#"
        @block("service", required_children = ["config"])
        type Service {
          @child("config") config: Config?
        }
        @block("config") type Config {}
        service web {}
        "#,
    );
    let svc = doc.block("service").unwrap();
    let errs = svc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::MissingRequired,
                message,
                ..
            } if message.contains("required child kind 'config'")
        )),
        "expected MissingRequired for kind 'config', got {errs:?}"
    );
}

// ─── Tables (`@table` + pipe-row syntax) ─────────────────────────

fn open_table_doc() -> Document {
    Document::open(
        r#"
        @table("user")
        type User { name: utf8  age: u32  active: bool }

        @block("db")
        type DB { @children("user") users: list<User> }

        db production {
          users:
            | "alice" | 30 | true |
            | "bob"   | 25 | false |
            | "cara"  | 42 | true |
        }
        "#,
        "test",
    )
    .expect("open")
}

#[test]
fn table_schema_lookup() {
    let doc = open_table_doc();
    assert!(doc.table_schema("user").is_some());
    assert!(doc.table_schema("nope").is_none());
}

#[test]
fn child_kind_table_yields_data_kind_table() {
    let doc = open_table_doc();
    let users = doc.get("db.users").expect("db.users");
    assert_eq!(users.kind(), "table");
}

#[test]
fn row_count_matches_source_rows() {
    let doc = open_table_doc();
    let users = doc.get("db.users").unwrap();
    assert_eq!(users.row_count(), Some(3));
    assert_eq!(users.len(), Some(3));
}

#[test]
fn row_returns_block_with_labels() {
    let doc = open_table_doc();
    let users = doc.get("db.users").unwrap();
    let alice = users.row(0).expect("row 0");
    assert_eq!(alice.kind(), "block");
    let block = alice.as_block().unwrap();
    let labels = block.labels().unwrap();
    assert_eq!(labels.len(), 3);
    assert_eq!(labels[0], Value::Utf8("alice".into()));
    // Number literals default to i64 — element-type coercion to the
    // schema's u32 isn't done in this pass.
    assert_eq!(labels[1], Value::I64(30));
    assert_eq!(labels[2], Value::Bool(true));
}

#[test]
fn column_projects_named_field() {
    let doc = open_table_doc();
    let users = doc.get("db.users").unwrap();
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
fn child_kind_block_still_yields_blocklist() {
    // When the kind isn't @table-schema'd, @children still returns
    // a plain DataKind::BlockList.
    let doc = Document::open(
        r#"
        @block("route") type Route { @inline(0) path: utf8 }
        @block("service") type Service { @children("route") routes: list<Route> }
        service web { route "/api" {} route "/healthz" {} }
        "#,
        "test",
    )
    .expect("open");
    let routes = doc.get("service.routes").unwrap();
    assert_eq!(routes.kind(), "block_list");
}

#[test]
fn mixed_rows_and_blocks_in_same_children_field() {
    // Both literal `user { name=...; ... }` blocks and pipe-row
    // entries under `users:` contribute to the same projection.
    let doc = Document::open(
        r#"
        @table("user") type User { name: utf8  age: u32 }
        @block("db") type DB { @children("user") users: list<User> }
        db x {
          users:
            | "row-a" | 1 |
            | "row-b" | 2 |
          user { name = "block-c"  age = 3 }
        }
        "#,
        "test",
    )
    .expect("open");
    let users = doc.get("db.users").unwrap();
    // BlockList because mixing with a `@block`-form? No — User is
    // @table; result is still Table.
    assert_eq!(users.kind(), "table");
    // Three entries total: two synthesised rows + one literal
    // block.
    assert_eq!(users.row_count(), Some(3));
    let names = users.column("name").unwrap();
    assert_eq!(
        names,
        vec![
            Value::Utf8("row-a".into()),
            Value::Utf8("row-b".into()),
            Value::Utf8("block-c".into()),
        ]
    );
}

#[test]
fn row_column_count_mismatch_errors() {
    let doc = Document::open(
        r#"
        @table("user") type User { name: utf8  age: u32 }
        @block("db") type DB { @children("user") users: list<User> }
        db x {
          users:
            | "alice" | 30 |
            | "bob"   |
        }
        "#,
        "test",
    )
    .expect("open");
    let users = doc.get("db.users").unwrap();
    let bob = users.row(1).unwrap().as_block().unwrap();
    let errs = bob.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::ChildrenTooFew,
                ..
            }
        )),
        "expected ChildrenTooFew for short row, got {errs:?}"
    );
}

#[test]
fn required_children_non_string_entries_ignored() {
    let doc = open(
        r#"
        @block("service", required_children = ["config", 42, true])
        type Service { @child("config") config: Config? }
        @block("config") type Config {}
        service web { config {} }
        "#,
    );
    let svc = doc.type_decl("Service").unwrap();
    // Only the string entry survives.
    assert_eq!(svc.required_children(), vec!["config".to_string()]);
}

// ─── References (scope-aware lookup + parent/self) ────────────────

fn open_refs() -> Document {
    Document::open(
        r#"
        @table("user") type User { name: utf8  age: u32 }
        @block("db") type DB {
          @children("user") users: list<User>
          active: &User
          pinned: &User
        }
        db production {
          users:
            | "alice" | 30 |
            | "bob"   | 25 |
            | "cara"  | 42 |
          active = users.alice
          pinned = users.bob
        }
        "#,
        "refs",
    )
    .expect("open refs")
}

#[test]
fn parser_accepts_self_and_parent_in_expression_position() {
    let doc = Document::open(
        r#"
        @schemaless anchor = 1
        @schemaless x = parent
        @schemaless y = self
        "#,
        "test",
    )
    .expect("parses");
    // Just confirm the field eval path triggers — `parent` at
    // doc root errors, `self` resolves to the document.
    let parent_err = doc.field("x").unwrap().value().unwrap_err();
    assert!(
        matches!(parent_err, EvalError::UnresolvedReference { .. }),
        "{parent_err:?}"
    );
    // `self` at the doc root yields the document, which is not a
    // leaf so materialise_dataref returns NotALeaf.
    let self_err = doc.field("y").unwrap().value().unwrap_err();
    assert!(
        matches!(self_err, EvalError::NotALeaf { .. }),
        "{self_err:?}"
    );
}

#[test]
fn parent_keyword_remains_valid_as_type_field_name() {
    // Existing source like `type User { parent: &User? }` must
    // keep parsing — `parent`/`self` are contextual keywords,
    // only special in expression atom position.
    let doc = Document::open(r#"type User { parent: &User? }"#, "test").expect("parses");
    let user = doc.type_decl("User").unwrap();
    assert!(user.field("parent").is_some());
}

#[test]
fn ref_field_reference_returns_dataref_navigator() {
    let doc = open_refs();
    let active = doc.get("db.active").expect("db.active present");
    let target = active
        .reference()
        .expect("reference() returns Some for &T field")
        .expect("ref resolves");
    // The target is the synthesised row block for "alice".
    let labels = target.as_block().unwrap().labels().unwrap();
    assert_eq!(labels.first(), Some(&Value::Utf8("alice".into())));
}

#[test]
fn ref_field_value_auto_derefs_to_target_leaf() {
    // `pinned = users.bob` resolves to the bob row (a Block).
    // Reading the field surfaces a `Value::DataPath` carrying the
    // source-level segments so reflective builtins
    // (`decorator_names`, …) can keep walking. Host code that
    // wants a `DataRef` should call `.reference()` instead.
    let doc = open_refs();
    let pinned = doc.get("db.pinned").expect("db.pinned present");
    let v = pinned.value().expect("ref field reads as a DataPath value");
    match v {
        Value::DataPath { kind, segments } => {
            assert_eq!(kind, "block");
            assert_eq!(segments, vec!["users".to_string(), "bob".to_string()]);
        }
        other => panic!("expected Value::DataPath, got {other:?}"),
    }
}

#[test]
fn non_ref_field_keeps_value_path() {
    // `count = 3` is a plain leaf assignment; no reference.
    let doc = Document::open("@schemaless count = 3", "t").unwrap();
    let count = doc.field("count").unwrap();
    assert_eq!(count.value().unwrap(), &Value::I64(3));
    assert!(count.reference().is_none());
}

#[test]
fn unresolved_member_path_errors_only_when_value_called() {
    // `&User` field whose RHS dotted path points to nothing.
    let doc = Document::open(
        r#"
        @block("db") type DB { dangling: &Sentinel }
        type Sentinel {}
        db x { dangling = somewhere.nowhere }
        "#,
        "t",
    )
    .expect("opens (validation is lazy)");
    let db = doc.block("db").unwrap();
    let dangling = db.field("dangling").unwrap();
    let r = dangling.reference().expect("&T field");
    match r {
        Ok(_) => panic!("expected UnresolvedReference"),
        Err(e) => assert!(matches!(e, EvalError::UnresolvedReference { .. }), "{e:?}"),
    }
}

#[test]
fn self_inside_block_returns_current_block_dataref() {
    // `self` inside a block resolves to the enclosing block.
    // Reading a child of that DataRef should produce the same
    // value as reading the child directly.
    let doc = Document::open(
        r#"
        @block("svc") type Svc { port: u32  echo: &Svc }
        svc web { port = 8080  echo = self }
        "#,
        "t",
    )
    .expect("opens");
    let svc = doc.get("svc").unwrap();
    let echo = svc.child("echo").unwrap();
    let target = echo.reference().unwrap().unwrap();
    // self → enclosing svc block; reading port through it.
    let port = target.child("port").unwrap().value().unwrap();
    assert_eq!(port, Value::I64(8080));
}

#[test]
fn parent_in_nested_block_walks_up_one_level() {
    let doc = Document::open(
        r#"
        @block("outer") type Outer { name: utf8 @child("inner") inner: Inner? }
        @block("inner") type Inner { up: &Outer }
        outer top { name = "the-top"  inner { up = parent } }
        "#,
        "t",
    )
    .expect("opens");
    let up = doc.get("outer.inner.up").unwrap();
    let target = up.reference().unwrap().unwrap();
    let name = target.child("name").unwrap().value().unwrap();
    assert_eq!(name, Value::Utf8("the-top".into()));
}

// ─── Strict schema validation ─────────────────────────────────────

#[test]
fn doc_schema_resolves_when_present() {
    let doc = Document::open(
        r#"
        @document type Root { name: utf8 }
        @schemaless name = "x"
        "#,
        "t",
    )
    .expect("opens");
    assert!(doc.doc_schema().is_some());
    assert_eq!(doc.doc_schema().unwrap().name(), "Root");
}

#[test]
fn doc_schema_absent_returns_none() {
    let doc = Document::open("@schemaless name = \"x\"", "t").unwrap();
    assert!(doc.doc_schema().is_none());
}

#[test]
fn multiple_document_decls_surface_error() {
    let doc = Document::open(
        r#"
        @document type A { x: utf8 }
        @document type B { y: utf8 }
        "#,
        "t",
    )
    .expect("opens");
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::MultipleDocumentSchemas,
                ..
            }
        )),
        "{errs:?}"
    );
}

#[test]
fn top_level_field_without_doc_schema_errors_on_value() {
    let doc = Document::open(r#"orphan = "x""#, "t").unwrap();
    let err = doc.field("orphan").unwrap().value().unwrap_err();
    assert!(
        matches!(
            err,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::NoDocumentSchema,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn top_level_field_with_matching_doc_schema_resolves() {
    let doc = Document::open(
        r#"
        @document type Cfg { name: utf8 }
        name = "alpha"
        "#,
        "t",
    )
    .unwrap();
    assert_eq!(
        doc.field("name").unwrap().value().unwrap(),
        &Value::Utf8("alpha".into())
    );
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn top_level_field_with_schemaless_decorator_resolves() {
    let doc = Document::open(r#"@schemaless port = 8080"#, "t").unwrap();
    assert_eq!(
        doc.field("port").unwrap().value().unwrap(),
        &Value::I64(8080)
    );
}

#[test]
fn top_level_unregistered_block_kind_errors() {
    let doc = Document::open(r#"random "x" { y = 1 }"#, "t").unwrap();
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::NoDocumentSchema
                    | crate::error::SchemaViolationKind::UnregisteredKind,
                ..
            }
        )),
        "{errs:?}"
    );
}

#[test]
fn top_level_block_kind_disallowed_by_doc_schema_errors() {
    let doc = Document::open(
        r#"
        @document type Cfg { @child("svc") svc: Svc }
        @block("svc") type Svc { @inline(0) id: utf8 }
        @block("other") type Other {}
        other "x" {}
        "#,
        "t",
    )
    .unwrap();
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::DisallowedChild,
                ..
            }
        )),
        "{errs:?}"
    );
}

// ─── Document-schema composition (issue #10) ──────────────────────

/// Open `user_src` as the root document with each `(key, src)` pair
/// registered as a system-importable library file (`import <key>`).
/// Mirrors how `wcl wdoc` serves its embedded stdlib.
fn open_with_libs(user_src: &str, libs: &[(&str, &str)]) -> Document {
    let mut reg = Registry::new();
    for (key, src) in libs {
        reg.register(key.to_string(), src.to_string());
    }
    let loader = reg.loader(disk_loader());
    Document::open_at_with_loader(user_src, "t", None, &Environment::new(), loader).expect("opens")
}

const LIB_PAGE: &str = r#"
@document type LibRoot { @children("page") pages: list<Page> }
@block("page") type Page { @inline(0) id: utf8 }
"#;

#[test]
fn imported_document_alone_governs_root() {
    // Backward compatibility: with only the imported (library)
    // @document and no root-authored one, the root is validated
    // against the library schema exactly as before.
    let doc = open_with_libs(
        "import <lib.wcl>\npage \"home\" {}\n",
        &[("lib.wcl", LIB_PAGE)],
    );
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn imported_document_still_rejects_unknown_kind() {
    let doc = open_with_libs(
        "import <lib.wcl>\nwidget \"x\" {}\n",
        &[("lib.wcl", LIB_PAGE)],
    );
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::UnregisteredKind
                    | crate::error::SchemaViolationKind::DisallowedChild,
                ..
            }
        )),
        "{errs:?}"
    );
}

#[test]
fn root_document_composes_with_imported_library_schema() {
    // The user declares their *own* @document at the root; it merges
    // with the imported library @document. Both the library's `page`
    // block and the user's own `widget` block are allowed, with no
    // MultipleDocumentSchemas error.
    let user = r#"
        import <lib.wcl>
        @document type UserRoot { @children("widget") widgets: list<Widget> }
        @block("widget") type Widget { @inline(0) id: utf8 }
        page "home" {}
        widget "side" {}
    "#;
    let doc = open_with_libs(user, &[("lib.wcl", LIB_PAGE)]);
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn two_imported_documents_merge_without_conflict() {
    let lib1 = r#"
        @document type R1 { @children("page") pages: list<Page> }
        @block("page") type Page { @inline(0) id: utf8 }
    "#;
    let lib2 = r#"
        @document type R2 { @children("note") notes: list<Note> }
        @block("note") type Note { @inline(0) id: utf8 }
    "#;
    let user = "import <lib1.wcl>\nimport <lib2.wcl>\npage \"h\" {}\nnote \"n\" {}\n";
    let doc = open_with_libs(user, &[("lib1.wcl", lib1), ("lib2.wcl", lib2)]);
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn root_field_declared_only_by_user_document_resolves() {
    // The lazy field path must also consult the merged schema: a
    // top-level field declared by the user's own @document (not the
    // imported one) resolves without UnknownField.
    let user = r#"
        import <lib.wcl>
        @document type UserRoot { title: utf8 }
        title = "hello"
    "#;
    let doc = open_with_libs(user, &[("lib.wcl", LIB_PAGE)]);
    assert_eq!(
        doc.field("title").unwrap().value().unwrap(),
        &Value::Utf8("hello".into())
    );
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn root_field_declared_only_by_imported_document_resolves() {
    let lib = r#"
        @document type LibRoot { title: utf8 }
    "#;
    let doc = open_with_libs("import <lib.wcl>\ntitle = \"x\"\n", &[("lib.wcl", lib)]);
    assert_eq!(
        doc.field("title").unwrap().value().unwrap(),
        &Value::Utf8("x".into())
    );
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn connections_field_resolves_across_imported_files() {
    // A `@connections` field declared on an imported `@document` must be
    // name-resolvable from a *different* file, exactly like a `@children`
    // field. The page lib imports first so its `@document` sorts ahead of
    // the connection-bearing one — the buggy `doc_schema()` lookup would
    // pick that wrong schema and fail with `unresolved reference`.
    let page_lib = r#"
        @document type LibRoot { @children("page") pages: list<Page> }
        @block("page") type Page { @inline(0) id: utf8 }
    "#;
    let model = r#"
        @block("system") type System { @inline(0) id: utf8  name: utf8 }
        @block("user")   type User   { @inline(0) id: utf8  name: utf8 }
        symbol_set RelKind { uses }
        connection PersonToSystem: User -> System : RelKind
        @document type Model {
            @children("system")          systems: list<System>
            @children("user")            users:   list<User>
            @connections(PersonToSystem) person_to_system: list<PersonToSystem>
        }
        system "web"      { name = "Web" }
        user   "customer" { name = "Customer" }
        customer -> web :uses
    "#;
    // `edges` / `users_count` are authored in the user-root file — a
    // different file than the @document + arrows (model.wcl). The
    // root-authored @document declares them so they're legal top-level
    // fields; it composes with the imported library schemas.
    let user = "import <page.wcl>\nimport <model.wcl>\n\
         @document type UserRoot { edges: i64  users_count: i64 }\n\
         edges = len(person_to_system)\nusers_count = len(users)\n";
    let doc = open_with_libs(user, &[("page.wcl", page_lib), ("model.wcl", model)]);
    // The @connections read (the bug) and the @children control must both
    // resolve to 1.
    assert_eq!(doc.field("edges").unwrap().value().unwrap(), &Value::I64(1));
    assert_eq!(
        doc.field("users_count").unwrap().value().unwrap(),
        &Value::I64(1)
    );
}

#[test]
fn connection_operand_label_referencing_connections_field_terminates() {
    // A block whose identifying label is computed from the `@connections`
    // field must not send connection-operand resolution into unbounded
    // recursion (projection → operand resolution → label eval → projection
    // → … stack overflow). The reentrancy guard suppresses projection while
    // a label is being evaluated for operand identification, so evaluation
    // terminates instead.
    let src = r#"
        @block("system") type System { @inline(0) id: utf8 }
        @block("user")   type User   { @inline(0) id: utf8 }
        symbol_set RelKind { uses }
        connection PersonToSystem: User -> System : RelKind
        @document type Model {
            @children("system")          systems: list<System>
            @children("user")            users:   list<User>
            @connections(PersonToSystem) person_to_system: list<PersonToSystem>
            count: i64
        }
        system "web"                                {}
        system $"sys-${len(person_to_system)}"      {}
        user   "customer"                           {}
        customer -> web :uses
        count = len(person_to_system)
    "#;
    let doc = open_with_libs(src, &[]);
    // Must return (not overflow). The block with the self-referential label
    // simply fails to match during operand resolution and is skipped; the
    // single `customer -> web` arrow projects to one edge.
    assert_eq!(doc.field("count").unwrap().value().unwrap(), &Value::I64(1));
}

/// Schema shared by the `@dynamic` projection tests: a `@dynamic`
/// connection plus a document that projects it, one literal `node "a"`,
/// and a literal `probe` field that surfaces the projected `edges` list
/// (a projected field isn't a literal `Field`, so `doc.field("edges")`
/// can't read it directly — `probe = edges` materialises it).
const DYNAMIC_CONN_SRC: &str = r#"
    @block("node") type Node { @inline(0) id: utf8 }
    symbol_set EdgeKind { default flow }
    @dynamic
    connection Edge: Node -> Node : EdgeKind
    @document type Model {
        @children("node") nodes: list<Node>
        @connections(Edge) edges: list<Edge>
        probe: list<Edge>
    }
    node "a" {}
    probe = edges
"#;

fn edge_records(doc: &Document) -> Vec<(String, String, String)> {
    let Value::List(items) = doc.field("probe").unwrap().value().unwrap().clone() else {
        panic!("probe field is not a list");
    };
    std::sync::Arc::unwrap_or_clone(items)
        .into_iter()
        .map(|v| {
            let Value::Record { fields, .. } = v else {
                panic!("edge is not a record");
            };
            let endpoint = |f: &Value| match f {
                Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => s.clone(),
                other => panic!("unexpected endpoint value: {other:?}"),
            };
            let Value::Symbol(kind) = fields.get("kind").unwrap().clone() else {
                panic!("kind is not a symbol");
            };
            (
                endpoint(fields.get("source").unwrap()),
                endpoint(fields.get("destination").unwrap()),
                kind,
            )
        })
        .collect()
}

#[test]
fn dynamic_connection_projects_raw_endpoint_for_unresolved_operand() {
    // `a` is a literal node; `gen_b` is not — it stands in for an id a
    // `wdoc_repeater` would generate. Under a `@dynamic` connection the
    // edge is still projected, carrying the raw `gen_b` string so a
    // downstream consumer can match it against a generated shape id.
    let src = format!("{DYNAMIC_CONN_SRC}\n        a -> gen_b\n");
    let doc = open(&src);
    assert_eq!(
        edge_records(&doc),
        vec![("a".to_string(), "gen_b".to_string(), "default".to_string())]
    );
}

#[test]
fn dynamic_connection_projects_when_both_operands_unresolved() {
    // Both endpoints are generated ids (FK between two repeater columns).
    let src = format!("{DYNAMIC_CONN_SRC}\n        gen_a -> gen_b :flow\n");
    let doc = open(&src);
    assert_eq!(
        edge_records(&doc),
        vec![("gen_a".to_string(), "gen_b".to_string(), "flow".to_string())]
    );
}

#[test]
fn non_dynamic_connection_drops_unresolved_operand() {
    // Same shape WITHOUT `@dynamic`: the unresolved `gen_b` makes the edge
    // record drop entirely (strict, pre-feature behaviour preserved).
    let src = DYNAMIC_CONN_SRC.replace("@dynamic\n    ", "");
    let src = format!("{src}\n        a -> gen_b\n");
    let doc = open(&src);
    assert!(
        edge_records(&doc).is_empty(),
        "non-@dynamic connection should drop an unresolved-operand edge"
    );
}

#[test]
fn connection_operand_resolves_block_by_opaque_label_not_by_reference() {
    // A connection operand identifies a block by its label *name*, treated
    // as an opaque literal — not by resolving that name as a reference.
    // Here a node's label (`src_name`) collides with a top-level field of a
    // different value; the node's identity stays `src_name` (so
    // `src_name -> target` projects) rather than the field's `"shadow"`.
    // `match_block_first_label` keeps this O(1) via `eval_literal` (resolving
    // each label as a reference instead is what made the operand-index build
    // pathologically slow on a large document).
    let src = r#"
        @block("node") type Node { @inline(0) id: identifier }
        symbol_set EdgeKind { default flow }
        connection Edge: Node -> Node : EdgeKind
        @document type Model {
            @children("node") nodes: list<Node>
            @connections(Edge) edges: list<Edge>
            src_name: utf8
            probe: list<Edge>
        }
        src_name = "shadow"
        node src_name {}
        node target {}
        src_name -> target :flow
        probe = edges
    "#;
    let doc = open(src);
    assert_eq!(
        edge_records(&doc),
        vec![(
            "src_name".to_string(),
            "target".to_string(),
            "flow".to_string()
        )]
    );
}

fn has_unknown_operand_error(doc: &Document) -> bool {
    doc.schema_errors().iter().any(|e| {
        matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::UnknownConnectionOperand,
                ..
            }
        )
    })
}

#[test]
fn validate_suppresses_unknown_operand_for_dynamic_connection() {
    // `wcl check` must NOT flag `a -> gen_b` when Edge is `@dynamic`:
    // `gen_b` may be a render-time-generated id.
    let src = format!("{DYNAMIC_CONN_SRC}\n        a -> gen_b\n");
    let doc = open(&src);
    assert!(
        !has_unknown_operand_error(&doc),
        "a @dynamic connection should not flag a generated-id operand:\n{:?}",
        doc.schema_errors()
    );
}

#[test]
fn validate_keeps_unknown_operand_error_without_dynamic() {
    // Drop `@dynamic` and the same statement must still be flagged — static
    // diagrams keep full typo-catching.
    let src = DYNAMIC_CONN_SRC.replace("@dynamic\n    ", "");
    let src = format!("{src}\n        a -> gen_b\n");
    let doc = open(&src);
    assert!(
        has_unknown_operand_error(&doc),
        "a non-@dynamic connection must still flag an unresolved operand"
    );
}

fn has_unknown_connection_error(doc: &Document) -> bool {
    doc.schema_errors().iter().any(|e| {
        matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::UnknownConnection,
                ..
            }
        )
    })
}

#[test]
fn interface_connection_endpoint_spans_every_conforming_pair() {
    // `connection Rel: &Entity -> &Entity` admits any pair of blocks
    // whose concrete types implement `Entity` — one schema for all pairs.
    let src = r#"
        @block("component") type Component { @inline(0) id: identifier  name: utf8 }
        @block("procedure") type Procedure { @inline(0) id: identifier  name: utf8 }
        interface Entity { name: utf8 }
        symbol_set RelKind { implements }
        connection Rel: &Entity -> &Entity : RelKind
        @document type M {
            @children("component") comps: list<Component>
            @children("procedure") procs: list<Procedure>
            @connections(Rel) rels: list<Rel>
        }
        component "auth" { name = "Auth" }
        procedure "login" { name = "Login" }
        auth -> login :implements
    "#;
    let doc = Document::open(src, "test").unwrap();
    assert!(
        !has_unknown_connection_error(&doc),
        "&Iface endpoint should admit Component -> Procedure: {:?}",
        doc.schema_errors()
    );
}

#[test]
fn interface_connection_endpoint_rejects_non_conforming_operand() {
    // A block whose concrete type does NOT implement the endpoint
    // interface is still rejected (no silent acceptance).
    let src = r#"
        @block("component") type Component { @inline(0) id: identifier  name: utf8 }
        @block("gadget") type Gadget { @inline(0) id: identifier }
        interface Entity { name: utf8 }
        symbol_set RelKind { implements }
        connection Rel: &Entity -> &Entity : RelKind
        @document type M {
            @children("component") comps: list<Component>
            @children("gadget") gadgets: list<Gadget>
            @connections(Rel) rels: list<Rel>
        }
        component "auth" { name = "Auth" }
        gadget "g1" {}
        auth -> g1 :implements
    "#;
    let doc = Document::open(src, "test").unwrap();
    assert!(
        has_unknown_connection_error(&doc),
        "a non-conforming operand (Gadget) must be rejected: {:?}",
        doc.schema_errors()
    );
}

#[test]
fn union_connection_endpoint_admits_variant_members() {
    // `connection Rel: AnyEnt -> AnyEnt` admits any pair whose concrete
    // types are variant members of the union.
    let src = r#"
        @block("component") type Component { @inline(0) id: identifier  name: utf8 }
        @block("procedure") type Procedure { @inline(0) id: identifier  name: utf8 }
        union AnyEnt { C Component  P Procedure }
        symbol_set RelKind { implements }
        connection Rel: AnyEnt -> AnyEnt : RelKind
        @document type M {
            @children("component") comps: list<Component>
            @children("procedure") procs: list<Procedure>
            @connections(Rel) rels: list<Rel>
        }
        component "auth" { name = "Auth" }
        procedure "login" { name = "Login" }
        auth -> login :implements
    "#;
    let doc = Document::open(src, "test").unwrap();
    assert!(
        !has_unknown_connection_error(&doc),
        "union endpoint should admit Component -> Procedure: {:?}",
        doc.schema_errors()
    );
}

#[test]
fn second_root_document_still_errors_alongside_import() {
    // A *root-authored* second @document is still an error even when a
    // library one is imported — only one root-authored @document per
    // namespace is allowed.
    let user = r#"
        import <lib.wcl>
        @document type A { x: utf8 }
        @document type B { y: utf8 }
    "#;
    let doc = open_with_libs(user, &[("lib.wcl", LIB_PAGE)]);
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::MultipleDocumentSchemas,
                ..
            }
        )),
        "{errs:?}"
    );
}

// A library that lives in its own `namespace wdoc`, declaring a couple
// of block kinds — models the wdoc stdlib for the namespace-collision
// tests below (issue #14).
const LIB_NS: &str = r#"
namespace wdoc
@document type Site {
    @children("page") pages: list<Page>
    @children("process") procs: list<Process>
}
@block("page") type Page { @inline(0) id: utf8 }
@block("process") type Process { @inline(0) text: utf8  x: f64? }
"#;

#[test]
fn user_block_kind_shadows_imported_same_kind() {
    // Issue #14: a user `@block("process")` deterministically wins over
    // the imported (stdlib) one for a bare `process` reference — no
    // silent collision. `cost` is a MyProcess-only field.
    let user = r#"
        import <wdoc.wcl>
        @block("process") type MyProcess { @inline(0) text: utf8  cost: i64 }
        process "mine" { cost = 5 }
    "#;
    let doc = open_with_libs(user, &[("wdoc.wcl", LIB_NS)]);
    let s = doc.block_schema("process").expect("process schema");
    assert_eq!(s.name(), "MyProcess");
    assert!(!s.is_imported(), "local declaration should win");
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn qualified_kind_selects_imported_namespace() {
    // `wdoc::process` explicitly selects the imported stdlib `Process`,
    // even though a local `@block("process")` shadows the bare kind.
    let user = r#"
        import <wdoc.wcl>
        @block("process") type MyProcess { @inline(0) text: utf8  cost: i64 }
    "#;
    let doc = open_with_libs(user, &[("wdoc.wcl", LIB_NS)]);
    let s = doc
        .block_schema_in(&["wdoc".to_string()], "process", &[])
        .expect("qualified process schema");
    assert_eq!(s.name(), "Process");
    assert!(s.is_imported(), "qualifier should select the imported one");
}

#[test]
fn bare_kind_falls_back_to_imported_when_not_shadowed() {
    // No local `page` declaration ⇒ a bare `page` resolves to the
    // imported stdlib `Page` (the no-collision case is unchanged).
    let doc = open_with_libs("import <wdoc.wcl>\n", &[("wdoc.wcl", LIB_NS)]);
    let s = doc.block_schema("page").expect("page schema");
    assert_eq!(s.name(), "Page");
    assert!(s.is_imported());
}

#[test]
fn user_plus_imported_same_kind_is_not_a_duplicate_error() {
    // A user `@block("process")` alongside the imported one lives in a
    // *different* namespace, so it is NOT a DuplicateBlockKind error.
    let user = r#"
        import <wdoc.wcl>
        @block("process") type MyProcess { @inline(0) text: utf8  cost: i64 }
    "#;
    let doc = open_with_libs(user, &[("wdoc.wcl", LIB_NS)]);
    assert!(
        !doc.schema_errors().iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::DuplicateBlockKind,
                ..
            }
        )),
        "{:?}",
        doc.schema_errors()
    );
}

#[test]
fn duplicate_root_block_kind_same_namespace_errors() {
    // Two root-authored `@block("foo")` in the same namespace are
    // genuinely ambiguous ⇒ DuplicateBlockKind.
    let doc = Document::open(
        r#"
        @block("foo") type A { @inline(0) id: utf8 }
        @block("foo") type B { @inline(0) id: utf8 }
        "#,
        "t",
    )
    .expect("opens");
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::DuplicateBlockKind,
                ..
            }
        )),
        "{errs:?}"
    );
}

// ─── Namespace-scoped resolution across files ─────────────────────
//
// Regression tests for the namespaced-schema bugs found by the `wad`
// migration: a bare kind must prefer its own namespace's declaration
// even when the declaration and the instance live in *different files*
// (independent of root import order), and `connection` dispatch must
// resolve endpoint types relative to the declaring file's namespace.

/// A library whose `decision` kind collides with `lib2`'s below.
const COLLIDING_NS: &str = r#"
namespace other
@document type OtherRoot { @children("card") cards: list<Card> }
@block("card") type Card { @inline(0) id: utf8 }
@block("decision") type OtherDecision { @inline(0) id: utf8  shape: utf8 }
"#;

const LIB2_SCHEMA: &str = r#"
namespace lib2
@block("decision") type Decision { @inline(0) id: utf8  name: utf8 }
@document type Model2 { @children("decision") decisions: list<Decision> }
"#;

const LIB2_DATA: &str = "namespace lib2\ndecision \"d1\" { name = \"First\" }\n";

#[test]
fn same_namespace_kind_wins_across_files_regardless_of_import_order() {
    // The `@block("decision")` declaration and the instance live in
    // *different files* of the same namespace (`lib2`), while another
    // imported library (`other`) declares a colliding kind. The
    // instance must resolve to lib2's schema whichever library the
    // root imports first — `name` is a lib2.Decision-only field, so
    // the wrong winner surfaces as UnknownField.
    for user in [
        "import <colliding.wcl>\nimport <lib2_schema.wcl>\nimport <lib2_data.wcl>\n",
        "import <lib2_schema.wcl>\nimport <lib2_data.wcl>\nimport <colliding.wcl>\n",
    ] {
        let doc = open_with_libs(
            user,
            &[
                ("colliding.wcl", COLLIDING_NS),
                ("lib2_schema.wcl", LIB2_SCHEMA),
                ("lib2_data.wcl", LIB2_DATA),
            ],
        );
        let block = doc
            .blocks()
            .find(|b| b.kind() == "decision")
            .expect("decision block");
        let schema = block.schema().expect("decision schema");
        assert_eq!(schema.full_name(), "lib2.Decision", "for root:\n{user}");
        assert!(
            doc.schema_errors().is_empty(),
            "for root:\n{user}\n{:?}",
            doc.schema_errors()
        );
    }
}

/// `namespace lib` schema library owning blocks, a connection and the
/// `@document` slots that project it.
const CONN_LIB: &str = r#"
namespace lib
@block("adr") type Adr { @inline(0) id: utf8  name: utf8 }
symbol_set LibRelKind { affects }
connection AdrAffectsAdr : Adr -> Adr : LibRelKind
@document type Model {
    @children("adr") adrs: list<Adr>
    @connections(AdrAffectsAdr) adr_affects: list<AdrAffectsAdr>
}
"#;

#[test]
fn namespaced_connection_dispatches_for_root_namespace_arrows() {
    // connection + endpoint types + @connections slot all live in
    // `namespace lib`; the data (blocks + arrow) is authored in the
    // root namespace. The arrow must dispatch to lib's connection
    // schema (`no connection schema accepts` was the bug) and the
    // projected slot must populate.
    let user = r#"
        import <conn_lib.wcl>
        @document type UserRoot { edges: i64 }
        adr "a1" { name = "First" }
        adr "a2" { name = "Second" }
        a1 -> a2 :affects
        edges = len(adr_affects)
    "#;
    let doc = open_with_libs(user, &[("conn_lib.wcl", CONN_LIB)]);
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    assert_eq!(doc.field("edges").unwrap().value().unwrap(), &Value::I64(1));
}

#[test]
fn namespaced_arrows_and_qualified_instances_dispatch() {
    // Arrows authored inside a `namespace lib` data file, plus a
    // root-file `lib::adr`-qualified instance wired by a root-file
    // arrow. Both must dispatch through lib's connection schema.
    let lib_data = r#"
namespace lib
adr "a1" { name = "First" }
adr "a2" { name = "Second" }
a1 -> a2 :affects
"#;
    let user = r#"
        import <conn_lib.wcl>
        import <lib_data.wcl>
        @document type UserRoot { edges: i64 }
        lib::adr "a3" { name = "Third" }
        a1 -> a3 :affects
        edges = len(adr_affects)
    "#;
    let doc = open_with_libs(
        user,
        &[("conn_lib.wcl", CONN_LIB), ("lib_data.wcl", lib_data)],
    );
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    assert_eq!(doc.field("edges").unwrap().value().unwrap(), &Value::I64(2));
}

#[test]
fn nested_block_kind_unregistered_errors_on_schema_errors() {
    // Parent schema explicitly allows `unregistered` as a child
    // (so DisallowedChild doesn't fire), but `unregistered` has
    // no @block/@table declaration — that's the UnregisteredKind
    // violation.
    let doc = Document::open(
        r#"
        @document type Cfg { @child("svc") svc: Svc }
        @block("svc") type Svc { @child("unregistered") nested: Whatever? }
        type Whatever {}
        svc "x" { unregistered { y = 1 } }
        "#,
        "t",
    )
    .unwrap();
    let svc = doc.block("svc").unwrap();
    let nested = svc.block("unregistered").unwrap();
    let errs = nested.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::UnregisteredKind,
                ..
            }
        )),
        "{errs:?}"
    );
}

#[test]
fn nested_block_under_schemaless_parent_silently_passes() {
    let doc = Document::open(
        r#"
        @document type Cfg { @child("wrapper") wrapper: Wrapper }
        @block("wrapper") type Wrapper {}
        @schemaless wrapper "x" { whatever { junk = 1 } }
        "#,
        "t",
    )
    .unwrap();
    // @schemaless on `wrapper "x"` silences the kid's
    // UnregisteredKind that would otherwise fire on `whatever`.
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn field_not_in_block_schema_errors_on_value() {
    let doc = Document::open(
        r#"
        @document type Cfg { @child("svc") svc: Svc }
        @block("svc") type Svc { name: utf8 }
        svc "x" { name = "ok"  surprise = 1 }
        "#,
        "t",
    )
    .unwrap();
    let svc = doc.block("svc").unwrap();
    let err = svc.field("surprise").unwrap().value().unwrap_err();
    assert!(
        matches!(
            err,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::UnknownField,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn field_marked_schemaless_resolves_even_when_unknown() {
    let doc = Document::open(
        r#"
        @document type Cfg { @child("svc") svc: Svc }
        @block("svc") type Svc { name: utf8 }
        svc "x" { name = "ok"  @schemaless surprise = 1 }
        "#,
        "t",
    )
    .unwrap();
    let svc = doc.block("svc").unwrap();
    let v = svc.field("surprise").unwrap().value().unwrap();
    assert_eq!(v, &Value::I64(1));
}

#[test]
fn schemaless_type_declaration_opens_all_instances() {
    // `@schemaless` on the *type* propagates to every instance: an
    // undeclared field resolves (no membership error) and the strict
    // validator passes — exactly as if each instance were marked
    // `@schemaless`. (Powers open, dynamic kinds like `wdoc_instance`,
    // whose forwarded fields can't be declared up front.)
    let doc = Document::open(
        r#"
        @document type Cfg { @child("svc") svc: Svc }
        @block("svc") @schemaless type Svc { name: utf8 }
        svc "x" { name = "ok"  surprise = 1 }
        "#,
        "t",
    )
    .unwrap();
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    let svc = doc.block("svc").unwrap();
    // The declared field still reads, and the undeclared one resolves
    // instead of erroring with UnknownField.
    assert_eq!(
        svc.field("name").unwrap().value().unwrap(),
        &Value::Utf8("ok".into())
    );
    assert_eq!(
        svc.field("surprise").unwrap().value().unwrap(),
        &Value::I64(1)
    );
}

// ─── Interfaces and `extends` ─────────────────────────────────────

#[test]
fn interface_declares_and_lookup_returns_some() {
    let doc = Document::open(
        r#"
        interface Drawable { bounds: utf8 }
        "#,
        "t",
    )
    .unwrap();
    let iface = doc.interface("Drawable").expect("interface present");
    assert_eq!(iface.name(), "Drawable");
    let fields: Vec<_> = iface.fields().map(|f| f.name().to_string()).collect();
    assert_eq!(fields, vec!["bounds".to_string()]);
}

#[test]
fn type_and_interface_with_same_name_clash() {
    let err = Document::open(
        r#"
        type Foo { x: utf8 }
        interface Foo { x: utf8 }
        "#,
        "t",
    )
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("duplicate declaration"), "{msg}");
}

#[test]
fn interface_in_bare_position_errors_at_open() {
    let err = Document::open(
        r#"
        interface Drawable { x: utf8 }
        type Holder { bare: Drawable }
        "#,
        "t",
    )
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("must be used through a reference"), "{msg}");
}

#[test]
fn interface_in_reference_position_resolves() {
    Document::open(
        r#"
        interface Drawable { x: utf8 }
        type Holder { d: &Drawable }
        "#,
        "t",
    )
    .expect("opens");
}

#[test]
fn interface_inside_list_under_reference_is_allowed() {
    Document::open(
        r#"
        interface Drawable { x: utf8 }
        type Holder { ds: list<&Drawable> }
        "#,
        "t",
    )
    .expect("opens");
}

#[test]
fn interface_in_function_param_under_reference_is_allowed() {
    Document::open(
        r#"
        interface Drawable { x: utf8 }
        type Holder { f: fn(&Drawable) -> i32 }
        "#,
        "t",
    )
    .expect("opens");
}

#[test]
fn extends_unknown_parent_errors_at_open() {
    let err = Document::open(
        r#"
        type Dog extends Animal { breed: utf8 }
        "#,
        "t",
    )
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("unknown extends target"), "{msg}");
}

#[test]
fn cyclic_extends_errors_at_open() {
    let err = Document::open(
        r#"
        type A extends B {}
        type B extends A {}
        "#,
        "t",
    )
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("cyclic extends"), "{msg}");
}

#[test]
fn extends_conflicting_field_types_errors_at_open() {
    let err = Document::open(
        r#"
        type A { x: utf8 }
        type B { x: i32 }
        type C extends A, B { y: utf8 }
        "#,
        "t",
    )
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("conflicting type for field"), "{msg}");
}

#[test]
fn extends_redeclaration_same_type_allowed() {
    Document::open(
        r#"
        type Animal { name: utf8 }
        type Dog extends Animal { name: utf8  breed: utf8 }
        "#,
        "t",
    )
    .expect("opens");
}

#[test]
fn extends_redeclaration_different_type_errors() {
    let err = Document::open(
        r#"
        type Animal { name: utf8 }
        type Dog extends Animal { name: i32  breed: utf8 }
        "#,
        "t",
    )
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("conflicting type for field"), "{msg}");
}

#[test]
fn type_decl_effective_fields_includes_ancestors() {
    let doc = Document::open(
        r#"
        type Animal { name: utf8  age: u32 }
        type Dog extends Animal { breed: utf8 }
        "#,
        "t",
    )
    .unwrap();
    let dog = doc.type_decl("Dog").unwrap();
    let names: Vec<_> = dog
        .effective_fields()
        .into_iter()
        .map(|f| f.name().to_string())
        .collect();
    assert_eq!(names, vec!["name", "age", "breed"]);
}

#[test]
fn type_decl_extends_lists_parents_in_order() {
    let doc = Document::open(
        r#"
        type A { x: utf8 }
        interface B { y: utf8 }
        type C extends A, B { z: utf8 }
        "#,
        "t",
    )
    .unwrap();
    let c = doc.type_decl("C").unwrap();
    let parents: Vec<String> = c.extends().iter().map(|p| p.join(".")).collect();
    assert_eq!(parents, vec!["A".to_string(), "B".to_string()]);
}

fn open_interfaces_doc() -> Document {
    Document::open(
        r#"
        interface Drawable { tag: utf8  rank: i32 }
        type Animal { name: utf8 }
        type Dog extends Animal { breed: utf8 }
        @block("animal") type AnimalBlock extends Animal { @inline(0) id: utf8 }
        @block("dog") type DogBlock extends Dog {
            @inline(0) id: utf8
            tag:  utf8
            rank: i32
        }
        @block("widget") type WidgetBlock {
            @inline(0) id: utf8
            tag: utf8
            rank: i32
        }
        @block("partial") type PartialBlock {
            @inline(0) id: utf8
            tag: utf8
            // missing `rank`
        }
        @document
        type Cfg {
            @child("animal") animal: AnimalBlock?
            @child("dog") dog: DogBlock?
            @child("widget") widget: WidgetBlock?
            @child("partial") partial: PartialBlock?
            ref_animal: &Animal
            ref_dog_as_animal: &Animal
            ref_widget_as_drawable: &Drawable
            ref_dog_as_drawable: &Drawable
            ref_partial_as_drawable: &Drawable
            ref_widget_as_animal: &Animal
        }
        animal "alice" {}
        dog "spot" { tag = "alpha"  rank = 1 }
        widget "w1" { tag = "alpha"  rank = 1 }
        partial "p1" { tag = "alpha" }
        ref_animal              = animal
        ref_dog_as_animal       = dog
        ref_widget_as_drawable  = widget
        ref_dog_as_drawable     = dog
        ref_partial_as_drawable = partial
        ref_widget_as_animal    = widget
        "#,
        "t",
    )
    .expect("opens")
}

#[test]
fn descendant_satisfies_ancestor_reference() {
    // `dog.spot` (DogBlock extends Dog extends Animal) read
    // through `&Animal` should resolve.
    let doc = open_interfaces_doc();
    let f = doc.field("ref_dog_as_animal").unwrap();
    f.reference()
        .expect("&T field exposes reference()")
        .expect("dog target accepted as Animal");
}

#[test]
fn exact_match_resolves_through_reference() {
    let doc = open_interfaces_doc();
    let f = doc.field("ref_animal").unwrap();
    f.reference()
        .expect("&T field")
        .expect("animal matches Animal exactly");
}

#[test]
fn conformant_target_resolves_through_interface_reference() {
    // WidgetBlock has tag + rank → satisfies Drawable.
    let doc = open_interfaces_doc();
    let f = doc.field("ref_widget_as_drawable").unwrap();
    f.reference()
        .expect("&Drawable field")
        .expect("WidgetBlock conforms to Drawable");
}

#[test]
fn target_missing_field_errors_interface_not_implemented() {
    // PartialBlock has `tag` but no `rank` → fails Drawable.
    let doc = open_interfaces_doc();
    let f = doc.field("ref_partial_as_drawable").unwrap();
    let r = f.reference().expect("&Drawable");
    match r {
        Ok(_) => panic!("expected InterfaceNotImplemented"),
        Err(e) => assert!(
            matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::InterfaceNotImplemented,
                    ..
                }
            ),
            "{e:?}"
        ),
    }
}

#[test]
fn sibling_target_errors_through_reference() {
    // WidgetBlock has no `extends Animal` chain → fails &Animal.
    let doc = open_interfaces_doc();
    let f = doc.field("ref_widget_as_animal").unwrap();
    let r = f.reference().expect("&Animal");
    match r {
        Ok(_) => panic!("expected InterfaceNotImplemented"),
        Err(e) => assert!(
            matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::InterfaceNotImplemented,
                    ..
                }
            ),
            "{e:?}"
        ),
    }
}

#[test]
fn dog_inherited_fields_also_satisfy_drawable_via_self_fields() {
    // DogBlock declares its own tag/rank, so it implements
    // Drawable directly. (The Animal-extends chain doesn't
    // contribute Drawable shape.)
    let doc = open_interfaces_doc();
    let f = doc.field("ref_dog_as_drawable").unwrap();
    f.reference()
        .expect("&Drawable")
        .expect("DogBlock implements Drawable");
}

#[test]
fn children_interface_accepts_extending_type() {
    // `@children(Renderable)` accepts any nested block whose @block
    // type transitively `extends Renderable`.
    let doc = Document::open(
        r#"
        @document type Site { @children("parent") parents: list<Parent> }
        interface Renderable { x: i64 }
        @block("a") type A extends Renderable { x: i64 }
        @block("parent") type Parent {
          @children(Renderable) kids: list<Renderable>
        }
        parent root { a { x = 1 } a { x = 2 } }
        "#,
        "test",
    )
    .expect("open");
    let errs = doc.schema_errors();
    assert!(errs.is_empty(), "expected no schema errors, got {errs:?}");
}

#[test]
fn children_interface_rejects_non_extending_type() {
    // Block kind `b` has a @block schema but no `extends Renderable`
    // chain — child should be rejected.
    let doc = Document::open(
        r#"
        @document type Site { @children("parent") parents: list<Parent> }
        interface Renderable { x: i64 }
        @block("a") type A extends Renderable { x: i64 }
        @block("b") type B { x: i64 }
        @block("parent") type Parent {
          @children(Renderable) kids: list<Renderable>
        }
        parent root { a { x = 1 } b { x = 2 } }
        "#,
        "test",
    )
    .expect("open");
    let errs = doc.schema_errors();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::DisallowedChild,
                ..
            }
        )),
        "expected DisallowedChild, got {errs:?}"
    );
}

#[test]
fn children_interface_accepts_indirect_extends_chain() {
    // C extends B extends Renderable — c blocks must still be accepted.
    let doc = Document::open(
        r#"
        @document type Site { @children("parent") parents: list<Parent> }
        interface Renderable { x: i64 }
        type B extends Renderable { x: i64 }
        @block("c") type C extends B { x: i64 }
        @block("parent") type Parent {
          @children(Renderable) kids: list<Renderable>
        }
        parent root { c { x = 1 } }
        "#,
        "test",
    )
    .expect("open");
    let errs = doc.schema_errors();
    assert!(errs.is_empty(), "expected no schema errors, got {errs:?}");
}

#[test]
fn interface_function_field_accepts_different_param_type() {
    // Interface declares `lower: fn(&I) -> i64`; impl narrows the
    // param to its own concrete type. Conformance must allow this —
    // function fields use return-type-only matching.
    let doc = Document::open(
        r#"
        interface I {
          x: i64
          lower: fn(&I) -> i64
        }
        @block("widget") type Widget extends I {
          @inline(0) id: utf8
          x: i64
          lower: fn(Widget) -> i64
        }
        @document type Cfg {
          @child("widget") widget: Widget?
          ref_w_as_i: &I
        }
        widget "w1" {
          x = 1
          lower = fn(w: Widget) -> i64 [ w.x + 1 ]
        }
        ref_w_as_i = widget
        "#,
        "t",
    )
    .expect("open");
    let f = doc.field("ref_w_as_i").unwrap();
    f.reference()
        .expect("&I field")
        .expect("Widget conforms to I despite narrowed lower param");
}

#[test]
fn interface_function_field_rejects_non_function_impl() {
    // Interface declares a function field; impl declares a scalar of
    // the same name. The Function/non-Function split is caught by the
    // strict-equality fallback inside the relaxed helper.
    let doc = Document::open(
        r#"
        interface I {
          x: i64
          lower: fn(&I) -> i64
        }
        @block("widget") type Widget {
          @inline(0) id: utf8
          x: i64
          lower: i64
        }
        @document type Cfg {
          @child("widget") widget: Widget?
          ref_w_as_i: &I
        }
        widget "w1" {
          x = 1
          lower = 7
        }
        ref_w_as_i = widget
        "#,
        "t",
    )
    .expect("open");
    let f = doc.field("ref_w_as_i").unwrap();
    let r = f.reference().expect("&I");
    match r {
        Ok(_) => panic!("expected InterfaceNotImplemented when impl field is not a function"),
        Err(e) => assert!(
            matches!(
                e,
                EvalError::SchemaViolation {
                    kind: crate::error::SchemaViolationKind::InterfaceNotImplemented,
                    ..
                }
            ),
            "{e:?}"
        ),
    }
}

#[test]
fn inline_default_fn_literal_infers_function_type() {
    let doc = Document::open(
        r#"
        @block("widget") type Widget {
          @inline(0) id: utf8
          x: i64
          plus_one = fn(w: Widget) -> i64 [ w.x + 1 ]
        }
        @document type Cfg { @child("widget") widget: Widget? }
        widget "w1" { x = 5 }
        "#,
        "t",
    )
    .expect("open");
    let widget_type = doc.type_decl("Widget").expect("Widget type");
    let plus_one = widget_type.field("plus_one").expect("plus_one field");
    let val = plus_one.default_value().expect("inline default value");
    let Value::Function(_) = val else {
        panic!("expected Value::Function, got {val:?}");
    };
}

#[test]
fn inline_default_primitive_infers_builtin_type() {
    let doc = Document::open(
        r#"
        type Q { port = 8080u32 }
        "#,
        "t",
    )
    .expect("open");
    let q = doc.type_decl("Q").expect("Q type");
    let port = q.field("port").expect("port field");
    assert_eq!(port.default_value(), Some(Value::U32(8080)));
}

#[test]
fn inline_default_rejects_non_inferable_rhs() {
    let err = Document::open(
        r#"
        type X { y = some_name }
        "#,
        "t",
    )
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("cannot infer type") && msg.contains("@default"),
        "{msg}"
    );
}

#[test]
fn inline_default_rejects_redundant_at_default_decorator() {
    let err = Document::open(
        r#"
        type X {
          @default(7)
          y = 8
        }
        "#,
        "t",
    )
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("inline `=` default and `@default(...)`"),
        "{msg}"
    );
}

#[test]
fn inline_default_satisfies_interface_function_field() {
    // `lower` on interface requires a function-typed field; impl
    // provides it via `lower = fn(...) -> ...` and inferred type.
    let doc = Document::open(
        r#"
        interface I {
          x: i64
          lower: fn(&I) -> i64
        }
        @block("widget") type Widget extends I {
          @inline(0) id: utf8
          x: i64
          lower = fn(w: Widget) -> i64 [ w.x ]
        }
        @document type Cfg {
          @child("widget") widget: Widget?
          ref_w_as_i: &I
        }
        widget "w1" { x = 3 }
        ref_w_as_i = widget
        "#,
        "t",
    )
    .expect("open");
    let f = doc.field("ref_w_as_i").unwrap();
    f.reference()
        .expect("&I")
        .expect("Widget conforms via inline-default function");
}

// ── `let` items (top-level & block-scoped composition helpers) ──────

#[test]
fn top_level_let_resolves_in_field() {
    let doc = open(
        r#"
        let base = 10
        x = base + 5
        "#,
    );
    assert_eq!(doc.field("x").unwrap().value().unwrap(), &Value::I64(15));
}

#[test]
fn let_is_not_addressable_as_document_data() {
    let doc = open(
        r#"
        let base = 10
        x = base + 5
        "#,
    );
    // The helper resolves in expressions but is invisible to queries.
    assert!(
        doc.get("base").is_none(),
        "let should not be path-addressable"
    );
    assert!(doc.field("base").is_none(), "let is not a field");
    let names: Vec<&str> = doc.fields().map(|f| f.name()).collect();
    assert!(
        !names.contains(&"base"),
        "let must not appear in fields(): {names:?}"
    );
    assert!(names.contains(&"x"));
}

#[test]
fn block_level_let_scopes_to_its_block() {
    let doc = open(
        r#"
        @schemaless cfg conf {
          let n = 3
          count = n * 2
        }
        "#,
    );
    let cfg = doc.block("cfg").unwrap();
    assert_eq!(cfg.field("count").unwrap().value().unwrap(), &Value::I64(6));
    // The let isn't a field of the block, nor path-addressable.
    assert!(cfg.field("n").is_none());
    assert!(doc.get("cfg.n").is_none());
    assert!(
        !cfg.fields().any(|f| f.name() == "n"),
        "block let leaked into fields()"
    );
}

#[test]
fn let_bound_function_composes_a_field() {
    let doc = open(
        r#"
        let double = fn(n: i64) -> i64 n * 2
        y = double(21)
        "#,
    );
    assert_eq!(doc.field("y").unwrap().value().unwrap(), &Value::I64(42));
}

#[test]
fn top_level_let_visible_inside_blocks() {
    let doc = open(
        r#"
        let scale = fn(n: i64) -> i64 n * 10
        @schemaless cfg conf {
          let n = 3
          label = scale(n)
        }
        "#,
    );
    let cfg = doc.block("cfg").unwrap();
    assert_eq!(
        cfg.field("label").unwrap().value().unwrap(),
        &Value::I64(30)
    );
}

#[test]
fn let_inside_schemad_block_is_not_a_schema_violation() {
    // `n` is a let helper inside a schema'd block; it must not be
    // flagged as an unknown field, and `count` (which uses it) must
    // still validate/evaluate.
    let doc = Document::open(
        r#"
        @document type Doc { @children("cfg") cfgs: list<Cfg> }
        @block("cfg") type Cfg { count: i64 }
        cfg { let n = 3  count = n * 2 }
        "#,
        "test",
    )
    .expect("open");
    let cfg = doc.block("cfg").unwrap();
    assert!(
        cfg.schema_errors().is_empty(),
        "let triggered schema errors: {:?}",
        cfg.schema_errors()
    );
    assert_eq!(cfg.field("count").unwrap().value().unwrap(), &Value::I64(6));
    assert!(
        doc.schema_errors().is_empty(),
        "document schema errors: {:?}",
        doc.schema_errors()
    );
}

#[test]
fn let_cycle_surfaces_as_cycle_error() {
    let doc = open(
        r#"
        let a = b
        let b = a
        x = a
        "#,
    );
    let err = doc.field("x").unwrap().value().unwrap_err();
    assert!(
        matches!(err, EvalError::Cycle { .. }),
        "expected Cycle, got {err:?}"
    );
}

#[test]
fn same_named_field_resolves_outward_not_self() {
    // `a = a` inside a nested block means the *outer* `a`, not the
    // field being defined: a mid-evaluation match is skipped by
    // `scope_lookup` so the walk continues outward (the wdoc
    // counterpart is a component slot bound from a same-named repeater
    // variable, `op = op`). Regression: this used to be a false
    // "cycle while evaluating 'a'".
    let doc = open(
        r#"
        @schemaless
        outer {
          a = 7
          inner {
            a = a
          }
        }
        "#,
    );
    let v = doc.get("outer.inner.a").unwrap().value().unwrap();
    assert_eq!(v, Value::I64(7));
}

#[test]
fn self_reference_with_no_outer_binding_is_still_a_cycle() {
    // With nothing outward to resolve to, `a = a` falls back to the
    // skipped mid-evaluation match and reports the genuine cycle.
    let doc = open(
        r#"
        @schemaless
        blk {
          a = a
        }
        "#,
    );
    let err = doc.get("blk.a").unwrap().value().unwrap_err();
    assert!(
        matches!(err, EvalError::Cycle { .. }),
        "expected Cycle, got {err:?}"
    );
}

#[test]
fn inner_let_shadows_outer() {
    let doc = open(
        r#"
        let v = 1
        @schemaless cfg conf {
          let v = 2
          out = v
        }
        top = v
        "#,
    );
    assert_eq!(doc.field("top").unwrap().value().unwrap(), &Value::I64(1));
    let cfg = doc.block("cfg").unwrap();
    assert_eq!(cfg.field("out").unwrap().value().unwrap(), &Value::I64(2));
}

// ── Value-type interface introspection (check_value_implements_iface) ──
//
// Bare `Value::Record`s (e.g. connection projections) are now
// structurally checked against an interface's fields, alongside the
// pre-existing variant-with-record-payload path.

fn record_value(pairs: &[(&str, Value)]) -> Value {
    let fields = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect::<std::collections::BTreeMap<_, _>>()
        .into();
    Value::Record {
        ty: Vec::new(),
        fields,
    }
}

#[test]
fn bare_record_satisfies_interface() {
    let doc = open("interface Named {\n  label: utf8\n}\n");
    let rec = record_value(&[("label", Value::Utf8("hi".to_string()))]);
    doc.check_value_implements_iface(&rec, &["Named".to_string()], crate::ast::Span::new(0, 0))
        .expect("record carrying `label: utf8` satisfies Named");
}

#[test]
fn bare_record_missing_field_is_rejected() {
    let doc = open("interface Named {\n  label: utf8\n}\n");
    let rec = record_value(&[("other", Value::Utf8("hi".to_string()))]);
    let err = doc
        .check_value_implements_iface(&rec, &["Named".to_string()], crate::ast::Span::new(0, 0))
        .expect_err("record missing `label` should fail");
    assert!(
        matches!(err, EvalError::VariantShapeMismatch { .. }),
        "{err:?}"
    );
}

#[test]
fn bare_record_wrong_field_type_is_rejected() {
    let doc = open("interface Named {\n  label: utf8\n}\n");
    let rec = record_value(&[("label", Value::I64(3))]);
    let err = doc
        .check_value_implements_iface(&rec, &["Named".to_string()], crate::ast::Span::new(0, 0))
        .expect_err("record whose `label` isn't utf8 should fail");
    assert!(
        matches!(err, EvalError::VariantShapeMismatch { .. }),
        "{err:?}"
    );
}

#[test]
fn variant_construction_omits_optional_field() {
    // Explicit `Union::Variant { … }` may leave out an optional (`?`)
    // field; it defaults to `none`.
    let doc = open("union U { V { a: i64  b: utf8? } }\nx = U::V { a: 1 }\n");
    let v = doc.field("x").unwrap().value().unwrap().clone();
    match v {
        Value::Variant {
            variant,
            payload: crate::value::VariantPayload::Record(m),
            ..
        } => {
            assert_eq!(variant, "V");
            assert_eq!(m.get("a"), Some(&Value::I64(1)));
            assert_eq!(m.get("b"), Some(&Value::None));
        }
        other => panic!("expected variant, got {other:?}"),
    }
}

#[test]
fn variant_construction_missing_required_field_errors() {
    // A missing *required* field is still a shape mismatch.
    let doc = open("union U { V { a: i64  b: utf8? } }\nx = U::V { b: \"hi\" }\n");
    let err = doc.field("x").unwrap().value();
    assert!(
        matches!(err, Err(EvalError::VariantShapeMismatch { .. })),
        "{err:?}"
    );
}

#[test]
fn non_record_value_passes_through_permissively() {
    // Scalars carry no field map, so they're not introspected.
    let doc = open("interface Named {\n  label: utf8\n}\n");
    doc.check_value_implements_iface(
        &Value::I64(1),
        &["Named".to_string()],
        crate::ast::Span::new(0, 0),
    )
    .expect("a scalar passes through (no runtime type tag to check)");
}

// --- Bare-record literals shape-inferred to union variants --------------

const SHAPE_DOC: &str = r#"
@document
type Cfg { @child("chart") c: Chart }

@block("chart")
type Chart { series: list<S> }

union S { Of { name: utf8  values: list<f64> } }
"#;

#[test]
fn bare_record_field_infers_variant() {
    let src =
        format!("{SHAPE_DOC}\nchart {{ series = [ {{ name: \"x\", values: [1.0, 2.0] }} ] }}\n");
    let doc = Document::open(&src, "test").expect("open");
    let chart = doc.block("chart").expect("chart block");
    let series = chart.field("series").expect("series field");
    let value = series.value().expect("series value");
    let Value::List(items) = value else {
        panic!("expected a list, got {value:?}");
    };
    assert_eq!(items.len(), 1);
    match &items[0] {
        Value::Variant {
            union,
            variant,
            payload: crate::value::VariantPayload::Record(map),
        } => {
            assert_eq!(union, &vec!["S".to_string()]);
            assert_eq!(variant, "Of");
            assert_eq!(map.get("name"), Some(&Value::Utf8("x".to_string())));
        }
        other => panic!("bare record was not inferred to a variant: {other:?}"),
    }
}

#[test]
fn explicit_variant_form_still_works() {
    let src =
        format!("{SHAPE_DOC}\nchart {{ series = [ S::Of {{ name: \"x\", values: [1.0] }} ] }}\n");
    let doc = Document::open(&src, "test").expect("open");
    let value = doc
        .block("chart")
        .unwrap()
        .field("series")
        .unwrap()
        .value()
        .expect("series value");
    let Value::List(items) = value else {
        panic!("expected a list, got {value:?}");
    };
    assert!(matches!(&items[0], Value::Variant { variant, .. } if variant == "Of"));
}

#[test]
fn bare_record_no_matching_variant_is_rejected() {
    // Field set that matches no variant shape surfaces VariantNoMatch.
    let src = format!("{SHAPE_DOC}\nchart {{ series = [ {{ bogus: 1 }} ] }}\n");
    let doc = Document::open(&src, "test").expect("open");
    let err = doc
        .block("chart")
        .unwrap()
        .field("series")
        .unwrap()
        .value()
        .expect_err("a record matching no variant should fail");
    assert!(
        matches!(
            err,
            EvalError::SchemaViolation {
                kind: crate::error::SchemaViolationKind::VariantNoMatch,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn fn_call_arg_bare_record_is_coerced() {
    // A bare record passed to a `list<S>` parameter is coerced before
    // the body pattern-matches it as a variant.
    let src = format!(
        "{SHAPE_DOC}\n\
         let pick = fn(xs: list<S>) -> utf8 match at(xs, 0) {{ \
           S::Of {{ name, .. }} => name, _ => \"none\" }}\n\
         @schemaless picked = pick([ {{ name: \"yo\", values: [2.0] }} ])\n"
    );
    let doc = Document::open(&src, "test").expect("open");
    let picked = doc
        .field("picked")
        .expect("picked field")
        .value()
        .expect("picked value");
    assert_eq!(picked, &Value::Utf8("yo".to_string()));
}

#[test]
fn untyped_bare_record_stays_a_record() {
    // No declared union type → the value is an anonymous record.
    let doc = open("x = { a: 1, b: \"hi\" }\n");
    let x = doc.field("x").expect("x field").value().expect("x value");
    match x {
        Value::Record { fields, .. } => {
            assert_eq!(fields.get("a"), Some(&Value::I64(1)));
            assert_eq!(fields.get("b"), Some(&Value::Utf8("hi".to_string())));
        }
        other => panic!("expected an anonymous record, got {other:?}"),
    }
}

// ─── Document-root `@children` projection (issue 13) ──────────────
// A `@children`/`@child` field declared on a `@document` schema collects
// matching top-level blocks, resolvable by name from any expression and
// consumable as ordinary list/record values by builtins (`len`, `map`, …).

const ROOT_CHILDREN_DOC: &str = r#"
@block("concept")
type Concept { @inline(0) id: identifier  name: utf8 }

@document
type Wiki {
  @children("concept") concepts: list<Concept>
  count: i64
  names: list<utf8>
}

count = len(concepts)
names = map(concepts, fn(c: Concept) -> utf8 c.name)

concept "intro"  { name = "Intro" }
concept "second" { name = "Second" }
"#;

#[test]
fn root_children_resolve_to_block_list() {
    let doc = Document::open(ROOT_CHILDREN_DOC, "test").expect("open");
    let concepts = doc.get("concepts").expect("concepts resolves at root");
    assert_eq!(concepts.kind(), "block_list");
    assert_eq!(concepts.len(), Some(2));
}

#[test]
fn root_children_consumable_by_len_in_expression() {
    // A bare reference to the children slot reifies to a list value, so
    // `len(concepts)` works exactly as it would over a literal list.
    let doc = Document::open(ROOT_CHILDREN_DOC, "test").expect("open");
    let count = doc
        .get("count")
        .expect("count")
        .value()
        .expect("count value");
    assert_eq!(count, Value::I64(2));
}

#[test]
fn root_children_consumable_by_map_to_records() {
    // Each collected block reifies to a record, so `c.name` member access
    // works inside `map`.
    let doc = Document::open(ROOT_CHILDREN_DOC, "test").expect("open");
    let names = doc
        .get("names")
        .expect("names")
        .value()
        .expect("names value");
    assert_eq!(
        names,
        Value::List(std::sync::Arc::new(vec![
            Value::Utf8("Intro".into()),
            Value::Utf8("Second".into()),
        ]))
    );
}

#[test]
fn root_children_top_level_let_closes_over_them() {
    // A top-level `let` body resolves children through the same root path.
    let src = r#"
@block("concept")
type Concept { @inline(0) id: identifier }

@document
type Wiki {
  @children("concept") concepts: list<Concept>
  n: i64
}

let total = len(concepts)
n = total

concept "a"
concept "b"
concept "c"
"#;
    let doc = Document::open(src, "test").expect("open");
    let n = doc.get("n").expect("n").value().expect("n value");
    assert_eq!(n, Value::I64(3));
}

#[test]
fn root_children_empty_is_zero_not_unresolved() {
    let src = r#"
@block("concept")
type Concept { @inline(0) id: identifier }

@document
type Wiki {
  @children("concept") concepts: list<Concept>
  count: i64
}

count = len(concepts)
"#;
    let doc = Document::open(src, "test").expect("open");
    let count = doc
        .get("count")
        .expect("count")
        .value()
        .expect("count value");
    assert_eq!(count, Value::I64(0));
}

#[test]
fn root_projections_memoised_references_agree() {
    // Two references to a root `@connections` slot and a union
    // `@children` slot: the second is served from the projection memo
    // and must observe the same value as the first.
    let src = r#"
@block("node") type Node { @inline(0) id: utf8 }
symbol_set EdgeKind { default flow }
connection Edge: Node -> Node : EdgeKind
union Entry { N { w: i64 } }
@block("box") type BoxT { w: i64 }

@document
type Model {
  @children("node") nodes: list<Node>
  @children("box") boxes: list<BoxT>
  @children(Entry) entries: list<Entry>
  @connections(Edge) edges: list<Edge>
  e1: list<Edge>
  e2: list<Edge>
  c1: list<Entry>
  c2: list<Entry>
}

node "a" {}
node "b" {}
box { w = 1 }
a -> b
e1 = edges
e2 = edges
c1 = entries
c2 = entries
"#;
    let doc = Document::open(src, "test").expect("open");
    let e1 = doc.field("e1").unwrap().value().unwrap().clone();
    let e2 = doc.field("e2").unwrap().value().unwrap().clone();
    assert_eq!(e1, e2, "memoised @connections reference must agree");
    let Value::List(edges) = &e1 else {
        panic!("edges should be a list, got {e1:?}");
    };
    assert_eq!(edges.len(), 1, "one projected edge");
    let c1 = doc.field("c1").unwrap().value().unwrap().clone();
    let c2 = doc.field("c2").unwrap().value().unwrap().clone();
    assert_eq!(c1, c2, "memoised union @children reference must agree");
    let Value::List(entries) = &c1 else {
        panic!("entries should be a list, got {c1:?}");
    };
    assert_eq!(
        entries.len(),
        1,
        "the box block dispatches to Entry::N, nodes have no matching shape"
    );
}

// ─── Builtin-callee shadowing semantics (lock the eval_call fast path) ──

#[test]
fn root_let_fn_shadows_builtin_in_call_position() {
    // A root `let` binding a function under a builtin's name wins over
    // the builtin — from a root field and from inside another user fn.
    let doc = Document::open(
        r#"
        let map = fn(xs: list<i64>, f: i64) -> utf8 "user"
        let call_it = fn(z: i64) -> utf8 map([3], z)
        @schemaless direct = map([1, 2], 0)
        @schemaless via_closure = call_it(0)
        "#,
        "test",
    )
    .expect("open");
    assert_eq!(
        doc.field("direct").unwrap().value().unwrap(),
        &Value::Utf8("user".into())
    );
    assert_eq!(
        doc.field("via_closure").unwrap().value().unwrap(),
        &Value::Utf8("user".into())
    );
}

#[test]
fn block_let_fn_shadows_builtin_inside_block_only() {
    // `Document::open` (not the lax `open` helper): the non-shadowed
    // path must dispatch to the *real* builtin registry.
    let doc = Document::open(
        r#"
        @schemaless outer {
          let flatten = fn(xs: list<i64>) -> utf8 "user"
          inside = flatten([1])
        }
        @schemaless outside = flatten([[1], [2]])
        "#,
        "test",
    )
    .expect("open");
    let outer = doc.block("outer").unwrap();
    assert_eq!(
        outer.field("inside").unwrap().value().unwrap(),
        &Value::Utf8("user".into()),
        "a block-level let shadows the builtin inside its block"
    );
    assert_eq!(
        doc.field("outside").unwrap().value().unwrap(),
        &Value::List(std::sync::Arc::new(vec![Value::I64(1), Value::I64(2)])),
        "outside the block the real builtin still dispatches"
    );
}

#[test]
fn non_function_binding_does_not_shadow_builtin_call() {
    // A *non-function* binding under a builtin's name does not shadow
    // it in call position (lookup_function falls through to the
    // builtin registry).
    let doc = Document::open(
        r#"
        let len = 5
        @schemaless n = len("abc")
        "#,
        "test",
    )
    .expect("open");
    assert_eq!(doc.field("n").unwrap().value().unwrap(), &Value::I64(3));
}

#[test]
fn in_block_imported_let_fn_shadows_builtin() {
    // A lazily in-block-imported `let` binding a function under a
    // builtin's name shadows it inside that block — invisible to any
    // open-time scan of the root + eager imports.
    let doc = open_with_libs(
        r#"
        @schemaless outer {
          import <shadow.wcl>
          picked = map([1], 0)
        }
        "#,
        &[(
            "shadow.wcl",
            r#"let map = fn(xs: list<i64>, f: i64) -> utf8 "user""#,
        )],
    );
    let outer = doc.block("outer").unwrap();
    assert_eq!(
        outer.field("picked").unwrap().value().unwrap(),
        &Value::Utf8("user".into()),
        "an in-block-imported let fn shadows the builtin inside the block"
    );
}

#[test]
fn binding_scope_frame_fn_shadows_builtin() {
    use std::sync::Arc;
    // A renderer-injected binding (component slot / repeater loop var)
    // holding a function under a builtin's name shadows the builtin.
    let doc = Document::open(
        r#"
        @schemaless fnsrc = fn(xs: list<i64>, f: i64) -> utf8 "user"
        @schemaless outer {
          inner { picked = map([1], 0) }
        }
        "#,
        "test",
    )
    .expect("open");
    let fv = doc.field("fnsrc").unwrap().value().unwrap().clone();
    let outer = doc.block("outer").unwrap();
    let bindings = Arc::new(vec![("map".to_string(), fv)]);
    let groups = outer.expand_bodies(&outer, vec![bindings]);
    let inner = groups[0].iter().find(|b| b.kind() == "inner").unwrap();
    assert_eq!(
        inner.field("picked").unwrap().value().unwrap(),
        &Value::Utf8("user".into()),
        "an injected fn binding shadows the builtin in a child block"
    );
}

fn open_by_ref() -> Document {
    Document::open(
        r#"
        @document
        type Doc { @children("server") servers: list<Server> }

        @block("server")
        type Server {
          @inline(0) name: identifier
          region: utf8?
          @child("body") overview: Body?
        }

        @block("body") @by_ref
        type Body {
          @children("note") notes: list<Note>
        }

        @block("note")
        type Note { @inline(0) text: utf8 }

        server web01 {
          region = "us-east"
          body { note "primary frontend" }
        }
        server web02 {
          region = "eu-west"
          body { note "replica" }
        }
        "#,
        "test",
    )
    .expect("open")
}

#[test]
fn by_ref_child_slot_reifies_to_resolvable_datapath() {
    let doc = open_by_ref();
    // Reifying a server record (the per-element step a `wdoc_repeater` runs
    // over `each = servers`) carries its own `@by_ref` body as a
    // root-resolvable reference rather than inlined content.
    let v = doc
        .block("server")
        .unwrap()
        .to_record_value_at(&["servers".to_string(), "web01".to_string()])
        .unwrap();
    let Value::Record { fields, .. } = v else {
        panic!("element is not a record");
    };
    // Scalars survive faithfully; the body is a reference, not a record.
    assert_eq!(fields.get("region"), Some(&Value::Utf8("us-east".into())));
    match fields.get("overview") {
        Some(Value::DataPath { segments, .. }) => {
            assert_eq!(segments, &["servers", "web01", "overview"]);
        }
        other => panic!("overview reified to {other:?}, expected a DataPath"),
    }
}

#[test]
fn by_ref_datapath_resolves_to_body_block_children() {
    let doc = open_by_ref();
    // The emitted reference re-resolves from document root to the live body
    // block, whose nested children are reachable as real block views.
    let body = doc
        .get("servers.web01.overview")
        .expect("resolve overview ref")
        .as_block()
        .expect("body block");
    assert_eq!(body.kind(), "body");
    let notes: Vec<String> = body
        .blocks()
        .filter(|b| b.kind() == "note")
        .map(|b| match &b.labels().unwrap()[0] {
            Value::Utf8(s) => s.clone(),
            other => panic!("expected utf8 label, got {other:?}"),
        })
        .collect();
    assert_eq!(notes, vec!["primary frontend"]);
    // The second server resolves independently to its own body.
    let body2 = doc
        .get("servers.web02.overview")
        .unwrap()
        .as_block()
        .unwrap();
    let note2 = body2.blocks().find(|b| b.kind() == "note").unwrap();
    assert_eq!(note2.labels().unwrap()[0], Value::Utf8("replica".into()));
}

#[test]
fn by_ref_numeric_labelled_body_nested_one_level_resolves() {
    // Regression for the nested-record bug: a `@by_ref` body on a record with
    // a NUMERIC `@inline(0)` label (a step number), itself nested in another
    // record's `@children`, must reify to a full root-resolvable path and
    // re-resolve. Previously the numeric label produced an empty base so the
    // body collapsed to `DataPath(["body"])` and failed to resolve.
    let doc = Document::open(
        r#"
        @document
        type Doc { @children("tut") tuts: list<Tut> }
        @block("tut")
        type Tut { @inline(0) id: identifier  @children("tstep") steps: list<TStep> }
        @block("tstep") @by_ref
        type TStep { @inline(0) n: u32  @children("note") notes: list<Note> }
        @block("note")
        type Note { @inline(0) text: utf8 }

        tut t1 {
          tstep 1 { note "step one" }
          tstep 2 { note "step two" }
        }
        "#,
        "test",
    )
    .expect("open");

    // Reify the tut (the per-element step a repeater runs over `each = tuts`);
    // its `steps` list carries each step as a resolvable reference whose path
    // includes the numeric label.
    let tut = doc
        .block("tut")
        .unwrap()
        .to_record_value_at(&["tuts".to_string(), "t1".to_string()])
        .unwrap();
    let Value::Record { fields, .. } = tut else {
        panic!("tut is not a record");
    };
    let Some(Value::List(steps)) = fields.get("steps") else {
        panic!("steps did not reify to a list");
    };
    match &steps[0] {
        Value::DataPath { segments, .. } => {
            assert_eq!(segments, &["tuts", "t1", "steps", "1"]);
        }
        other => panic!("step reified to {other:?}, expected a @by_ref DataPath"),
    }

    // And the emitted path re-resolves from document root via the numeric
    // label segment "1" (matched against the `u32` label).
    let step = doc.get("tuts.t1.steps.1").unwrap().as_block().unwrap();
    assert_eq!(step.kind(), "tstep");
    let note = step.blocks().find(|b| b.kind() == "note").unwrap();
    assert_eq!(note.labels().unwrap()[0], Value::Utf8("step one".into()));
}

#[test]
fn by_ref_direct_member_access_yields_datapath() {
    let doc = open_by_ref();
    // A direct member chain to a `@by_ref` block also evaluates to the
    // reference (not an inlined record), so `project servers.web01.overview`
    // works at page scope, not just through a repeater binding.
    let dr = doc.get("servers.web01.overview").unwrap();
    match dr.value() {
        Ok(Value::DataPath { segments, .. }) => {
            assert_eq!(segments, &["servers", "web01", "overview"]);
        }
        // `Document::get` lands on the block directly; the by-ref rule fires
        // in the value-reifying path, so assert via that path too.
        _ => {
            let v = doc
                .block("server")
                .unwrap()
                .to_record_value_at(&["servers".to_string(), "web01".to_string()])
                .unwrap();
            let Value::Record { fields, .. } = v else {
                panic!("not a record")
            };
            assert!(matches!(
                fields.get("overview"),
                Some(Value::DataPath { .. })
            ));
        }
    }
}
