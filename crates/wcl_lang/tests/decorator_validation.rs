use wcl_lang::{Document, Environment, EvalError, Registry, SchemaViolationKind, disk_loader};

#[test]
fn undeclared_decorator_on_block_is_spanned_at_its_name() {
    let source = r#"@document type Root { @children("item") items: list<Item> }
@block("item") type Item {}
@missing
item {}
"#;
    let doc = Document::open(source, "decorators.wcl").expect("document opens");

    let error = doc
        .schema_errors()
        .into_iter()
        .find(|error| error.to_string().contains("decorator 'missing'"))
        .expect("undeclared decorator is rejected");
    let EvalError::SchemaViolation { span, .. } = error else {
        panic!("undeclared decorator must be a schema violation");
    };

    assert_eq!(span.offset(), source.find("missing").unwrap());
    assert_eq!(span.len(), "missing".len());
}

#[test]
fn undeclared_decorator_on_document_field_is_a_schema_violation() {
    let source = r#"@document type Root { title: utf8 }
@missing
title = "Hello"
"#;
    let doc = Document::open(source, "decorators.wcl").expect("document opens");

    assert!(doc.schema_errors().iter().any(|error| matches!(
        error,
        EvalError::SchemaViolation {
            kind: SchemaViolationKind::UndeclaredDecorator,
            detail: Some(name),
            span,
            ..
        } if name == "missing"
            && span.offset() == source.find("missing").unwrap()
            && span.len() == "missing".len()
    )));
}

#[test]
fn locally_and_imported_declared_decorators_are_accepted() {
    let mut registry = Registry::new();
    registry.register(
        "decorators.wcl",
        "namespace library\n@decorator(\"imported\") type Imported {}\n",
    );
    let source = r#"import <decorators.wcl>
@decorator("local") type Local {}
@document type Root { @children("item") items: list<Item> }
@block("item") type Item {}
@local
@library.imported
item {}
"#;
    let doc = Document::open_at_with_loader(
        source,
        "decorators.wcl",
        None,
        &Environment::new(),
        registry.loader(disk_loader()),
    )
    .expect("document opens");

    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn annotations_only_schemaless_exempts_decorators_but_not_fields() {
    let source = r#"@document type Root { @children("item") items: list<Item> }
@block("item") type Item { title: utf8 }
@schemaless(annotations = true)
@missing
item {
  title = "Hello"
  rogue = true
}
"#;
    let doc = Document::open(source, "decorators.wcl").expect("document opens");
    let errors = doc.schema_errors();

    assert!(
        !errors.iter().any(|error| matches!(
            error,
            EvalError::SchemaViolation {
                kind: SchemaViolationKind::UndeclaredDecorator,
                ..
            }
        )),
        "decorators should be exempt: {errors:?}"
    );
    assert!(errors.iter().any(|error| matches!(
        error,
        EvalError::SchemaViolation {
            kind: SchemaViolationKind::UnknownField,
            message,
            ..
        } if message.contains("rogue")
    )));
}

#[test]
fn undeclared_decorator_on_nested_data_field_is_rejected() {
    let source = r#"@document type Root { @children("item") items: list<Item> }
@block("item") type Item { title: utf8 }
item {
  @missing
  title = "Hello"
}
"#;
    let doc = Document::open(source, "decorators.wcl").expect("document opens");

    assert!(doc.schema_errors().iter().any(|error| matches!(
        error,
        EvalError::SchemaViolation {
            kind: SchemaViolationKind::UndeclaredDecorator,
            detail: Some(name),
            ..
        } if name == "missing"
    )));
}

#[test]
fn bare_schemaless_exempts_the_node_and_its_decorators() {
    let source = r#"@document type Root { @children("item") items: list<Item> }
@block("item") type Item {}
@schemaless
@missing
item {
  rogue = true
  unknown_child {}
}
"#;
    let doc = Document::open(source, "decorators.wcl").expect("document opens");

    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
}

#[test]
fn annotations_only_schemaless_on_field_keeps_membership_checking() {
    let source = r#"@document type Root { title: utf8 }
@schemaless(annotations = true)
@missing
rogue = "Hello"
"#;
    let doc = Document::open(source, "decorators.wcl").expect("document opens");
    let errors = doc.schema_errors();

    assert!(!errors.iter().any(|error| matches!(
        error,
        EvalError::SchemaViolation {
            kind: SchemaViolationKind::UndeclaredDecorator,
            ..
        }
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        EvalError::SchemaViolation {
            kind: SchemaViolationKind::UnknownField,
            message,
            ..
        } if message.contains("rogue")
    )));
}
