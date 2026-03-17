use std::collections::HashMap;
use wcl_core::ast::*;
use wcl_core::diagnostic::DiagnosticBag;
use wcl_core::span::Span;
use wcl_eval::value::Value;

/// A resolved schema definition
#[derive(Debug, Clone)]
pub struct ResolvedSchema {
    pub name: String,
    pub fields: Vec<ResolvedField>,
    pub open: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ResolvedField {
    pub name: String,
    pub type_expr: TypeExpr,
    pub required: bool,
    pub default: Option<Value>,
    pub validate: Option<ValidateConstraints>,
    pub ref_target: Option<String>,
    pub id_pattern: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct ValidateConstraints {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub pattern: Option<String>,
    pub one_of: Option<Vec<Value>>,
    pub custom_msg: Option<String>,
}

/// Registry of schemas extracted from the document
#[derive(Debug, Default)]
pub struct SchemaRegistry {
    pub schemas: HashMap<String, ResolvedSchema>,
}

impl SchemaRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Extract and register schemas from the document AST
    pub fn collect(&mut self, doc: &Document, diagnostics: &mut DiagnosticBag) {
        for item in &doc.items {
            if let DocItem::Body(BodyItem::Schema(schema)) = item {
                let name = string_lit_to_string(&schema.name);
                if self.schemas.contains_key(&name) {
                    diagnostics.error_with_code(
                        format!("duplicate schema name '{}'", name),
                        schema.span,
                        "E001",
                    );
                    continue;
                }
                let resolved = self.resolve_schema(schema, diagnostics);
                self.schemas.insert(name, resolved);
            }
        }
    }

    fn resolve_schema(&self, schema: &Schema, _diagnostics: &mut DiagnosticBag) -> ResolvedSchema {
        let open = schema.decorators.iter().any(|d| d.name.name == "open");
        let mut fields = Vec::new();

        for field in &schema.fields {
            let required = !has_decorator(&field.decorators_before, "optional")
                && !has_decorator(&field.decorators_after, "optional");
            let default = get_decorator_value(&field.decorators_before, "default")
                .or_else(|| get_decorator_value(&field.decorators_after, "default"));
            let validate = get_validate_constraints(&field.decorators_before)
                .or_else(|| get_validate_constraints(&field.decorators_after));
            let ref_target = get_decorator_string_arg(&field.decorators_before, "ref")
                .or_else(|| get_decorator_string_arg(&field.decorators_after, "ref"));
            let id_pattern = get_decorator_string_arg(&field.decorators_before, "id_pattern")
                .or_else(|| get_decorator_string_arg(&field.decorators_after, "id_pattern"));

            fields.push(ResolvedField {
                name: field.name.name.clone(),
                type_expr: field.type_expr.clone(),
                required,
                default,
                validate,
                ref_target,
                id_pattern,
                span: field.span,
            });
        }

        ResolvedSchema {
            name: string_lit_to_string(&schema.name),
            fields,
            open,
            span: schema.span,
        }
    }

    /// Validate all blocks in the document against their schemas
    pub fn validate(
        &self,
        doc: &Document,
        diagnostics: &mut DiagnosticBag,
    ) {
        self.validate_items(&doc.items, diagnostics);
    }

    fn validate_items(&self, items: &[DocItem], diagnostics: &mut DiagnosticBag) {
        for item in items {
            if let DocItem::Body(BodyItem::Block(block)) = item {
                self.validate_block(block, diagnostics);
            }
        }
    }

    fn validate_block(&self, block: &Block, diagnostics: &mut DiagnosticBag) {
        // Check if there's a schema for this block type
        if let Some(schema) = self.schemas.get(&block.kind.name) {
            // Check required fields
            for field in &schema.fields {
                if field.required {
                    let has_attr = block.body.iter().any(|item| {
                        matches!(item, BodyItem::Attribute(attr) if attr.name.name == field.name)
                    });
                    let is_id_field = field.name == "id";
                    let has_id = block.inline_id.is_some();

                    if !has_attr && (!is_id_field || !has_id) {
                        diagnostics.error_with_code(
                            format!(
                                "missing required field '{}' in block '{}'",
                                field.name, block.kind.name
                            ),
                            block.span,
                            "E070",
                        );
                    }
                }
            }

            // Check for unknown attributes (closed schema)
            if !schema.open {
                for item in &block.body {
                    if let BodyItem::Attribute(attr) = item {
                        let known = schema.fields.iter().any(|f| f.name == attr.name.name);
                        if !known {
                            diagnostics.error_with_code(
                                format!(
                                    "unknown attribute '{}' in schema '{}'",
                                    attr.name.name, schema.name
                                ),
                                attr.span,
                                "E072",
                            );
                        }
                    }
                }
            }
        }

        // Recursively validate nested blocks
        for item in &block.body {
            if let BodyItem::Block(child) = item {
                self.validate_block(child, diagnostics);
            }
        }
    }
}

// ── Helper functions (pub(crate) so decorator.rs can reuse them) ──────────────

pub(crate) fn string_lit_to_string(s: &StringLit) -> String {
    s.parts
        .iter()
        .map(|p| match p {
            StringPart::Literal(s) => s.clone(),
            StringPart::Interpolation(_) => "?".to_string(),
        })
        .collect()
}

pub(crate) fn has_decorator(decorators: &[Decorator], name: &str) -> bool {
    decorators.iter().any(|d| d.name.name == name)
}

pub(crate) fn get_decorator_string_arg(decorators: &[Decorator], name: &str) -> Option<String> {
    decorators
        .iter()
        .find(|d| d.name.name == name)
        .and_then(|d| {
            d.args.first().and_then(|arg| match arg {
                DecoratorArg::Positional(Expr::StringLit(s)) => Some(string_lit_to_string(s)),
                _ => None,
            })
        })
}

pub(crate) fn get_decorator_value(decorators: &[Decorator], name: &str) -> Option<Value> {
    decorators
        .iter()
        .find(|d| d.name.name == name)
        .and_then(|d| {
            d.args.first().and_then(|arg| match arg {
                DecoratorArg::Positional(expr) => expr_to_value(expr),
                _ => None,
            })
        })
}

pub(crate) fn expr_to_value(expr: &Expr) -> Option<Value> {
    match expr {
        Expr::IntLit(i, _) => Some(Value::Int(*i)),
        Expr::FloatLit(f, _) => Some(Value::Float(*f)),
        Expr::BoolLit(b, _) => Some(Value::Bool(*b)),
        Expr::NullLit(_) => Some(Value::Null),
        Expr::StringLit(s) => Some(Value::String(string_lit_to_string(s))),
        Expr::List(items, _) => {
            let vals: Option<Vec<_>> = items.iter().map(expr_to_value).collect();
            vals.map(Value::List)
        }
        _ => None,
    }
}

pub(crate) fn get_validate_constraints(decorators: &[Decorator]) -> Option<ValidateConstraints> {
    decorators
        .iter()
        .find(|d| d.name.name == "validate")
        .map(|d| {
            let mut constraints = ValidateConstraints::default();
            for arg in &d.args {
                if let DecoratorArg::Named(name, expr) = arg {
                        let val = expr_to_value(expr);
                        match name.name.as_str() {
                            "min" => {
                                constraints.min = val.and_then(|v| match v {
                                    Value::Int(i) => Some(i as f64),
                                    Value::Float(f) => Some(f),
                                    _ => None,
                                })
                            }
                            "max" => {
                                constraints.max = val.and_then(|v| match v {
                                    Value::Int(i) => Some(i as f64),
                                    Value::Float(f) => Some(f),
                                    _ => None,
                                })
                            }
                            "pattern" => {
                                constraints.pattern = val.and_then(|v| match v {
                                    Value::String(s) => Some(s),
                                    _ => None,
                                })
                            }
                            "one_of" => {
                                constraints.one_of = val.and_then(|v| match v {
                                    Value::List(items) => Some(items),
                                    _ => None,
                                })
                            }
                            "custom_msg" => {
                                constraints.custom_msg = val.and_then(|v| match v {
                                    Value::String(s) => Some(s),
                                    _ => None,
                                })
                            }
                            _ => {}
                        }
                }
            }
            constraints
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_core::span::{FileId, Span};
    use wcl_core::trivia::Trivia;

    fn dummy_span() -> Span {
        Span::new(FileId(0), 0, 1)
    }

    fn make_string_lit(s: &str) -> StringLit {
        StringLit {
            parts: vec![StringPart::Literal(s.to_string())],
            span: dummy_span(),
        }
    }

    fn make_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: dummy_span(),
        }
    }

    fn make_schema_field(name: &str, type_expr: TypeExpr) -> SchemaField {
        SchemaField {
            decorators_before: vec![],
            name: make_ident(name),
            type_expr,
            decorators_after: vec![],
            trivia: Trivia::default(),
            span: dummy_span(),
        }
    }

    fn make_schema(name: &str, fields: Vec<SchemaField>) -> Schema {
        Schema {
            decorators: vec![],
            name: make_string_lit(name),
            fields,
            trivia: Trivia::default(),
            span: dummy_span(),
        }
    }

    fn make_document(items: Vec<DocItem>) -> Document {
        Document {
            items,
            trivia: Trivia::default(),
            span: dummy_span(),
        }
    }

    #[test]
    fn collect_simple_schema() {
        let schema = make_schema(
            "service",
            vec![make_schema_field("name", TypeExpr::String(dummy_span()))],
        );
        let doc = make_document(vec![DocItem::Body(BodyItem::Schema(schema))]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);

        assert!(!diags.has_errors());
        assert!(reg.schemas.contains_key("service"));
        let s = &reg.schemas["service"];
        assert_eq!(s.fields.len(), 1);
        assert_eq!(s.fields[0].name, "name");
    }

    #[test]
    fn collect_duplicate_schema_errors() {
        let schema1 = make_schema("service", vec![]);
        let schema2 = make_schema("service", vec![]);
        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(schema1)),
            DocItem::Body(BodyItem::Schema(schema2)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);

        assert!(diags.has_errors());
        assert_eq!(diags.error_count(), 1);
    }

    #[test]
    fn collect_open_schema() {
        let open_dec = Decorator {
            name: make_ident("open"),
            args: vec![],
            span: dummy_span(),
        };
        let mut schema = make_schema("service", vec![]);
        schema.decorators.push(open_dec);
        let doc = make_document(vec![DocItem::Body(BodyItem::Schema(schema))]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);

        assert!(!diags.has_errors());
        assert!(reg.schemas["service"].open);
    }

    #[test]
    fn string_lit_to_string_works() {
        let s = StringLit {
            parts: vec![StringPart::Literal("hello".to_string())],
            span: dummy_span(),
        };
        assert_eq!(string_lit_to_string(&s), "hello");
    }

    #[test]
    fn has_decorator_finds_by_name() {
        let dec = Decorator {
            name: make_ident("optional"),
            args: vec![],
            span: dummy_span(),
        };
        assert!(has_decorator(&[dec.clone()], "optional"));
        assert!(!has_decorator(&[dec], "required"));
    }
}
