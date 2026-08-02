use wcl_lang::{Document, EvalError, SchemaViolationKind, Span, Value, ast, parse_for_edit};

fn first_type_decorator(src: &str) -> ast::Decorator {
    let parsed = parse_for_edit(src, "decorator_arguments.wcl").expect("source parses");
    let ast::Item::TypeDecl(decl) = &parsed.items[0] else {
        panic!("expected a type declaration");
    };
    decl.decorators[0].clone()
}

fn schema_errors(src: &str) -> Vec<EvalError> {
    Document::open(src, "decorator_arguments.wcl")
        .expect("source opens")
        .schema_errors()
}

#[test]
fn decorator_retains_name_and_positional_argument_spans() {
    let src = "@foo.bar(11, \"two\")\ntype Example {}\n";
    let decorator = first_type_decorator(src);

    assert_eq!(decorator.name_span, Span::new(1, 8));
    assert_eq!(
        decorator.positional_spans,
        vec![Span::new(9, 11), Span::new(13, 18)]
    );
}

#[test]
fn positional_argument_with_wrong_type_is_spanned_on_the_argument() {
    let src = r#"
@decorator("deploy") type Deploy { @inline(0) target: utf8 }
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@deploy(42)
service api {}
"#;

    let errors = schema_errors(src);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
        panic!("expected a schema violation: {errors:?}");
    };
    assert_eq!(*kind, SchemaViolationKind::FieldTypeMismatch);
    assert_eq!(span.offset(), src.find("42").expect("argument is present"));
    assert_eq!(span.len(), 2);
}

#[test]
fn unknown_named_argument_is_spanned_on_the_argument() {
    let src = r#"
@decorator("deploy") type Deploy { target: utf8? }
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@deploy(targte = "prod")
service api {}
"#;

    let errors = schema_errors(src);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
        panic!("expected a schema violation: {errors:?}");
    };
    assert_eq!(*kind, SchemaViolationKind::UnknownField);
    let argument = "targte = \"prod\"";
    assert_eq!(
        span.offset(),
        src.find(argument).expect("argument is present")
    );
    assert_eq!(span.len(), argument.len());
}

#[test]
fn named_argument_with_wrong_type_is_spanned_on_the_argument() {
    let src = r#"
@decorator("deploy") type Deploy { target: utf8 }
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@deploy(target = 42)
service api {}
"#;

    let errors = schema_errors(src);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
        panic!("expected a schema violation: {errors:?}");
    };
    assert_eq!(*kind, SchemaViolationKind::FieldTypeMismatch);
    let argument = "target = 42";
    assert_eq!(
        span.offset(),
        src.find(argument).expect("argument is present")
    );
    assert_eq!(span.len(), argument.len());
}

#[test]
fn missing_required_argument_is_spanned_on_the_decorator_name() {
    let src = r#"
@decorator("deploy") type Deploy { target: utf8 }
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@deploy
service api {}
"#;

    let errors = schema_errors(src);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
        panic!("expected a schema violation: {errors:?}");
    };
    assert_eq!(*kind, SchemaViolationKind::MissingRequired);
    assert_eq!(span.offset(), src.rfind("deploy").expect("use is present"));
    assert_eq!(span.len(), "deploy".len());
}

#[test]
fn surplus_positional_argument_is_spanned_on_the_surplus_value() {
    let src = r#"
@decorator("deploy") type Deploy { @inline(0) target: utf8 }
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@deploy("prod", "surplus")
service api {}
"#;

    let errors = schema_errors(src);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
        panic!("expected a schema violation: {errors:?}");
    };
    assert_eq!(*kind, SchemaViolationKind::UnknownField);
    let argument = "\"surplus\"";
    assert_eq!(
        span.offset(),
        src.find(argument).expect("argument is present")
    );
    assert_eq!(span.len(), argument.len());
}

#[test]
fn named_only_slot_rejects_a_positional_argument_as_surplus() {
    let src = r#"
@decorator("deploy") type Deploy { target: utf8? }
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@deploy("prod")
service api {}
"#;

    let errors = schema_errors(src);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
        panic!("expected a schema violation: {errors:?}");
    };
    assert_eq!(*kind, SchemaViolationKind::UnknownField);
    let argument = "\"prod\"";
    assert_eq!(
        span.offset(),
        src.rfind(argument).expect("argument is present")
    );
    assert_eq!(span.len(), argument.len());
}

#[test]
fn constraint_decorators_on_slots_apply_to_use_site_values() {
    let src = r#"
@decorator("retries") type Retries { @inline(0) @min(1) count: i64 }
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@retries(0)
service api {}
"#;

    let errors = schema_errors(src);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
        panic!("expected a schema violation: {errors:?}");
    };
    assert_eq!(*kind, SchemaViolationKind::ConstraintViolation);
    let argument_offset = src.find("@retries(0)").expect("use is present") + "@retries(".len();
    assert_eq!(span.offset(), argument_offset);
    assert_eq!(span.len(), 1);
}

#[test]
fn synthesized_single_slot_builtins_are_positional_and_still_resolve() {
    let doc = Document::open(
        "@block(\"service\") type Service {}\n",
        "decorator_arguments.wcl",
    )
    .expect("source opens");

    for name in [
        "block",
        "decorator",
        "inline",
        "default",
        "child",
        "children",
        "table",
        "connections",
        "document",
        "contextual",
    ] {
        let schema = doc.decorator_schema(name).expect("built-in schema exists");
        let fields: Vec<_> = schema.fields().collect();
        assert_eq!(fields.len(), 1, "@{name} should have one slot");
        assert_eq!(fields[0].inline_slot(), Some(0), "@{name} slot");
    }

    let service = doc.type_decl("Service").expect("service declaration");
    let block = service
        .decorators()
        .find(|decorator| decorator.name() == "block")
        .expect("@block use");
    assert_eq!(
        block.resolved_arg_value("name"),
        Some(Ok(Value::Utf8("service".into())))
    );
}

#[test]
fn positional_readback_uses_inline_slot_not_declaration_order() {
    let src = r#"
@decorator("deploy") type Deploy {
  note: utf8?
  @inline(0) target: utf8
}
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@deploy("prod")
service api {}
"#;
    let doc = Document::open(src, "decorator_arguments.wcl").expect("source opens");
    let service = doc.block("service").expect("service block");
    let deploy = service
        .decorators()
        .find(|decorator| decorator.name() == "deploy")
        .expect("@deploy use");

    assert_eq!(deploy.resolved_arg_value("note"), None);
    assert_eq!(
        deploy.resolved_arg_value("target"),
        Some(Ok(Value::Utf8("prod".into())))
    );
}

#[test]
fn decorator_with_only_named_arguments_is_unaffected() {
    let src = r#"
@decorator("deploy") type Deploy { target: utf8  note: utf8? }
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@deploy(target = "prod")
service api {}
"#;

    assert!(schema_errors(src).is_empty(), "{:?}", schema_errors(src));
}

#[test]
fn decorator_argument_must_belong_to_its_declared_symbol_set() {
    let src = r#"
symbol_set Environment { prod dev }
@decorator("deploy") type Deploy { @inline(0) environment: Environment }
@document type Config { @children("service") services: list<Service> }
@block("service") type Service { @inline(0) name: identifier }

@deploy(:qa)
service api {}
"#;

    let errors = schema_errors(src);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
        panic!("expected a schema violation: {errors:?}");
    };
    assert_eq!(*kind, SchemaViolationKind::SymbolNotInSet);
    let argument = ":qa";
    assert_eq!(
        span.offset(),
        src.find(argument).expect("argument is present")
    );
    assert_eq!(span.len(), argument.len());
}
