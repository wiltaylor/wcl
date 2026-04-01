//! WCL Struct Registry — collects and resolves struct definitions.
//!
//! Structs define value/data shapes that can be used as types in schemas,
//! other structs, function parameters, and type annotations.

use crate::eval::value::Value;
use crate::lang::ast::{
    BodyItem, DecoratorArg, DocItem, Document, SchemaField, StringPart, StructDef,
};
use crate::lang::diagnostic::DiagnosticBag;
use crate::lang::span::Span;
use indexmap::IndexMap;

/// Registry of all struct definitions in a document.
#[derive(Debug, Clone, Default)]
pub struct StructRegistry {
    pub structs: IndexMap<String, ResolvedStruct>,
}

/// A resolved struct field.
#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub type_expr: crate::lang::ast::TypeExpr,
    pub required: bool,
    pub span: Span,
}

/// A resolved struct variant.
#[derive(Debug, Clone)]
pub struct StructVariant {
    pub tag_value: String,
    pub fields: Vec<StructField>,
    pub span: Span,
}

/// A resolved struct definition.
#[derive(Debug, Clone)]
pub struct ResolvedStruct {
    pub name: String,
    pub fields: Vec<StructField>,
    pub tag_field: Option<String>,
    pub variants: Vec<StructVariant>,
    pub span: Span,
}

impl StructRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect all struct definitions from a parsed document.
    pub fn collect(&mut self, doc: &Document, diagnostics: &mut DiagnosticBag) {
        for item in &doc.items {
            if let DocItem::Body(BodyItem::StructDef(struct_def)) = item {
                self.register_struct(struct_def, diagnostics);
            }
        }
    }

    fn register_struct(&mut self, def: &StructDef, diagnostics: &mut DiagnosticBag) {
        let name = match &def.name.parts[..] {
            [StringPart::Literal(s)] => s.clone(),
            _ => {
                diagnostics.error("struct name must be a plain string", def.name.span);
                return;
            }
        };

        if self.structs.contains_key(&name) {
            diagnostics.error_with_code(
                format!("duplicate struct name '{}'", name),
                def.span,
                "E112",
            );
            return;
        }

        let fields = Self::resolve_struct_fields(&def.fields);
        let variants = Self::resolve_struct_variants(&def.variants);

        // Check for @tagged decorator
        let tag_field = def
            .decorators
            .iter()
            .find(|d| d.name.name == "tagged")
            .and_then(|d| {
                d.args.first().and_then(|arg| match arg {
                    DecoratorArg::Positional(crate::lang::ast::Expr::StringLit(s)) => {
                        match &s.parts[..] {
                            [StringPart::Literal(v)] => Some(v.clone()),
                            _ => None,
                        }
                    }
                    _ => None,
                })
            });

        self.structs.insert(
            name.clone(),
            ResolvedStruct {
                name,
                fields,
                tag_field,
                variants,
                span: def.span,
            },
        );
    }

    fn resolve_struct_fields(fields: &[SchemaField]) -> Vec<StructField> {
        fields
            .iter()
            .map(|f| {
                let required = !f
                    .decorators_before
                    .iter()
                    .chain(f.decorators_after.iter())
                    .any(|d| d.name.name == "optional");
                StructField {
                    name: f.name.name.clone(),
                    type_expr: f.type_expr.clone(),
                    required,
                    span: f.span,
                }
            })
            .collect()
    }

    fn resolve_struct_variants(variants: &[crate::lang::ast::SchemaVariant]) -> Vec<StructVariant> {
        variants
            .iter()
            .map(|v| {
                let tag_value = match &v.tag_value.parts[..] {
                    [StringPart::Literal(s)] => s.clone(),
                    _ => String::new(),
                };
                StructVariant {
                    tag_value,
                    fields: Self::resolve_struct_fields(&v.fields),
                    span: v.span,
                }
            })
            .collect()
    }

    /// Look up a struct by name.
    pub fn get(&self, name: &str) -> Option<&ResolvedStruct> {
        self.structs.get(name)
    }

    /// Check if a Value conforms to a named struct type.
    pub fn check_value(&self, value: &Value, struct_name: &str) -> bool {
        let Some(resolved) = self.structs.get(struct_name) else {
            return false;
        };
        Self::check_value_against_struct(value, resolved)
    }

    fn check_value_against_struct(value: &Value, resolved: &ResolvedStruct) -> bool {
        let Value::Map(map) = value else {
            return false;
        };

        for field in &resolved.fields {
            if field.required && !map.contains_key(&field.name) {
                return false;
            }
            if let Some(val) = map.get(&field.name) {
                if !crate::schema::types::check_type(val, &field.type_expr) {
                    return false;
                }
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::span::FileId;

    #[test]
    fn collect_struct_defs() {
        let source = r#"
            struct "Point" {
                x : f64
                y : f64
            }
        "#;
        let (doc, diags) = crate::lang::parse(source, FileId(0));
        assert!(diags.into_diagnostics().iter().all(|d| !d.is_error()));

        let mut registry = StructRegistry::new();
        let mut diag_bag = DiagnosticBag::new();
        registry.collect(&doc, &mut diag_bag);

        assert!(diag_bag.into_diagnostics().iter().all(|d| !d.is_error()));
        assert_eq!(registry.structs.len(), 1);
        let point = registry.get("Point").unwrap();
        assert_eq!(point.fields.len(), 2);
        assert_eq!(point.fields[0].name, "x");
        assert_eq!(point.fields[1].name, "y");
        assert!(point.tag_field.is_none());
        assert!(point.variants.is_empty());
    }

    #[test]
    fn duplicate_struct_error() {
        let source = r#"
            struct "Foo" { x : i32 }
            struct "Foo" { y : i32 }
        "#;
        let (doc, _diags) = crate::lang::parse(source, FileId(0));

        let mut registry = StructRegistry::new();
        let mut diag_bag = DiagnosticBag::new();
        registry.collect(&doc, &mut diag_bag);

        let errors: Vec<_> = diag_bag
            .into_diagnostics()
            .into_iter()
            .filter(|d| d.is_error())
            .collect();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate struct name"));
    }

    #[test]
    fn struct_with_variants() {
        let source = r#"
            @tagged("type")
            struct "Message" {
                type : string

                variant "text" {
                    body : string
                }
                variant "image" {
                    url : string
                    width : i32
                }
            }
        "#;
        let (doc, _diags) = crate::lang::parse(source, FileId(0));

        let mut registry = StructRegistry::new();
        let mut diag_bag = DiagnosticBag::new();
        registry.collect(&doc, &mut diag_bag);

        let msg = registry.get("Message").unwrap();
        assert_eq!(msg.fields.len(), 1);
        assert_eq!(msg.variants.len(), 2);
        assert_eq!(msg.tag_field, Some("type".to_string()));
    }
}
