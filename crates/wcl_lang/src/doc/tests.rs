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
        &Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
    );
}

#[test]
fn eval_empty_list_literal() {
    let doc = open("x = []");
    assert_eq!(
        doc.field("x").unwrap().value().unwrap(),
        &Value::List(vec![])
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
    assert_eq!(outer[0], Value::List(vec![Value::I64(1), Value::I64(2)]));
}

#[test]
fn eval_list_literal_resolves_identifiers() {
    let doc = open("a = 1\nb = 2\nx = [a, b, 3]");
    assert_eq!(
        doc.field("x").unwrap().value().unwrap(),
        &Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
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
        Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
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
    // `pinned = bob` resolves via the scope chain to the bob row
    // (a Block), which isn't a leaf, so `.value()` errors. This
    // is the expected behaviour for `&User` — host code should
    // use `.reference()` to navigate further.
    let doc = open_refs();
    let pinned = doc.get("db.pinned").expect("db.pinned present");
    let err = pinned.value().unwrap_err();
    assert!(matches!(err, EvalError::NotALeaf { .. }), "{err:?}");
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
