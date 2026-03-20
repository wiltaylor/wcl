use wcl_core::ast::*;
use wcl_core::diagnostic::DiagnosticBag;
use wcl_core::span::{FileId, Span};
use wcl_core::trivia::Trivia;
use wcl_eval::value::Value;
use wcl_schema::types::{check_type, type_name};
use wcl_schema::{IdRegistry, SchemaRegistry};

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
        labels: vec![],
        body,
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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

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
    reg.validate(&doc, &indexmap::IndexMap::new(), &mut diags);

    assert!(
        !diags.has_errors(),
        "valid ID should not trigger error: {:?}",
        diags.diagnostics()
    );
}
