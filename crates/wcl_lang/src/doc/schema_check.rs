//! Per-block schema validation.
//!
//! Walks one [`Block`](super::Block) against its declared `@block` / `@table`
//! schema and produces a list of [`EvalError::SchemaViolation`]s for every
//! deviation: unknown fields, disallowed nested-block kinds, missing or
//! over-quota children, table-row arity mismatches.
//!
//! Bare `@schemaless` on a block short-circuits all checks. The narrow
//! `@schemaless(annotations = true)` form exempts only decorators. Bare
//! `@schemaless` on a field exempts that field from membership checking.

use std::collections::{HashMap, HashSet};

use crate::ast;
use crate::error::EvalError;

use crate::value::Value;

use super::cells::ItemCellKind;
use super::{Block, BuiltinDecorator, DeclName, Document, TypeField};

/// Report a decorator argument whose value does not fit the slot its
/// schema declares.
fn decorator_slot_value_error(
    doc: &Document,
    decorator: &super::Decorator<'_>,
    slot: TypeField<'_>,
    value: &Value,
    span: ast::Span,
) -> Option<EvalError> {
    use crate::error::SchemaViolationKind as Kind;

    // A schemaless slot declares only the argument's name/cardinality; the
    // host owns its value type (for example, `@default` accepts any value).
    if has_schemaless(&slot.ast.decorators) {
        return None;
    }

    let declared_type = doc.resolve_alias_in(slot.type_ref(), slot.file_ns);
    if !crate::doc::types::value_matches_declared(value, &declared_type, slot.optional()) {
        return Some(EvalError::schema_violation_named(
            Kind::FieldTypeMismatch,
            format!(
                "argument for decorator '@{}' slot '{}' is declared as {} but the value is {}",
                decorator.full_name(),
                slot.name(),
                slot.type_ref(),
                value.type_name(),
            ),
            slot.name(),
            span,
        ));
    }
    if let Some(error) = crate::doc::types::symbol_set_membership_error_in(
        doc,
        &declared_type,
        value,
        slot.name(),
        span,
        slot.file_ns,
    ) {
        return Some(error);
    }
    constraint_violation(
        doc,
        &slot.ast.decorators,
        slot.type_ref(),
        slot.file_ns,
        value,
    )
    .map(|message| {
        EvalError::schema_violation_named(
            Kind::ConstraintViolation,
            format!(
                "argument for decorator '@{}' slot '{}': {message}",
                decorator.full_name(),
                slot.name(),
            ),
            slot.name(),
            span,
        )
    })
}

/// Check the values written into a declared decorator's positional slots.
/// Slot numbers come from `@inline(N)`; declaration order is not positional.
/// Undeclared decorators are left to the declaration validator.
pub(super) fn decorator_argument_errors(
    doc: &Document,
    decorator: &super::Decorator<'_>,
) -> Vec<EvalError> {
    use crate::error::SchemaViolationKind as Kind;

    let Some(schema) = decorator.schema() else {
        return Vec::new();
    };
    let Ok(values) = decorator.positional() else {
        return Vec::new();
    };
    let mut errors = Vec::new();
    let named_slots: HashSet<&str> = decorator.named().map(|arg| arg.name()).collect();
    let has_structural_arguments = values.iter().enumerate().any(|(index, _)| {
        schema
            .fields()
            .all(|field| field.inline_slot() != Some(index as u64))
    }) || decorator
        .named()
        .any(|argument| schema.field(argument.name()).is_none());
    let structural_union_slot = if has_structural_arguments {
        let mut candidates = schema.fields().filter(|slot| {
            let filled_by_name = named_slots.contains(slot.name());
            let filled_positionally = slot
                .inline_slot()
                .is_some_and(|index| (index as usize) < values.len());
            !filled_by_name
                && !filled_positionally
                && matches!(slot.resolved_type(), super::ResolvedType::Union(_))
        });
        let candidate = candidates.next();
        if candidates.next().is_none() {
            candidate
        } else {
            None
        }
    } else {
        None
    };
    if let Some(slot) = structural_union_slot
        && let Some(Err(error)) = decorator.resolved_arg_value(slot.name())
    {
        errors.push(error);
    }
    for (index, value) in values.iter().enumerate() {
        // Parsed and synthetic decorators keep these vectors aligned. Falling
        // back to the decorator span makes a manually mutated AST noisy rather
        // than silently dropping validation through a truncating `zip`.
        let span = decorator
            .positional_spans()
            .get(index)
            .copied()
            .unwrap_or_else(|| decorator.span());
        let Some(slot) = schema
            .fields()
            .find(|field| field.inline_slot() == Some(index as u64))
        else {
            if structural_union_slot.is_some() {
                continue;
            }
            errors.push(EvalError::schema_violation(
                Kind::UnknownField,
                format!(
                    "positional argument {} for decorator '@{}' has no @inline({}) slot in schema '{}'",
                    index + 1,
                    decorator.full_name(),
                    index,
                    schema.name(),
                ),
                span,
            ));
            continue;
        };
        if let Some(error) = decorator_slot_value_error(doc, decorator, slot, value, span) {
            errors.push(error);
        }
    }
    for named in decorator.named() {
        let Some(slot) = schema.field(named.name()) else {
            if structural_union_slot.is_some() {
                continue;
            }
            errors.push(EvalError::schema_violation_named(
                Kind::UnknownField,
                format!(
                    "argument '{}' is not declared by decorator schema '{}'",
                    named.name(),
                    schema.name(),
                ),
                named.name(),
                named.span(),
            ));
            continue;
        };
        let Ok(value) = named.value() else {
            continue;
        };
        if let Some(error) = decorator_slot_value_error(doc, decorator, slot, &value, named.span())
        {
            errors.push(error);
        }
    }
    for slot in schema.fields() {
        if slot.optional() || slot.default_value().is_some() {
            continue;
        }
        let filled_by_name = named_slots.contains(slot.name());
        let filled_positionally = slot
            .inline_slot()
            .is_some_and(|index| (index as usize) < values.len());
        let filled_structurally =
            structural_union_slot.is_some_and(|structural| structural.name() == slot.name());
        if !filled_by_name && !filled_positionally && !filled_structurally {
            errors.push(EvalError::schema_violation_named(
                Kind::MissingRequired,
                format!(
                    "decorator '@{}' is missing required argument '{}'",
                    decorator.full_name(),
                    slot.name(),
                ),
                slot.name(),
                decorator.name_span(),
            ));
        }
    }
    errors
}

/// Check the constraint decorators that apply to a field's value: the
/// field declaration's own registered `@min` / `@max` / `@non_empty`, plus those on
/// every type-alias declaration its declared type goes through
/// (`@min(1) type Port = u16`). Returns the first violation, rendered
/// for embedding in a schema-violation message. Decorator arguments
/// evaluate through the document, so `@min(8_000)` and `@min(8e3)` both
/// work; a non-numeric bound is ignored rather than flagged (the
/// decorator is data, not schema). Constraint identity comes from the
/// decorator registry, so a local metadata decorator shadowing one of these
/// spellings does not accidentally become a constraint.
pub(super) fn constraint_violation(
    doc: &Document,
    field_decorators: &[ast::Decorator],
    declared_ty: &crate::ast::TypeRef,
    context_ns: &[String],
    value: &Value,
) -> Option<String> {
    let chain = doc.alias_chain_in(declared_ty, context_ns);
    let alias_decorators = chain.iter().flat_map(|link| link.ast.decorators.iter());
    for d in field_decorators.iter().chain(alias_decorators) {
        match registered_constraint(doc, d) {
            Some(constraint @ (RegisteredConstraint::Min | RegisteredConstraint::Max)) => {
                let Some(bound) = d
                    .positional
                    .first()
                    .and_then(|e| doc.eval(e).ok())
                    .and_then(|v| v.as_f64())
                else {
                    continue;
                };
                let Some(actual) = value.as_f64() else {
                    continue;
                };
                if matches!(constraint, RegisteredConstraint::Min) && actual < bound {
                    return Some(format!("value {actual} is below @min({bound})"));
                }
                if matches!(constraint, RegisteredConstraint::Max) && actual > bound {
                    return Some(format!("value {actual} is above @max({bound})"));
                }
            }
            Some(RegisteredConstraint::NonEmpty) => {
                let empty = match value {
                    Value::Utf8(s) | Value::Ascii(s) => s.is_empty(),
                    // `none` elements don't count: a list literal may
                    // carry them (an else-less `if` contributes one) and
                    // every consumer drops them, so a list of nothing but
                    // absences is empty to everyone who reads it.
                    Value::List(xs) => xs.iter().all(|x| matches!(x, Value::None)),
                    _ => false,
                };
                if empty {
                    return Some("value is empty but the type is @non_empty".to_string());
                }
            }
            None => {}
        }
    }
    None
}

#[derive(Clone, Copy)]
/// A constraint decorator the language enforces itself.
enum RegisteredConstraint {
    /// `@min(n)` — a numeric floor, or a length floor for collections.
    Min,
    /// `@max(n)` — a numeric ceiling, or a length ceiling.
    Max,
    /// `@non_empty` — rejects empty strings and collections.
    NonEmpty,
}

/// Recognise a decorator as one of the built-in constraints, or
/// `None` when it is an ordinary host decorator.
fn registered_constraint(
    doc: &Document,
    decorator: &ast::Decorator,
) -> Option<RegisteredConstraint> {
    let name = decorator.name.last()?;
    let schema = doc.decorator_schema(name)?;
    // Language built-ins are synthetic declarations. A source-authored
    // schema with the same decorator name wins registry resolution and must
    // remain ordinary metadata.
    if !schema.span().is_empty() {
        return None;
    }
    match schema.name() {
        "MinDecorator" => Some(RegisteredConstraint::Min),
        "MaxDecorator" => Some(RegisteredConstraint::Max),
        "NonEmptyDecorator" => Some(RegisteredConstraint::NonEmpty),
        _ => None,
    }
}

/// When `declared` carries `@ref("kind")` and `value` holds an id (or a
/// list of ids) that names no existing block of that kind anywhere in
/// the document, return the dangling-reference message. A `none` value
/// (an unset optional reference) and non-id values are skipped — the
/// latter are caught by the ordinary value-vs-type check.
pub(super) fn ref_violation(
    doc: &Document,
    declared: &TypeField<'_>,
    value: &Value,
) -> Option<String> {
    let kind = declared.ref_block_kind()?;
    let mut ids = Vec::new();
    collect_ref_ids(value, &mut ids);
    let missing: Vec<String> = ids
        .into_iter()
        .filter(|id| !doc.has_block_with_id(&kind, id))
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(if missing.len() == 1 {
        format!(
            "@ref(\"{kind}\") target '{}' is not the id of any '{kind}' block",
            missing[0]
        )
    } else {
        format!(
            "@ref(\"{kind}\") targets {} name no '{kind}' block",
            missing
                .iter()
                .map(|s| format!("'{s}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// Gather the referenced id strings from an `@ref` field value: a scalar
/// id, or every id in a (possibly nested) list. `none` and non-id values
/// contribute nothing.
fn collect_ref_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => out.push(s.clone()),
        Value::List(items) => {
            for it in items.iter() {
                collect_ref_ids(it, out);
            }
        }
        _ => {}
    }
}

/// The block's identity label: its first label, when the block's resolved
/// schema declares `@inline(0) id: identifier` — i.e. the label IS an id
/// (an `acme` `component`). The field must be *named* `id`:
/// identifier-typed labels under other names are
/// parameters (`code wcl`'s language, a component's `name`), which repeat
/// freely. `None` for unlabeled blocks and schema-less kinds.
pub(super) fn identity_label(block: &Block<'_>) -> Option<String> {
    use crate::ast::{BuiltinType, TypeRef};
    let schema = block.schema()?;
    let id_typed = schema.fields().any(|f| {
        f.name() == "id"
            && f.inline_slot() == Some(0)
            && matches!(f.type_ref(), TypeRef::Builtin(BuiltinType::Identifier))
    });
    if !id_typed {
        return None;
    }
    block.labels().ok()?.first()?.as_path_segment()
}

/// Flag every repeated (kind, identity label) among `siblings` — two
/// blocks of one kind sharing an id make every reference to that id
/// ambiguous, and gathered lists silently carry both. Pushed as
/// `DuplicateBlockId` at each repeat's span.
pub(super) fn duplicate_id_violations<'a>(
    siblings: impl Iterator<Item = Block<'a>>,
    errs: &mut Vec<EvalError>,
) {
    errs.extend(
        duplicate_id_errors(siblings)
            .into_iter()
            .map(|(error, _)| error),
    );
}

/// Report sibling blocks of one kind that share an identity label,
/// which would make every reference to that id ambiguous.
pub(super) fn duplicate_id_errors<'a>(
    siblings: impl Iterator<Item = Block<'a>>,
) -> Vec<(EvalError, Block<'a>)> {
    use crate::error::SchemaViolationKind as Kind;
    let mut seen: HashMap<(String, String, String), ast::Span> = HashMap::new();
    let mut errors = Vec::new();
    for b in siblings {
        let Some(label) = identity_label(&b) else {
            continue;
        };
        let key = (b.kind_ns().join("."), b.kind().to_string(), label.clone());
        match seen.get(&key) {
            Some(_first) => {
                errors.push((
                    EvalError::schema_violation(
                        Kind::DuplicateBlockId,
                        format!(
                            "duplicate id: '{}' block '{label}' is already declared \
                         — ids must be unique among a parent's (or the document's) \
                         '{}' blocks",
                            b.kind(),
                            b.kind(),
                        ),
                        b.span(),
                    ),
                    b,
                ));
            }
            None => {
                seen.insert(key, b.span());
            }
        }
    }
    errors
}

/// Whether these decorators include `@schemaless`, waiving membership
/// checking.
pub(super) fn has_schemaless(decorators: &[ast::Decorator]) -> bool {
    decorators
        .iter()
        .any(|decorator| matches!(decorator.schemaless_mode(), Some(ast::SchemalessMode::Full)))
}

/// Whether a node opts its annotations out of decorator declaration checks.
/// Bare `@schemaless` includes this exemption; the narrow form spells it as
/// the literal named argument `annotations = true`.
pub(super) fn has_annotation_exemption(decorators: &[ast::Decorator]) -> bool {
    decorators
        .iter()
        .any(|decorator| decorator.schemaless_mode().is_some())
}

/// `true` when `decorators` carries `@contextual` — see
/// [`TypeDecl::is_contextual`](crate::doc::TypeDecl::is_contextual).
pub(super) fn has_contextual(decorators: &[ast::Decorator]) -> bool {
    let name = BuiltinDecorator::Contextual.as_str();
    decorators
        .iter()
        .any(|d| d.name.len() == 1 && d.name[0] == name)
}

/// `true` when `decorators` carries `@by_ref`. A block kind so marked is
/// reified to a resolvable [`Value::DataPath`](crate::value::Value::DataPath)
/// reference when it appears as a `@child`/`@children` slot of a reified
/// block, rather than having its content inlined — see
/// [`Block::to_record_value`](crate::doc::views::Block::to_record_value).
pub(super) fn has_by_ref(decorators: &[ast::Decorator]) -> bool {
    let name = BuiltinDecorator::ByRef.as_str();
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
        let (lhs, rhs) = match (lhs, rhs) {
            (Some(lhs), Some(rhs)) => (lhs, rhs),
            (lhs, rhs) => {
                // An operand that doesn't name a literal block is a typo for a
                // static connection — but under a `@dynamic` connection it may
                // be an id generated at expansion time by a `@contextual`
                // block, which we can't resolve statically. Suppress
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
        };
        let Some(lhs_decl) = doc.operand_schema(&lhs) else {
            continue; // block kind without a schema; UnregisteredKind already fires.
        };
        let Some(rhs_decl) = doc.operand_schema(&rhs) else {
            continue;
        };
        let mut matches: Vec<crate::doc::ConnectionDecl<'_>> = Vec::new();
        for decl in doc.connection_decls() {
            let src_fqn = decl_type_fqn(doc, &decl, decl.source_type());
            let dst_fqn = decl_type_fqn(doc, &decl, decl.destination_type());
            if crate::doc::connection_type_matches(doc, &lhs_decl, src_fqn.as_deref())
                && crate::doc::connection_type_matches(doc, &rhs_decl, dst_fqn.as_deref())
            {
                matches.push(decl);
            }
        }
        let chosen = match matches.len() {
            0 => {
                let lhs_ty = lhs_decl.full_name();
                let rhs_ty = rhs_decl.full_name();
                errs.push(EvalError::schema_violation(
                    Kind::UnknownConnection,
                    format!("no connection schema accepts '{lhs_ty} -> {rhs_ty}'",),
                    stmt.span,
                ));
                continue;
            }
            1 => matches.into_iter().next().unwrap(),
            _ => {
                let lhs_ty = lhs_decl.full_name();
                let rhs_ty = rhs_decl.full_name();
                let names: Vec<String> = matches.iter().map(|m| m.full_name()).collect();
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

/// Resolve a connection declaration's endpoint type to its FQN, relative
/// to the file that declared the connection (so a namespaced library's
/// bare `Adr` means its own `lib.Adr`).
fn decl_type_fqn(
    doc: &crate::doc::Document,
    decl: &crate::doc::ConnectionDecl<'_>,
    t: &crate::ast::TypeRef,
) -> Option<String> {
    doc.resolve_type_fqn_in(t, decl.file_ns())
}

/// `true` when some `@dynamic` connection schema plausibly accepts a
/// statement with an unresolved operand: each *resolved* operand's block
/// type must satisfy the schema's corresponding role, while an unresolved
/// operand is treated as a wildcard (it may name a render-time-generated
/// id). Gates suppression of `UnknownConnectionOperand` in
/// [`validate_connection_stmts`].
type ResolvedOperand = Option<crate::doc::ConnOperand>;

/// Whether a `@dynamic` connection declaration accepts these operand
/// types, which it does more permissively than a static one.
fn dynamic_connection_admits(
    doc: &crate::doc::Document,
    lhs: &ResolvedOperand,
    rhs: &ResolvedOperand,
) -> bool {
    let role_ok = |operand: &ResolvedOperand, fqn: Option<&str>| match operand {
        // Wildcard: an unresolved operand can't be type-checked.
        None => true,
        Some(op) => doc
            .operand_schema(op)
            .is_some_and(|d| crate::doc::connection_type_matches(doc, &d, fqn)),
    };
    doc.connection_decls()
        .filter(|d| d.is_dynamic())
        .any(|decl| {
            let src_fqn = decl_type_fqn(doc, &decl, decl.source_type());
            let dst_fqn = decl_type_fqn(doc, &decl, decl.destination_type());
            role_ok(lhs, src_fqn.as_deref()) && role_ok(rhs, dst_fqn.as_deref())
        })
}

/// The two checks that read a block's own fields against its schema:
/// every written field is declared (`UnknownField`), and every field the
/// schema lists in `required_fields` is written (`MissingRequired`).
///
/// Split out because a `@contextual` block gets exactly these and no
/// more — its body only has meaning once expanded, so the child walk
/// stops at it, but the fields it was *written with* are checkable
/// wherever it sits. For an instance of a `@declares_kind` kind that is
/// the whole of the check, and the pair is exactly what a hand-rolled
/// slot checker used to do.
fn validate_own_fields(block: &Block<'_>, schema: &crate::doc::TypeDecl<'_>) -> Vec<EvalError> {
    use crate::error::SchemaViolationKind as Kind;
    let mut errs = Vec::new();
    if schema.is_schemaless() {
        return errs;
    }
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
    for required in schema.required_fields() {
        if block.field(&required).is_none() {
            errs.push(EvalError::schema_violation(
                Kind::MissingRequired,
                format!(
                    "block '{}' is missing required field '{}'",
                    block.kind(),
                    required
                ),
                block.span(),
            ));
        }
    }
    errs
}

/// Validate one block against its schema: field membership, required
/// fields, child cardinality and constraint decorators.
pub(super) fn compute_schema_errors<'a>(block: &Block<'a>) -> Vec<EvalError> {
    use crate::error::SchemaViolationKind as Kind;
    let mut errs = Vec::new();

    // Whole-block opt-out — `@schemaless service web { … }` skips
    // every check inside the block (its contents are unrestricted).
    if has_schemaless(&block.ast.decorators) {
        return errs;
    }

    if !has_annotation_exemption(&block.ast.decorators) {
        for decorator in block.decorators() {
            errs.extend(decorator_argument_errors(block.doc, &decorator));
        }
    }
    for field in block.fields() {
        if has_annotation_exemption(&field.ast.decorators) {
            continue;
        }
        for decorator in field.decorators() {
            errs.extend(decorator_argument_errors(block.doc, &decorator));
        }
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

    // A `@schemaless` *type* declaration opens every instance — undeclared
    // fields and children pass, like a whole-block `@schemaless` on the
    // instance. (Used by dynamic, open kinds such as `wdoc_instance`.)
    if schema.is_schemaless() {
        return errs;
    }

    // Field-membership + `required_fields`: every literal `Item::Field`
    // inside this block must be named by the schema, and every field the
    // schema requires must be written. `@schemaless` on a field exempts
    // that specific field from membership.
    errs.extend(validate_own_fields(block, &schema));

    // A schema *derived* from a `@declares_kind` instance describes the
    // params the kind takes and nothing else: it declares no child slots
    // because a declarer's params are fields. The instance's own nested
    // blocks are content the host places (into whatever its declarer's
    // body marks), so the child walk below would read every one of them
    // as a disallowed child. The fields are the whole of the check.
    if schema.is_derived() {
        return errs;
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
                        crate::doc::types::variant_dispatch::block_to_variant(
                            block.doc, &blk, union,
                        )
                    }
                    crate::doc::UnionChildKind::TableRow => {
                        crate::doc::types::variant_dispatch::table_row_to_variant(
                            block.doc, &blk, union,
                        )
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
                    && crate::doc::types::variant_dispatch::block_to_variant(block.doc, &blk, union)
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
        // `@schemaless` on the schema's own field declaration opts every
        // instance out of the value-vs-type check: the declared type
        // names the intended shape, but the values are dynamic and
        // interpreted by the consumer (e.g. a repeater's `each` input,
        // or a computed table's stringified cells).
        if has_schemaless(&declared.ast.decorators) {
            continue;
        }
        let value = match literal_field.value() {
            Ok(value) => value,
            Err(error) => {
                // A literal list whose element type is a union is static
                // authored data: failure to infer one of its record variants
                // is a schema error. Surface it here instead of silently
                // accepting the field. Keep computed fields lazy — template
                // expressions may legitimately refer to lambda bindings that
                // do not exist until evaluation.
                let resolved = block.doc.resolve_alias(declared.type_ref());
                let literal_union_list = matches!(
                    (&literal_field.ast.expr, &resolved),
                    (
                        ast::Expr::ListLit { .. },
                        crate::ast::TypeRef::List(inner)
                    ) if matches!(inner.as_ref(), crate::ast::TypeRef::Named { path, .. }
                        if block.doc.union_fqn_for_path(path).is_some())
                );
                if literal_union_list {
                    errs.push(error.clone());
                }
                continue;
            }
        };

        // An optional field written out as `none` is absent, not
        // ill-typed: there is no value left to check against the
        // declared type, a symbol set, or a `@min`/`@non_empty` bound.
        if declared.optional() && matches!(value, Value::None) {
            continue;
        }

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

        // Resolve type aliases once: a field declared with an alias
        // (`port: Port` where `type Port = u16`) validates against the
        // target type, and an alias of a union dispatches like the union.
        let resolved_ty = block
            .doc
            .resolve_alias_in(declared.type_ref(), declared.file_ns);

        // Union path — preserved verbatim.
        if let crate::ast::TypeRef::Named { path, .. } = &resolved_ty
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
        if !crate::doc::types::value_matches_type_ref(value, &resolved_ty) {
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
        } else if let Some(err) = crate::doc::types::symbol_set_membership_error_in(
            block.doc,
            &resolved_ty,
            value,
            literal_field.name(),
            literal_field.span(),
            declared.file_ns,
        ) {
            errs.push(err);
        } else if let Some(msg) = constraint_violation(
            block.doc,
            &declared.ast.decorators,
            declared.type_ref(),
            declared.file_ns,
            value,
        ) {
            errs.push(EvalError::schema_violation(
                Kind::ConstraintViolation,
                format!("field '{}': {msg}", literal_field.name()),
                literal_field.span(),
            ));
        } else if let Some(msg) = ref_violation(block.doc, &declared, value) {
            errs.push(EvalError::schema_violation(
                Kind::DanglingReference,
                format!("field '{}': {msg}", literal_field.name()),
                literal_field.span(),
            ));
        }
    }

    // 0. Table row-form validation: if this block's schema is a
    // `@table`, its labels are the row's column values and must
    // match the schema field count.
    if block
        .doc
        .table_schema_in(block.kind_ns(), block.kind(), block.file_ns())
        .is_some()
    {
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

    // 1b. Duplicate identity labels among this block's direct children:
    // two same-kind siblings sharing an id-typed label are ambiguous.
    duplicate_id_violations(block.blocks(), &mut errs);

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
    // Legal children recurse into their own `schema_errors()` so a
    // violation any number of levels deep reaches the strict report —
    // matching the lazy per-field path, which has no depth limit.
    // Exceptions to the recursion:
    //   - union-dispatched blocks: their fields are a variant payload
    //     shape, not a `@block` schema, and are validated by dispatch;
    //   - `@contextual` kinds: context-polymorphic bodies that only have
    //     meaning once expanded with bindings.
    for nested in block.blocks() {
        if nested.schema().is_none() && block.doc.is_possible_block_slot_fill(nested.kind()) {
            continue;
        }
        if allowed.iter().any(|k| k == nested.kind()) {
            if !nested.is_contextual() {
                errs.extend(nested.schema_errors().iter().cloned());
            }
            continue;
        }
        let matches_union = union_slots.iter().any(|u| {
            crate::doc::types::variant_dispatch::block_to_variant(block.doc, &nested, *u).is_ok()
        });
        if matches_union {
            continue;
        }
        let matches_interface = !interface_slots.is_empty()
            && nested.schema().is_some_and(|t| {
                interface_slots
                    .iter()
                    .any(|iface| t.is_descendant_of(&iface.full_name()))
            });
        if matches_interface {
            if !nested.is_contextual() {
                errs.extend(nested.schema_errors().iter().cloned());
            }
            continue;
        }
        // A `@contextual` block emits whatever its body contains once
        // expanded (page content in a page, shapes in a diagram, rows in
        // a node table), so accept it wherever children are allowed at
        // all. Its body is not recursed into — but the fields it was
        // written with are checked against its schema here, since the
        // walk above skips contextual bodies. For an instance of a
        // `@declares_kind` kind that is the whole check: unknown param,
        // missing required param.
        if nested.is_contextual() {
            if let Some(schema) = nested.schema() {
                errs.extend(validate_own_fields(&nested, &schema));
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
