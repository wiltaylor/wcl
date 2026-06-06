//! Per-block schema validation.
//!
//! Walks one [`Block`](super::Block) against its declared `@block` / `@table`
//! schema and produces a list of [`EvalError::SchemaViolation`]s for every
//! deviation: unknown fields, disallowed nested-block kinds, missing or
//! over-quota children, table-row arity mismatches.
//!
//! Whole-block `@schemaless` short-circuits all checks. Per-field
//! `@schemaless` exempts just that field from the membership check.

use std::collections::{HashMap, HashSet};

use crate::ast;
use crate::error::EvalError;

use super::cells::ItemCellKind;
use super::{Block, BuiltinDecorator, DeclName};

pub(super) fn has_schemaless(decorators: &[ast::Decorator]) -> bool {
    let name = BuiltinDecorator::Schemaless.as_str();
    decorators
        .iter()
        .any(|d| d.name.len() == 1 && d.name[0] == name)
}

/// Validate every `Item::Connection` in a flat item list against the
/// declared `connection` schemas in the document. Used at both the
/// document root and inside `@block` bodies.
pub(super) fn validate_connection_stmts(
    doc: &crate::doc::Document,
    items: &[ast::Item],
    scope: &super::scope::Scope<'_>,
) -> Vec<EvalError> {
    use crate::error::SchemaViolationKind as Kind;
    let mut errs = Vec::new();
    for item in items {
        let ast::Item::Connection(stmt) = item else {
            continue;
        };
        let lhs = doc.resolve_connection_operand(scope, &stmt.lhs);
        let rhs = doc.resolve_connection_operand(scope, &stmt.rhs);
        if lhs.is_none() || rhs.is_none() {
            // An operand that doesn't name a literal block is a typo for a
            // static connection — but under a `@dynamic` connection it may
            // be an id generated at render time (`wdoc_repeater` /
            // `wdoc_component`), which we can't resolve statically. Suppress
            // the error only when such a connection plausibly accepts this
            // statement; otherwise flag it as before. (Mirrors the original
            // single-operand reporting: source is reported first.)
            if !dynamic_connection_admits(doc, &lhs, &rhs) {
                if lhs.is_none() {
                    errs.push(EvalError::schema_violation(
                        Kind::UnknownConnectionOperand,
                        format!(
                            "connection source '{}' does not name a block in scope",
                            stmt.lhs
                        ),
                        stmt.lhs_span,
                    ));
                } else {
                    errs.push(EvalError::schema_violation(
                        Kind::UnknownConnectionOperand,
                        format!(
                            "connection destination '{}' does not name a block in scope",
                            stmt.rhs
                        ),
                        stmt.rhs_span,
                    ));
                }
            }
            continue;
        }
        let (_, lhs_kind) = lhs.unwrap();
        let (_, rhs_kind) = rhs.unwrap();
        let Some(lhs_decl) = doc.block_schema(&lhs_kind) else {
            continue; // block kind without a schema; UnregisteredKind already fires.
        };
        let Some(rhs_decl) = doc.block_schema(&rhs_kind) else {
            continue;
        };
        let mut matches: Vec<crate::doc::ConnectionDecl<'_>> = Vec::new();
        for decl in doc.connection_decls() {
            let src_fqn = decl_type_fqn(doc, decl.source_type());
            let dst_fqn = decl_type_fqn(doc, decl.destination_type());
            if crate::doc::connection_type_matches(&lhs_decl, src_fqn.as_deref())
                && crate::doc::connection_type_matches(&rhs_decl, dst_fqn.as_deref())
            {
                matches.push(decl);
            }
        }
        let chosen = match matches.len() {
            0 => {
                let lhs_ty = lhs_decl.name_segments().join(".");
                let rhs_ty = rhs_decl.name_segments().join(".");
                errs.push(EvalError::schema_violation(
                    Kind::UnknownConnection,
                    format!("no connection schema accepts '{lhs_ty} -> {rhs_ty}'",),
                    stmt.span,
                ));
                continue;
            }
            1 => matches.into_iter().next().unwrap(),
            _ => {
                let lhs_ty = lhs_decl.name_segments().join(".");
                let rhs_ty = rhs_decl.name_segments().join(".");
                let names: Vec<String> = matches
                    .iter()
                    .map(|m| m.name_segments().join("."))
                    .collect();
                errs.push(EvalError::schema_violation(
                    Kind::AmbiguousConnection,
                    format!(
                        "connection '{lhs_ty} -> {rhs_ty}' matches multiple schemas: {}",
                        names.join(", ")
                    ),
                    stmt.span,
                ));
                continue;
            }
        };
        if let Some(kind_name) = &stmt.kind {
            let kind_ok = chosen.kind_set().map(|s| s.has(kind_name)).unwrap_or(false);
            if !kind_ok {
                errs.push(EvalError::schema_violation(
                    Kind::UnknownConnectionKind,
                    format!(
                        "connection kind ':{}' is not a member of '{}'",
                        kind_name,
                        chosen.kind_set_path().join(".")
                    ),
                    stmt.kind_span.unwrap_or(stmt.span),
                ));
            }
        }
    }
    errs
}

fn decl_type_fqn(doc: &crate::doc::Document, t: &crate::value::TypeRef) -> Option<String> {
    doc.resolve_type_fqn(t)
}

/// `true` when some `@dynamic` connection schema plausibly accepts a
/// statement with an unresolved operand: each *resolved* operand's block
/// type must satisfy the schema's corresponding role, while an unresolved
/// operand is treated as a wildcard (it may name a render-time-generated
/// id). Gates suppression of `UnknownConnectionOperand` in
/// [`validate_connection_stmts`].
type ResolvedOperand = Option<(crate::value::Value, String)>;

fn dynamic_connection_admits(
    doc: &crate::doc::Document,
    lhs: &ResolvedOperand,
    rhs: &ResolvedOperand,
) -> bool {
    let role_ok = |operand: &ResolvedOperand, fqn: Option<&str>| match operand {
        // Wildcard: an unresolved operand can't be type-checked.
        None => true,
        Some((_, kind)) => doc
            .block_schema(kind)
            .is_some_and(|d| crate::doc::connection_type_matches(&d, fqn)),
    };
    doc.connection_decls()
        .filter(|d| d.is_dynamic())
        .any(|decl| {
            let src_fqn = decl_type_fqn(doc, decl.source_type());
            let dst_fqn = decl_type_fqn(doc, decl.destination_type());
            role_ok(lhs, src_fqn.as_deref()) && role_ok(rhs, dst_fqn.as_deref())
        })
}

/// Validate a `wdoc_component` instance against its definition's slots:
/// every instance field must name a declared slot, and every slot
/// without a `default` must be supplied. (The definition itself —
/// only `wdoc_slot`/`wdoc_body` children, one body — is validated by the
/// generic schema on the `wdoc_component` block.)
pub(super) fn validate_component_instance(
    doc: &crate::doc::Document,
    instance: &Block<'_>,
) -> Vec<EvalError> {
    use crate::error::SchemaViolationKind as Kind;
    use crate::value::Value;
    let mut errs = Vec::new();
    let Some(def) = doc.component_def(instance.kind()) else {
        return errs;
    };
    let slot_name = |s: &Block<'_>| -> Option<String> {
        match s.labels().ok()?.into_iter().next()? {
            Value::Identifier(n) | Value::Utf8(n) | Value::Ascii(n) => Some(n),
            _ => None,
        }
    };
    let slots: Vec<Block<'_>> = def.blocks().filter(|b| b.kind() == "wdoc_slot").collect();
    let slot_names: std::collections::HashSet<String> =
        slots.iter().filter_map(slot_name).collect();

    // Every instance field must be a declared slot.
    for f in instance.fields() {
        if has_schemaless(&f.ast.decorators) {
            continue;
        }
        if !slot_names.contains(f.name()) {
            errs.push(EvalError::schema_violation_named(
                Kind::UnknownField,
                format!(
                    "field '{}' is not a slot of component '{}'",
                    f.name(),
                    instance.kind()
                ),
                f.name(),
                f.span(),
            ));
        }
    }
    // Every slot without a default must be supplied.
    for s in &slots {
        let Some(name) = slot_name(s) else { continue };
        let has_default = s.field("default").is_some();
        if !has_default && instance.field(&name).is_none() {
            errs.push(EvalError::schema_violation(
                Kind::MissingRequired,
                format!(
                    "component '{}' is missing required slot '{}'",
                    instance.kind(),
                    name
                ),
                instance.span(),
            ));
        }
    }
    errs
}

pub(super) fn compute_schema_errors<'a>(block: &Block<'a>) -> Vec<EvalError> {
    use crate::error::SchemaViolationKind as Kind;
    let mut errs = Vec::new();

    // Whole-block opt-out — `@schemaless service web { … }` skips
    // every check inside the block (its contents are unrestricted).
    if has_schemaless(&block.ast.decorators) {
        return errs;
    }

    // Connection statements live alongside fields and nested blocks;
    // validate them regardless of whether the surrounding type has a
    // `@connections(...)` field, so an unwired statement still
    // surfaces.
    let scope = block.child_scope();
    // Validate the block's own connection statements and any spliced in
    // by an in-block `import`, so imported connections are checked just
    // like inline ones.
    for src in block.realize_and_sources() {
        errs.extend(validate_connection_stmts(block.doc, src.items, &scope));
    }

    let Some(schema) = block.schema() else {
        // A `wdoc_component` instance has no `@block` schema of its own —
        // it's validated against its definition's slots instead.
        if block.doc.is_component_kind(block.kind()) {
            errs.extend(validate_component_instance(block.doc, block));
            return errs;
        }
        // Strict mode: a block whose kind has no `@block`/`@table`
        // declaration is itself the violation.
        errs.push(EvalError::schema_violation(
            Kind::UnregisteredKind,
            format!(
                "block kind '{}' has no @block or @table declaration",
                block.kind()
            ),
            block.span(),
        ));
        return errs;
    };

    // Field-membership: every literal `Item::Field` inside this
    // block must be named by the schema. `@schemaless` on a field
    // exempts that specific field.
    let declared_field_names: HashSet<String> =
        schema.fields().map(|f| f.name().to_string()).collect();
    for f in block.fields() {
        if has_schemaless(&f.ast.decorators) {
            continue;
        }
        if !declared_field_names.contains(f.name()) {
            errs.push(EvalError::schema_violation_named(
                Kind::UnknownField,
                format!(
                    "field '{}' is not declared by schema '{}'",
                    f.name(),
                    schema.name()
                ),
                f.name(),
                f.span(),
            ));
        }
    }

    // Surface dispatch failures for union-typed @child / @children
    // fields. Both block-to-variant and table-row-to-variant report
    // through this channel — the typed_field accessor silently skips
    // failures and depends on schema_errors() to expose them.
    for declared in schema.fields() {
        if let Some(crate::doc::ChildKind::Union(union)) = declared.children_kind_or_union() {
            for (kind, blk) in block.union_children_blocks(declared.name()) {
                let result = match kind {
                    crate::doc::UnionChildKind::Nested => {
                        crate::doc::variant_dispatch::block_to_variant(block.doc, &blk, union)
                    }
                    crate::doc::UnionChildKind::TableRow => {
                        crate::doc::variant_dispatch::table_row_to_variant(block.doc, &blk, union)
                    }
                };
                if let Err(e) = result {
                    errs.push(e);
                }
            }
        }
        if let Some(crate::doc::ChildKind::Union(union)) = declared.child_kind_or_union() {
            // Single-block @child: the first matching nested block is
            // used; report no-match if every nested block fails.
            let mut had_match = false;
            for (kind, blk) in block.union_children_blocks(declared.name()) {
                if matches!(kind, crate::doc::UnionChildKind::Nested)
                    && crate::doc::variant_dispatch::block_to_variant(block.doc, &blk, union)
                        .is_ok()
                {
                    had_match = true;
                    break;
                }
            }
            if !had_match {
                errs.push(EvalError::schema_violation(
                    Kind::VariantNoMatch,
                    format!(
                        "no nested block matches union '{}' for field '{}'",
                        union.ast.name.join("."),
                        declared.name(),
                    ),
                    block.span(),
                ));
            }
        }
    }

    // Value-vs-declared-type. Two paths:
    //   1. Union-typed fields: must hold a variant of the declared union.
    //   2. Everything else: run the conservative `value_matches_type_ref`
    //      shared with the dispatch matcher. Tensor / function /
    //      reference types stay permissive.
    for declared in schema.fields() {
        let Some(literal_field) = block.field(declared.name()) else {
            continue;
        };
        if has_schemaless(&literal_field.ast.decorators) {
            continue;
        }
        // A `wdoc_repeater`'s `each` is its dynamic input list: the schema
        // types it `list<WdocItem>` only to name the list shape, and its
        // elements are read at render time and intentionally untyped (the
        // components stdlib documents this). The in-page generator bypass
        // below already skips validating a repeater's own fields; this
        // keeps a document-level / TOC repeater (a recognised schema child,
        // so `schema_errors` runs on it directly) consistent.
        if declared.name() == "each" && block.kind() == "wdoc_repeater" {
            continue;
        }
        let Ok(value) = literal_field.value() else {
            continue;
        };

        // Computed-children splice: a `@children(kind)` / `@child(kind)`
        // slot authored as `field = <list expr>` instead of nested
        // blocks. The value is a list spliced into child blocks (see
        // `Block::computed_children`); its elements are validated as
        // nested blocks by the kind / disallowed-child walk below, so the
        // scalar value-vs-type check doesn't apply (a list of records
        // never "matches" a `list<BlockKind>` value type). A `@children`
        // slot still requires a list value.
        let is_children = declared.children_kind_or_union().is_some();
        let is_child = declared.child_kind_or_union().is_some();
        if is_children || is_child {
            if is_children
                && !matches!(
                    value,
                    crate::value::Value::List(_) | crate::value::Value::None
                )
            {
                errs.push(EvalError::schema_violation(
                    Kind::FieldTypeMismatch,
                    format!(
                        "field '{}' is a @children slot spliced from an expression, \
                         so it must be a list, but the value is {}",
                        literal_field.name(),
                        value.type_name(),
                    ),
                    literal_field.span(),
                ));
            }
            continue;
        }

        // Union path — preserved verbatim.
        if let crate::value::TypeRef::Named(path) = declared.type_ref()
            && let Some(union_decl) = block.doc.union_decl(&path.join("."))
        {
            let expected_fqn = union_decl.ast.name.clone();
            if let crate::value::Value::Variant { union, variant, .. } = value
                && union != &expected_fqn
            {
                errs.push(EvalError::schema_violation(
                    Kind::VariantUnionMismatch,
                    format!(
                        "field '{}' declared as union '{}' but value is {}::{}",
                        literal_field.name(),
                        expected_fqn.join("."),
                        union.join("."),
                        variant,
                    ),
                    literal_field.span(),
                ));
            } else if let crate::value::Value::Record { .. } = value {
                // A bare record that `Field::value` couldn't coerce to a
                // variant (e.g. via a `@schemaless` bypass) — flag it
                // rather than silently accepting an un-inferred record.
                errs.push(EvalError::schema_violation(
                    Kind::VariantNoMatch,
                    format!(
                        "field '{}' declared as union '{}' but value is an \
                         un-inferred record (no variant matches its shape)",
                        literal_field.name(),
                        expected_fqn.join("."),
                    ),
                    literal_field.span(),
                ));
            }
            continue;
        }

        // Generic value-vs-type check for non-union typed fields.
        if !crate::doc::value_matches_type_ref(value, declared.type_ref()) {
            errs.push(EvalError::schema_violation(
                Kind::FieldTypeMismatch,
                format!(
                    "field '{}' declared as {} but value is {}",
                    literal_field.name(),
                    declared.type_ref(),
                    value.type_name(),
                ),
                literal_field.span(),
            ));
        }
    }

    // 0. Table row-form validation: if this block's schema is a
    // `@table`, its labels are the row's column values and must
    // match the schema field count.
    if block.doc.table_schema(block.kind()).is_some() {
        let label_count = block.labels().map(|v| v.len()).unwrap_or(0);
        let field_count = schema.fields().count();
        if label_count < field_count {
            errs.push(EvalError::schema_violation(
                Kind::ChildrenTooFew,
                format!(
                    "table row for '{}' has {} values, expected {}",
                    block.kind(),
                    label_count,
                    field_count
                ),
                block.span(),
            ));
        } else if label_count > field_count {
            errs.push(EvalError::schema_violation(
                Kind::ChildrenTooMany,
                format!(
                    "table row for '{}' has {} values, expected {}",
                    block.kind(),
                    label_count,
                    field_count
                ),
                block.span(),
            ));
        }
        // Tables don't carry nested-block children themselves, so the
        // rest of the validation doesn't apply.
        return errs;
    }

    // 1. Gather per-kind counts of nested blocks. Both literal
    // `Item::Block` entries and synthesised `Item::Table` rows
    // contribute. For synth rows the kind comes from the parent
    // schema's `@children(K)` decoration on the matching field.
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut total: usize = 0;
    for nested in block.blocks() {
        *counts.entry(nested.kind().to_string()).or_insert(0) += 1;
        total += 1;
    }
    if let ItemCellKind::Block { synth_rows, .. } = &block.cells.kind {
        for sr in synth_rows {
            // Find the schema field matching the table header's
            // field_name and read its @children(kind).
            if let Some(field) = schema.field(&sr.field_name)
                && let Some(kind) = field.children_block_kind_str()
            {
                *counts.entry(kind.to_string()).or_insert(0) += 1;
                total += 1;
            }
        }
    }

    // 2. Build the allowed-child set: union of @child/@children kinds
    // across this type's fields.
    let allowed = schema.allowed_child_kinds();
    // Also collect any union types referenced by @child(SomeUnion) /
    // @children(SomeUnion). A nested block that doesn't match an
    // allowed *kind* is still legal if it structurally matches a
    // variant of one of these unions.
    let union_slots: Vec<crate::doc::UnionDecl<'_>> = schema
        .fields()
        .filter_map(|f| {
            f.children_kind_or_union()
                .and_then(|k| k.as_union().copied())
                .or_else(|| f.child_kind_or_union().and_then(|k| k.as_union().copied()))
        })
        .collect();
    // Interface slots from `@child(SomeInterface)` / `@children(SomeInterface)`.
    // A nested block is legal here if its `@block` type's `extends`
    // chain transitively contains the interface.
    let interface_slots: Vec<crate::doc::InterfaceDecl<'_>> = schema
        .fields()
        .filter_map(|f| {
            f.children_kind_or_union()
                .and_then(|k| k.as_interface().copied())
                .or_else(|| {
                    f.child_kind_or_union()
                        .and_then(|k| k.as_interface().copied())
                })
        })
        .collect();

    // 3. Per-kind: any nested block whose kind isn't in `allowed`
    // AND which doesn't match any union variant is a DisallowedChild.
    for nested in block.blocks() {
        if allowed.iter().any(|k| k == nested.kind()) {
            continue;
        }
        let matches_union = union_slots.iter().any(|u| {
            crate::doc::variant_dispatch::block_to_variant(block.doc, &nested, *u).is_ok()
        });
        if matches_union {
            continue;
        }
        let matches_interface = !interface_slots.is_empty()
            && block.doc.block_schema(nested.kind()).is_some_and(|t| {
                interface_slots
                    .iter()
                    .any(|iface| t.is_descendant_of(&iface.full_name()))
            });
        if matches_interface {
            continue;
        }
        // Generators — `wdoc_repeater` / `wdoc_content` and user-defined
        // `wdoc_component` instances — are context-polymorphic: they emit
        // whatever their body contains (WdocBlocks in a page, SvgBlocks in
        // a diagram), so accept them in any interface `@children` slot
        // (`WdocBlock` or `SvgBlock`). A component instance also has its
        // slot fields validated here, since the validator is shallow.
        let is_generator = matches!(nested.kind(), "wdoc_repeater" | "wdoc_content")
            || block.doc.is_component_kind(nested.kind());
        if !interface_slots.is_empty() && is_generator {
            if block.doc.is_component_kind(nested.kind()) {
                errs.extend(validate_component_instance(block.doc, &nested));
            }
            continue;
        }
        errs.push(EvalError::schema_violation_named(
            Kind::DisallowedChild,
            format!(
                "block kind '{}' is not allowed inside '{}'",
                nested.kind(),
                block.kind()
            ),
            nested.kind(),
            nested.span(),
        ));
    }

    // 4. `max_children = N` on @block: total nested-block count ≤ N.
    if let Some(maxn) = schema.max_children()
        && (total as u64) > maxn
    {
        errs.push(EvalError::schema_violation(
            Kind::BlockChildrenOverflow,
            format!(
                "block '{}' contains {} children (max allowed: {})",
                block.kind(),
                total,
                maxn
            ),
            block.span(),
        ));
    }

    // 4b. `required_children = ["kind", ...]` on @block: each listed
    //     kind must appear at least once.
    for required in schema.required_children() {
        if *counts.get(&required).unwrap_or(&0) == 0 {
            errs.push(EvalError::schema_violation(
                Kind::MissingRequired,
                format!(
                    "block '{}' is missing required child kind '{}'",
                    block.kind(),
                    required
                ),
                block.span(),
            ));
        }
    }

    // 5. Field-level cardinality (@child / @children).
    for f in schema.fields() {
        if let Some(kind) = f.child_block_kind() {
            // @child(K): expect exactly 1 (or 0..1 if field is optional).
            let count = *counts.get(&kind).unwrap_or(&0);
            if count == 0 && !f.optional() {
                errs.push(EvalError::schema_violation(
                    Kind::MissingRequired,
                    format!(
                        "block '{}' is missing required child '{}' (for field '{}')",
                        block.kind(),
                        kind,
                        f.name()
                    ),
                    block.span(),
                ));
            } else if count > 1 {
                errs.push(EvalError::schema_violation(
                    Kind::ChildrenTooMany,
                    format!(
                        "field '{}' expects a single '{}' child, found {}",
                        f.name(),
                        kind,
                        count
                    ),
                    block.span(),
                ));
            }
        } else if let Some(kind) = f.children_block_kind() {
            let count = *counts.get(&kind).unwrap_or(&0) as u64;
            if let Some(min) = f.children_min()
                && count < min
            {
                errs.push(EvalError::schema_violation(
                    Kind::ChildrenTooFew,
                    format!(
                        "field '{}' requires at least {} '{}' children, found {}",
                        f.name(),
                        min,
                        kind,
                        count
                    ),
                    block.span(),
                ));
            }
            if let Some(maxn) = f.children_max()
                && count > maxn
            {
                errs.push(EvalError::schema_violation(
                    Kind::ChildrenTooMany,
                    format!(
                        "field '{}' allows at most {} '{}' children, found {}",
                        f.name(),
                        maxn,
                        kind,
                        count
                    ),
                    block.span(),
                ));
            }
        }
    }

    errs
}
