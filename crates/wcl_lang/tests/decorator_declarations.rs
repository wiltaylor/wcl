use wcl_lang::{Document, Environment, EvalError, Registry, SchemaViolationKind, disk_loader};

fn undeclared_decorator_errors(source: &str) -> Vec<EvalError> {
    let document = Document::open(source, "decorator-declarations.wcl")
        .expect("decorator position fixture parses");
    document
        .schema_errors()
        .into_iter()
        .filter(|error| {
            matches!(
                error,
                EvalError::SchemaViolation {
                    detail: Some(detail),
                    ..
                } if detail == "missing"
            )
        })
        .collect()
}

fn assert_missing_is_name_spanned(source: &str) {
    let errors = undeclared_decorator_errors(source);
    assert_eq!(errors.len(), 1, "schema errors: {errors:?}");
    let EvalError::SchemaViolation { span, .. } = &errors[0] else {
        unreachable!("filtered to schema violations")
    };
    assert_eq!(
        &source[span.offset()..span.offset() + span.len()],
        "missing"
    );
}

fn sources_for_every_position(exemption: &str) -> Vec<(&'static str, String)> {
    let decorators = format!("{exemption}\n@missing");
    vec![
        (
            "document field",
            format!("@document type Root {{ value: i64 }}\n{decorators}\nvalue = 1\n"),
        ),
        ("fn item", format!("{decorators}\nfn answer() -> i64 42\n")),
        (
            "block",
            format!(
                "@document type Root {{ @child(\"thing\") thing: Thing }}\n@block(\"thing\") type Thing {{}}\n{decorators}\nthing {{}}\n"
            ),
        ),
        ("type", format!("{decorators}\ntype Example {{}}\n")),
        (
            "interface",
            format!("{decorators}\ninterface Example {{}}\n"),
        ),
        (
            "type field",
            format!("type Example {{\n{decorators}\nvalue: i64\n}}\n"),
        ),
        (
            "union",
            format!("{decorators}\nunion Example {{ Value none }}\n"),
        ),
        (
            "union variant",
            format!("union Example {{\n{decorators}\nValue none\n}}\n"),
        ),
        (
            "symbol set",
            format!("{decorators}\nsymbol_set Example {{ value }}\n"),
        ),
        (
            "symbol entry",
            format!("symbol_set Example {{\n{decorators}\nvalue\n}}\n"),
        ),
        (
            "connection",
            format!(
                "type Endpoint {{}}\nsymbol_set Kind {{ link }}\n{decorators}\nconnection Edge: Endpoint -> Endpoint : Kind\n"
            ),
        ),
    ]
}

#[test]
fn undeclared_decorator_on_block_is_spanned_on_its_name() {
    let source = r#"
@document
type Root {
  @child("thing") thing: Thing
}

@block("thing")
type Thing {}

@missing
thing {}
"#;

    assert_missing_is_name_spanned(source);
}

#[test]
fn undeclared_decorator_on_document_field_is_spanned_on_its_name() {
    assert_missing_is_name_spanned(
        r#"
@document
type Root { @min(100) value: i64 }

@missing
value = 1
"#,
    );
}

#[test]
fn undeclared_decorator_on_fn_item_is_spanned_on_its_name() {
    assert_missing_is_name_spanned("@missing\nfn answer() -> i64 42\n");
}

#[test]
fn undeclared_decorator_on_type_is_spanned_on_its_name() {
    assert_missing_is_name_spanned("@missing\ntype Example {}\n");
}

#[test]
fn undeclared_decorator_on_interface_is_spanned_on_its_name() {
    assert_missing_is_name_spanned("@missing\ninterface Example {}\n");
}

#[test]
fn undeclared_decorator_on_type_field_is_spanned_on_its_name() {
    assert_missing_is_name_spanned("type Example {\n  @missing\n  value: i64\n}\n");
}

#[test]
fn undeclared_decorator_on_union_is_spanned_on_its_name() {
    assert_missing_is_name_spanned("@missing\nunion Example { Value none }\n");
}

#[test]
fn undeclared_decorator_on_union_variant_is_spanned_on_its_name() {
    assert_missing_is_name_spanned("union Example {\n  @missing\n  Value none\n}\n");
}

#[test]
fn undeclared_decorator_on_symbol_set_is_spanned_on_its_name() {
    assert_missing_is_name_spanned("@missing\nsymbol_set Example { value }\n");
}

#[test]
fn undeclared_decorator_on_symbol_entry_is_spanned_on_its_name() {
    assert_missing_is_name_spanned("symbol_set Example {\n  @missing\n  value\n}\n");
}

#[test]
fn undeclared_decorator_on_connection_is_spanned_on_its_name() {
    assert_missing_is_name_spanned(
        "type Endpoint {}\nsymbol_set Kind { link }\n@missing\nconnection Edge: Endpoint -> Endpoint : Kind\n",
    );
}

#[test]
fn bare_schemaless_exempts_annotations_in_every_position() {
    for (position, source) in sources_for_every_position("@schemaless") {
        assert!(
            undeclared_decorator_errors(&source).is_empty(),
            "{position} should exempt annotations"
        );
    }
}

#[test]
fn annotation_only_schemaless_exempts_annotations_in_every_position() {
    for (position, source) in sources_for_every_position("@schemaless(annotations = true)") {
        assert!(
            undeclared_decorator_errors(&source).is_empty(),
            "{position} should exempt annotations"
        );
    }
}

#[test]
fn annotation_only_schemaless_keeps_block_contents_strict() {
    let source = r#"
@document
type Root { @child("thing") thing: Thing }

@block("thing")
type Thing { known: i64 }

@schemaless(annotations = true)
@missing
thing { unknown = 1 }
"#;
    let document = Document::open(source, "decorator-declarations.wcl").expect("fixture parses");
    let errors = document.schema_errors();

    assert!(undeclared_decorator_errors(source).is_empty());
    assert!(
        errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                detail: Some(detail),
                ..
            } if detail == "unknown"
        )),
        "narrow annotation exemption must retain field validation: {errors:?}"
    );
}

#[test]
fn migrated_builtin_decorators_have_registered_schemas() {
    let document = Document::open("", "decorator-declarations.wcl").expect("empty document opens");
    for name in [
        "doc",
        "min",
        "max",
        "non_empty",
        "ref",
        "by_ref",
        "dynamic",
        "unit",
    ] {
        assert!(
            document.decorator_schema(name).is_some(),
            "@{name} should have a built-in decorator schema"
        );
    }
}

#[test]
fn locally_declared_decorator_is_accepted() {
    let source = "@decorator(\"known\") type Known {}\n@known type Example {}\n";
    let document = Document::open(source, "decorator-declarations.wcl").expect("fixture parses");
    assert!(
        document.schema_errors().is_empty(),
        "declared decorator should validate: {:?}",
        document.schema_errors()
    );
}

#[test]
fn imported_decorator_declaration_is_accepted() {
    let mut registry = Registry::new();
    registry.register("decorators.wcl", "@decorator(\"known\") type Known {}\n");
    let loader = registry.loader(disk_loader());
    let document = Document::open_at_with_loader(
        "import <decorators.wcl>\n@known type Example {}\n",
        "decorator-declarations.wcl",
        None,
        &Environment::new(),
        loader,
    )
    .expect("fixture with imported decorator parses");
    assert!(
        document.schema_errors().is_empty(),
        "imported decorator should validate: {:?}",
        document.schema_errors()
    );
}

#[test]
fn constraint_names_are_interpreted_through_the_decorator_registry() {
    let source = r#"
@document
type Root { @min(100) value: i64 }

@decorator("min")
type Metadata { value: i64 }

value = 1
"#;
    let document = Document::open(source, "decorator-declarations.wcl").expect("fixture parses");
    let errors = document.schema_errors();

    assert!(
        !errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                kind: SchemaViolationKind::ConstraintViolation,
                ..
            }
        )),
        "a local metadata decorator shadowing the built-in name must not enforce its constraint: {errors:?}"
    );
}
