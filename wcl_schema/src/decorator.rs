use std::collections::HashMap;
use wcl_core::ast::*;
use wcl_core::diagnostic::DiagnosticBag;
use wcl_core::span::Span;
use wcl_eval::value::Value;

/// A resolved decorator schema
#[derive(Debug, Clone)]
pub struct ResolvedDecoratorSchema {
    pub name: String,
    pub targets: Vec<DecoratorTarget>,
    pub params: Vec<DecoratorParam>,
    pub constraints: Vec<Constraint>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct DecoratorParam {
    pub name: String,
    pub type_expr: TypeExpr,
    pub required: bool,
    pub default: Option<Value>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Constraint {
    AnyOf(Vec<String>),
    AllOf(Vec<String>),
    OneOf(Vec<String>),
    Requires(HashMap<String, Vec<String>>),
}

/// Registry of decorator schemas (includes built-ins)
#[derive(Debug)]
pub struct DecoratorSchemaRegistry {
    pub schemas: HashMap<String, ResolvedDecoratorSchema>,
}

impl DecoratorSchemaRegistry {
    pub fn new() -> Self {
        let mut reg = DecoratorSchemaRegistry {
            schemas: HashMap::new(),
        };
        reg.register_builtins();
        reg
    }

    fn register_builtins(&mut self) {
        // Section 15.5 built-in decorators
        self.insert(ResolvedDecoratorSchema {
            name: "optional".to_string(),
            targets: vec![DecoratorTarget::Schema],
            params: vec![],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "required".to_string(),
            targets: vec![DecoratorTarget::Schema],
            params: vec![],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "default".to_string(),
            targets: vec![DecoratorTarget::Schema],
            params: vec![DecoratorParam {
                name: "value".to_string(),
                type_expr: TypeExpr::Any(Span::dummy()),
                required: true,
                default: None,
                span: Span::dummy(),
            }],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "sensitive".to_string(),
            targets: vec![DecoratorTarget::Attribute],
            params: vec![DecoratorParam {
                name: "redact_in_logs".to_string(),
                type_expr: TypeExpr::Bool(Span::dummy()),
                required: false,
                default: Some(Value::Bool(true)),
                span: Span::dummy(),
            }],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "deprecated".to_string(),
            targets: vec![DecoratorTarget::Block, DecoratorTarget::Attribute],
            params: vec![
                DecoratorParam {
                    name: "message".to_string(),
                    type_expr: TypeExpr::String(Span::dummy()),
                    required: true,
                    default: None,
                    span: Span::dummy(),
                },
                DecoratorParam {
                    name: "since".to_string(),
                    type_expr: TypeExpr::String(Span::dummy()),
                    required: false,
                    default: None,
                    span: Span::dummy(),
                },
            ],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "validate".to_string(),
            targets: vec![DecoratorTarget::Attribute],
            params: vec![
                DecoratorParam {
                    name: "min".to_string(),
                    type_expr: TypeExpr::Float(Span::dummy()),
                    required: false,
                    default: None,
                    span: Span::dummy(),
                },
                DecoratorParam {
                    name: "max".to_string(),
                    type_expr: TypeExpr::Float(Span::dummy()),
                    required: false,
                    default: None,
                    span: Span::dummy(),
                },
                DecoratorParam {
                    name: "pattern".to_string(),
                    type_expr: TypeExpr::String(Span::dummy()),
                    required: false,
                    default: None,
                    span: Span::dummy(),
                },
                DecoratorParam {
                    name: "one_of".to_string(),
                    type_expr: TypeExpr::List(Box::new(TypeExpr::Any(Span::dummy())), Span::dummy()),
                    required: false,
                    default: None,
                    span: Span::dummy(),
                },
                DecoratorParam {
                    name: "custom_msg".to_string(),
                    type_expr: TypeExpr::String(Span::dummy()),
                    required: false,
                    default: None,
                    span: Span::dummy(),
                },
            ],
            constraints: vec![Constraint::AnyOf(vec![
                "min".to_string(),
                "max".to_string(),
                "pattern".to_string(),
                "one_of".to_string(),
            ])],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "doc".to_string(),
            targets: vec![
                DecoratorTarget::Block,
                DecoratorTarget::Attribute,
                DecoratorTarget::Table,
                DecoratorTarget::Schema,
            ],
            params: vec![DecoratorParam {
                name: "text".to_string(),
                type_expr: TypeExpr::String(Span::dummy()),
                required: true,
                default: None,
                span: Span::dummy(),
            }],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "allow".to_string(),
            targets: vec![DecoratorTarget::Attribute, DecoratorTarget::Block],
            params: vec![DecoratorParam {
                name: "rule".to_string(),
                type_expr: TypeExpr::String(Span::dummy()),
                required: true,
                default: None,
                span: Span::dummy(),
            }],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "id_pattern".to_string(),
            targets: vec![DecoratorTarget::Schema],
            params: vec![DecoratorParam {
                name: "pattern".to_string(),
                type_expr: TypeExpr::String(Span::dummy()),
                required: true,
                default: None,
                span: Span::dummy(),
            }],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "ref".to_string(),
            targets: vec![DecoratorTarget::Schema],
            params: vec![DecoratorParam {
                name: "schema".to_string(),
                type_expr: TypeExpr::String(Span::dummy()),
                required: true,
                default: None,
                span: Span::dummy(),
            }],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "open".to_string(),
            targets: vec![DecoratorTarget::Schema],
            params: vec![],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "warning".to_string(),
            targets: vec![DecoratorTarget::Block],
            params: vec![],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "partial_requires".to_string(),
            targets: vec![DecoratorTarget::Block],
            params: vec![DecoratorParam {
                name: "fields".to_string(),
                type_expr: TypeExpr::List(
                    Box::new(TypeExpr::String(Span::dummy())),
                    Span::dummy(),
                ),
                required: true,
                default: None,
                span: Span::dummy(),
            }],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "merge_order".to_string(),
            targets: vec![DecoratorTarget::Block],
            params: vec![DecoratorParam {
                name: "order".to_string(),
                type_expr: TypeExpr::Int(Span::dummy()),
                required: true,
                default: None,
                span: Span::dummy(),
            }],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "example".to_string(),
            targets: vec![DecoratorTarget::Schema],
            params: vec![],
            constraints: vec![],
            span: Span::dummy(),
        });
        self.insert(ResolvedDecoratorSchema {
            name: "table_index".to_string(),
            targets: vec![DecoratorTarget::Table],
            params: vec![
                DecoratorParam {
                    name: "columns".to_string(),
                    type_expr: TypeExpr::List(
                        Box::new(TypeExpr::String(Span::dummy())),
                        Span::dummy(),
                    ),
                    required: true,
                    default: None,
                    span: Span::dummy(),
                },
                DecoratorParam {
                    name: "unique".to_string(),
                    type_expr: TypeExpr::Bool(Span::dummy()),
                    required: false,
                    default: Some(Value::Bool(false)),
                    span: Span::dummy(),
                },
            ],
            constraints: vec![],
            span: Span::dummy(),
        });
    }

    fn insert(&mut self, schema: ResolvedDecoratorSchema) {
        self.schemas.insert(schema.name.clone(), schema);
    }

    /// Collect user-defined decorator schemas from the document
    pub fn collect(&mut self, doc: &Document, diagnostics: &mut DiagnosticBag) {
        for item in &doc.items {
            if let DocItem::Body(BodyItem::DecoratorSchema(ds)) = item {
                let name = crate::schema::string_lit_to_string(&ds.name);
                if self.schemas.contains_key(&name) {
                    diagnostics.error_with_code(
                        format!("duplicate decorator schema '{}'", name),
                        ds.span,
                        "E001",
                    );
                    continue;
                }
                let resolved = self.resolve_decorator_schema(ds);
                self.schemas.insert(name, resolved);
            }
        }
    }

    fn resolve_decorator_schema(&self, ds: &DecoratorSchema) -> ResolvedDecoratorSchema {
        let mut params = Vec::new();
        for field in &ds.fields {
            let required = !crate::schema::has_decorator(&field.decorators_before, "optional")
                && !crate::schema::has_decorator(&field.decorators_after, "optional");
            let default = crate::schema::get_decorator_value(&field.decorators_before, "default")
                .or_else(|| {
                    crate::schema::get_decorator_value(&field.decorators_after, "default")
                });
            params.push(DecoratorParam {
                name: field.name.name.clone(),
                type_expr: field.type_expr.clone(),
                required,
                default,
                span: field.span,
            });
        }

        ResolvedDecoratorSchema {
            name: crate::schema::string_lit_to_string(&ds.name),
            targets: ds.target.clone(),
            params,
            constraints: vec![],
            span: ds.span,
        }
    }

    /// Validate all decorators in the document
    pub fn validate_all(&self, doc: &Document, diagnostics: &mut DiagnosticBag) {
        self.validate_items(&doc.items, diagnostics);
    }

    fn validate_items(&self, items: &[DocItem], diagnostics: &mut DiagnosticBag) {
        for item in items {
            if let DocItem::Body(body_item) = item {
                self.validate_body_item(body_item, diagnostics);
            }
        }
    }

    fn validate_body_item(&self, item: &BodyItem, diagnostics: &mut DiagnosticBag) {
        match item {
            BodyItem::Block(block) => {
                for dec in &block.decorators {
                    self.validate_decorator(dec, DecoratorTarget::Block, diagnostics);
                }
                for child in &block.body {
                    self.validate_body_item(child, diagnostics);
                }
            }
            BodyItem::Attribute(attr) => {
                for dec in &attr.decorators {
                    self.validate_decorator(dec, DecoratorTarget::Attribute, diagnostics);
                }
            }
            BodyItem::Table(table) => {
                for dec in &table.decorators {
                    self.validate_decorator(dec, DecoratorTarget::Table, diagnostics);
                }
            }
            _ => {}
        }
    }

    fn validate_decorator(
        &self,
        decorator: &Decorator,
        target: DecoratorTarget,
        diagnostics: &mut DiagnosticBag,
    ) {
        let name = &decorator.name.name;

        if let Some(schema) = self.schemas.get(name) {
            // Check target validity
            if !schema.targets.contains(&target) {
                diagnostics.error_with_code(
                    format!(
                        "decorator @{} cannot be applied to {:?}",
                        name, target
                    ),
                    decorator.span,
                    "E061",
                );
            }
            // Required params check (simplified — positional-only)
            let required_count = schema.params.iter().filter(|p| p.required).count();
            let provided_count = decorator.args.len();
            if provided_count < required_count {
                diagnostics.error_with_code(
                    format!(
                        "decorator @{} requires {} argument(s), got {}",
                        name, required_count, provided_count
                    ),
                    decorator.span,
                    "E062",
                );
            }
        }
        // Unknown decorators may be attribute macros — no error
    }
}

impl Default for DecoratorSchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_registered() {
        let reg = DecoratorSchemaRegistry::new();
        // A selection of known built-ins from spec Section 15.5
        for name in &[
            "optional",
            "required",
            "default",
            "sensitive",
            "deprecated",
            "validate",
            "doc",
            "allow",
            "id_pattern",
            "ref",
            "open",
            "warning",
            "partial_requires",
            "merge_order",
            "example",
            "table_index",
        ] {
            assert!(
                reg.schemas.contains_key(*name),
                "missing built-in decorator schema: @{}",
                name
            );
        }
    }

    #[test]
    fn validate_targets_are_set() {
        let reg = DecoratorSchemaRegistry::new();

        // @sensitive targets Attribute
        assert!(reg.schemas["sensitive"]
            .targets
            .contains(&DecoratorTarget::Attribute));

        // @doc targets all four kinds
        let doc_targets = &reg.schemas["doc"].targets;
        assert!(doc_targets.contains(&DecoratorTarget::Block));
        assert!(doc_targets.contains(&DecoratorTarget::Attribute));
        assert!(doc_targets.contains(&DecoratorTarget::Table));
        assert!(doc_targets.contains(&DecoratorTarget::Schema));

        // @table_index targets Table only
        let ti_targets = &reg.schemas["table_index"].targets;
        assert!(ti_targets.contains(&DecoratorTarget::Table));
        assert!(!ti_targets.contains(&DecoratorTarget::Block));
    }

    #[test]
    fn validate_builtin_params() {
        let reg = DecoratorSchemaRegistry::new();

        // @deprecated has two params: message (required) and since (optional)
        let dep = &reg.schemas["deprecated"];
        assert_eq!(dep.params.len(), 2);
        assert!(dep.params[0].required);    // message
        assert!(!dep.params[1].required);   // since

        // @sensitive has default value true
        let sens = &reg.schemas["sensitive"];
        assert_eq!(sens.params.len(), 1);
        assert_eq!(sens.params[0].default, Some(Value::Bool(true)));
    }
}
