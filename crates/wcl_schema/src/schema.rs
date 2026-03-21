use indexmap::IndexMap;
use regex::Regex;
use std::collections::HashMap;
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
    pub text_field: Option<String>,
    pub allowed_children: Option<Vec<String>>,
    pub allowed_parents: Option<Vec<String>>,
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
    pub text: bool,
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

    fn resolve_schema(&self, schema: &Schema, diagnostics: &mut DiagnosticBag) -> ResolvedSchema {
        let open = schema.decorators.iter().any(|d| d.name.name == "open");
        let allowed_children = get_decorator_string_list_arg(&schema.decorators, "children");
        let allowed_parents = get_decorator_string_list_arg(&schema.decorators, "parent");
        let mut fields = Vec::new();
        let mut text_field = None;

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
            let is_text = has_decorator(&field.decorators_before, "text")
                || has_decorator(&field.decorators_after, "text");

            if is_text {
                if field.name.name != "content" {
                    diagnostics.error_with_code(
                        format!(
                            "@text field must be named 'content', found '{}'",
                            field.name.name
                        ),
                        field.span,
                        "E094",
                    );
                }
                if !matches!(field.type_expr, TypeExpr::String(_)) {
                    diagnostics.error_with_code(
                        "@text field must have type 'string'".to_string(),
                        field.span,
                        "E094",
                    );
                }
                if text_field.is_some() {
                    diagnostics.error_with_code(
                        "schema may have at most one @text field".to_string(),
                        field.span,
                        "E094",
                    );
                } else {
                    text_field = Some(field.name.name.clone());
                }
            }

            fields.push(ResolvedField {
                name: field.name.name.clone(),
                type_expr: field.type_expr.clone(),
                required,
                default,
                validate,
                ref_target,
                id_pattern,
                text: is_text,
                span: field.span,
            });
        }

        ResolvedSchema {
            name: string_lit_to_string(&schema.name),
            fields,
            open,
            text_field,
            allowed_children,
            allowed_parents,
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
            match item {
                DocItem::Body(BodyItem::Block(block)) => {
                    let block_values = resolve_block_values(block, values);
                    self.validate_block(block, block_values.as_ref(), block_ids, None, diagnostics);
                }
                DocItem::Body(BodyItem::Table(table)) => {
                    self.validate_table_containment(table, None, diagnostics);
                }
                _ => {}
            }
        }
    }

    fn validate_table_containment(
        &self,
        table: &Table,
        parent_kind: Option<&str>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let parent_name = parent_kind.unwrap_or("_root");
        let table_child_name = match &table.schema_ref {
            Some(sr) => format!("table:{}", sr.name),
            None => "table".to_string(),
        };
        let table_label = match &table.schema_ref {
            Some(sr) => format!("table:{}", sr.name),
            None => "anonymous table".to_string(),
        };

        // E096: Check table's @parent constraint
        if let Some(table_schema) = self.schemas.get(&table_child_name) {
            if let Some(ref allowed) = table_schema.allowed_parents {
                if !allowed.iter().any(|p| p == parent_name) {
                    diagnostics.error_with_code(
                        format!(
                            "{} is not allowed inside '{}'; allowed parents: [{}]",
                            table_label,
                            parent_name,
                            allowed.join(", "),
                        ),
                        table.span,
                        "E096",
                    );
                }
            }
        }

        // E095: Check parent's @children constraint
        if let Some(parent_schema) = self.schemas.get(parent_name) {
            if let Some(ref allowed) = parent_schema.allowed_children {
                if !allowed.iter().any(|c| c == &table_child_name) {
                    diagnostics.error_with_code(
                        format!(
                            "{} is not allowed as a child of '{}'; allowed children: [{}]",
                            table_label,
                            parent_name,
                            allowed.join(", "),
                        ),
                        table.span,
                        "E095",
                    );
                }
            }
        }
    }

    fn validate_block(
        &self,
        block: &Block,
        block_values: Option<&IndexMap<String, Value>>,
        block_ids: &HashMap<String, Vec<String>>,
        parent_kind: Option<&str>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let child_kind = &block.kind.name;
        let parent_name = parent_kind.unwrap_or("_root");

        // E096: Check child's @parent constraint
        if let Some(child_schema) = self.schemas.get(child_kind) {
            if let Some(ref allowed) = child_schema.allowed_parents {
                if !allowed.iter().any(|p| p == parent_name) {
                    diagnostics.error_with_code(
                        format!(
                            "block kind '{}' is not allowed inside '{}'; allowed parents: [{}]",
                            child_kind,
                            parent_name,
                            allowed.join(", "),
                        ),
                        block.span,
                        "E096",
                    );
                }
            }
        }

        // E095: Check parent's @children constraint
        if let Some(parent_schema) = self.schemas.get(parent_name) {
            if let Some(ref allowed) = parent_schema.allowed_children {
                if !allowed.iter().any(|c| c == child_kind) {
                    diagnostics.error_with_code(
                        format!(
                            "block kind '{}' is not allowed as a child of '{}'; allowed children: [{}]",
                            child_kind,
                            parent_name,
                            allowed.join(", "),
                        ),
                        block.span,
                        "E095",
                    );
                }
            }
        }

        // Check if there's a schema for this block type
        if let Some(schema) = self.schemas.get(&block.kind.name) {
            // Text block validation
            if block.text_content.is_some() && schema.text_field.is_none() {
                diagnostics.error_with_code(
                    format!(
                        "block '{}' uses text block syntax but schema '{}' has no @text field",
                        block.kind.name, schema.name
                    ),
                    block.span,
                    "E093",
                );
            }
            if schema.text_field.is_some() && block.text_content.is_none() {
                diagnostics.error_with_code(
                    format!(
                        "schema '{}' expects text block syntax (heredoc or string) but block uses brace body",
                        schema.name
                    ),
                    block.span,
                    "E094",
                );
            }

            // Check required fields
            for field in &schema.fields {
                if field.required {
                    let has_attr = block.body.iter().any(|item| {
                        matches!(item, BodyItem::Attribute(attr) if attr.name.name == field.name)
                    });
                    let is_id_field = field.name == "id";
                    let has_id = block.inline_id.is_some();
                    // @text field is satisfied by text_content
                    let is_text_field = field.text && block.text_content.is_some();

                    if !has_attr && (!is_id_field || !has_id) && !is_text_field {
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
                                validate_ref(
                                    val,
                                    target,
                                    &field.name,
                                    attr.span,
                                    block_ids,
                                    diagnostics,
                                );
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

        // Recursively validate nested blocks and tables
        for item in &block.body {
            match item {
                BodyItem::Block(child) => {
                    let child_values = block_values.and_then(|bv| resolve_block_values(child, bv));
                    self.validate_block(
                        child,
                        child_values.as_ref(),
                        block_ids,
                        Some(&block.kind.name),
                        diagnostics,
                    );
                }
                BodyItem::Table(table) => {
                    self.validate_table_containment(table, Some(&block.kind.name), diagnostics);
                }
                _ => {}
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

pub(crate) fn get_decorator_string_list_arg(
    decorators: &[Decorator],
    name: &str,
) -> Option<Vec<String>> {
    decorators
        .iter()
        .find(|d| d.name.name == name)
        .and_then(|d| {
            d.args.first().and_then(|arg| match arg {
                DecoratorArg::Positional(Expr::List(items, _)) => items
                    .iter()
                    .map(|item| match item {
                        Expr::StringLit(s) => Some(string_lit_to_string(s)),
                        _ => None,
                    })
                    .collect(),
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
pub(crate) fn value_type_label(value: &Value) -> &'static str {
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
pub(crate) fn validate_constraints(
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
                let full = if msg.is_empty() {
                    base
                } else {
                    format!("{}: {}", base, msg)
                };
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
                let full = if msg.is_empty() {
                    base
                } else {
                    format!("{}: {}", base, msg)
                };
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
                    let full = if msg.is_empty() {
                        base
                    } else {
                        format!("{}: {}", base, msg)
                    };
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
            let full = if msg.is_empty() {
                base
            } else {
                format!("{}: {}", base, msg)
            };
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
fn resolve_block_values(
    block: &Block,
    values: &IndexMap<String, Value>,
) -> Option<IndexMap<String, Value>> {
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

    // ── @text schema tests ────────────────────────────────────────────────

    fn make_text_schema_field(name: &str, type_expr: TypeExpr) -> SchemaField {
        SchemaField {
            decorators_before: vec![],
            name: make_ident(name),
            type_expr,
            decorators_after: vec![Decorator {
                name: make_ident("text"),
                args: vec![],
                span: dummy_span(),
            }],
            trivia: Trivia::default(),
            span: dummy_span(),
        }
    }

    fn make_block_node(kind: &str, id: Option<&str>, text_content: Option<&str>) -> Block {
        Block {
            decorators: vec![],
            partial: false,
            kind: make_ident(kind),
            inline_id: id.map(|v| {
                InlineId::Literal(IdentifierLit {
                    value: v.to_string(),
                    span: dummy_span(),
                })
            }),
            labels: vec![],
            body: vec![],
            text_content: text_content.map(|s| make_string_lit(s)),
            trivia: Trivia::default(),
            span: dummy_span(),
        }
    }

    #[test]
    fn text_schema_resolved_with_text_field() {
        let schema = make_schema(
            "readme",
            vec![make_text_schema_field(
                "content",
                TypeExpr::String(dummy_span()),
            )],
        );
        let doc = make_document(vec![DocItem::Body(BodyItem::Schema(schema))]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);

        assert!(!diags.has_errors());
        let s = &reg.schemas["readme"];
        assert_eq!(s.text_field, Some("content".to_string()));
        assert!(s.fields[0].text);
    }

    #[test]
    fn text_field_wrong_name_emits_e094() {
        let schema = make_schema(
            "readme",
            vec![make_text_schema_field(
                "body",
                TypeExpr::String(dummy_span()),
            )],
        );
        let doc = make_document(vec![DocItem::Body(BodyItem::Schema(schema))]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);

        assert!(diags.has_errors());
        let e094: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E094"))
            .collect();
        assert_eq!(e094.len(), 1);
        assert!(e094[0].message.contains("content"));
    }

    #[test]
    fn text_field_wrong_type_emits_e094() {
        let schema = make_schema(
            "readme",
            vec![make_text_schema_field(
                "content",
                TypeExpr::Int(dummy_span()),
            )],
        );
        let doc = make_document(vec![DocItem::Body(BodyItem::Schema(schema))]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);

        assert!(diags.has_errors());
        let e094: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E094"))
            .collect();
        assert!(!e094.is_empty());
    }

    #[test]
    fn text_block_without_text_schema_emits_e093() {
        // Schema without @text
        let schema = make_schema(
            "readme",
            vec![make_schema_field("name", TypeExpr::String(dummy_span()))],
        );
        // Block with text_content
        let block = make_block_node("readme", Some("doc"), Some("content here"));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(schema)),
            DocItem::Body(BodyItem::Block(block)),
        ]);

        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        assert!(!diags.has_errors());

        let values = IndexMap::new();
        reg.validate(&doc, &values, &mut diags);

        let e093: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E093"))
            .collect();
        assert_eq!(e093.len(), 1);
    }

    #[test]
    fn text_schema_with_brace_body_emits_e094() {
        // Schema with @text
        let schema = make_schema(
            "readme",
            vec![make_text_schema_field(
                "content",
                TypeExpr::String(dummy_span()),
            )],
        );
        // Block with brace body (no text_content)
        let mut block = make_block_node("readme", Some("doc"), None);
        block.body.push(BodyItem::Attribute(Attribute {
            decorators: vec![],
            name: make_ident("content"),
            value: Expr::StringLit(make_string_lit("text")),
            trivia: Trivia::default(),
            span: dummy_span(),
        }));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(schema)),
            DocItem::Body(BodyItem::Block(block)),
        ]);

        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        assert!(!diags.has_errors());

        let values = IndexMap::new();
        reg.validate(&doc, &values, &mut diags);

        let e094: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E094"))
            .collect();
        assert_eq!(e094.len(), 1);
    }

    #[test]
    fn text_schema_with_text_block_passes() {
        // Schema with @text
        let schema = make_schema(
            "readme",
            vec![make_text_schema_field(
                "content",
                TypeExpr::String(dummy_span()),
            )],
        );
        // Block with text_content
        let block = make_block_node("readme", Some("doc"), Some("Hello world"));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(schema)),
            DocItem::Body(BodyItem::Block(block)),
        ]);

        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        assert!(!diags.has_errors());

        let values = IndexMap::new();
        reg.validate(&doc, &values, &mut diags);

        // No E093 or E094
        let text_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E093") || d.code.as_deref() == Some("E094"))
            .collect();
        assert!(text_errors.is_empty());

        // No E070 for missing content field (it's implicitly satisfied)
        let e070: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E070"))
            .collect();
        assert!(e070.is_empty());
    }

    // ── Containment tests (@children / @parent) ─────────────────────────

    fn make_decorator_with_string_list(name: &str, values: &[&str]) -> Decorator {
        let items: Vec<Expr> = values
            .iter()
            .map(|v| Expr::StringLit(make_string_lit(v)))
            .collect();
        Decorator {
            name: make_ident(name),
            args: vec![DecoratorArg::Positional(Expr::List(items, dummy_span()))],
            span: dummy_span(),
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
            trivia: Trivia::default(),
            span: dummy_span(),
        }
    }

    fn make_table_node(schema_ref: Option<&str>) -> Table {
        Table {
            decorators: vec![],
            partial: false,
            inline_id: Some(InlineId::Literal(IdentifierLit {
                value: "t".to_string(),
                span: dummy_span(),
            })),
            schema_ref: schema_ref.map(|s| make_ident(s)),
            columns: vec![],
            rows: vec![],
            import_expr: None,
            trivia: Trivia::default(),
            span: dummy_span(),
        }
    }

    #[test]
    fn children_allows_valid_child() {
        let parent_schema = make_schema_with_decorators(
            "service",
            vec![],
            vec![make_decorator_with_string_list("children", &["endpoint"])],
        );
        let child = make_block_node("endpoint", Some("ep1"), None);
        let mut parent = make_block_node("service", Some("svc1"), None);
        parent.body.push(BodyItem::Block(child));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(parent_schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        assert!(
            !diags.has_errors(),
            "unexpected errors: {:?}",
            diags.diagnostics()
        );
    }

    #[test]
    fn children_rejects_invalid_child() {
        let parent_schema = make_schema_with_decorators(
            "service",
            vec![],
            vec![make_decorator_with_string_list("children", &["endpoint"])],
        );
        let child = make_block_node("middleware", Some("mw1"), None);
        let mut parent = make_block_node("service", Some("svc1"), None);
        parent.body.push(BodyItem::Block(child));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(parent_schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e095: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095"))
            .collect();
        assert_eq!(e095.len(), 1);
    }

    #[test]
    fn parent_allows_valid_parent() {
        let child_schema = make_schema_with_decorators(
            "endpoint",
            vec![],
            vec![make_decorator_with_string_list("parent", &["service"])],
        );
        let child = make_block_node("endpoint", Some("ep1"), None);
        let mut parent = make_block_node("service", Some("svc1"), None);
        parent.body.push(BodyItem::Block(child));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(child_schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e096: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert!(e096.is_empty());
    }

    #[test]
    fn parent_rejects_at_root() {
        let child_schema = make_schema_with_decorators(
            "endpoint",
            vec![],
            vec![make_decorator_with_string_list("parent", &["service"])],
        );
        let child = make_block_node("endpoint", Some("ep1"), None);

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(child_schema)),
            DocItem::Body(BodyItem::Block(child)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e096: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert_eq!(e096.len(), 1);
    }

    #[test]
    fn parent_root_allows_top_level() {
        let schema = make_schema_with_decorators(
            "service",
            vec![],
            vec![make_decorator_with_string_list("parent", &["_root"])],
        );
        let block = make_block_node("service", Some("svc1"), None);

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(schema)),
            DocItem::Body(BodyItem::Block(block)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e096: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert!(e096.is_empty());
    }

    #[test]
    fn parent_root_rejects_nested() {
        let schema = make_schema_with_decorators(
            "service",
            vec![],
            vec![make_decorator_with_string_list("parent", &["_root"])],
        );
        let inner = make_block_node("service", Some("svc2"), None);
        let mut outer = make_block_node("wrapper", Some("w1"), None);
        outer.body.push(BodyItem::Block(inner));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(schema)),
            DocItem::Body(BodyItem::Block(outer)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e096: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert_eq!(e096.len(), 1);
    }

    #[test]
    fn children_empty_rejects_all() {
        let parent_schema = make_schema_with_decorators(
            "leaf",
            vec![],
            vec![make_decorator_with_string_list("children", &[])],
        );
        let child = make_block_node("anything", None, None);
        let mut parent = make_block_node("leaf", Some("l1"), None);
        parent.body.push(BodyItem::Block(child));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(parent_schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e095: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095"))
            .collect();
        assert_eq!(e095.len(), 1);
    }

    #[test]
    fn no_constraint_allows_anything() {
        let schema = make_schema("service", vec![]);
        let child = make_block_node("anything", None, None);
        let mut parent = make_block_node("service", Some("svc"), None);
        parent.body.push(BodyItem::Block(child));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let containment: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095") || d.code.as_deref() == Some("E096"))
            .collect();
        assert!(containment.is_empty());
    }

    #[test]
    fn both_constraints_fire() {
        let parent_schema = make_schema_with_decorators(
            "service",
            vec![],
            vec![make_decorator_with_string_list("children", &["x"])],
        );
        let child_schema = make_schema_with_decorators(
            "endpoint",
            vec![],
            vec![make_decorator_with_string_list("parent", &["y"])],
        );
        let child = make_block_node("endpoint", Some("ep"), None);
        let mut parent = make_block_node("service", Some("svc"), None);
        parent.body.push(BodyItem::Block(child));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(parent_schema)),
            DocItem::Body(BodyItem::Schema(child_schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e095: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095"))
            .collect();
        let e096: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert_eq!(e095.len(), 1);
        assert_eq!(e096.len(), 1);
    }

    #[test]
    fn root_schema_children() {
        let root_schema = make_schema_with_decorators(
            "_root",
            vec![],
            vec![make_decorator_with_string_list("children", &["service"])],
        );
        let block = make_block_node("config", None, None);

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(root_schema)),
            DocItem::Body(BodyItem::Block(block)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e095: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095"))
            .collect();
        assert_eq!(e095.len(), 1);
    }

    #[test]
    fn unschemaed_block_unconstrained() {
        // No schema for "mystery" — no containment errors
        let block = make_block_node("mystery", None, None);
        let doc = make_document(vec![DocItem::Body(BodyItem::Block(block))]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let containment: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095") || d.code.as_deref() == Some("E096"))
            .collect();
        assert!(containment.is_empty());
    }

    // ── Table containment tests ─────────────────────────────────────────

    #[test]
    fn children_with_table_schema_allows() {
        let parent_schema = make_schema_with_decorators(
            "data",
            vec![],
            vec![make_decorator_with_string_list(
                "children",
                &["table:user_row"],
            )],
        );
        let table = make_table_node(Some("user_row"));
        let mut parent = make_block_node("data", Some("d1"), None);
        parent.body.push(BodyItem::Table(table));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(parent_schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let containment: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095") || d.code.as_deref() == Some("E096"))
            .collect();
        assert!(containment.is_empty());
    }

    #[test]
    fn children_without_table_rejects() {
        let parent_schema = make_schema_with_decorators(
            "data",
            vec![],
            vec![make_decorator_with_string_list("children", &["endpoint"])],
        );
        let table = make_table_node(Some("user_row"));
        let mut parent = make_block_node("data", Some("d1"), None);
        parent.body.push(BodyItem::Table(table));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(parent_schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e095: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095"))
            .collect();
        assert_eq!(e095.len(), 1);
    }

    #[test]
    fn children_anon_table_allowed() {
        let parent_schema = make_schema_with_decorators(
            "data",
            vec![],
            vec![make_decorator_with_string_list("children", &["table"])],
        );
        let table = make_table_node(None);
        let mut parent = make_block_node("data", Some("d1"), None);
        parent.body.push(BodyItem::Table(table));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(parent_schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let containment: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095") || d.code.as_deref() == Some("E096"))
            .collect();
        assert!(containment.is_empty());
    }

    #[test]
    fn children_anon_table_rejected() {
        let parent_schema = make_schema_with_decorators(
            "data",
            vec![],
            vec![make_decorator_with_string_list(
                "children",
                &["table:user_row"],
            )],
        );
        let table = make_table_node(None); // anonymous
        let mut parent = make_block_node("data", Some("d1"), None);
        parent.body.push(BodyItem::Table(table));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(parent_schema)),
            DocItem::Body(BodyItem::Block(parent)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e095: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095"))
            .collect();
        assert_eq!(e095.len(), 1);
    }

    #[test]
    fn table_parent_constraint() {
        let table_schema = make_schema_with_decorators(
            "table:user_row",
            vec![],
            vec![make_decorator_with_string_list("parent", &["data"])],
        );
        let table = make_table_node(Some("user_row"));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(table_schema)),
            DocItem::Body(BodyItem::Table(table)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e096: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert_eq!(e096.len(), 1);
    }

    #[test]
    fn table_parent_allows() {
        let table_schema = make_schema_with_decorators(
            "table:user_row",
            vec![],
            vec![make_decorator_with_string_list(
                "parent",
                &["data", "_root"],
            )],
        );
        let table = make_table_node(Some("user_row"));

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(table_schema)),
            DocItem::Body(BodyItem::Table(table)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e096: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert!(e096.is_empty());
    }

    #[test]
    fn anon_table_parent_constraint() {
        let table_schema = make_schema_with_decorators(
            "table",
            vec![],
            vec![make_decorator_with_string_list("parent", &["service"])],
        );
        let table = make_table_node(None);

        let doc = make_document(vec![
            DocItem::Body(BodyItem::Schema(table_schema)),
            DocItem::Body(BodyItem::Table(table)),
        ]);
        let mut reg = SchemaRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.collect(&doc, &mut diags);
        reg.validate(&doc, &IndexMap::new(), &mut diags);
        let e096: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert_eq!(e096.len(), 1);
    }
}
