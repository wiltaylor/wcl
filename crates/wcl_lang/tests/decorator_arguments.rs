//! Integration tests: decorator argument checking.

use wcl_lang::{
    Document, Environment, EvalError, Registry, SchemaViolationKind, Span, Value, ast, disk_loader,
    parse_for_edit,
};

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
        "inline",
        "default",
        "child",
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

    let block = doc.decorator_schema("block").expect("@block schema exists");
    let block_fields: Vec<_> = block.fields().collect();
    assert_eq!(block_fields.len(), 4);
    assert_eq!(block_fields[0].name(), "name");
    assert_eq!(block_fields[0].inline_slot(), Some(0));
    assert!(
        block_fields[1..]
            .iter()
            .all(|field| field.inline_slot().is_none()),
        "@block constraint slots are named-only"
    );

    let children = doc
        .decorator_schema("children")
        .expect("@children schema exists");
    let children_fields: Vec<_> = children.fields().collect();
    assert_eq!(children_fields.len(), 3);
    assert_eq!(children_fields[0].name(), "kind");
    assert_eq!(children_fields[0].inline_slot(), Some(0));
    assert_eq!(children_fields[1].name(), "min");
    assert_eq!(children_fields[2].name(), "max");
    assert!(children_fields[1..].iter().all(|field| field.optional()));

    let decorator = doc
        .decorator_schema("decorator")
        .expect("@decorator schema exists");
    assert_eq!(
        decorator
            .field("name")
            .and_then(|field| field.inline_slot()),
        Some(0)
    );
    assert_eq!(
        decorator
            .field("repeatable")
            .and_then(|field| field.inline_slot()),
        None,
        "repeatable is named-only"
    );

    let schemaless = doc
        .decorator_schema("schemaless")
        .expect("@schemaless schema exists");
    assert_eq!(
        schemaless
            .field("reason")
            .and_then(|field| field.inline_slot()),
        Some(0)
    );
    assert_eq!(
        schemaless
            .field("annotations")
            .and_then(|field| field.inline_slot()),
        None,
        "annotations is named-only"
    );

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
fn builtin_reference_slots_and_polymorphic_defaults_keep_their_authored_forms() {
    let source = r#"
interface ChildType {}
type Endpoint {}
symbol_set Kind { link }
connection Edge: Endpoint -> Endpoint : Kind
@document type Root {
  @children(ChildType) children: list<ChildType>
  @connections(Edge) edges: list<Edge>
}
type Options { @default(true) enabled: bool }
"#;

    let errors = schema_errors(source);
    assert!(errors.is_empty(), "{errors:#?}");
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

fn sources_for_every_argument_position(decorators: &str) -> Vec<(&'static str, String)> {
    let declaration = "@decorator(\"typed\") type Typed { @inline(0) value: i64 }\n";
    vec![
        (
            "document field",
            format!("{declaration}@document type Root {{ value: i64 }}\n{decorators}\nvalue = 1\n"),
        ),
        (
            "fn item",
            format!("{declaration}{decorators}\nfn answer() -> i64 42\n"),
        ),
        (
            "block",
            format!(
                "{declaration}@document type Root {{ @child(\"thing\") thing: Thing }}\n@block(\"thing\") type Thing {{}}\n{decorators}\nthing {{}}\n"
            ),
        ),
        (
            "type",
            format!("{declaration}{decorators}\ntype Example {{}}\n"),
        ),
        (
            "interface",
            format!("{declaration}{decorators}\ninterface Example {{}}\n"),
        ),
        (
            "type field",
            format!("{declaration}type Example {{\n{decorators}\nvalue: i64\n}}\n"),
        ),
        (
            "union",
            format!("{declaration}{decorators}\nunion Example {{ Value none }}\n"),
        ),
        (
            "union variant",
            format!("{declaration}union Example {{\n{decorators}\nValue none\n}}\n"),
        ),
        (
            "symbol set",
            format!("{declaration}{decorators}\nsymbol_set Example {{ value }}\n"),
        ),
        (
            "symbol entry",
            format!("{declaration}symbol_set Example {{\n{decorators}\nvalue\n}}\n"),
        ),
        (
            "connection",
            format!(
                "{declaration}type Endpoint {{}}\nsymbol_set Kind {{ link }}\n{decorators}\nconnection Edge: Endpoint -> Endpoint : Kind\n"
            ),
        ),
    ]
}

#[test]
fn decorator_arguments_are_checked_in_every_position() {
    for (position, source) in sources_for_every_argument_position("@typed(\"wrong\")") {
        let errors = schema_errors(&source);
        let mismatches: Vec<_> = errors
            .iter()
            .filter(|error| {
                matches!(
                    error,
                    EvalError::SchemaViolation {
                        kind: SchemaViolationKind::FieldTypeMismatch,
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(
            mismatches.len(),
            1,
            "{position} must validate decorator arguments: {errors:#?}"
        );
    }
}

#[test]
fn qualified_decorator_arguments_use_the_selected_namespace_schema() {
    let mut registry = Registry::new();
    registry.register(
        "one.wcl",
        "namespace one\n@decorator(\"note\") type Note { code: i64 }\n",
    );
    registry.register(
        "two.wcl",
        "namespace two\n@decorator(\"note\") type Note { message: utf8 }\n",
    );
    let source = "import <one.wcl>\nimport <two.wcl>\n@two.note(message = 7) type Target {}\n";
    let document = Document::open_at_with_loader(
        source,
        "decorator_arguments.wcl",
        None,
        &Environment::new(),
        registry.loader(disk_loader()),
    )
    .expect("source opens");
    let errors = document.schema_errors();

    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(matches!(
        errors[0],
        EvalError::SchemaViolation {
            kind: SchemaViolationKind::FieldTypeMismatch,
            ..
        }
    ));
}

#[test]
fn qualified_decorator_slots_resolve_aliases_in_the_schema_namespace() {
    let mut registry = Registry::new();
    registry.register(
        "lib.wcl",
        r#"
namespace lib
type Count = i64
symbol_set Level { low }
@decorator("note") type Note { count: Count  level: Level }
"#,
    );
    let document = Document::open_at_with_loader(
        "import <lib.wcl>\n@lib.note(count = 7, level = :low) type Target {}\n",
        "decorator_arguments.wcl",
        None,
        &Environment::new(),
        registry.loader(disk_loader()),
    )
    .expect("source opens");

    let errors = document.schema_errors();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn qualified_decorator_slots_check_symbol_sets_in_the_schema_namespace() {
    let mut registry = Registry::new();
    registry.register(
        "lib.wcl",
        r#"
namespace lib
symbol_set Level { low }
@decorator("note") type Note { level: Level }
"#,
    );
    let document = Document::open_at_with_loader(
        "import <lib.wcl>\n@lib.note(level = :high) type Target {}\n",
        "decorator_arguments.wcl",
        None,
        &Environment::new(),
        registry.loader(disk_loader()),
    )
    .expect("source opens");

    let errors = document.schema_errors();
    assert_eq!(
        errors
            .iter()
            .filter(|error| matches!(
                error,
                EvalError::SchemaViolation {
                    kind: SchemaViolationKind::SymbolNotInSet,
                    ..
                }
            ))
            .count(),
        1,
        "{errors:#?}"
    );
}

#[test]
fn annotation_exemptions_skip_every_decorator_check_in_every_position() {
    for exemption in ["@schemaless", "@schemaless(annotations = true)"] {
        let decorators = format!("{exemption}\n@typed(\"wrong\")\n@typed(\"wrong\")");
        for (position, source) in sources_for_every_argument_position(&decorators) {
            let errors = schema_errors(&source);
            assert!(
                errors.is_empty(),
                "{exemption} must exempt {position} completely: {errors:#?}"
            );
        }
    }
}

#[test]
fn bare_schemaless_block_skips_decorator_checks_in_its_contents() {
    let source = r#"
@decorator("typed") type Typed { @inline(0) value: i64 }
@document type Root { @child("thing") thing: Thing }
@block("thing") type Thing { known: i64 }

@schemaless
thing {
  @typed("wrong")
  known = 1
}
"#;

    assert!(
        schema_errors(source).is_empty(),
        "{:#?}",
        schema_errors(source)
    );
}

#[test]
fn union_dispatched_block_decorator_arguments_are_checked() {
    let source = r#"
@decorator("typed") type Typed { @inline(0) value: i64 }
union Shape { Circle { radius: f64 } }
@document type Root { @child("scene") scene: Scene }
@block("scene") type Scene { @children(Shape) shapes: list<Shape> }
scene "main" { @typed("wrong") circle { radius = 1.5 } }
"#;

    let errors = schema_errors(source);
    assert!(
        errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                kind: SchemaViolationKind::FieldTypeMismatch,
                ..
            }
        )),
        "{errors:#?}"
    );
}

#[test]
fn structural_union_payload_is_not_treated_as_unknown_decorator_fields() {
    let source = r#"
union Choice { Two { message: utf8 } }
@decorator("select") type Select { value: Choice }
@select(message = "selected") type Target {}
"#;

    let errors = schema_errors(source);
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn structural_union_type_mismatch_is_spanned_on_the_offending_argument() {
    let source = r#"
union Choice { Two { message: utf8 } }
@decorator("select") type Select { value: Choice }
@select(message = 42) type Target {}
"#;

    let errors = schema_errors(source);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
        panic!("expected a schema violation: {errors:#?}");
    };
    assert_eq!(*kind, SchemaViolationKind::VariantNoMatch);
    let argument = "message = 42";
    assert_eq!(
        span.offset(),
        source.find(argument).expect("argument is present")
    );
    assert_eq!(span.len(), argument.len());
}

#[test]
fn omitted_union_typed_slot_uses_its_declared_default() {
    let source = r#"
union Choice { One { code: i64 } }
@decorator("select")
type Select {
  @inline(0) label: utf8?
  @default(Choice::One { code: 7 }) value: Choice
}
@select("label")
type Target {}
"#;
    let document = Document::open(source, "decorator_arguments.wcl").expect("source opens");
    let decorator = document
        .type_decl("Target")
        .expect("target type")
        .decorators()
        .next()
        .expect("decorator");
    let value = decorator
        .resolved_arg_value("value")
        .expect("declared slot")
        .expect("default evaluates");

    assert!(matches!(value, Value::Variant { ref variant, .. } if variant == "One"));
}
