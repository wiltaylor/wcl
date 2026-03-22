use wcl_core::ast::*;
use wcl_core::diagnostic::DiagnosticBag;
use wcl_core::span::{FileId, Span};
use wcl_core::trivia::Trivia;
use wcl_eval::value::Value;
use wcl_schema::types::{check_type, type_name};
use wcl_schema::{IdRegistry, SchemaRegistry, SymbolSetRegistry};

// ── Span / AST helpers ───────────────────────────────────────────────────────

fn sp() -> Span {
    Span::new(FileId(0), 0, 1)
}

fn make_ident(name: &str) -> Ident {
    Ident {
        name: name.to_string(),
        span: sp(),
    }
}

fn make_string_lit(s: &str) -> StringLit {
    StringLit {
        parts: vec![StringPart::Literal(s.to_string())],
        span: sp(),
    }
}

fn make_schema_field(name: &str, type_expr: TypeExpr) -> SchemaField {
    SchemaField {
        decorators_before: vec![],
        name: make_ident(name),
        type_expr,
        decorators_after: vec![],
        trivia: Trivia::default(),
        span: sp(),
    }
}

fn make_schema(name: &str, fields: Vec<SchemaField>) -> Schema {
    Schema {
        decorators: vec![],
        name: make_string_lit(name),
        fields,
        variants: vec![],
        trivia: Trivia::default(),
        span: sp(),
    }
}

fn make_attribute(name: &str, value: Expr) -> Attribute {
    Attribute {
        decorators: vec![],
        name: make_ident(name),
        value,
        trivia: Trivia::default(),
        span: sp(),
    }
}

fn make_block(kind: &str, id: Option<&str>, partial: bool, body: Vec<BodyItem>) -> Block {
    Block {
        decorators: vec![],
        partial,
        kind: make_ident(kind),
        inline_id: id.map(|v| {
            InlineId::Literal(IdentifierLit {
                value: v.to_string(),
                span: sp(),
            })
        }),
        inline_args: vec![],
        body,
        text_content: None,
        trivia: Trivia::default(),
        span: Span::new(FileId(0), 0, 10),
    }
}

fn make_doc(items: Vec<DocItem>) -> Document {
    Document {
        items,
        trivia: Trivia::default(),
        span: sp(),
    }
}

// ── Type checking tests ───────────────────────────────────────────────────────

#[test]
fn type_check_string_value_against_string_type() {
    assert!(check_type(
        &Value::String("hello".into()),
        &TypeExpr::String(sp())
    ));
}

#[test]
fn type_check_int_value_against_int_type() {
    assert!(check_type(&Value::Int(42), &TypeExpr::Int(sp())));
}

#[test]
fn type_check_float_value_against_float_type() {
    assert!(check_type(&Value::Float(3.14), &TypeExpr::Float(sp())));
}

#[test]
fn type_check_bool_value_against_bool_type() {
    assert!(check_type(&Value::Bool(true), &TypeExpr::Bool(sp())));
    assert!(check_type(&Value::Bool(false), &TypeExpr::Bool(sp())));
}

#[test]
fn type_check_null_value_against_null_type() {
    assert!(check_type(&Value::Null, &TypeExpr::Null(sp())));
}

#[test]
fn type_check_any_accepts_every_value() {
    assert!(check_type(&Value::String("x".into()), &TypeExpr::Any(sp())));
    assert!(check_type(&Value::Int(1), &TypeExpr::Any(sp())));
    assert!(check_type(&Value::Float(1.0), &TypeExpr::Any(sp())));
    assert!(check_type(&Value::Bool(false), &TypeExpr::Any(sp())));
    assert!(check_type(&Value::Null, &TypeExpr::Any(sp())));
}

#[test]
fn type_check_list_of_ints() {
    let list_type = TypeExpr::List(Box::new(TypeExpr::Int(sp())), sp());
    assert!(check_type(
        &Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        &list_type
    ));
}

#[test]
fn type_check_union_string_or_null() {
    let union_type = TypeExpr::Union(vec![TypeExpr::String(sp()), TypeExpr::Null(sp())], sp());
    assert!(check_type(&Value::String("ok".into()), &union_type));
    assert!(check_type(&Value::Null, &union_type));
}

// ── Type mismatch detection ───────────────────────────────────────────────────

#[test]
fn type_mismatch_string_vs_int() {
    assert!(!check_type(
        &Value::String("x".into()),
        &TypeExpr::Int(sp())
    ));
}

#[test]
fn type_mismatch_int_vs_string() {
    assert!(!check_type(&Value::Int(1), &TypeExpr::String(sp())));
}

#[test]
fn type_mismatch_float_vs_int() {
    assert!(!check_type(&Value::Float(1.0), &TypeExpr::Int(sp())));
}

#[test]
fn type_mismatch_int_vs_float() {
    assert!(!check_type(&Value::Int(1), &TypeExpr::Float(sp())));
}

#[test]
fn type_mismatch_bool_vs_null() {
    assert!(!check_type(&Value::Bool(true), &TypeExpr::Null(sp())));
}

#[test]
fn type_mismatch_null_vs_bool() {
    assert!(!check_type(&Value::Null, &TypeExpr::Bool(sp())));
}

#[test]
fn type_mismatch_string_not_in_union_of_int_and_bool() {
    let union_type = TypeExpr::Union(vec![TypeExpr::Int(sp()), TypeExpr::Bool(sp())], sp());
    assert!(!check_type(&Value::String("no".into()), &union_type));
}

#[test]
fn type_mismatch_heterogeneous_list_vs_typed_list() {
    let list_type = TypeExpr::List(Box::new(TypeExpr::Int(sp())), sp());
    // Mixed int + string fails
    assert!(!check_type(
        &Value::List(vec![Value::Int(1), Value::String("oops".into())]),
        &list_type
    ));
}

// ── type_name helper ─────────────────────────────────────────────────────────

#[test]
fn type_name_returns_correct_strings() {
    assert_eq!(type_name(&TypeExpr::String(sp())), "string");
    assert_eq!(type_name(&TypeExpr::Int(sp())), "int");
    assert_eq!(type_name(&TypeExpr::Float(sp())), "float");
    assert_eq!(type_name(&TypeExpr::Bool(sp())), "bool");
    assert_eq!(type_name(&TypeExpr::Null(sp())), "null");
    assert_eq!(type_name(&TypeExpr::Any(sp())), "any");
}

#[test]
fn type_name_compound_list() {
    let t = TypeExpr::List(Box::new(TypeExpr::String(sp())), sp());
    assert_eq!(type_name(&t), "list(string)");
}

#[test]
fn type_name_union() {
    let t = TypeExpr::Union(vec![TypeExpr::Int(sp()), TypeExpr::Null(sp())], sp());
    assert_eq!(type_name(&t), "union(int, null)");
}

// ── SchemaRegistry: missing required field detection ─────────────────────────

#[test]
fn schema_validation_missing_required_field_emits_error() {
    // Define schema "service" with required field "name: string"
    let schema = make_schema(
        "service",
        vec![make_schema_field("name", TypeExpr::String(sp()))],
    );
    // Block "service" with no attributes — missing required "name"
    let block = make_block("service", Some("alpha"), false, vec![]);
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(diags.has_errors(), "expected a missing-field error");
}

#[test]
fn schema_validation_present_required_field_no_error() {
    let schema = make_schema(
        "service",
        vec![make_schema_field("name", TypeExpr::String(sp()))],
    );
    let block = make_block(
        "service",
        Some("alpha"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "name",
            Expr::StringLit(make_string_lit("my-service")),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(
        !diags.has_errors(),
        "unexpected errors: {:?}",
        diags.diagnostics()
    );
}

#[test]
fn schema_validation_unknown_attribute_in_closed_schema_emits_error() {
    let schema = make_schema(
        "service",
        vec![make_schema_field("name", TypeExpr::String(sp()))],
    );
    // Block has an unknown attribute "extra" not in the schema
    let block = make_block(
        "service",
        Some("alpha"),
        false,
        vec![
            BodyItem::Attribute(make_attribute(
                "name",
                Expr::StringLit(make_string_lit("x")),
            )),
            BodyItem::Attribute(make_attribute("extra", Expr::IntLit(1, sp()))),
        ],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(diags.has_errors(), "expected an unknown-attribute error");
}

#[test]
fn schema_validation_open_schema_allows_unknown_attributes() {
    let open_dec = Decorator {
        name: make_ident("open"),
        args: vec![],
        span: sp(),
    };
    let mut schema = make_schema(
        "service",
        vec![make_schema_field("name", TypeExpr::String(sp()))],
    );
    schema.decorators.push(open_dec);

    let block = make_block(
        "service",
        Some("alpha"),
        false,
        vec![
            BodyItem::Attribute(make_attribute(
                "name",
                Expr::StringLit(make_string_lit("x")),
            )),
            BodyItem::Attribute(make_attribute("anything", Expr::BoolLit(true, sp()))),
        ],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(
        !diags.has_errors(),
        "open schema should allow unknown attributes"
    );
}

// ── SchemaRegistry: duplicate schema name detection ──────────────────────────

#[test]
fn schema_registry_duplicate_schema_name_emits_error() {
    let schema1 = make_schema("service", vec![]);
    let schema2 = make_schema("service", vec![]);
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema1)),
        DocItem::Body(BodyItem::Schema(schema2)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);

    assert!(
        diags.has_errors(),
        "duplicate schema name should produce an error"
    );
    assert_eq!(diags.error_count(), 1);
}

#[test]
fn schema_registry_unique_schema_names_no_error() {
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(make_schema("service", vec![]))),
        DocItem::Body(BodyItem::Schema(make_schema("endpoint", vec![]))),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);

    assert!(!diags.has_errors());
    assert!(reg.schemas.contains_key("service"));
    assert!(reg.schemas.contains_key("endpoint"));
}

// ── IdRegistry: duplicate ID detection ───────────────────────────────────────

#[test]
fn id_registry_unique_ids_no_error() {
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Block(make_block(
            "service",
            Some("alpha"),
            false,
            vec![],
        ))),
        DocItem::Body(BodyItem::Block(make_block(
            "service",
            Some("beta"),
            false,
            vec![],
        ))),
    ]);

    let mut reg = IdRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.check_document(&doc, &mut diags);

    assert!(!diags.has_errors());
}

#[test]
fn id_registry_duplicate_non_partial_blocks_emit_error() {
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Block(make_block(
            "service",
            Some("alpha"),
            false,
            vec![],
        ))),
        DocItem::Body(BodyItem::Block(make_block(
            "service",
            Some("alpha"),
            false,
            vec![],
        ))),
    ]);

    let mut reg = IdRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.check_document(&doc, &mut diags);

    assert!(diags.has_errors(), "duplicate IDs should produce an error");
    let has_e030 = diags
        .diagnostics()
        .iter()
        .any(|d| d.code.as_deref() == Some("E030") && d.message.contains("alpha"));
    assert!(has_e030, "expected an E030 duplicate-id diagnostic");
}

// ── IdRegistry: partial blocks exemption ─────────────────────────────────────

#[test]
fn id_registry_two_partial_blocks_same_id_allowed() {
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Block(make_block(
            "service",
            Some("alpha"),
            true,
            vec![],
        ))),
        DocItem::Body(BodyItem::Block(make_block(
            "service",
            Some("alpha"),
            true,
            vec![],
        ))),
    ]);

    let mut reg = IdRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.check_document(&doc, &mut diags);

    assert!(
        !diags.has_errors(),
        "two partial blocks with the same ID should be allowed (they merge)"
    );
}

#[test]
fn id_registry_partial_and_non_partial_same_id_is_error() {
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Block(make_block(
            "service",
            Some("alpha"),
            false,
            vec![],
        ))),
        DocItem::Body(BodyItem::Block(make_block(
            "service",
            Some("alpha"),
            true,
            vec![],
        ))),
    ]);

    let mut reg = IdRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.check_document(&doc, &mut diags);

    assert!(
        diags.has_errors(),
        "mixing partial and non-partial with same ID should error"
    );
}

#[test]
fn id_registry_blocks_without_ids_never_conflict() {
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Block(make_block("service", None, false, vec![]))),
        DocItem::Body(BodyItem::Block(make_block("service", None, false, vec![]))),
        DocItem::Body(BodyItem::Block(make_block("service", None, false, vec![]))),
    ]);

    let mut reg = IdRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.check_document(&doc, &mut diags);

    assert!(
        !diags.has_errors(),
        "blocks without IDs should never conflict"
    );
}

#[test]
fn id_registry_same_id_in_different_scopes_no_error() {
    // Two top-level blocks each containing a nested block with the same local ID.
    let nested_alpha = make_block("port", Some("http"), false, vec![]);
    let nested_beta = make_block("port", Some("http"), false, vec![]);

    let svc_a = make_block(
        "service",
        Some("alpha"),
        false,
        vec![BodyItem::Block(nested_alpha)],
    );
    let svc_b = make_block(
        "service",
        Some("beta"),
        false,
        vec![BodyItem::Block(nested_beta)],
    );

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Block(svc_a)),
        DocItem::Body(BodyItem::Block(svc_b)),
    ]);

    let mut reg = IdRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.check_document(&doc, &mut diags);

    assert!(
        !diags.has_errors(),
        "same ID in different scopes should not conflict"
    );
}

// ── C2: Type checking in validate ────────────────────────────────────────────

#[test]
fn type_mismatch_string_value_for_int_field_emits_e071() {
    let schema = make_schema(
        "service",
        vec![make_schema_field("port", TypeExpr::Int(sp()))],
    );
    let block = make_block(
        "service",
        Some("web"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "port",
            Expr::StringLit(make_string_lit("not-a-number")),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(diags.has_errors(), "expected a type mismatch error");
    let has_e071 = diags
        .diagnostics()
        .iter()
        .any(|d| d.code.as_deref() == Some("E071"));
    assert!(has_e071, "expected E071 type mismatch diagnostic");
}

#[test]
fn type_match_int_value_for_int_field_no_error() {
    let schema = make_schema(
        "service",
        vec![make_schema_field("port", TypeExpr::Int(sp()))],
    );
    let block = make_block(
        "service",
        Some("web"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "port",
            Expr::IntLit(8080, sp()),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(
        !diags.has_errors(),
        "int value for int field should pass: {:?}",
        diags.diagnostics()
    );
}

// ── C3: @validate constraint enforcement ─────────────────────────────────────

#[test]
fn validate_min_below_minimum_emits_e073() {
    // Build a schema field with @validate(min=10)
    let validate_dec = Decorator {
        name: make_ident("validate"),
        args: vec![DecoratorArg::Named(
            make_ident("min"),
            Expr::IntLit(10, sp()),
        )],
        span: sp(),
    };
    let mut field = make_schema_field("port", TypeExpr::Int(sp()));
    field.decorators_after.push(validate_dec);

    let schema = make_schema("service", vec![field]);
    let block = make_block(
        "service",
        Some("web"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "port",
            Expr::IntLit(5, sp()),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(diags.has_errors(), "expected min constraint violation");
    let has_e073 = diags
        .diagnostics()
        .iter()
        .any(|d| d.code.as_deref() == Some("E073"));
    assert!(has_e073, "expected E073 constraint diagnostic");
}

#[test]
fn validate_pattern_mismatch_emits_e074() {
    let validate_dec = Decorator {
        name: make_ident("validate"),
        args: vec![DecoratorArg::Named(
            make_ident("pattern"),
            Expr::StringLit(make_string_lit("^[a-z]+$")),
        )],
        span: sp(),
    };
    let mut field = make_schema_field("name", TypeExpr::String(sp()));
    field.decorators_after.push(validate_dec);

    let schema = make_schema("service", vec![field]);
    let block = make_block(
        "service",
        Some("web"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "name",
            Expr::StringLit(make_string_lit("UPPERCASE")),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(diags.has_errors(), "expected pattern constraint violation");
    let has_e074 = diags
        .diagnostics()
        .iter()
        .any(|d| d.code.as_deref() == Some("E074"));
    assert!(has_e074, "expected E074 pattern mismatch diagnostic");
}

#[test]
fn validate_one_of_not_in_set_emits_e075() {
    let validate_dec = Decorator {
        name: make_ident("validate"),
        args: vec![DecoratorArg::Named(
            make_ident("one_of"),
            Expr::List(
                vec![
                    Expr::StringLit(make_string_lit("tcp")),
                    Expr::StringLit(make_string_lit("udp")),
                ],
                sp(),
            ),
        )],
        span: sp(),
    };
    let mut field = make_schema_field("protocol", TypeExpr::String(sp()));
    field.decorators_after.push(validate_dec);

    let schema = make_schema("service", vec![field]);
    let block = make_block(
        "service",
        Some("web"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "protocol",
            Expr::StringLit(make_string_lit("http")),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(diags.has_errors(), "expected one_of constraint violation");
    let has_e075 = diags
        .diagnostics()
        .iter()
        .any(|d| d.code.as_deref() == Some("E075"));
    assert!(has_e075, "expected E075 one_of mismatch diagnostic");
}

// ── M4: @ref target validation ───────────────────────────────────────────────

#[test]
fn ref_to_nonexistent_block_emits_e076() {
    // Schema: endpoint has a field "service_ref" with @ref("service")
    let ref_dec = Decorator {
        name: make_ident("ref"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "service",
        )))],
        span: sp(),
    };
    let mut field = make_schema_field("service_ref", TypeExpr::String(sp()));
    field.decorators_after.push(ref_dec);

    let schema = make_schema("endpoint", vec![field]);

    // An endpoint block referencing a nonexistent service
    let block = make_block(
        "endpoint",
        Some("api"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "service_ref",
            Expr::StringLit(make_string_lit("nonexistent-svc")),
        ))],
    );
    // No service blocks exist
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(diags.has_errors(), "expected ref validation error");
    let has_e076 = diags
        .diagnostics()
        .iter()
        .any(|d| d.code.as_deref() == Some("E076"));
    assert!(has_e076, "expected E076 ref target not found diagnostic");
}

#[test]
fn ref_to_existing_block_no_error() {
    let ref_dec = Decorator {
        name: make_ident("ref"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "service",
        )))],
        span: sp(),
    };
    let mut field = make_schema_field("service_ref", TypeExpr::String(sp()));
    field.decorators_after.push(ref_dec);

    let schema = make_schema("endpoint", vec![field]);

    // A service block exists with id "web"
    let svc_block = make_block("service", Some("web"), false, vec![]);
    // An endpoint references "web"
    let ep_block = make_block(
        "endpoint",
        Some("api"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "service_ref",
            Expr::StringLit(make_string_lit("web")),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(svc_block)),
        DocItem::Body(BodyItem::Block(ep_block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(
        !diags.has_errors(),
        "ref to existing block should not error: {:?}",
        diags.diagnostics()
    );
}

// ── M5: @id_pattern enforcement ──────────────────────────────────────────────

#[test]
fn id_pattern_mismatch_emits_e077() {
    // Schema: service has a field with @id_pattern("^[a-z][a-z0-9-]*$")
    let id_pat_dec = Decorator {
        name: make_ident("id_pattern"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "^[a-z][a-z0-9-]*$",
        )))],
        span: sp(),
    };
    let mut field = make_schema_field("name", TypeExpr::String(sp()));
    field.decorators_after.push(id_pat_dec);

    let schema = make_schema("service", vec![field]);

    // Block with an ID that does NOT match the pattern
    let block = make_block(
        "service",
        Some("123-bad-id"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "name",
            Expr::StringLit(make_string_lit("test")),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(diags.has_errors(), "expected id_pattern violation");
    let has_e077 = diags
        .diagnostics()
        .iter()
        .any(|d| d.code.as_deref() == Some("E077"));
    assert!(has_e077, "expected E077 id_pattern mismatch diagnostic");
}

#[test]
fn id_pattern_match_no_error() {
    let id_pat_dec = Decorator {
        name: make_ident("id_pattern"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "^[a-z][a-z0-9-]*$",
        )))],
        span: sp(),
    };
    let mut field = make_schema_field("name", TypeExpr::String(sp()));
    field.decorators_after.push(id_pat_dec);

    let schema = make_schema("service", vec![field]);

    let block = make_block(
        "service",
        Some("good-id"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "name",
            Expr::StringLit(make_string_lit("test")),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(
        !diags.has_errors(),
        "valid ID should not trigger error: {:?}",
        diags.diagnostics()
    );
}

// ── Feature 1: @child cardinality ─────────────────────────────────────────────

fn make_child_decorator(kind: &str, min: Option<i64>, max: Option<i64>) -> Decorator {
    let mut args = vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
        kind,
    )))];
    if let Some(m) = min {
        args.push(DecoratorArg::Named(
            make_ident("min"),
            Expr::IntLit(m, sp()),
        ));
    }
    if let Some(m) = max {
        args.push(DecoratorArg::Named(
            make_ident("max"),
            Expr::IntLit(m, sp()),
        ));
    }
    Decorator {
        name: make_ident("child"),
        args,
        span: sp(),
    }
}

fn make_schema_with_decorators(
    name: &str,
    fields: Vec<SchemaField>,
    decorators: Vec<Decorator>,
) -> Schema {
    Schema {
        decorators,
        name: make_string_lit(name),
        fields,
        variants: vec![],
        trivia: Trivia::default(),
        span: sp(),
    }
}

#[test]
fn child_min_too_few_emits_e097() {
    let schema = make_schema_with_decorators(
        "server",
        vec![make_schema_field("host", TypeExpr::String(sp()))],
        vec![make_child_decorator("endpoint", Some(1), None)],
    );
    let block = make_block(
        "server",
        Some("web"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "host",
            Expr::StringLit(make_string_lit("localhost")),
        ))],
    );
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    let e097: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E097"))
        .collect();
    assert_eq!(e097.len(), 1, "expected E097: {:?}", diags.diagnostics());
    assert!(e097[0].message.contains("endpoint"));
}

#[test]
fn child_max_exceeded_emits_e098() {
    let schema = make_schema_with_decorators(
        "server",
        vec![make_schema_field("host", TypeExpr::String(sp()))],
        vec![make_child_decorator("config", None, Some(1))],
    );
    let mut block = make_block(
        "server",
        Some("web"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "host",
            Expr::StringLit(make_string_lit("localhost")),
        ))],
    );
    block.body.push(BodyItem::Block(make_block(
        "config",
        Some("a"),
        false,
        vec![],
    )));
    block.body.push(BodyItem::Block(make_block(
        "config",
        Some("b"),
        false,
        vec![],
    )));
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    let e098: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E098"))
        .collect();
    assert_eq!(e098.len(), 1, "expected E098: {:?}", diags.diagnostics());
    assert!(e098[0].message.contains("config"));
}

#[test]
fn child_within_bounds_no_error() {
    let schema = make_schema_with_decorators(
        "server",
        vec![make_schema_field("host", TypeExpr::String(sp()))],
        vec![make_child_decorator("endpoint", Some(1), Some(3))],
    );
    let mut block = make_block(
        "server",
        Some("web"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "host",
            Expr::StringLit(make_string_lit("localhost")),
        ))],
    );
    block.body.push(BodyItem::Block(make_block(
        "endpoint",
        Some("ep1"),
        false,
        vec![],
    )));
    block.body.push(BodyItem::Block(make_block(
        "endpoint",
        Some("ep2"),
        false,
        vec![],
    )));
    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    let cardinality_errors: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E097") || d.code.as_deref() == Some("E098"))
        .collect();
    assert!(
        cardinality_errors.is_empty(),
        "unexpected cardinality errors: {:?}",
        cardinality_errors
    );
}

#[test]
fn child_adds_to_allowed_children() {
    // @child("endpoint") should implicitly add "endpoint" to allowed children
    let schema = make_schema_with_decorators(
        "server",
        vec![],
        vec![make_child_decorator("endpoint", None, None)],
    );
    // An endpoint child should be allowed
    let mut block = make_block("server", Some("web"), false, vec![]);
    block.body.push(BodyItem::Block(make_block(
        "endpoint",
        Some("ep1"),
        false,
        vec![],
    )));
    // A "middleware" child should be rejected (allowed_children = ["endpoint"])
    block.body.push(BodyItem::Block(make_block(
        "middleware",
        Some("mw1"),
        false,
        vec![],
    )));

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    let e095: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E095"))
        .collect();
    assert_eq!(e095.len(), 1);
    assert!(e095[0].message.contains("middleware"));
}

// ── Feature 2: Self-nesting depth limit ───────────────────────────────────────

fn make_child_decorator_with_depth(kind: &str, max_depth: i64) -> Decorator {
    Decorator {
        name: make_ident("child"),
        args: vec![
            DecoratorArg::Positional(Expr::StringLit(make_string_lit(kind))),
            DecoratorArg::Named(make_ident("max_depth"), Expr::IntLit(max_depth, sp())),
        ],
        span: sp(),
    }
}

#[test]
fn self_nesting_exceeds_depth_emits_e099() {
    let schema = make_schema_with_decorators(
        "menu",
        vec![make_schema_field("label", TypeExpr::String(sp()))],
        vec![make_child_decorator_with_depth("menu", 2)],
    );
    // menu -> menu -> menu (depth 3 exceeds max_depth 2)
    let deep = make_block(
        "menu",
        Some("deep"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "label",
            Expr::StringLit(make_string_lit("Recent")),
        ))],
    );
    let sub = make_block(
        "menu",
        Some("sub"),
        false,
        vec![
            BodyItem::Attribute(make_attribute(
                "label",
                Expr::StringLit(make_string_lit("Open")),
            )),
            BodyItem::Block(deep),
        ],
    );
    let top = make_block(
        "menu",
        Some("top"),
        false,
        vec![
            BodyItem::Attribute(make_attribute(
                "label",
                Expr::StringLit(make_string_lit("File")),
            )),
            BodyItem::Block(sub),
        ],
    );

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(top)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    let e099: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E099"))
        .collect();
    assert_eq!(e099.len(), 1, "expected E099: {:?}", diags.diagnostics());
    assert!(e099[0].message.contains("menu"));
}

#[test]
fn self_nesting_within_depth_no_error() {
    let schema = make_schema_with_decorators(
        "menu",
        vec![make_schema_field("label", TypeExpr::String(sp()))],
        vec![make_child_decorator_with_depth("menu", 2)],
    );
    // menu -> menu (depth 2 is within max_depth 2)
    let sub = make_block(
        "menu",
        Some("sub"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "label",
            Expr::StringLit(make_string_lit("Open")),
        ))],
    );
    let top = make_block(
        "menu",
        Some("top"),
        false,
        vec![
            BodyItem::Attribute(make_attribute(
                "label",
                Expr::StringLit(make_string_lit("File")),
            )),
            BodyItem::Block(sub),
        ],
    );

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(top)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    let e099: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E099"))
        .collect();
    assert!(
        e099.is_empty(),
        "unexpected E099 errors: {:?}",
        diags.diagnostics()
    );
}

// ── Feature 3: Union field types (verify existing) ────────────────────────────

#[test]
fn union_type_accepts_all_variants() {
    let union_type = TypeExpr::Union(
        vec![
            TypeExpr::String(sp()),
            TypeExpr::Int(sp()),
            TypeExpr::Bool(sp()),
        ],
        sp(),
    );
    let schema = make_schema("config", vec![make_schema_field("value", union_type)]);

    // String value
    let block_s = make_block(
        "config",
        Some("a"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "value",
            Expr::StringLit(make_string_lit("hello")),
        ))],
    );
    // Int value
    let block_i = make_block(
        "config",
        Some("b"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "value",
            Expr::IntLit(42, sp()),
        ))],
    );
    // Bool value
    let block_b = make_block(
        "config",
        Some("c"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "value",
            Expr::BoolLit(true, sp()),
        ))],
    );

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block_s)),
        DocItem::Body(BodyItem::Block(block_i)),
        DocItem::Body(BodyItem::Block(block_b)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(
        !diags.has_errors(),
        "union should accept all three types: {:?}",
        diags.diagnostics()
    );
}

#[test]
fn union_type_rejects_wrong_type() {
    let union_type = TypeExpr::Union(vec![TypeExpr::Int(sp()), TypeExpr::Bool(sp())], sp());
    let schema = make_schema("config", vec![make_schema_field("value", union_type)]);

    let block = make_block(
        "config",
        Some("a"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "value",
            Expr::StringLit(make_string_lit("not allowed")),
        ))],
    );

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    let e071: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E071"))
        .collect();
    assert_eq!(e071.len(), 1);
}

// ── Feature 4: Tagged variant schemas ─────────────────────────────────────────

#[test]
fn tagged_variant_validates_matching_variant_fields() {
    let tagged_dec = Decorator {
        name: make_ident("tagged"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "style",
        )))],
        span: sp(),
    };
    let mut schema = make_schema(
        "api",
        vec![make_schema_field("style", TypeExpr::String(sp())), {
            let mut f = make_schema_field("version", TypeExpr::String(sp()));
            f.decorators_after.push(Decorator {
                name: make_ident("optional"),
                args: vec![],
                span: sp(),
            });
            f
        }],
    );
    schema.decorators.push(tagged_dec);
    schema.variants.push(SchemaVariant {
        decorators: vec![],
        tag_value: make_string_lit("rest"),
        fields: vec![make_schema_field("base_path", TypeExpr::String(sp()))],
        trivia: Trivia::default(),
        span: sp(),
    });
    schema.variants.push(SchemaVariant {
        decorators: vec![],
        tag_value: make_string_lit("graphql"),
        fields: vec![{
            let mut f = make_schema_field("schema_path", TypeExpr::String(sp()));
            f.decorators_after.push(Decorator {
                name: make_ident("optional"),
                args: vec![],
                span: sp(),
            });
            f
        }],
        trivia: Trivia::default(),
        span: sp(),
    });

    // REST API block — missing required base_path
    let rest_block = make_block(
        "api",
        Some("rest-api"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "style",
            Expr::StringLit(make_string_lit("rest")),
        ))],
    );

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(rest_block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    // Should get E070 for missing base_path (variant required field)
    let e070: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E070") && d.message.contains("base_path"))
        .collect();
    assert_eq!(
        e070.len(),
        1,
        "expected missing base_path: {:?}",
        diags.diagnostics()
    );
}

#[test]
fn tagged_variant_passes_when_fields_present() {
    let tagged_dec = Decorator {
        name: make_ident("tagged"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "style",
        )))],
        span: sp(),
    };
    let mut schema = make_schema(
        "api",
        vec![make_schema_field("style", TypeExpr::String(sp()))],
    );
    schema.decorators.push(tagged_dec);
    schema.variants.push(SchemaVariant {
        decorators: vec![],
        tag_value: make_string_lit("rest"),
        fields: vec![make_schema_field("base_path", TypeExpr::String(sp()))],
        trivia: Trivia::default(),
        span: sp(),
    });

    let rest_block = make_block(
        "api",
        Some("rest-api"),
        false,
        vec![
            BodyItem::Attribute(make_attribute(
                "style",
                Expr::StringLit(make_string_lit("rest")),
            )),
            BodyItem::Attribute(make_attribute(
                "base_path",
                Expr::StringLit(make_string_lit("/api/v1")),
            )),
        ],
    );

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(rest_block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(
        !diags.has_errors(),
        "valid variant block should not error: {:?}",
        diags.diagnostics()
    );
}

#[test]
fn tagged_variant_graphql_optional_passes() {
    let tagged_dec = Decorator {
        name: make_ident("tagged"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "style",
        )))],
        span: sp(),
    };
    let mut schema = make_schema(
        "api",
        vec![make_schema_field("style", TypeExpr::String(sp()))],
    );
    schema.decorators.push(tagged_dec);
    schema.variants.push(SchemaVariant {
        decorators: vec![],
        tag_value: make_string_lit("graphql"),
        fields: vec![{
            let mut f = make_schema_field("schema_path", TypeExpr::String(sp()));
            f.decorators_after.push(Decorator {
                name: make_ident("optional"),
                args: vec![],
                span: sp(),
            });
            f
        }],
        trivia: Trivia::default(),
        span: sp(),
    });

    // graphql block with no schema_path — should pass since optional
    let gql_block = make_block(
        "api",
        Some("gql-api"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "style",
            Expr::StringLit(make_string_lit("graphql")),
        ))],
    );

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(gql_block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    assert!(
        !diags.has_errors(),
        "optional variant field should not error: {:?}",
        diags.diagnostics()
    );
}

#[test]
fn tagged_variant_no_match_only_common_fields() {
    let tagged_dec = Decorator {
        name: make_ident("tagged"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "style",
        )))],
        span: sp(),
    };
    let mut schema = make_schema(
        "api",
        vec![make_schema_field("style", TypeExpr::String(sp()))],
    );
    schema.decorators.push(tagged_dec);
    schema.variants.push(SchemaVariant {
        decorators: vec![],
        tag_value: make_string_lit("rest"),
        fields: vec![make_schema_field("base_path", TypeExpr::String(sp()))],
        trivia: Trivia::default(),
        span: sp(),
    });

    // Block with style="unknown" — no variant matches, only common validated
    let block = make_block(
        "api",
        Some("custom"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "style",
            Expr::StringLit(make_string_lit("unknown")),
        ))],
    );

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    // No error — unknown variant value is valid, base_path not required
    assert!(
        !diags.has_errors(),
        "unknown variant should not error: {:?}",
        diags.diagnostics()
    );
}

#[test]
fn doc_decorator_round_trips_through_resolution() {
    // Schema-level @doc
    let doc_dec = Decorator {
        name: make_ident("doc"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "A service definition.",
        )))],
        span: sp(),
    };
    // Field-level @doc
    let mut field = make_schema_field("name", TypeExpr::String(sp()));
    field.decorators_after.push(Decorator {
        name: make_ident("doc"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "The service name.",
        )))],
        span: sp(),
    });

    let schema = make_schema_with_decorators("service", vec![field], vec![doc_dec]);
    let doc = make_doc(vec![DocItem::Body(BodyItem::Schema(schema))]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);

    assert!(!diags.has_errors());
    let s = &reg.schemas["service"];
    assert_eq!(s.doc.as_deref(), Some("A service definition."));
    assert_eq!(s.fields[0].doc.as_deref(), Some("The service name."));
}

#[test]
fn tagged_variant_children_override() {
    let tagged_dec = Decorator {
        name: make_ident("tagged"),
        args: vec![DecoratorArg::Positional(Expr::StringLit(make_string_lit(
            "style",
        )))],
        span: sp(),
    };
    let children_dec = Decorator {
        name: make_ident("children"),
        args: vec![DecoratorArg::Positional(Expr::List(
            vec![Expr::StringLit(make_string_lit("resource"))],
            sp(),
        ))],
        span: sp(),
    };
    let mut schema = make_schema(
        "api",
        vec![make_schema_field("style", TypeExpr::String(sp()))],
    );
    schema.decorators.push(tagged_dec);
    schema.variants.push(SchemaVariant {
        decorators: vec![children_dec],
        tag_value: make_string_lit("rest"),
        fields: vec![],
        trivia: Trivia::default(),
        span: sp(),
    });

    // REST API block with disallowed child
    let mut block = make_block(
        "api",
        Some("rest-api"),
        false,
        vec![BodyItem::Attribute(make_attribute(
            "style",
            Expr::StringLit(make_string_lit("rest")),
        ))],
    );
    block.body.push(BodyItem::Block(make_block(
        "forbidden",
        None,
        false,
        vec![],
    )));

    let doc = make_doc(vec![
        DocItem::Body(BodyItem::Schema(schema)),
        DocItem::Body(BodyItem::Block(block)),
    ]);

    let mut reg = SchemaRegistry::new();
    let mut diags = DiagnosticBag::new();
    reg.collect(&doc, &mut diags);
    reg.validate(
        &doc,
        &indexmap::IndexMap::new(),
        &SymbolSetRegistry::new(),
        &mut diags,
    );

    let e095: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E095"))
        .collect();
    assert_eq!(
        e095.len(),
        1,
        "expected E095 for forbidden child: {:?}",
        diags.diagnostics()
    );
}

// ── Symbol type, symbol sets, and @symbol_set tests ──────────────────────────

#[test]
fn symbol_type_check_accepts_symbol_value() {
    assert!(check_type(
        &Value::Symbol("GET".into()),
        &TypeExpr::Symbol(sp())
    ));
}

#[test]
fn symbol_type_check_rejects_string() {
    assert!(!check_type(
        &Value::String("GET".into()),
        &TypeExpr::Symbol(sp())
    ));
}

#[test]
fn symbol_type_name() {
    assert_eq!(type_name(&TypeExpr::Symbol(sp())), "symbol");
}

#[test]
fn symbol_value_equality() {
    assert_eq!(Value::Symbol("GET".into()), Value::Symbol("GET".into()));
    assert_ne!(Value::Symbol("GET".into()), Value::Symbol("POST".into()));
    // Symbol != String even with same text
    assert_ne!(Value::Symbol("GET".into()), Value::String("GET".into()));
}

#[test]
fn symbol_set_collection_and_validation() {
    // Test symbol_set registry directly
    let decl = SymbolSetDecl {
        name: make_ident("http_method"),
        members: vec![
            SymbolMember {
                name: "GET".into(),
                value: None,
                span: sp(),
            },
            SymbolMember {
                name: "POST".into(),
                value: None,
                span: sp(),
            },
            SymbolMember {
                name: "PUT".into(),
                value: None,
                span: sp(),
            },
        ],
        trivia: Trivia::default(),
        span: sp(),
    };
    let doc = Document {
        items: vec![DocItem::Body(BodyItem::SymbolSetDecl(decl))],
        trivia: Trivia::default(),
        span: sp(),
    };

    let mut diags = DiagnosticBag::new();
    let mut reg = SymbolSetRegistry::new();
    reg.collect(&doc, &mut diags);
    assert!(reg.set_exists("http_method"));
    assert!(reg.contains("http_method", "GET"));
    assert!(reg.contains("http_method", "POST"));
    assert!(!reg.contains("http_method", "PATCH"));
}

#[test]
fn symbol_set_duplicate_name_e102() {
    let decl1 = SymbolSetDecl {
        name: make_ident("colors"),
        members: vec![SymbolMember {
            name: "red".into(),
            value: None,
            span: sp(),
        }],
        trivia: Trivia::default(),
        span: sp(),
    };
    let decl2 = SymbolSetDecl {
        name: make_ident("colors"),
        members: vec![SymbolMember {
            name: "blue".into(),
            value: None,
            span: sp(),
        }],
        trivia: Trivia::default(),
        span: sp(),
    };
    let doc = Document {
        items: vec![
            DocItem::Body(BodyItem::SymbolSetDecl(decl1)),
            DocItem::Body(BodyItem::SymbolSetDecl(decl2)),
        ],
        trivia: Trivia::default(),
        span: sp(),
    };

    let mut diags = DiagnosticBag::new();
    let mut reg = SymbolSetRegistry::new();
    reg.collect(&doc, &mut diags);
    let e102: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E102"))
        .collect();
    assert_eq!(e102.len(), 1, "expected E102 for duplicate symbol_set name");
}

#[test]
fn symbol_set_duplicate_member_e103() {
    let decl = SymbolSetDecl {
        name: make_ident("colors"),
        members: vec![
            SymbolMember {
                name: "red".into(),
                value: None,
                span: sp(),
            },
            SymbolMember {
                name: "green".into(),
                value: None,
                span: sp(),
            },
            SymbolMember {
                name: "red".into(),
                value: None,
                span: sp(),
            },
        ],
        trivia: Trivia::default(),
        span: sp(),
    };
    let doc = Document {
        items: vec![DocItem::Body(BodyItem::SymbolSetDecl(decl))],
        trivia: Trivia::default(),
        span: sp(),
    };

    let mut diags = DiagnosticBag::new();
    let mut reg = SymbolSetRegistry::new();
    reg.collect(&doc, &mut diags);
    let e103: Vec<_> = diags
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_deref() == Some("E103"))
        .collect();
    assert_eq!(e103.len(), 1, "expected E103 for duplicate symbol");
}

#[test]
fn symbol_set_all_accepts_any() {
    let reg = SymbolSetRegistry::new();
    assert!(reg.contains("all", "anything"));
    assert!(reg.set_exists("all"));
}

#[test]
fn symbol_set_value_mapping() {
    let decl = SymbolSetDecl {
        name: make_ident("multiplicity"),
        members: vec![
            SymbolMember {
                name: "zero_or_one".into(),
                value: Some(make_string_lit("0..1")),
                span: sp(),
            },
            SymbolMember {
                name: "one".into(),
                value: Some(make_string_lit("1")),
                span: sp(),
            },
            SymbolMember {
                name: "many".into(),
                value: None,
                span: sp(),
            },
        ],
        trivia: Trivia::default(),
        span: sp(),
    };
    let doc = Document {
        items: vec![DocItem::Body(BodyItem::SymbolSetDecl(decl))],
        trivia: Trivia::default(),
        span: sp(),
    };

    let mut diags = DiagnosticBag::new();
    let mut reg = SymbolSetRegistry::new();
    reg.collect(&doc, &mut diags);
    assert_eq!(reg.serialize_symbol("multiplicity", "zero_or_one"), "0..1");
    assert_eq!(reg.serialize_symbol("multiplicity", "one"), "1");
    assert_eq!(reg.serialize_symbol("multiplicity", "many"), "many");
}
