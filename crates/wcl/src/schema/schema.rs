use crate::eval::value::Value;
use crate::lang::ast::*;
use crate::lang::diagnostic::DiagnosticBag;
use crate::lang::span::Span;
use indexmap::IndexMap;
use regex::Regex;
use std::collections::{HashMap, HashSet};

use crate::schema::types::{check_type, type_name};

// ── Symbol Set Registry ──────────────────────────────────────────────────────

/// Info about a single symbol set.
#[derive(Debug, Clone)]
pub struct SymbolSetInfo {
    /// The declared member names.
    pub members: Vec<String>,
    /// Maps symbol_name → serialization string (only for members with `= "..."`)
    pub value_map: HashMap<String, String>,
    pub span: Span,
}

/// Registry of all `symbol_set` declarations in a document.
#[derive(Debug, Default)]
pub struct SymbolSetRegistry {
    pub sets: HashMap<String, SymbolSetInfo>,
}

impl SymbolSetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect symbol_set declarations from the document AST.
    pub fn collect(&mut self, doc: &Document, diagnostics: &mut DiagnosticBag) {
        for item in &doc.items {
            if let DocItem::Body(BodyItem::SymbolSetDecl(decl)) = item {
                let name = decl.name.name.clone();
                if self.sets.contains_key(&name) {
                    diagnostics.error_with_code(
                        format!("duplicate symbol_set name '{}'", name),
                        decl.span,
                        "E102",
                    );
                    continue;
                }
                let mut members = Vec::new();
                let mut value_map = HashMap::new();
                let mut seen_members = HashMap::new();
                for member in &decl.members {
                    if let Some(prev_span) = seen_members.get(&member.name) {
                        diagnostics.error_with_code(
                            format!(
                                "duplicate symbol ':{name}' in symbol_set '{set}'",
                                name = member.name,
                                set = decl.name.name,
                            ),
                            member.span,
                            "E103",
                        );
                        let _ = prev_span;
                        continue;
                    }
                    seen_members.insert(member.name.clone(), member.span);
                    members.push(member.name.clone());
                    if let Some(ref val) = member.value {
                        let s = string_lit_to_string(val);
                        value_map.insert(member.name.clone(), s);
                    }
                }
                self.sets.insert(
                    name,
                    SymbolSetInfo {
                        members,
                        value_map,
                        span: decl.span,
                    },
                );
            }
        }
    }

    /// Check if a symbol name is a member of the named set.
    pub fn contains(&self, set_name: &str, symbol_name: &str) -> bool {
        if set_name == "all" {
            return true;
        }
        self.sets
            .get(set_name)
            .map(|info| info.members.contains(&symbol_name.to_string()))
            .unwrap_or(false)
    }

    /// Returns true if a set with the given name exists (or name is "all").
    pub fn set_exists(&self, set_name: &str) -> bool {
        set_name == "all" || self.sets.contains_key(set_name)
    }

    /// Get the serialization string for a symbol in a set.
    /// Returns the mapped value if one exists, otherwise the symbol name itself.
    pub fn serialize_symbol(&self, set_name: &str, symbol_name: &str) -> String {
        if let Some(info) = self.sets.get(set_name) {
            if let Some(mapped) = info.value_map.get(symbol_name) {
                return mapped.clone();
            }
        }
        symbol_name.to_string()
    }
}

/// A resolved schema definition
#[derive(Debug, Clone)]
pub struct ResolvedSchema {
    pub name: String,
    pub doc: Option<String>,
    pub fields: Vec<ResolvedField>,
    pub open: bool,
    pub text_field: Option<String>,
    pub allowed_children: Option<Vec<String>>,
    pub allowed_parents: Option<Vec<String>>,
    pub child_constraints: Vec<ChildConstraint>,
    pub tag_field: Option<String>,
    pub variants: Vec<ResolvedVariant>,
    pub span: Span,
}

/// Per-child-kind cardinality and depth constraints from `@child` decorators.
#[derive(Debug, Clone)]
pub struct ChildConstraint {
    pub kind: String,
    pub min: Option<usize>,
    pub max: Option<usize>,
    pub max_depth: Option<usize>,
    pub span: Span,
}

/// A resolved tagged variant arm inside a schema.
#[derive(Debug, Clone)]
pub struct ResolvedVariant {
    pub tag_value: String,
    pub doc: Option<String>,
    pub fields: Vec<ResolvedField>,
    pub allowed_children: Option<Vec<String>>,
    pub child_constraints: Vec<ChildConstraint>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ResolvedField {
    pub name: String,
    pub doc: Option<String>,
    pub type_expr: TypeExpr,
    pub required: bool,
    pub default: Option<Value>,
    pub validate: Option<ValidateConstraints>,
    pub ref_target: Option<String>,
    pub id_pattern: Option<String>,
    pub text: bool,
    pub inline_index: Option<usize>,
    pub symbol_set: Option<String>,
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
    pub schemas: HashMap<String, Vec<ResolvedSchema>>,
    /// Namespace aliases from `use` declarations for alias-aware lookup.
    pub namespace_aliases: HashMap<String, String>,
}

impl SchemaRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a schema by name and parent context.
    ///
    /// Resolution order: parent-scoped match (via `@parent`) > unscoped fallback.
    /// Falls back to namespace alias resolution if direct lookup fails.
    pub fn get_schema(&self, name: &str, parent_kind: Option<&str>) -> Option<&ResolvedSchema> {
        let candidates = self.schemas.get(name).or_else(|| {
            self.namespace_aliases
                .get(name)
                .and_then(|qualified| self.schemas.get(qualified))
        })?;

        if let Some(parent) = parent_kind {
            // Resolve parent through aliases (e.g. "style" → "wdoc::style")
            let resolved_parent = self
                .namespace_aliases
                .get(parent)
                .map(String::as_str)
                .unwrap_or(parent);
            // Prefer parent-scoped match
            if let Some(scoped) = candidates.iter().find(|s| {
                s.allowed_parents
                    .as_ref()
                    .is_some_and(|ps| ps.iter().any(|p| p == resolved_parent))
            }) {
                return Some(scoped);
            }
        }

        // Fall back to unscoped (no @parent)
        candidates.iter().find(|s| s.allowed_parents.is_none())
    }

    /// Check if placing a child kind under a given parent violates @parent constraints.
    ///
    /// Examines ALL schema variants for the child name. Returns the combined allowed
    /// parents list if the placement is invalid, or `None` if it's valid.
    /// Placement is valid if any variant has no @parent (unscoped) or any variant's
    /// @parent list includes the current parent.
    fn check_parent_violation(&self, child_kind: &str, parent_name: &str) -> Option<Vec<String>> {
        let candidates = self.schemas.get(child_kind).or_else(|| {
            self.namespace_aliases
                .get(child_kind)
                .and_then(|qualified| self.schemas.get(qualified))
        })?;

        // Resolve parent through aliases
        let resolved_parent = self
            .namespace_aliases
            .get(parent_name)
            .map(String::as_str)
            .unwrap_or(parent_name);

        // If any variant has no @parent, placement is always valid
        if candidates.iter().any(|s| s.allowed_parents.is_none()) {
            return None;
        }

        // Check if any variant allows this parent
        let any_allows = candidates.iter().any(|s| {
            s.allowed_parents
                .as_ref()
                .is_some_and(|ps| ps.iter().any(|p| p == resolved_parent))
        });

        if any_allows {
            None
        } else {
            // Collect all allowed parents across variants for the error message
            let mut all_parents: Vec<String> = candidates
                .iter()
                .filter_map(|s| s.allowed_parents.as_ref())
                .flatten()
                .cloned()
                .collect();
            all_parents.sort();
            all_parents.dedup();
            Some(all_parents)
        }
    }

    /// Iterate all schemas (flattened across parent scopes).
    pub fn all_schemas(&self) -> impl Iterator<Item = (&String, &ResolvedSchema)> {
        self.schemas
            .iter()
            .flat_map(|(name, vec)| vec.iter().map(move |s| (name, s)))
    }

    /// Extract and register schemas from the document AST.
    ///
    /// Multiple schemas with the same name are allowed if their `@parent` scopes
    /// don't overlap. E001 fires only on true duplicates (both unscoped or
    /// overlapping parent lists).
    pub fn collect(&mut self, doc: &Document, diagnostics: &mut DiagnosticBag) {
        for item in &doc.items {
            if let DocItem::Body(BodyItem::Schema(schema)) = item {
                let name = string_lit_to_string(&schema.name);
                let resolved = self.resolve_schema(schema, diagnostics);
                let candidates = self.schemas.entry(name.clone()).or_default();

                let has_conflict = candidates.iter().any(|existing| {
                    match (&existing.allowed_parents, &resolved.allowed_parents) {
                        (None, None) => true,
                        (Some(ep), Some(np)) => ep.iter().any(|e| np.contains(e)),
                        _ => false,
                    }
                });

                if has_conflict {
                    diagnostics.error_with_code(
                        format!("duplicate schema name '{}' with overlapping scope", name),
                        schema.span,
                        "E001",
                    );
                } else {
                    candidates.push(resolved);
                }
            }
        }
    }

    fn resolve_schema(&self, schema: &Schema, diagnostics: &mut DiagnosticBag) -> ResolvedSchema {
        let doc = get_decorator_string_arg(&schema.decorators, "doc");
        let open = schema.decorators.iter().any(|d| d.name.name == "open");
        let allowed_children = get_decorator_string_list_arg(&schema.decorators, "children");
        let allowed_parents = get_decorator_string_list_arg(&schema.decorators, "parent");
        let tag_field = get_decorator_string_arg(&schema.decorators, "tagged");

        // Resolve @parent/@children values through namespace aliases
        let aliases = &self.namespace_aliases;
        let resolve_names = |names: Vec<String>| -> Vec<String> {
            names
                .into_iter()
                .map(|n| aliases.get(&n).cloned().unwrap_or(n))
                .collect()
        };
        let mut allowed_children = allowed_children.map(&resolve_names);
        let allowed_parents = allowed_parents.map(&resolve_names);

        // Parse @child decorators
        let child_constraints: Vec<ChildConstraint> = schema
            .decorators
            .iter()
            .filter(|d| d.name.name == "child")
            .filter_map(parse_child_decorator)
            .map(|mut cc| {
                cc.kind = aliases.get(&cc.kind).cloned().unwrap_or(cc.kind);
                cc
            })
            .collect();

        // Merge @child kinds into allowed_children
        if !child_constraints.is_empty() {
            let children = allowed_children.get_or_insert_with(Vec::new);
            for cc in &child_constraints {
                if !children.contains(&cc.kind) {
                    children.push(cc.kind.clone());
                }
            }
        }

        // Resolve variants
        let mut variants = Vec::new();
        for variant in &schema.variants {
            let variant_fields = resolve_fields(&variant.fields, &mut None, diagnostics);
            let variant_allowed_children =
                get_decorator_string_list_arg(&variant.decorators, "children");
            let variant_child_constraints: Vec<ChildConstraint> = variant
                .decorators
                .iter()
                .filter(|d| d.name.name == "child")
                .filter_map(parse_child_decorator)
                .collect();
            let variant_doc = get_decorator_string_arg(&variant.decorators, "doc");
            variants.push(ResolvedVariant {
                tag_value: string_lit_to_string(&variant.tag_value),
                doc: variant_doc,
                fields: variant_fields,
                allowed_children: variant_allowed_children,
                child_constraints: variant_child_constraints,
                span: variant.span,
            });
        }

        let mut fields = Vec::new();
        let mut text_field = None;

        for field in &schema.fields {
            let field_doc = get_decorator_string_arg(&field.decorators_before, "doc")
                .or_else(|| get_decorator_string_arg(&field.decorators_after, "doc"));
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
            let inline_index = get_decorator_int_arg(&field.decorators_before, "inline")
                .or_else(|| get_decorator_int_arg(&field.decorators_after, "inline"))
                .map(|n| n as usize);
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

            let symbol_set = get_decorator_string_arg(&field.decorators_before, "symbol_set")
                .or_else(|| get_decorator_string_arg(&field.decorators_after, "symbol_set"));

            fields.push(ResolvedField {
                name: field.name.name.clone(),
                doc: field_doc,
                type_expr: field.type_expr.clone(),
                required,
                default,
                validate,
                ref_target,
                id_pattern,
                text: is_text,
                inline_index,
                symbol_set,
                span: field.span,
            });
        }

        ResolvedSchema {
            name: string_lit_to_string(&schema.name),
            doc,
            fields,
            open,
            text_field,
            allowed_children,
            allowed_parents,
            child_constraints,
            tag_field,
            variants,
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
        symbol_sets: &SymbolSetRegistry,
        diagnostics: &mut DiagnosticBag,
    ) {
        // Collect all block IDs grouped by kind for @ref validation.
        let block_ids = collect_block_ids(&doc.items);
        self.validate_items(&doc.items, values, &block_ids, symbol_sets, diagnostics);
    }

    fn validate_items(
        &self,
        items: &[DocItem],
        values: &IndexMap<String, Value>,
        block_ids: &BlockIdIndex,
        symbol_sets: &SymbolSetRegistry,
        diagnostics: &mut DiagnosticBag,
    ) {
        for item in items {
            match item {
                DocItem::Body(BodyItem::Block(block)) => {
                    let block_values = resolve_block_values(block, values);
                    self.validate_block(
                        block,
                        block_values.as_ref(),
                        block_ids,
                        None,
                        None,
                        &HashMap::new(),
                        symbol_sets,
                        diagnostics,
                    );
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
        let raw_parent = parent_kind.unwrap_or("_root");
        let parent_name = self
            .namespace_aliases
            .get(raw_parent)
            .map(String::as_str)
            .unwrap_or(raw_parent);
        let table_child_name = match &table.schema_ref {
            Some(sr) => format!("table:{}", sr.name),
            None => "table".to_string(),
        };
        let table_label = match &table.schema_ref {
            Some(sr) => format!("table:{}", sr.name),
            None => "anonymous table".to_string(),
        };

        // E096: Check table's @parent constraint (across all schema variants)
        if let Some(allowed) = self.check_parent_violation(&table_child_name, parent_name) {
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

        // E095: Check parent's @children constraint
        if let Some(parent_schema) = self.get_schema(parent_name, None) {
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

    #[allow(clippy::too_many_arguments)]
    fn validate_block(
        &self,
        block: &Block,
        block_values: Option<&IndexMap<String, Value>>,
        block_ids: &BlockIdIndex,
        parent_kind: Option<&str>,
        parent_qualified_id: Option<&str>,
        kind_depths: &HashMap<String, usize>,
        symbol_sets: &SymbolSetRegistry,
        diagnostics: &mut DiagnosticBag,
    ) {
        // Compute this block's qualified ID for scoped ref resolution.
        let block_qid = block
            .inline_id
            .as_ref()
            .and_then(inline_id_to_string)
            .map(|bare_id| match parent_qualified_id {
                Some(pqid) => format!("{}.{}", pqid, bare_id),
                None => bare_id,
            });
        let raw_child = &block.kind.name;
        let child_kind = self
            .namespace_aliases
            .get(raw_child)
            .map(String::as_str)
            .unwrap_or(raw_child);
        let raw_parent = parent_kind.unwrap_or("_root");
        let parent_name = self
            .namespace_aliases
            .get(raw_parent)
            .map(String::as_str)
            .unwrap_or(raw_parent);

        // E099: Check self-nesting depth limit
        if let Some(parent_schema) = self.get_schema(parent_name, None) {
            for cc in &parent_schema.child_constraints {
                if cc.kind == *child_kind {
                    if let Some(max_depth) = cc.max_depth {
                        let current_depth = kind_depths.get(child_kind).copied().unwrap_or(0);
                        if current_depth >= max_depth {
                            diagnostics.error_with_code(
                                format!(
                                    "block '{}' exceeds maximum nesting depth of {}",
                                    child_kind, max_depth,
                                ),
                                block.span,
                                "E099",
                            );
                        }
                    }
                }
            }
        }

        // E096: Check child's @parent constraint (across all schema variants for this name)
        if let Some(allowed) = self.check_parent_violation(child_kind, parent_name) {
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

        // E095: Check parent's @children constraint
        if let Some(parent_schema) = self.get_schema(parent_name, None) {
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
        if let Some(schema) = self.get_schema(&block.kind.name, parent_kind) {
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
                    // @text field is satisfied by text_content
                    let is_text_field = field.text && block.text_content.is_some();
                    // @inline(N) field is satisfied by inline_args (+ inline_id at index 0)
                    let effective_args_len =
                        block.inline_args.len() + if block.inline_id.is_some() { 1 } else { 0 };
                    let is_inline_satisfied = field
                        .inline_index
                        .is_some_and(|idx| idx < effective_args_len);

                    if !has_attr && !is_text_field && !is_inline_satisfied {
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
                        let known = schema.fields.iter().any(|f| f.name == attr.name.name)
                            || schema
                                .variants
                                .iter()
                                .any(|v| v.fields.iter().any(|f| f.name == attr.name.name));
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
                                    block_qid.as_deref(),
                                    diagnostics,
                                );
                            }

                            // M6: @symbol_set validation
                            if let Some(ref set_name) = field.symbol_set {
                                if !symbol_sets.set_exists(set_name) {
                                    diagnostics.error_with_code(
                                        format!(
                                            "referenced symbol_set '{}' does not exist (field '{}')",
                                            set_name, field.name
                                        ),
                                        attr.span,
                                        "E101",
                                    );
                                } else if let Value::Symbol(ref sym) = val {
                                    if !symbol_sets.contains(set_name, sym) {
                                        diagnostics.error_with_code(
                                            format!(
                                                "symbol ':{sym}' is not a member of symbol_set '{set_name}'",
                                            ),
                                            attr.span,
                                            "E100",
                                        );
                                    }
                                }
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

        // Child cardinality validation (E097/E098) and tagged variant validation
        if let Some(schema) = self.get_schema(&block.kind.name, parent_kind) {
            // E097/E098: @child cardinality constraints
            // Determine which constraints to use (variant may override)
            let active_constraints = &schema.child_constraints;
            let mut active_children_override: Option<&Option<Vec<String>>> = None;

            // Tagged variant validation
            if let Some(ref tag_name) = schema.tag_field {
                // Find the tag field value
                let tag_value = block_values
                    .and_then(|bv| bv.get(tag_name))
                    .cloned()
                    .or_else(|| {
                        block.body.iter().find_map(|item| {
                            if let BodyItem::Attribute(attr) = item {
                                if attr.name.name == *tag_name {
                                    return expr_to_value(&attr.value);
                                }
                            }
                            None
                        })
                    });

                if let Some(Value::String(ref tv)) = tag_value {
                    if let Some(variant) = schema.variants.iter().find(|v| v.tag_value == *tv) {
                        // Validate variant's required fields
                        for field in &variant.fields {
                            if field.required {
                                let has_attr = block.body.iter().any(|item| {
                                    matches!(item, BodyItem::Attribute(attr) if attr.name.name == field.name)
                                });
                                if !has_attr {
                                    diagnostics.error_with_code(
                                        format!(
                                            "missing required field '{}' in block '{}' (variant '{}')",
                                            field.name, block.kind.name, tv
                                        ),
                                        block.span,
                                        "E070",
                                    );
                                }
                            }
                        }

                        // Type check variant fields
                        for item in &block.body {
                            if let BodyItem::Attribute(attr) = item {
                                if let Some(field) =
                                    variant.fields.iter().find(|f| f.name == attr.name.name)
                                {
                                    let value = block_values
                                        .and_then(|bv| bv.get(&attr.name.name))
                                        .cloned()
                                        .or_else(|| expr_to_value(&attr.value));

                                    if let Some(ref val) = value {
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

                                        if let Some(ref constraints) = field.validate {
                                            validate_constraints(
                                                val,
                                                constraints,
                                                &field.name,
                                                attr.span,
                                                diagnostics,
                                            );
                                        }

                                        // symbol_set validation for variant fields
                                        if let Some(ref set_name) = field.symbol_set {
                                            if !symbol_sets.set_exists(set_name) {
                                                diagnostics.error_with_code(
                                                    format!(
                                                        "referenced symbol_set '{}' does not exist (field '{}')",
                                                        set_name, field.name
                                                    ),
                                                    attr.span,
                                                    "E101",
                                                );
                                            } else if let Value::Symbol(ref sym) = val {
                                                if !symbol_sets.contains(set_name, sym) {
                                                    diagnostics.error_with_code(
                                                        format!(
                                                            "symbol ':{sym}' is not a member of symbol_set '{set_name}'",
                                                        ),
                                                        attr.span,
                                                        "E100",
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // If variant has allowed_children, use it instead
                        if variant.allowed_children.is_some() {
                            active_children_override = Some(&variant.allowed_children);
                        }
                    }
                }
            }

            // Check cardinality constraints
            for cc in active_constraints {
                let count = block
                    .body
                    .iter()
                    .filter(
                        |item| matches!(item, BodyItem::Block(child) if child.kind.name == cc.kind),
                    )
                    .count();

                if let Some(min) = cc.min {
                    if count < min {
                        diagnostics.error_with_code(
                            format!(
                                "block '{}' requires at least {} '{}' child(ren), found {}",
                                block.kind.name, min, cc.kind, count,
                            ),
                            block.span,
                            "E097",
                        );
                    }
                }

                if let Some(max) = cc.max {
                    if count > max {
                        diagnostics.error_with_code(
                            format!(
                                "block '{}' allows at most {} '{}' child(ren), found {}",
                                block.kind.name, max, cc.kind, count,
                            ),
                            block.span,
                            "E098",
                        );
                    }
                }
            }

            // If variant overrides children, validate child containment with variant's list
            if let Some(Some(ref allowed)) = active_children_override {
                for item in &block.body {
                    if let BodyItem::Block(child) = item {
                        if !allowed.iter().any(|c| c == &child.kind.name) {
                            diagnostics.error_with_code(
                                format!(
                                    "block kind '{}' is not allowed as a child of '{}' (variant); allowed children: [{}]",
                                    child.kind.name,
                                    block.kind.name,
                                    allowed.join(", "),
                                ),
                                child.span,
                                "E095",
                            );
                        }
                    }
                }
            }
        }

        // Recursively validate nested blocks and tables
        let mut child_depths = kind_depths.clone();
        *child_depths.entry(block.kind.name.clone()).or_insert(0) += 1;
        for item in &block.body {
            match item {
                BodyItem::Block(child) => {
                    let child_values = block_values.and_then(|bv| resolve_block_values(child, bv));
                    self.validate_block(
                        child,
                        child_values.as_ref(),
                        block_ids,
                        Some(&block.kind.name),
                        block_qid.as_deref(),
                        &child_depths,
                        symbol_sets,
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

/// Parse a `@child("kind", min=N, max=N, max_depth=N)` decorator.
fn parse_child_decorator(d: &Decorator) -> Option<ChildConstraint> {
    let kind = d.args.first().and_then(|arg| match arg {
        DecoratorArg::Positional(Expr::StringLit(s)) => Some(string_lit_to_string(s)),
        _ => None,
    })?;
    let mut min = None;
    let mut max = None;
    let mut max_depth = None;
    for arg in &d.args {
        if let DecoratorArg::Named(name, expr) = arg {
            let val = expr_to_value(expr).and_then(|v| match v {
                Value::Int(i) => Some(i as usize),
                _ => None,
            });
            match name.name.as_str() {
                "min" => min = val,
                "max" => max = val,
                "max_depth" => max_depth = val,
                _ => {}
            }
        }
    }
    Some(ChildConstraint {
        kind,
        min,
        max,
        max_depth,
        span: d.span,
    })
}

/// Resolve a list of schema fields (shared by base schema and variants).
fn resolve_fields(
    fields: &[SchemaField],
    text_field: &mut Option<String>,
    diagnostics: &mut DiagnosticBag,
) -> Vec<ResolvedField> {
    let mut result = Vec::new();
    for field in fields {
        let field_doc = get_decorator_string_arg(&field.decorators_before, "doc")
            .or_else(|| get_decorator_string_arg(&field.decorators_after, "doc"));
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
        let inline_index = get_decorator_int_arg(&field.decorators_before, "inline")
            .or_else(|| get_decorator_int_arg(&field.decorators_after, "inline"))
            .map(|n| n as usize);
        let is_text = has_decorator(&field.decorators_before, "text")
            || has_decorator(&field.decorators_after, "text");

        if is_text {
            if let Some(text_field) = text_field {
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
                if !text_field.is_empty() {
                    diagnostics.error_with_code(
                        "schema may have at most one @text field".to_string(),
                        field.span,
                        "E094",
                    );
                } else {
                    *text_field = field.name.name.clone();
                }
            }
        }

        let symbol_set = get_decorator_string_arg(&field.decorators_before, "symbol_set")
            .or_else(|| get_decorator_string_arg(&field.decorators_after, "symbol_set"));

        result.push(ResolvedField {
            name: field.name.name.clone(),
            doc: field_doc,
            type_expr: field.type_expr.clone(),
            required,
            default,
            validate,
            ref_target,
            id_pattern,
            text: is_text,
            inline_index,
            symbol_set,
            span: field.span,
        });
    }
    result
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

pub(crate) fn get_decorator_int_arg(decorators: &[Decorator], name: &str) -> Option<i64> {
    decorators
        .iter()
        .find(|d| d.name.name == name)
        .and_then(|d| {
            d.args.first().and_then(|arg| match arg {
                DecoratorArg::Positional(Expr::IntLit(n, _)) => Some(*n),
                _ => None,
            })
        })
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
        Expr::SymbolLit(name, _) => Some(Value::Symbol(name.clone())),
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
        Value::BigInt(_) => "bigint",
        Value::Float(_) => "float",
        Value::Bool(_) => "bool",
        Value::Null => "null",
        Value::Identifier(_) => "identifier",
        Value::List(_) => "list",
        Value::Map(_) => "map",
        Value::Set(_) => "set",
        Value::Symbol(_) => "symbol",
        Value::BlockRef(_) => "block",
        Value::Function(_) => "function",
        Value::Date(_) => "date",
        Value::Duration(_) => "duration",
        Value::Pattern(_) => "pattern",
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
    block_ids: &BlockIdIndex,
    current_qualified_id: Option<&str>,
    diagnostics: &mut DiagnosticBag,
) {
    let ref_id = match value {
        Value::String(s) => Some(s.clone()),
        Value::Identifier(s) => Some(s.clone()),
        _ => None,
    };
    if let Some(ref_id) = ref_id {
        // Resolve the ref ID using scoped resolution:
        // 1. Handle `../` relative prefix
        // 2. Try as peer in current scope (prepend parent's qualified ID)
        // 3. Try as absolute qualified ID
        // 4. Try as bare ID in kind's ID list (original behavior)
        let resolved = resolve_ref_id(&ref_id, current_qualified_id, block_ids, target_kind);
        if !resolved {
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

/// Resolve a ref ID using scoped lookup.
///
/// Returns `true` if the ID resolves to a valid block.
fn resolve_ref_id(
    ref_id: &str,
    current_qualified_id: Option<&str>,
    block_ids: &BlockIdIndex,
    target_kind: &str,
) -> bool {
    // Step 1: Handle `../` relative prefix.
    if ref_id.starts_with("../") {
        let mut scope_parts: Vec<&str> = current_qualified_id
            .map(|q| q.split('.').collect())
            .unwrap_or_default();
        let mut rest = ref_id;
        while rest.starts_with("../") {
            rest = &rest[3..];
            scope_parts.pop(); // go up one level
        }
        // Construct the absolute qualified ID.
        let absolute = if scope_parts.is_empty() {
            rest.to_string()
        } else {
            format!("{}.{}", scope_parts.join("."), rest)
        };
        // Check qualified ID set.
        if block_ids.qualified.contains(&absolute) {
            return true;
        }
        // Also try as bare ID in kind list.
        if let Some(ids) = block_ids.by_kind.get(target_kind) {
            if ids.contains(&absolute) {
                return true;
            }
        }
        return false;
    }

    // Step 2: Try as bare ID in kind's ID list (original behavior).
    if let Some(ids) = block_ids.by_kind.get(target_kind) {
        if ids.contains(&ref_id.to_string()) {
            return true;
        }
    }

    // Step 3: Try as peer in current scope (prepend parent's qualified path).
    if let Some(qid) = current_qualified_id {
        // Parent scope is the qualified_id with the last segment removed.
        let parent_path = qid.rsplit_once('.').map(|(p, _)| p);
        let peer_qid = match parent_path {
            Some(p) => format!("{}.{}", p, ref_id),
            None => ref_id.to_string(), // already at root level
        };
        if block_ids.qualified.contains(&peer_qid) {
            return true;
        }
    }

    // Step 4: Try as absolute qualified path.
    if block_ids.qualified.contains(ref_id) {
        return true;
    }

    false
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

/// Collected block IDs: kind-grouped bare IDs and a flat set of all qualified IDs.
struct BlockIdIndex {
    /// Maps block kind → list of bare IDs (for `@ref("kind")` validation).
    by_kind: HashMap<String, Vec<String>>,
    /// Set of all qualified IDs (e.g. `"alpha"`, `"alpha.http"`).
    qualified: HashSet<String>,
}

/// Collect all block IDs from the document.
fn collect_block_ids(items: &[DocItem]) -> BlockIdIndex {
    let mut by_kind: HashMap<String, Vec<String>> = HashMap::new();
    let mut qualified = HashSet::new();
    for item in items {
        if let DocItem::Body(BodyItem::Block(block)) = item {
            collect_block_ids_recursive(block, None, &mut by_kind, &mut qualified);
        }
    }
    BlockIdIndex { by_kind, qualified }
}

fn collect_block_ids_recursive(
    block: &Block,
    parent_path: Option<&str>,
    by_kind: &mut HashMap<String, Vec<String>>,
    qualified: &mut HashSet<String>,
) {
    let child_path = if let Some(ref inline_id) = block.inline_id {
        if let Some(id_str) = inline_id_to_string(inline_id) {
            by_kind
                .entry(block.kind.name.clone())
                .or_default()
                .push(id_str.clone());

            let qid = match parent_path {
                Some(p) => format!("{}.{}", p, id_str),
                None => id_str,
            };
            qualified.insert(qid.clone());
            Some(qid)
        } else {
            parent_path.map(|s| s.to_string())
        }
    } else {
        parent_path.map(|s| s.to_string())
    };

    for item in &block.body {
        if let BodyItem::Block(child) = item {
            collect_block_ids_recursive(child, child_path.as_deref(), by_kind, qualified);
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
    use crate::lang::span::{FileId, Span};
    use crate::lang::trivia::Trivia;

    fn dummy_span() -> Span {
        Span::new(FileId(0), 0, 1)
    }

    fn make_string_lit(s: &str) -> StringLit {
        StringLit {
            parts: vec![StringPart::Literal(s.to_string())],
            heredoc: None,
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
            variants: vec![],
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
        let s = reg.get_schema("service", None).unwrap();
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
        assert!(reg.get_schema("service", None).unwrap().open);
    }

    #[test]
    fn string_lit_to_string_works() {
        let s = StringLit {
            parts: vec![StringPart::Literal("hello".to_string())],
            heredoc: None,
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
            arrow_target: None,
            inline_args: vec![],
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
        let s = reg.get_schema("readme", None).unwrap();
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
                TypeExpr::I64(dummy_span()),
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
        reg.validate(&doc, &values, &SymbolSetRegistry::new(), &mut diags);

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
            assign_op: crate::lang::ast::AssignOp::Assign,
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
        reg.validate(&doc, &values, &SymbolSetRegistry::new(), &mut diags);

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
        reg.validate(&doc, &values, &SymbolSetRegistry::new(), &mut diags);

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
            variants: vec![],
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
        reg.validate(
            &doc,
            &IndexMap::new(),
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
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
        reg.validate(
            &doc,
            &IndexMap::new(),
            &SymbolSetRegistry::new(),
            &mut diags,
        );
        let e096: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert_eq!(e096.len(), 1);
    }
}
