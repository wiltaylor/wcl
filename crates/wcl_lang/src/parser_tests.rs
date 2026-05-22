use super::*;
use crate::value::{BuiltinType, TensorDim};

fn parse(src: &str) -> Source {
    Parser::new(src, "test").parse_source().expect("parse ok").0
}

fn parse_with_index(src: &str) -> (Source, SymbolIndex) {
    Parser::new(src, "test").parse_source().expect("parse ok")
}

fn parse_err(src: &str) -> ParseError {
    Parser::new(src, "test")
        .parse_source()
        .expect_err("expected parse error")
}

#[track_caller]
fn assert_syntax_err(err: ParseError, needle: &str) {
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains(needle),
            "expected syntax error containing {needle:?}, got: {}",
            e.message,
        ),
        other => panic!("expected ParseError::Syntax containing {needle:?}, got: {other:?}"),
    }
}

fn field<'a>(items: &'a [Item], name: &str) -> &'a Field {
    items
        .iter()
        .find_map(|i| match i {
            Item::Field(f) if f.name == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no field '{name}'"))
}

fn blocks(items: &[Item]) -> Vec<&Block> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::Block(b) => Some(b),
            _ => None,
        })
        .collect()
}

#[test]
fn parse_empty_document() {
    let s = parse("");
    assert!(s.items.is_empty());
}

#[test]
fn parse_single_string_field() {
    let s = parse(r#"name = "alpha""#);
    assert_eq!(field(&s.items, "name").expr, Expr::Utf8("alpha".into()));
}

#[test]
fn parse_mixed_scalar_fields() {
    let s = parse(
        r#"
        name = "alpha"
        count = 3
        ratio = 2.5
        enabled = true
        "#,
    );
    assert_eq!(field(&s.items, "name").expr, Expr::Utf8("alpha".into()));
    assert_eq!(field(&s.items, "count").expr, Expr::I64(3));
    assert_eq!(field(&s.items, "ratio").expr, Expr::F64(2.5));
    assert_eq!(field(&s.items, "enabled").expr, Expr::Bool(true));
}

#[test]
fn parse_block_with_label() {
    let s = parse(
        r#"
        service "web" {
          port = 8080
          host = "0.0.0.0"
        }
        "#,
    );
    let blks = blocks(&s.items);
    let block = blks[0];
    assert_eq!(block.kind, "service");
    assert_eq!(block.labels, vec![Expr::Utf8("web".into())]);
    assert_eq!(field(&block.items, "port").expr, Expr::I64(8080));
    assert_eq!(
        field(&block.items, "host").expr,
        Expr::Utf8("0.0.0.0".into())
    );
}

#[test]
fn parse_block_without_label() {
    let s = parse("metadata { region = \"us-east-1\" }");
    let block = blocks(&s.items)[0];
    assert_eq!(block.kind, "metadata");
    assert!(block.labels.is_empty());
}

#[test]
fn parse_block_with_multiple_labels() {
    let s = parse(r#"resource "aws_s3_bucket" "logs" { acl = "private" }"#);
    let block = blocks(&s.items)[0];
    assert_eq!(block.kind, "resource");
    assert_eq!(
        block.labels,
        vec![
            Expr::Utf8("aws_s3_bucket".into()),
            Expr::Utf8("logs".into())
        ]
    );
}

#[test]
fn parse_nested_blocks() {
    let s = parse(
        r#"
        service "web" {
          metadata {
            region = "us-east-1"
          }
        }
        "#,
    );
    let outer = blocks(&s.items)[0];
    let inner = blocks(&outer.items)[0];
    assert_eq!(inner.kind, "metadata");
    assert_eq!(
        field(&inner.items, "region").expr,
        Expr::Utf8("us-east-1".into())
    );
}

#[test]
fn error_on_missing_value() {
    assert_syntax_err(parse_err("name ="), "expected value");
}

#[test]
fn error_on_unclosed_block() {
    assert_syntax_err(parse_err("service \"web\" { port = 1"), "end of file");
}

#[test]
fn error_when_first_token_is_not_ident() {
    assert_syntax_err(parse_err("= 1"), "expected identifier");
}

#[test]
fn spans_cover_full_field() {
    let src = r#"name = "alpha""#;
    let s = parse(src);
    let f = field(&s.items, "name");
    assert_eq!(&src[f.span.start..f.span.end], src);
}

fn type_decls(items: &[Item]) -> Vec<&TypeDecl> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::TypeDecl(t) => Some(t),
            _ => None,
        })
        .collect()
}

#[test]
fn parse_simple_type_declaration() {
    let s = parse("type User { name: utf8 }");
    let t = type_decls(&s.items)[0];
    assert_eq!(t.name, vec!["User".to_string()]);
    assert_eq!(t.fields.len(), 1);
    assert_eq!(t.fields[0].name, "name");
    assert_eq!(t.fields[0].ty, TypeRef::Builtin(BuiltinType::Utf8));
    assert!(!t.fields[0].optional);
}

#[test]
fn parse_type_with_optional_field() {
    let s = parse("type User { bio: utf8? age: u32? }");
    let t = type_decls(&s.items)[0];
    assert!(t.fields[0].optional);
    assert!(t.fields[1].optional);
    assert_eq!(t.fields[1].ty, TypeRef::Builtin(BuiltinType::U32));
}

#[test]
fn parse_empty_type_body() {
    let s = parse("type Empty {}");
    let t = type_decls(&s.items)[0];
    assert_eq!(t.name, vec!["Empty".to_string()]);
    assert!(t.fields.is_empty());
}

#[test]
fn parse_type_with_named_ref() {
    let s = parse("type Tree { parent: Tree? }");
    let t = type_decls(&s.items)[0];
    assert_eq!(t.fields[0].ty, TypeRef::Named(vec!["Tree".into()]));
    assert!(t.fields[0].optional);
}

#[test]
fn parse_reference_type_to_named() {
    let s = parse("type Post { author: &User? }");
    let t = type_decls(&s.items)[0];
    assert_eq!(
        t.fields[0].ty,
        TypeRef::Reference(Box::new(TypeRef::Named(vec!["User".into()])))
    );
    assert!(t.fields[0].optional);
}

#[test]
fn parse_reference_type_to_builtin() {
    let s = parse("type Score { value: &i32 }");
    let t = type_decls(&s.items)[0];
    assert_eq!(
        t.fields[0].ty,
        TypeRef::Reference(Box::new(TypeRef::Builtin(BuiltinType::I32)))
    );
    assert!(!t.fields[0].optional);
}

#[test]
fn nested_reference_rejected() {
    let err = parse_err("type X { y: &&User }");
    match err {
        ParseError::Syntax(e) => {
            assert!(e.message.contains("expected type"), "{}", e.message)
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn parse_bare_ident_as_reference_value() {
    let s = parse("owner = wil_taylor");
    assert_eq!(
        field(&s.items, "owner").expr,
        Expr::Identifier("wil_taylor".into())
    );
}

#[test]
fn parse_none_as_value() {
    let s = parse("maybe = none");
    assert_eq!(field(&s.items, "maybe").expr, Expr::None);
}

#[test]
fn contextual_keyword_field_named_type_still_works() {
    // `type` followed by `=` is just a field named "type".
    let s = parse("type = 1");
    assert_eq!(field(&s.items, "type").expr, Expr::I64(1));
}

#[test]
fn contextual_keyword_block_with_kind_type_still_works() {
    let s = parse(r#"type "label" { x = 1 }"#);
    let block = blocks(&s.items)[0];
    assert_eq!(block.kind, "type");
    assert_eq!(block.labels, vec![Expr::Utf8("label".into())]);
}

#[test]
fn type_decl_without_brace_errors() {
    assert_syntax_err(parse_err("type Foo = 1"), "'{'");
}

fn union_decls(items: &[Item]) -> Vec<&UnionDecl> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::UnionDecl(u) => Some(u),
            _ => None,
        })
        .collect()
}

#[test]
fn parse_union_with_all_three_body_forms() {
    let s = parse(
        r#"
        type Point { x: f64 y: f64 }
        union Shape {
          Circle { center: Point radius: f64 }
          Polygon Point
          Empty none
        }
        "#,
    );
    let u = union_decls(&s.items)[0];
    assert_eq!(u.name, vec!["Shape".to_string()]);
    assert_eq!(u.variants.len(), 3);
    assert_eq!(u.variants[0].name, "Circle");
    assert!(matches!(u.variants[0].body, VariantBody::Record(_)));
    assert_eq!(u.variants[1].name, "Polygon");
    match &u.variants[1].body {
        VariantBody::TypeRef { ty, .. } => {
            assert_eq!(*ty, TypeRef::Named(vec!["Point".into()]))
        }
        _ => panic!("expected TypeRef body"),
    }
    assert_eq!(u.variants[2].name, "Empty");
    assert!(matches!(u.variants[2].body, VariantBody::Unit));
}

#[test]
fn parse_empty_union() {
    let s = parse("union Nothing {}");
    let u = union_decls(&s.items)[0];
    assert_eq!(u.name, vec!["Nothing".to_string()]);
    assert!(u.variants.is_empty());
}

#[test]
fn parse_reference_variant_body() {
    // `&Path` in variant body position now parses as InterfaceRef
    // — the variant payload is any value implementing the named
    // interface. Concrete type refs without `&` still parse as
    // TypeRef.
    let s = parse("type Item {} union Wrap { Boxed &Item }");
    let u = union_decls(&s.items)[0];
    match &u.variants[0].body {
        VariantBody::InterfaceRef { iface, .. } => {
            assert_eq!(*iface, vec!["Item".to_string()]);
        }
        other => panic!("expected InterfaceRef body, got {other:?}"),
    }
}

#[test]
fn variant_body_question_mark_rejected() {
    let err = parse_err("type T {} union X { V T? }");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("'?' is not allowed on a variant body"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn union_decl_without_brace_errors() {
    assert_syntax_err(parse_err("union Foo = 1"), "'{'");
}

#[test]
fn type_field_without_colon_errors() {
    assert_syntax_err(parse_err("type Foo { x utf8 }"), "':'");
}

#[test]
fn parse_list_type() {
    let s = parse("type Q { items: list<i32> }");
    let t = type_decls(&s.items)[0];
    assert_eq!(
        t.fields[0].ty,
        TypeRef::List(Box::new(TypeRef::Builtin(BuiltinType::I32)))
    );
}

#[test]
fn parse_nested_list_type() {
    let s = parse("type Q { items: list<list<f32>> }");
    let t = type_decls(&s.items)[0];
    assert_eq!(
        t.fields[0].ty,
        TypeRef::List(Box::new(TypeRef::List(Box::new(TypeRef::Builtin(
            BuiltinType::F32
        )))))
    );
}

#[test]
fn parse_list_of_reference() {
    let s = parse("type User {}\ntype Q { items: list<&User> }");
    let t = type_decls(&s.items)
        .into_iter()
        .find(|t| t.name == vec!["Q".to_string()])
        .unwrap();
    assert_eq!(
        t.fields[0].ty,
        TypeRef::List(Box::new(TypeRef::Reference(Box::new(TypeRef::Named(
            vec!["User".into()]
        )))))
    );
}

#[test]
fn parse_optional_list() {
    let s = parse("type Q { items: list<i32>? }");
    let t = type_decls(&s.items)[0];
    assert!(t.fields[0].optional);
    assert_eq!(
        t.fields[0].ty,
        TypeRef::List(Box::new(TypeRef::Builtin(BuiltinType::I32)))
    );
}

#[test]
fn parse_tensor_with_concrete_dims() {
    let s = parse("type Q { w: tensor<f32, [3, 4]> }");
    let t = type_decls(&s.items)[0];
    let TypeRef::Tensor { element, dims } = &t.fields[0].ty else {
        panic!("expected tensor");
    };
    assert_eq!(**element, TypeRef::Builtin(BuiltinType::F32));
    assert_eq!(dims, &vec![TensorDim::Fixed(3), TensorDim::Fixed(4)]);
}

#[test]
fn parse_tensor_with_symbolic_dim() {
    let s = parse("type Q { w: tensor<f32, [N, 128]> }");
    let t = type_decls(&s.items)[0];
    let TypeRef::Tensor { dims, .. } = &t.fields[0].ty else {
        panic!("expected tensor");
    };
    assert_eq!(
        dims,
        &vec![TensorDim::Symbolic("N".into()), TensorDim::Fixed(128)]
    );
}

#[test]
fn parse_tensor_single_dim() {
    let s = parse("type Q { w: tensor<u8, [256]> }");
    let t = type_decls(&s.items)[0];
    let TypeRef::Tensor { dims, .. } = &t.fields[0].ty else {
        panic!("expected tensor");
    };
    assert_eq!(dims, &vec![TensorDim::Fixed(256)]);
}

#[test]
fn parse_tensor_trailing_comma_in_dims() {
    let s = parse("type Q { w: tensor<f32, [3, 4,]> }");
    let t = type_decls(&s.items)[0];
    let TypeRef::Tensor { dims, .. } = &t.fields[0].ty else {
        panic!("expected tensor");
    };
    assert_eq!(dims.len(), 2);
}

#[test]
fn tensor_requires_at_least_one_dim() {
    let err = parse_err("type Q { w: tensor<f32, []> }");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("at least one dimension"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn tensor_missing_close_gt_errors() {
    assert_syntax_err(parse_err("type Q { w: tensor<f32, [4] }"), "'>'");
}

#[test]
fn list_keyword_as_type_name_still_works() {
    // A user type named `list` is OK; field: list (no '<') resolves to it.
    let s = parse("type list {}\ntype Q { x: list }");
    let q = type_decls(&s.items)
        .into_iter()
        .find(|t| t.name == vec!["Q".to_string()])
        .unwrap();
    assert_eq!(q.fields[0].ty, TypeRef::Named(vec!["list".into()]));
}

#[test]
fn parse_decorator_no_args() {
    let s = parse("@hidden\ntype X {}");
    let t = type_decls(&s.items)[0];
    assert_eq!(t.decorators.len(), 1);
    assert_eq!(t.decorators[0].name, vec!["hidden".to_string()]);
    assert!(t.decorators[0].positional.is_empty());
    assert!(t.decorators[0].named.is_empty());
}

#[test]
fn parse_decorator_empty_parens() {
    let s = parse("@hidden()\ntype X {}");
    let t = type_decls(&s.items)[0];
    assert_eq!(t.decorators.len(), 1);
    assert!(t.decorators[0].positional.is_empty());
}

#[test]
fn parse_decorator_positional_args() {
    let s = parse("@range(1, 10)\ntype X {}");
    let t = type_decls(&s.items)[0];
    assert_eq!(
        t.decorators[0].positional,
        vec![Expr::I64(1), Expr::I64(10)]
    );
}

#[test]
fn parse_decorator_named_args() {
    let s = parse("@validate(min = 1, max = 10)\ntype X {}");
    let t = type_decls(&s.items)[0];
    let d = &t.decorators[0];
    assert!(d.positional.is_empty());
    assert_eq!(d.named.len(), 2);
    assert_eq!(d.named[0].name, "min");
    assert_eq!(d.named[0].value, Expr::I64(1));
    assert_eq!(d.named[1].name, "max");
    assert_eq!(d.named[1].value, Expr::I64(10));
}

#[test]
fn parse_decorator_mixed_args() {
    let s = parse("@range(0, max = 100)\ntype X {}");
    let t = type_decls(&s.items)[0];
    let d = &t.decorators[0];
    assert_eq!(d.positional, vec![Expr::I64(0)]);
    assert_eq!(d.named.len(), 1);
    assert_eq!(d.named[0].name, "max");
}

#[test]
fn parse_decorator_positional_after_named_errors() {
    let err = parse_err("@x(min = 1, 5)\ntype X {}");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message
                .contains("positional argument cannot follow named"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn parse_decorator_trailing_comma() {
    let s = parse("@x(1, 2,)\ntype X {}");
    let t = type_decls(&s.items)[0];
    assert_eq!(t.decorators[0].positional.len(), 2);
}

#[test]
fn parse_dotted_decorator() {
    let s = parse("@ui.color(:red)\ntype X {}");
    let t = type_decls(&s.items)[0];
    assert_eq!(
        t.decorators[0].name,
        vec!["ui".to_string(), "color".to_string()]
    );
    assert_eq!(t.decorators[0].positional, vec![Expr::Symbol("red".into())]);
}

#[test]
fn parse_decorator_on_type_field() {
    let s = parse("type X { @max(64) name: utf8 }");
    let t = type_decls(&s.items)[0];
    assert_eq!(t.fields[0].decorators.len(), 1);
    assert_eq!(t.fields[0].decorators[0].name, vec!["max".to_string()]);
}

#[test]
fn parse_decorator_on_variant() {
    let s = parse("union U { @hidden Circle { radius: f64 } }");
    let u = union_decls(&s.items)[0];
    assert_eq!(u.variants[0].decorators.len(), 1);
    assert_eq!(u.variants[0].decorators[0].name, vec!["hidden".to_string()]);
}

#[test]
fn parse_decorator_on_symbol_entry() {
    let s = parse("symbol_set C { @default red green }");
    let set = symbol_set_decls(&s.items)[0];
    assert_eq!(set.symbols[0].decorators.len(), 1);
    assert!(set.symbols[1].decorators.is_empty());
}

#[test]
fn parse_decorator_on_top_level_field() {
    let s = parse("@logged\nport = 8080");
    let f = field(&s.items, "port");
    assert_eq!(f.decorators.len(), 1);
    assert_eq!(f.decorators[0].name, vec!["logged".to_string()]);
}

#[test]
fn parse_decorator_on_block() {
    let s = parse(r#"@logged service "web" { port = 8080 }"#);
    let b = blocks(&s.items)[0];
    assert_eq!(b.decorators.len(), 1);
    assert_eq!(b.decorators[0].name, vec!["logged".to_string()]);
}

#[test]
fn parse_multiple_stacked_decorators() {
    let s = parse("@a @b\ntype X {}");
    let t = type_decls(&s.items)[0];
    assert_eq!(t.decorators.len(), 2);
    assert_eq!(t.decorators[0].name, vec!["a".to_string()]);
    assert_eq!(t.decorators[1].name, vec!["b".to_string()]);
}

#[test]
fn decorator_on_namespace_errors() {
    let err = parse_err("@x\nnamespace foo");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("not allowed on namespace/use"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn decorator_on_use_errors() {
    let err = parse_err("type X {}\n@x use X");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("not allowed on namespace/use"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

fn symbol_set_decls(items: &[Item]) -> Vec<&SymbolSetDecl> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::SymbolSetDecl(s) => Some(s),
            _ => None,
        })
        .collect()
}

#[test]
fn parse_symbol_set_decl() {
    let s = parse("symbol_set Color { red green blue }");
    let set = symbol_set_decls(&s.items)[0];
    assert_eq!(set.name, vec!["Color".to_string()]);
    assert_eq!(
        set.symbols
            .iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>(),
        vec!["red".to_string(), "green".to_string(), "blue".to_string()]
    );
}

#[test]
fn parse_connection_decl_basic() {
    let s = parse(
        "symbol_set EdgeKind { uses depends_on }\n\
         connection DependsOn: Service -> Service : EdgeKind",
    );
    let conn = s
        .items
        .iter()
        .filter_map(|i| match i {
            Item::ConnectionDecl(c) => Some(c),
            _ => None,
        })
        .next()
        .expect("connection decl present");
    assert_eq!(conn.name, vec!["DependsOn".to_string()]);
    assert_eq!(conn.source.to_string(), "Service");
    assert_eq!(conn.destination.to_string(), "Service");
    assert_eq!(conn.kind_set, vec!["EdgeKind".to_string()]);
}

#[test]
fn parse_connection_stmt_default_kind() {
    let s = parse("web -> db");
    let stmt = s
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Connection(c) => Some(c),
            _ => None,
        })
        .next()
        .expect("connection stmt present");
    assert_eq!(stmt.lhs, "web");
    assert_eq!(stmt.rhs, "db");
    assert!(stmt.kind.is_none());
}

#[test]
fn parse_connection_stmt_explicit_kind() {
    let s = parse("web -> db :uses");
    let stmt = s
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Connection(c) => Some(c),
            _ => None,
        })
        .next()
        .expect("connection stmt present");
    assert_eq!(stmt.lhs, "web");
    assert_eq!(stmt.rhs, "db");
    assert_eq!(stmt.kind.as_deref(), Some("uses"));
}

#[test]
fn parse_empty_symbol_set() {
    let s = parse("symbol_set Empty {}");
    let set = symbol_set_decls(&s.items)[0];
    assert!(set.symbols.is_empty());
}

#[test]
fn parse_dotted_symbol_set_name() {
    let s = parse("symbol_set foo.bar.X { a b }");
    let set = symbol_set_decls(&s.items)[0];
    assert_eq!(
        set.name,
        vec!["foo".to_string(), "bar".to_string(), "X".to_string()]
    );
    assert_eq!(set.symbols.len(), 2);
}

#[test]
fn parse_symbol_value() {
    let s = parse("tag = :wide");
    assert_eq!(field(&s.items, "tag").expr, Expr::Symbol("wide".into()));
}

#[test]
fn parse_symbol_typed_field() {
    let s = parse("type Q { tag: symbol }");
    let t = type_decls(&s.items)[0];
    assert_eq!(t.fields[0].ty, TypeRef::Builtin(BuiltinType::Symbol));
}

#[test]
fn parse_named_symbol_set_field() {
    let s = parse("symbol_set C { x }\ntype Q { f: C }");
    let q = type_decls(&s.items)
        .into_iter()
        .find(|t| t.name == vec!["Q".to_string()])
        .unwrap();
    assert_eq!(q.fields[0].ty, TypeRef::Named(vec!["C".into()]));
}

#[test]
fn symbol_set_requires_brace() {
    assert_syntax_err(parse_err("symbol_set Foo = 1"), "'{'");
}

#[test]
fn parse_namespace_declaration() {
    let s = parse("namespace foo.bar");
    match &s.items[0] {
        Item::NamespaceDecl(n) => {
            assert_eq!(n.path, vec!["foo".to_string(), "bar".to_string()])
        }
        _ => panic!("expected NamespaceDecl"),
    }
}

#[test]
fn parse_use_bare_no_alias() {
    let s = parse("use foo.bar.Baz");
    match &s.items[0] {
        Item::UseDecl(u) => {
            assert_eq!(
                u.path,
                vec!["foo".to_string(), "bar".to_string(), "Baz".to_string()]
            );
            assert!(matches!(u.form, UseForm::Bare(None)));
        }
        _ => panic!("expected UseDecl"),
    }
}

#[test]
fn parse_use_bare_with_alias() {
    let s = parse("use foo.bar.Baz as MyBaz");
    match &s.items[0] {
        Item::UseDecl(u) => match &u.form {
            UseForm::Bare(Some(a)) => assert_eq!(a, "MyBaz"),
            _ => panic!("expected Bare(Some)"),
        },
        _ => panic!("expected UseDecl"),
    }
}

#[test]
fn parse_use_brace_list() {
    let s = parse("use foo.bar.{X, Y as Z}");
    match &s.items[0] {
        Item::UseDecl(u) => {
            assert_eq!(u.path, vec!["foo".to_string(), "bar".to_string()]);
            let UseForm::List(items) = &u.form else {
                panic!("expected List");
            };
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "X");
            assert_eq!(items[0].alias, None);
            assert_eq!(items[1].name, "Y");
            assert_eq!(items[1].alias.as_deref(), Some("Z"));
        }
        _ => panic!("expected UseDecl"),
    }
}

#[test]
fn parse_use_brace_trailing_comma() {
    let s = parse("use foo.bar.{X, Y,}");
    match &s.items[0] {
        Item::UseDecl(u) => match &u.form {
            UseForm::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected List"),
        },
        _ => panic!("expected UseDecl"),
    }
}

#[test]
fn parse_use_brace_empty_list() {
    let s = parse("use foo.bar.{}");
    match &s.items[0] {
        Item::UseDecl(u) => match &u.form {
            UseForm::List(items) => assert!(items.is_empty()),
            _ => panic!("expected List"),
        },
        _ => panic!("expected UseDecl"),
    }
}

#[test]
fn parse_dotted_type_decl() {
    let s = parse("type a.b.X {}");
    match &s.items[0] {
        Item::TypeDecl(t) => assert_eq!(
            t.name,
            vec!["a".to_string(), "b".to_string(), "X".to_string()]
        ),
        _ => panic!("expected TypeDecl"),
    }
}

#[test]
fn parse_dotted_type_ref() {
    let s = parse("type Q { f: a.b.X }");
    match &s.items[0] {
        Item::TypeDecl(t) => assert_eq!(
            t.fields[0].ty,
            TypeRef::Named(vec!["a".to_string(), "b".to_string(), "X".to_string()])
        ),
        _ => panic!("expected TypeDecl"),
    }
}

#[test]
fn parse_dotted_reference_type() {
    let s = parse("type Q { f: &a.b.X? }");
    match &s.items[0] {
        Item::TypeDecl(t) => assert_eq!(
            t.fields[0].ty,
            TypeRef::Reference(Box::new(TypeRef::Named(vec![
                "a".to_string(),
                "b".to_string(),
                "X".to_string()
            ])))
        ),
        _ => panic!("expected TypeDecl"),
    }
}

#[test]
fn path_trailing_dot_errors() {
    let err = parse_err("namespace foo.");
    match err {
        ParseError::Syntax(e) => {
            assert!(e.message.contains("expected identifier"), "{}", e.message)
        }
        _ => panic!("expected syntax error"),
    }
}

// ─── Functions & expressions ────────────────────────────────────────

#[test]
fn parse_function_literal_bare_body() {
    let s = parse("double = fn(x: i32) -> i32 x * 2");
    let f = field(&s.items, "double");
    let Expr::Function(lit) = &f.expr else {
        panic!("expected function literal")
    };
    assert_eq!(lit.params.len(), 1);
    assert_eq!(lit.params[0].name, "x");
    assert_eq!(lit.params[0].ty, TypeRef::Builtin(BuiltinType::I32));
    assert_eq!(lit.return_ty, TypeRef::Builtin(BuiltinType::I32));
    let Expr::Binary { op, .. } = &*lit.body else {
        panic!("expected binary body")
    };
    assert_eq!(*op, BinOp::Mul);
}

#[test]
fn parse_function_literal_block_body() {
    let s = parse("sum_squared = fn(x: i32, y: i32) -> i32 {\n  let s = x + y;\n  s * s\n}");
    let f = field(&s.items, "sum_squared");
    let Expr::Function(lit) = &f.expr else {
        panic!("expected function literal")
    };
    assert_eq!(lit.params.len(), 2);
    let Expr::Block { lets, tail, .. } = &*lit.body else {
        panic!("expected block body")
    };
    assert_eq!(lets.len(), 1);
    assert_eq!(lets[0].name, "s");
    let Expr::Binary { op, .. } = &**tail else {
        panic!("expected binary tail")
    };
    assert_eq!(*op, BinOp::Mul);
}

#[test]
fn parse_function_with_no_params() {
    let s = parse("k = fn() -> i32 42");
    let f = field(&s.items, "k");
    let Expr::Function(lit) = &f.expr else {
        panic!("expected function literal")
    };
    assert!(lit.params.is_empty());
    assert!(matches!(&*lit.body, Expr::I64(42)));
}

#[test]
fn parse_function_type_in_field() {
    let s = parse("type Handler { on_click: fn(i32) -> bool on_drag: fn(i32, i32) -> bool }");
    let t = type_decls(&s.items)[0];
    let TypeRef::Function { params, return_ty } = &t.fields[0].ty else {
        panic!("expected fn type")
    };
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], TypeRef::Builtin(BuiltinType::I32));
    assert_eq!(**return_ty, TypeRef::Builtin(BuiltinType::Bool));
    let TypeRef::Function { params, .. } = &t.fields[1].ty else {
        panic!("expected fn type")
    };
    assert_eq!(params.len(), 2);
}

#[test]
fn parse_function_type_zero_args() {
    let s = parse("type T { thunk: fn() -> i32 }");
    let t = type_decls(&s.items)[0];
    let TypeRef::Function { params, return_ty } = &t.fields[0].ty else {
        panic!("expected fn type")
    };
    assert!(params.is_empty());
    assert_eq!(**return_ty, TypeRef::Builtin(BuiltinType::I32));
}

#[test]
fn parse_call_expression() {
    let s = parse("y = f(1, 2)");
    let f = field(&s.items, "y");
    let Expr::Call { callee, args, .. } = &f.expr else {
        panic!("expected call")
    };
    assert!(matches!(&**callee, Expr::Identifier(n) if n == "f"));
    assert_eq!(args.len(), 2);
}

#[test]
fn parse_arithmetic_precedence() {
    // 1 + 2 * 3 should bind as 1 + (2 * 3).
    let s = parse("a = 1 + 2 * 3");
    let f = field(&s.items, "a");
    let Expr::Binary {
        op: BinOp::Add,
        lhs,
        rhs,
        ..
    } = &f.expr
    else {
        panic!("expected top-level Add")
    };
    assert!(matches!(&**lhs, Expr::I64(1)));
    let Expr::Binary { op: BinOp::Mul, .. } = &**rhs else {
        panic!("expected nested Mul on rhs")
    };
}

#[test]
fn parse_comparison_and_logical() {
    // x > 100 && x < 1000 should bind as (x > 100) && (x < 1000).
    let s = parse("ok = x > 100 && x < 1000");
    let f = field(&s.items, "ok");
    let Expr::Binary {
        op: BinOp::And,
        lhs,
        rhs,
        ..
    } = &f.expr
    else {
        panic!("expected top-level And")
    };
    assert!(matches!(&**lhs, Expr::Binary { op: BinOp::Gt, .. }));
    assert!(matches!(&**rhs, Expr::Binary { op: BinOp::Lt, .. }));
}

#[test]
fn parse_unary_neg_and_not() {
    let s = parse("a = -x\nb = !flag");
    let a = field(&s.items, "a");
    let Expr::Unary {
        op: UnaryOp::Neg, ..
    } = &a.expr
    else {
        panic!("expected unary neg")
    };
    let b = field(&s.items, "b");
    let Expr::Unary {
        op: UnaryOp::Not, ..
    } = &b.expr
    else {
        panic!("expected unary not")
    };
}

#[test]
fn parse_paren_expression() {
    let s = parse("x = (1 + 2) * 3");
    let f = field(&s.items, "x");
    let Expr::Binary {
        op: BinOp::Mul,
        lhs,
        ..
    } = &f.expr
    else {
        panic!("expected top-level Mul")
    };
    assert!(matches!(&**lhs, Expr::Paren { .. }));
}

#[test]
fn parse_function_literal_with_call_in_body() {
    let s = parse("apply = fn(n: i32) -> i32 add(n, 1)");
    let f = field(&s.items, "apply");
    let Expr::Function(lit) = &f.expr else {
        panic!("expected function")
    };
    assert!(matches!(&*lit.body, Expr::Call { .. }));
}

#[test]
fn parse_function_returning_function() {
    let s = parse("k = fn(x: i32) -> fn(i32) -> i32 fn(y: i32) -> i32 x + y");
    let f = field(&s.items, "k");
    let Expr::Function(outer) = &f.expr else {
        panic!("expected outer fn")
    };
    let TypeRef::Function { .. } = &outer.return_ty else {
        panic!("outer return should be fn type")
    };
    assert!(matches!(&*outer.body, Expr::Function(_)));
}

#[test]
fn parse_block_let_bindings_then_tail() {
    // Standalone block expression (no surrounding fn).
    let s = parse("x = { let a = 1; let b = 2; a + b }");
    let f = field(&s.items, "x");
    let Expr::Block { lets, tail, .. } = &f.expr else {
        panic!("expected block")
    };
    assert_eq!(lets.len(), 2);
    assert!(matches!(&**tail, Expr::Binary { op: BinOp::Add, .. }));
}

#[test]
fn missing_arrow_errors() {
    assert_syntax_err(parse_err("f = fn(x: i32) i32 x"), "'->'");
}

#[test]
fn missing_return_type_errors() {
    // `fn(x: i32) ->` with nothing after the arrow should fail to parse
    // a type ref.
    let err = parse_err("f = fn(x: i32) -> ");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("expected type") || e.message.contains("end of file"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn trailing_semi_on_block_tail_errors() {
    let err = parse_err("x = { let a = 1; a; }");
    match err {
        ParseError::Syntax(_) => {}
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn empty_block_expression_errors() {
    let err = parse_err("x = {}");
    match err {
        ParseError::Syntax(e) => {
            assert!(e.message.contains("final expression"), "{}", e.message)
        }
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn field_named_fn_still_parses_as_field() {
    let s = parse("fn = 1");
    assert_eq!(field(&s.items, "fn").expr, Expr::I64(1));
}

#[test]
fn parse_empty_list_literal() {
    let s = parse("x = []");
    let f = field(&s.items, "x");
    let Expr::ListLit { elements, .. } = &f.expr else {
        panic!("expected list literal, got {:?}", f.expr)
    };
    assert!(elements.is_empty());
}

#[test]
fn parse_list_literal_with_elements() {
    let s = parse("x = [1, 2, 3]");
    let f = field(&s.items, "x");
    let Expr::ListLit { elements, .. } = &f.expr else {
        panic!("expected list literal")
    };
    assert_eq!(elements.len(), 3);
    assert_eq!(elements[0], Expr::I64(1));
    assert_eq!(elements[2], Expr::I64(3));
}

#[test]
fn parse_nested_list_literal() {
    let s = parse("x = [[1, 2], [3, 4]]");
    let f = field(&s.items, "x");
    let Expr::ListLit { elements, .. } = &f.expr else {
        panic!("expected outer list literal")
    };
    assert_eq!(elements.len(), 2);
    let Expr::ListLit {
        elements: inner, ..
    } = &elements[0]
    else {
        panic!("expected inner list literal")
    };
    assert_eq!(inner.len(), 2);
    assert_eq!(inner[0], Expr::I64(1));
}

#[test]
fn parse_list_literal_trailing_comma() {
    let s = parse("x = [1, 2,]");
    let f = field(&s.items, "x");
    let Expr::ListLit { elements, .. } = &f.expr else {
        panic!("expected list literal")
    };
    assert_eq!(elements.len(), 2);
}

fn table_items(items: &[Item]) -> Vec<&crate::ast::TableItem> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::Table(t) => Some(t),
            _ => None,
        })
        .collect()
}

#[test]
fn parse_table_with_one_row() {
    let s = parse(r#"db x { users: | "a" | 30 | true | }"#);
    let b = blocks(&s.items)[0];
    let tables = table_items(&b.items);
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].field_name, "users");
    assert_eq!(tables[0].rows.len(), 1);
    assert_eq!(tables[0].rows[0].values.len(), 3);
}

#[test]
fn parse_table_with_multiple_rows() {
    let s = parse(
        r#"
        db x {
          users:
            | "a" | 30 | true |
            | "b" | 25 | false |
            | "c" | 42 | true |
        }
        "#,
    );
    let b = blocks(&s.items)[0];
    let tables = table_items(&b.items);
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].rows.len(), 3);
}

#[test]
fn parse_table_trailing_pipe_optional() {
    let s = parse(r#"db x { users: | "a" | 30 }"#);
    let b = blocks(&s.items)[0];
    let tables = table_items(&b.items);
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].rows[0].values.len(), 2);
}

#[test]
fn parse_empty_table_header() {
    let s = parse(r#"db x { users: }"#);
    let b = blocks(&s.items)[0];
    let tables = table_items(&b.items);
    assert_eq!(tables.len(), 1);
    assert!(tables[0].rows.is_empty());
}

#[test]
fn parse_table_alongside_other_items() {
    let s = parse(
        r#"
        db x {
          port = 8080
          users:
            | "a" | 1 |
          meta { region = "us" }
        }
        "#,
    );
    let b = blocks(&s.items)[0];
    assert!(
        table_items(&b.items)
            .iter()
            .any(|t| t.field_name == "users")
    );
    // Other items still parse.
    let inner_blocks: Vec<&Block> = b
        .items
        .iter()
        .filter_map(|i| {
            if let Item::Block(b) = i {
                Some(b)
            } else {
                None
            }
        })
        .collect();
    assert!(inner_blocks.iter().any(|b| b.kind == "meta"));
}

#[test]
fn parser_rejects_decorator_on_table_header() {
    let err = parse_err(r#"db x { @logged users: | 1 | }"#);
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("decorators are not allowed on table"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn parse_list_literal_of_strings() {
    let s = parse(r#"x = ["a", "b"]"#);
    let f = field(&s.items, "x");
    let Expr::ListLit { elements, .. } = &f.expr else {
        panic!("expected list literal")
    };
    assert_eq!(elements[0], Expr::Utf8("a".into()));
    assert_eq!(elements[1], Expr::Utf8("b".into()));
}

// ─── Symbol index ────────────────────────────────────────────────

#[test]
fn index_includes_top_level_decls_and_members() {
    let (_, idx) = parse_with_index(
        r#"
        type User { name: utf8 age: u32 }
        union Shape { Circle { r: f64 } Square none }
        symbol_set Color { red green }
        port = 8080
        "#,
    );
    let rec = idx.lookup("User").expect("User indexed");
    assert!(matches!(rec.kind, SymbolKind::TypeDecl));
    let rec = idx.lookup("User.name").expect("User.name indexed");
    assert!(matches!(rec.kind, SymbolKind::TypeField { .. }));
    let rec = idx.lookup("Shape.Circle").expect("Shape.Circle indexed");
    assert!(matches!(rec.kind, SymbolKind::UnionVariant { .. }));
    let rec = idx.lookup("Color.red").expect("Color.red indexed");
    assert!(matches!(rec.kind, SymbolKind::SymbolEntry { .. }));
    let rec = idx.lookup("port").expect("port indexed");
    assert!(matches!(rec.kind, SymbolKind::Field));
}

#[test]
fn index_composes_with_file_namespace() {
    let (_, idx) = parse_with_index("namespace foo\ntype Bar { x: i32 }");
    assert!(idx.lookup("foo.Bar").is_some());
    assert!(idx.lookup("foo.Bar.x").is_some());
    // Without the namespace prefix the entries should NOT be found.
    assert!(idx.lookup("Bar").is_none());
    assert!(idx.lookup("Bar.x").is_none());
}

#[test]
fn index_tracks_blocks_by_kind() {
    let (_, idx) = parse_with_index(
        r#"
        service "a" { port = 1 }
        service "b" { port = 2 }
        metadata { region = "us" }
        "#,
    );
    assert_eq!(idx.blocks_with_kind("service").len(), 2);
    assert_eq!(idx.blocks_with_kind("metadata").len(), 1);
    assert_eq!(idx.blocks_with_kind("unknown").len(), 0);
}

#[test]
fn parser_rejects_duplicate_top_level_field() {
    let err = parse_err("port = 1\nport = 2");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("duplicate declaration 'port'"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn parser_rejects_field_and_typedecl_with_same_fqn() {
    let err = parse_err("Foo = 1\ntype Foo {}");
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
fn parser_rejects_duplicate_type_field() {
    let err = parse_err("type Foo { x: i32 x: u8 }");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("duplicate field 'x' in type 'Foo'"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn parser_rejects_duplicate_variant() {
    let err = parse_err("union X { A none A none }");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("duplicate variant 'A' in union 'X'"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn parse_top_level_import() {
    let s = parse(r#"import "./foo.wcl""#);
    match &s.items[0] {
        Item::Import(i) => assert_eq!(i.path, "./foo.wcl"),
        _ => panic!("expected Item::Import"),
    }
}

#[test]
fn parse_block_level_import() {
    let s = parse(r#"service "web" { import "./x.wcl" }"#);
    let b = blocks(&s.items)[0];
    assert!(matches!(b.items.first(), Some(Item::Import(_))));
}

#[test]
fn parser_rejects_decorator_on_import() {
    let err = parse_err(r#"@logged import "./p.wcl""#);
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("decorators are not allowed on import"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}

#[test]
fn parser_rejects_duplicate_symbol_entry() {
    let err = parse_err("symbol_set C { a a }");
    match err {
        ParseError::Syntax(e) => assert!(
            e.message.contains("duplicate symbol 'a' in symbol_set 'C'"),
            "{}",
            e.message
        ),
        _ => panic!("expected syntax error"),
    }
}
