use std::collections::HashMap;
use indexmap::IndexMap;
use regex::Regex;
use wcl_core::ast::*;
use wcl_core::diagnostic::DiagnosticBag;
use wcl_core::span::Span;
use wcl_eval::value::Value;

use crate::types::{check_type, type_name};

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

    /// Validate all blocks in the document against their schemas.
    ///
    /// Accepts the evaluated values map so that type checking and constraint
    /// validation can operate on resolved values (not just AST literals).
    pub fn validate(
        &self,
        doc: &Document,
        values: &IndexMap<String, Value>,
        diagnostics: &mut DiagnosticBag,
    ) {
        // Collect all block IDs grouped by kind for @ref validation.
        let block_ids = collect_block_ids(&doc.items);
        self.validate_items(&doc.items, values, &block_ids, diagnostics);
    }

    fn validate_items(
        &self,
        items: &[DocItem],
        values: &IndexMap<String, Value>,
        block_ids: &HashMap<String, Vec<String>>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for item in items {
            if let DocItem::Body(BodyItem::Block(block)) = item {
                // Try to find the block's evaluated values in the values map.
                let block_values = resolve_block_values(block, values);
                self.validate_block(block, block_values.as_ref(), block_ids, diagnostics);
            }
        }
    }

    fn validate_block(
        &self,
        block: &Block,
        block_values: Option<&IndexMap<String, Value>>,
        block_ids: &HashMap<String, Vec<String>>,
        diagnostics: &mut DiagnosticBag,
    ) {
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

            // C2: Type checking — for each attribute with a schema field, check the value type
            for item in &block.body {
                if let BodyItem::Attribute(attr) = item {
                    if let Some(field) = schema.fields.iter().find(|f| f.name == attr.name.name) {
                        // Resolve value: prefer evaluated values, fall back to AST literal
                        let value = block_values
                            .and_then(|bv| bv.get(&attr.name.name))
                            .cloned()
                            .or_else(|| expr_to_value(&attr.value));

                        if let Some(ref val) = value {
                            // Type check
                            if !check_type(val, &field.type_expr) {
                                diagnostics.error_with_code(
                                    format!(
                                        "type mismatch for '{}': expected {}, got {}",
                                        field.name,
                                        type_name(&field.type_expr),
                                        value_type_label(val),
                                    ),
                                    attr.span,
                                    "E071",
                                );
                            }

                            // C3: Enforce @validate constraints
                            if let Some(ref constraints) = field.validate {
                                validate_constraints(
                                    val,
                                    constraints,
                                    &field.name,
                                    attr.span,
                                    diagnostics,
                                );
                            }

                            // M4: @ref target validation
                            if let Some(ref target) = field.ref_target {
                                validate_ref(val, target, &field.name, attr.span, block_ids, diagnostics);
                            }
                        }
                    }
                }
            }

            // M5: @id_pattern enforcement — check block's inline ID against pattern
            if let Some(ref inline_id) = block.inline_id {
                let id_str = inline_id_to_string(inline_id);
                if let Some(id_str) = id_str {
                    for field in &schema.fields {
                        if let Some(ref pattern) = field.id_pattern {
                            if let Ok(re) = Regex::new(pattern) {
                                if !re.is_match(&id_str) {
                                    diagnostics.error_with_code(
                                        format!(
                                            "block ID '{}' does not match pattern '{}' required by schema '{}'",
                                            id_str, pattern, schema.name,
                                        ),
                                        block.span,
                                        "E077",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Recursively validate nested blocks
        for item in &block.body {
            if let BodyItem::Block(child) = item {
                // Try to find child block's evaluated values within the parent's values
                let child_values = block_values
                    .and_then(|bv| resolve_block_values(child, bv));
                self.validate_block(child, child_values.as_ref(), block_ids, diagnostics);
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

// ── Validation helper functions ───────────────────────────────────────────────

/// Return a human-readable label for a Value's runtime type.
fn value_type_label(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Bool(_) => "bool",
        Value::Null => "null",
        Value::Identifier(_) => "identifier",
        Value::List(_) => "list",
        Value::Map(_) => "map",
        Value::Set(_) => "set",
        Value::BlockRef(_) => "block",
        Value::Function(_) => "function",
    }
}

/// Extract a numeric value (as f64) from a Value.
fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// C3: Validate a value against constraints.
fn validate_constraints(
    value: &Value,
    constraints: &ValidateConstraints,
    field_name: &str,
    span: Span,
    diagnostics: &mut DiagnosticBag,
) {
    // min/max
    if let Some(n) = value_as_f64(value) {
        if let Some(min) = constraints.min {
            if n < min {
                let msg = constraints.custom_msg.as_deref().unwrap_or("");
                let base = format!(
                    "validation failed for '{}': value {} is less than minimum {}",
                    field_name, n, min,
                );
                let full = if msg.is_empty() { base } else { format!("{}: {}", base, msg) };
                diagnostics.error_with_code(full, span, "E073");
            }
        }
        if let Some(max) = constraints.max {
            if n > max {
                let msg = constraints.custom_msg.as_deref().unwrap_or("");
                let base = format!(
                    "validation failed for '{}': value {} exceeds maximum {}",
                    field_name, n, max,
                );
                let full = if msg.is_empty() { base } else { format!("{}: {}", base, msg) };
                diagnostics.error_with_code(full, span, "E073");
            }
        }
    }

    // pattern
    if let Some(ref pattern) = constraints.pattern {
        if let Value::String(s) = value {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(s) {
                    let msg = constraints.custom_msg.as_deref().unwrap_or("");
                    let base = format!(
                        "validation failed for '{}': value '{}' does not match pattern '{}'",
                        field_name, s, pattern,
                    );
                    let full = if msg.is_empty() { base } else { format!("{}: {}", base, msg) };
                    diagnostics.error_with_code(full, span, "E074");
                }
            }
        }
    }

    // one_of
    if let Some(ref allowed) = constraints.one_of {
        if !allowed.iter().any(|a| values_equal(a, value)) {
            let msg = constraints.custom_msg.as_deref().unwrap_or("");
            let base = format!(
                "validation failed for '{}': value '{}' is not one of the allowed values",
                field_name, value,
            );
            let full = if msg.is_empty() { base } else { format!("{}: {}", base, msg) };
            diagnostics.error_with_code(full, span, "E075");
        }
    }
}

/// M4: Validate a @ref field — the value should reference an existing block ID.
fn validate_ref(
    value: &Value,
    target_kind: &str,
    field_name: &str,
    span: Span,
    block_ids: &HashMap<String, Vec<String>>,
    diagnostics: &mut DiagnosticBag,
) {
    let ref_id = match value {
        Value::String(s) => Some(s.clone()),
        Value::Identifier(s) => Some(s.clone()),
        _ => None,
    };
    if let Some(ref_id) = ref_id {
        let ids = block_ids.get(target_kind);
        let exists = ids.is_some_and(|ids| ids.contains(&ref_id));
        if !exists {
            diagnostics.error_with_code(
                format!(
                    "reference '{}' in field '{}' does not match any '{}' block ID",
                    ref_id, field_name, target_kind,
                ),
                span,
                "E076",
            );
        }
    }
}

/// Simple structural equality for Value (used by one_of check).
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Null, Value::Null) => true,
        (Value::Identifier(a), Value::Identifier(b)) => a == b,
        _ => false,
    }
}

/// Collect all block IDs from the document, grouped by block kind.
fn collect_block_ids(items: &[DocItem]) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    for item in items {
        if let DocItem::Body(BodyItem::Block(block)) = item {
            collect_block_ids_recursive(block, &mut result);
        }
    }
    result
}

fn collect_block_ids_recursive(block: &Block, result: &mut HashMap<String, Vec<String>>) {
    if let Some(ref inline_id) = block.inline_id {
        if let Some(id_str) = inline_id_to_string(inline_id) {
            result
                .entry(block.kind.name.clone())
                .or_default()
                .push(id_str);
        }
    }
    for item in &block.body {
        if let BodyItem::Block(child) = item {
            collect_block_ids_recursive(child, result);
        }
    }
}

/// Convert an InlineId to a string (if possible).
fn inline_id_to_string(id: &InlineId) -> Option<String> {
    match id {
        InlineId::Literal(lit) => Some(lit.value.clone()),
        InlineId::Interpolated(parts) => {
            // Only handle pure-literal interpolations
            let s: String = parts
                .iter()
                .map(|p| match p {
                    StringPart::Literal(s) => Some(s.clone()),
                    StringPart::Interpolation(_) => None,
                })
                .collect::<Option<String>>()?;
            Some(s)
        }
    }
}

/// Try to resolve a block's evaluated attribute values from the parent values map.
///
/// The evaluator stores block values as `Value::BlockRef(...)` keyed by the block kind.
/// For a block like `service#web`, look for values["service"] which may be a BlockRef
/// or a list of BlockRefs.
fn resolve_block_values(block: &Block, values: &IndexMap<String, Value>) -> Option<IndexMap<String, Value>> {
    let kind = &block.kind.name;
    let block_id = block.inline_id.as_ref().and_then(inline_id_to_string);

    match values.get(kind) {
        Some(Value::BlockRef(bref)) => {
            // Single block of this kind — check if ID matches
            if bref.id == block_id {
                Some(bref.attributes.clone())
            } else {
                None
            }
        }
        Some(Value::List(items)) => {
            // Multiple blocks of same kind — find matching one
            for item in items {
                if let Value::BlockRef(bref) = item {
                    if bref.id == block_id {
                        return Some(bref.attributes.clone());
                    }
                }
            }
            None
        }
        Some(Value::Map(map)) => {
            // Some evaluators store blocks as maps keyed by ID
            if let Some(id) = &block_id {
                match map.get(id) {
                    Some(Value::Map(attrs)) => Some(attrs.clone()),
                    Some(Value::BlockRef(bref)) => Some(bref.attributes.clone()),
                    _ => None,
                }
            } else {
                // No ID, try using the map directly as attributes
                Some(map.clone())
            }
        }
        _ => None,
    }
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
