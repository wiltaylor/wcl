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
use super::{Block, DeclName};

pub(super) fn has_schemaless(decorators: &[ast::Decorator]) -> bool {
    decorators
        .iter()
        .any(|d| d.name.len() == 1 && d.name[0] == "schemaless")
}

pub(super) fn compute_schema_errors<'a>(block: &Block<'a>) -> Vec<EvalError> {
    use crate::error::SchemaViolationKind as Kind;
    let mut errs = Vec::new();

    // Whole-block opt-out — `@schemaless service web { … }` skips
    // every check inside the block (its contents are unrestricted).
    if has_schemaless(&block.ast.decorators) {
        return errs;
    }

    let Some(schema) = block.schema() else {
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
            errs.push(EvalError::schema_violation(
                Kind::UnknownField,
                format!(
                    "field '{}' is not declared by schema '{}'",
                    f.name(),
                    schema.name()
                ),
                f.span(),
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

    // 3. Per-kind: any nested block whose kind isn't in `allowed`
    // is a DisallowedChild.
    for nested in block.blocks() {
        if !allowed.iter().any(|k| k == nested.kind()) {
            errs.push(EvalError::schema_violation(
                Kind::DisallowedChild,
                format!(
                    "block kind '{}' is not allowed inside '{}'",
                    nested.kind(),
                    block.kind()
                ),
                nested.span(),
            ));
        }
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
