use wcl_lang::{
    BuiltinType, Document, Environment, EvalError, Registry, TypeRef, Value, disk_loader,
};

fn schema_errors(src: &str) -> Vec<EvalError> {
    Document::open(src, "decorator-applicability.wcl")
        .expect("test document parses")
        .schema_errors()
}

fn schema_errors_with_libs(src: &str, libs: &[(&str, &str)]) -> Vec<EvalError> {
    let mut registry = Registry::new();
    for (name, source) in libs {
        registry.register((*name).to_string(), (*source).to_string());
    }
    let loader = registry.loader(disk_loader());
    Document::open_at_with_loader(
        src,
        "decorator-applicability.wcl",
        None,
        &Environment::new(),
        loader,
    )
    .expect("test document and libraries parse")
    .schema_errors()
}

#[test]
fn decorator_in_an_excluded_position_is_an_error_on_its_name() {
    let src = r#"
@decorator("dev")
@applies_to(on = [:type])
type Dev {}

@block("vm") type Vm {}
@document type Root { @children("vm") vms: list<Vm> }

@dev
vm {}
"#;

    let errors = schema_errors(src);
    let error = errors
        .iter()
        .find(|error| {
            error
                .to_string()
                .contains("not applicable in the 'block' position")
        })
        .unwrap_or_else(|| panic!("expected an applicability error, got {errors:#?}"));
    let EvalError::SchemaViolation { span, .. } = error else {
        panic!("expected a schema violation, got {error:?}");
    };
    let use_site = src.rfind("@dev").expect("decorator use site");
    assert_eq!(span.offset(), use_site + 1);
    assert_eq!(span.len(), "dev".len());
}

#[test]
fn decorator_on_an_excluded_block_kind_is_an_error() {
    let src = r#"
@decorator("dev")
@applies_to(on = [:block], kinds = ["vm"])
type Dev {}

@block("vm") type Vm {}
@block("container") type Container {}
@document type Root { @children("container") containers: list<Container> }

@dev
container {}
"#;

    let errors = schema_errors(src);
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("not applicable to block kind 'container'")),
        "expected a kind applicability error, got {errors:#?}"
    );
}

#[test]
fn kinds_without_the_block_position_is_an_error_at_the_declaration() {
    let src = r#"
@decorator("dev")
@applies_to(on = [:type], kinds = ["vm"])
type Dev {}
@block("vm") type Vm {}
"#;

    let errors = schema_errors(src);
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("'kinds' requires the 'block' position")),
        "expected an invalid applicability declaration, got {errors:#?}"
    );
}

#[test]
fn an_unknown_applicable_kind_is_reported_at_the_declaration() {
    let src = r#"
@decorator("dev")
@applies_to(on = [:block], kinds = ["vmm"])
type Dev {}
"#;

    let errors = schema_errors(src);
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("@applies_to names unknown block kind 'vmm'")),
        "expected an unknown-kind declaration error, got {errors:#?}"
    );
}

#[test]
fn an_unknown_declared_kind_does_not_blame_valid_use_sites() {
    let src = r#"
@decorator("dev")
@applies_to(on = [:block], kinds = ["vmm"])
type Dev {}
@block("vm") type Vm {}
@document type Root { @children("vm") vms: list<Vm> }
@dev
vm {}
"#;

    let errors = schema_errors(src);
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("@applies_to names unknown block kind 'vmm'")),
        "declaration must still be blamed: {errors:#?}"
    );
    assert!(
        !errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::DecoratorNotApplicable,
                ..
            }
        )),
        "an invalid declaration must not blame its use sites: {errors:#?}"
    );
}

#[test]
fn applies_to_on_a_type_that_declares_no_decorator_is_an_error() {
    let src = r#"
@applies_to(on = [:type])
type Ordinary {}
"#;

    let errors = schema_errors(src);
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("@applies_to is attached to no decorator schema")),
        "expected an unattached applicability error, got {errors:#?}"
    );
}

#[test]
fn applies_to_uses_a_synthesised_closed_set_of_positions() {
    let doc = Document::open("", "decorator-applicability.wcl").expect("empty document opens");
    let schema = doc
        .decorator_schema("applies_to")
        .expect("built-in @applies_to schema");
    let on = schema.field("on").expect("on slot");
    let TypeRef::List(position_type) = on.type_ref() else {
        panic!("on must be a list of the closed position type");
    };
    assert_eq!(position_type.to_string(), "DecoratorPosition");

    let positions = doc
        .symbol_set("DecoratorPosition")
        .expect("synthesised decorator-position symbol set");
    let actual: Vec<&str> = positions.symbols().map(|symbol| symbol.name()).collect();
    assert_eq!(
        actual,
        [
            "field",
            "fn",
            "block",
            "type",
            "interface",
            "type_field",
            "union",
            "variant",
            "symbol_set",
            "symbol",
            "connection",
        ]
    );
}

#[test]
fn applies_to_rejects_a_name_outside_the_position_set() {
    let src = r#"
@decorator("dev")
@applies_to(on = [:blok])
type Dev {}
"#;

    let errors = schema_errors(src);
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("unknown decorator position 'blok'")),
        "expected a closed-position-set error, got {errors:#?}"
    );
}

#[test]
fn an_unknown_position_does_not_blame_decorator_use_sites() {
    let src = r#"
@decorator("dev")
@applies_to(on = [:blok])
type Dev {}
@dev type Target {}
"#;
    let errors = schema_errors(src);
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("unknown decorator position 'blok'")),
        "declaration must still be blamed: {errors:#?}"
    );
    assert!(
        !errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::DecoratorNotApplicable,
                ..
            }
        )),
        "an invalid position declaration must not blame use sites: {errors:#?}"
    );
}

#[test]
fn an_at_most_once_decorator_repeated_on_one_node_is_an_error_on_the_repeat() {
    let src = r#"
@decorator("once") type Once {}
@once
@once
type Target {}
"#;

    let errors = schema_errors(src);
    let error = errors
        .iter()
        .find(|error| error.to_string().contains("may appear at most once"))
        .unwrap_or_else(|| panic!("expected a cardinality error, got {errors:#?}"));
    let EvalError::SchemaViolation { span, .. } = error else {
        panic!("expected a schema violation, got {error:?}");
    };
    let repeat = src.rfind("@once").expect("repeat use site");
    assert_eq!(span.offset(), repeat + 1);
    assert_eq!(span.len(), "once".len());
}

#[test]
fn decorator_schema_exposes_repeatable_as_an_optional_false_bool() {
    let doc = Document::open("", "decorator-applicability.wcl").expect("empty document opens");
    let schema = doc
        .decorator_schema("decorator")
        .expect("built-in @decorator schema");
    let repeatable = schema.field("repeatable").expect("repeatable slot");
    assert_eq!(repeatable.type_ref(), &TypeRef::Builtin(BuiltinType::Bool));
    assert!(repeatable.optional());
    assert_eq!(repeatable.default_value(), Some(Value::Bool(false)));
}

#[test]
fn applicability_is_enforced_in_all_eleven_decorator_positions() {
    let cases = [
        ("field", "@dev\nvalue = 1"),
        ("fn", "@dev\nfn work() -> i64 1"),
        ("block", "@dev\nthing {}"),
        ("type", "@dev\ntype Target {}"),
        ("interface", "@dev\ninterface Target {}"),
        ("type_field", "type Target { @dev value: utf8 }"),
        ("union", "@dev\nunion Target { Unit none }"),
        ("variant", "union Target { @dev Unit none }"),
        ("symbol_set", "@dev\nsymbol_set Target { value }"),
        ("symbol", "symbol_set Target { @dev value }"),
        (
            "connection",
            "type Node {}\nsymbol_set K { edge }\n@dev\nconnection Edge: Node -> Node : K",
        ),
    ];

    for (position, use_site) in cases {
        let allowed_elsewhere = if position == "type" { "block" } else { "type" };
        let src = format!(
            "@decorator(\"dev\")\n\
             @applies_to(on = [:{allowed_elsewhere}])\n\
             type Dev {{}}\n\
             {use_site}\n"
        );
        let errors = schema_errors(&src);
        assert!(
            errors.iter().any(|error| error
                .to_string()
                .contains(&format!("not applicable in the '{position}' position"))),
            "expected an applicability error for {position}, got {errors:#?}\nsource:\n{src}"
        );
    }
}

#[test]
fn a_decorator_without_applies_to_is_legal_everywhere() {
    let src = r#"
@decorator("free") type Free {}
@free type OnType {}
@free symbol_set OnSet { @free value }
@free freeform {}
"#;
    let errors = schema_errors(src);
    assert!(
        !errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::DecoratorNotApplicable,
                ..
            }
        )),
        "unrestricted decorators must have no applicability errors: {errors:#?}"
    );
}

#[test]
fn decorator_uses_in_lazy_block_imports_are_checked() {
    let src = r#"
@decorator("dev")
@applies_to(on = [:type])
type Dev {}
@block("box") @schemaless type Box {}
@document type Root { @children("box") boxes: list<Box> }
box { import <fragment.wcl> }
"#;
    let errors = schema_errors_with_libs(src, &[("fragment.wcl", "@dev\nvalue = 1")]);
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("not applicable in the 'field' position")),
        "lazy block imports must be traversed: {errors:#?}"
    );
}

#[test]
fn a_repeatable_decorator_may_repeat_on_one_node() {
    let src = r#"
@decorator("many", repeatable = true) type Many {}
@many
@many
type Target {}
"#;
    let errors = schema_errors(src);
    assert!(
        !errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::DecoratorCardinality,
                ..
            }
        )),
        "repeatable decorator must be accepted: {errors:#?}"
    );
}

#[test]
fn qualified_and_bare_spellings_count_as_the_same_decorator() {
    let library = r#"
namespace lib
@decorator("mark") type Mark {}
"#;
    let src = r#"
import <lib.wcl>
@mark
@lib.mark
type Target {}
"#;
    let errors = schema_errors_with_libs(src, &[("lib.wcl", library)]);
    assert!(
        errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::DecoratorCardinality,
                ..
            }
        )),
        "both spellings must count toward one declaration: {errors:#?}"
    );
}

#[test]
fn an_imported_block_kind_resolves_in_an_applicability_declaration() {
    let library = r#"
namespace infra
@block("vm") type Vm {}
"#;
    let src = r#"
import <infra.wcl>
@decorator("dev")
@applies_to(on = [:block], kinds = ["vm"])
type Dev {}
"#;
    let errors = schema_errors_with_libs(src, &[("infra.wcl", library)]);
    assert!(
        !errors.iter().any(|error| error
            .to_string()
            .contains("@applies_to names unknown block kind")),
        "imported kind must resolve at the declaration: {errors:#?}"
    );
}

const DERIVED_KIND_SCHEMA: &str = r#"
@document("d") type D {
  @children("widget") widgets: list<Widget>
  @children("screen") screens: list<Screen>
}
@block("widget") @declares_kind(name = 0, params = "params", body = "body")
type Widget {
  @inline(0) name: identifier
  @children("param") params: list<Param>
  @child("body") body: WidgetBody
}
@block("param") type Param {
  @inline(0) name: identifier
  default: utf8?
}
@block("body") @schemaless type WidgetBody {}
@block("screen") type Screen { @inline(0) name: identifier }

widget metric_card {
  param label
  body {}
}
screen dash {}
"#;

#[test]
fn a_derived_kind_resolves_in_an_applicability_declaration() {
    let src = format!(
        "@decorator(\"dev\")\n\
         @applies_to(on = [:block], kinds = [\"metric_card\"])\n\
         type Dev {{}}\n{DERIVED_KIND_SCHEMA}"
    );
    let errors = schema_errors(&src);
    assert!(
        !errors.iter().any(|error| error
            .to_string()
            .contains("@applies_to names unknown block kind 'metric_card'")),
        "derived kind must resolve at the declaration: {errors:#?}"
    );
}

#[test]
fn applicability_to_a_kind_declarer_does_not_propagate_to_derived_kinds() {
    let src = format!(
        "@decorator(\"dev\")\n\
         @applies_to(on = [:block], kinds = [\"widget\"])\n\
         type Dev {{}}\n{DERIVED_KIND_SCHEMA}\n\
         @dev\nmetric_card {{ label = \"CPU\" }}\n"
    );
    let errors = schema_errors(&src);
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("not applicable to block kind 'metric_card'")),
        "derived kinds must not inherit declarer applicability: {errors:#?}"
    );
}

#[test]
fn the_unit_decorator_is_declared_repeatable_and_ten_occurrences_are_accepted() {
    let src = r#"
@unit("u0", 1)
@unit("u1", 2)
@unit("u2", 3)
@unit("u3", 4)
@unit("u4", 5)
@unit("u5", 6)
@unit("u6", 7)
@unit("u7", 8)
@unit("u8", 9)
@unit("u9", 10)
type Units = i64
"#;
    let doc = Document::open(src, "decorator-applicability.wcl").expect("unit aliases parse");
    let schema = doc
        .decorator_schema("unit")
        .expect("built-in @unit decorator schema");
    let declaration = schema
        .decorators()
        .find(|decorator| decorator.full_name() == "decorator")
        .expect("unit is declared by @decorator");
    assert_eq!(
        declaration.named_arg("repeatable"),
        Some(Ok(Value::Bool(true)))
    );
    let errors = doc.schema_errors();
    assert!(
        !errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                kind: wcl_lang::SchemaViolationKind::DecoratorCardinality,
                ..
            }
        )),
        "ten unit declarations must remain valid: {errors:#?}"
    );
}
