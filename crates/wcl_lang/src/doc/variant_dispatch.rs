//! Structural-shape dispatch for unions.
//!
//! When a `@child(SomeUnion)` / `@children(SomeUnion)` field is read,
//! when a decorator schema slots a value into a union-typed field, or
//! when a `@children(SomeUnion)` table is materialised, the inputs
//! (block, decorator args, row labels) are projected onto a union
//! variant by *shape*. The unique structural match becomes a
//! `Value::Variant`.
//!
//! Conflicting variant shapes are caught at declaration time by
//! `validate_union` (foundation pass); the dispatcher's defensive
//! `VariantAmbiguous` arm should be unreachable in normal use.

use std::collections::BTreeMap;

use crate::ast;
use crate::error::{EvalError, SchemaViolationKind};
use crate::value::{Value, VariantPayload};

use super::{Block, Document, UnionDecl};

/// Project a block's effective fields into a `Value::Variant` by
/// matching the field shape against the union's variants.
pub(super) fn block_to_variant<'a>(
    doc: &'a Document,
    block: &Block<'a>,
    union_decl: UnionDecl<'a>,
) -> Result<Value, EvalError> {
    // Collect (field name → value) from the block's literal fields.
    let mut field_map: BTreeMap<String, Value> = BTreeMap::new();
    for f in block.fields() {
        let v = match f.value() {
            Ok(v) => v.clone(),
            Err(e) => return Err(e.clone()),
        };
        field_map.insert(f.name().to_string(), v);
    }
    let variant = match_record_variant_by_shape(doc, &field_map, &union_decl, block.span())?;
    Ok(Value::Variant {
        union: union_decl.ast.name.clone(),
        variant: variant.name.clone(),
        payload: VariantPayload::Record(std::sync::Arc::new(field_map)),
    })
}

/// Project a decorator's positional + named args into a `Value::Variant`.
///
/// Positional args populate the matched variant's record fields in
/// declared source order; named args populate by name. A positional
/// arg with no corresponding declared field is a shape error.
pub(super) fn decorator_to_variant<'a>(
    doc: &'a Document,
    positional: &[Value],
    positional_spans: &[ast::Span],
    named: &BTreeMap<String, Value>,
    named_spans: &BTreeMap<String, ast::Span>,
    union_decl: UnionDecl<'a>,
    span: ast::Span,
) -> Result<Value, EvalError> {
    // Build the candidate field map by walking each variant in order
    // and trying to overlay positional + named against its declared
    // record fields. The variant whose shape fully covers all
    // supplied args (and is satisfied by them) wins; ambiguity falls
    // out of the foundation's shape-collision check.
    let effective = doc.effective_variants_of(union_decl.ast)?;
    let mut name_matches: Vec<(&ast::UnionVariant, BTreeMap<String, Value>)> = Vec::new();
    let mut full_matches: Vec<(&ast::UnionVariant, BTreeMap<String, Value>)> = Vec::new();
    'variants: for v in &effective {
        let ast::VariantBody::Record {
            fields: decl_fields,
            ..
        } = &v.body
        else {
            continue;
        };
        // Positional args populate declared fields in source order;
        // overrun = no match.
        if positional.len() > decl_fields.len() {
            continue;
        }
        let mut map: BTreeMap<String, Value> = BTreeMap::new();
        for (i, val) in positional.iter().enumerate() {
            map.insert(decl_fields[i].name.clone(), val.clone());
        }
        for (k, val) in named {
            // A named arg with no declared slot disqualifies this variant.
            if !decl_fields.iter().any(|df| &df.name == k) {
                continue 'variants;
            }
            map.insert(k.clone(), val.clone());
        }
        if map.len() != decl_fields.len() {
            // Missing fields: not a full shape match.
            continue;
        }
        // Name match — now check field-value types.
        name_matches.push((*v, map.clone()));
        if all_field_types_match(decl_fields, &map) {
            full_matches.push((*v, map));
        }
    }
    pick_unique_match(
        union_decl,
        full_matches,
        name_matches,
        span,
        Some(DecoratorArgumentSpans {
            positional: positional_spans,
            named: named_spans,
        }),
    )
}

#[derive(Clone, Copy)]
struct DecoratorArgumentSpans<'a> {
    positional: &'a [ast::Span],
    named: &'a BTreeMap<String, ast::Span>,
}

/// Project a single synthesised table row (a `Block` whose `labels`
/// are the row values) into a `Value::Variant`. The first label
/// names the variant; remaining labels populate its record fields
/// positionally.
pub(super) fn table_row_to_variant<'a>(
    doc: &'a Document,
    row_block: &Block<'a>,
    union_decl: UnionDecl<'a>,
) -> Result<Value, EvalError> {
    let labels = row_block.labels()?;
    let span = row_block.span();
    let Some(first) = labels.first() else {
        return Err(EvalError::variant_shape_mismatch(
            "non-empty row",
            "empty row",
            span,
        ));
    };
    let variant_name = match first {
        Value::Identifier(s) | Value::Symbol(s) | Value::Utf8(s) | Value::Ascii(s) => s.clone(),
        other => {
            return Err(EvalError::variant_shape_mismatch(
                "first column to be a variant identifier",
                other.type_name(),
                span,
            ));
        }
    };
    let effective = doc.effective_variants_of(union_decl.ast)?;
    let Some(variant) = effective.iter().find(|v| v.name == variant_name) else {
        return Err(EvalError::unknown_variant(
            union_decl.ast.name.join("."),
            variant_name,
            span,
        ));
    };
    let ast::VariantBody::Record {
        fields: decl_fields,
        ..
    } = &variant.body
    else {
        return Err(EvalError::variant_shape_mismatch(
            "record variant body",
            "non-record variant",
            span,
        ));
    };
    let payload_labels = &labels[1..];
    if payload_labels.len() != decl_fields.len() {
        return Err(EvalError::variant_shape_mismatch(
            format!("{} columns for {}", decl_fields.len() + 1, variant_name),
            format!("{} columns", labels.len()),
            span,
        ));
    }
    let mut map: BTreeMap<String, Value> = BTreeMap::new();
    for (df, val) in decl_fields.iter().zip(payload_labels.iter()) {
        map.insert(df.name.clone(), val.clone());
    }
    // Field-value type check — surface a focused near-miss diagnostic.
    if let Some((field_name, got, expected)) = first_type_mismatch(decl_fields, &map) {
        return Err(EvalError::schema_violation(
            SchemaViolationKind::VariantNoMatch,
            format!(
                "field '{field_name}' of variant {}::{variant_name} expects {expected:?}, got {got}",
                union_decl.ast.name.join("."),
            ),
            span,
        ));
    }
    Ok(Value::Variant {
        union: union_decl.ast.name.clone(),
        variant: variant_name,
        payload: VariantPayload::Record(std::sync::Arc::new(map)),
    })
}

/// Type-directed coercion of a freshly-evaluated value against its
/// declared type. Rewrites a bare `Value::Record` into a
/// `Value::Variant` when the declared type names a union, recursing
/// through `list<…>` element types and through the matched variant's
/// own union-typed fields. Any other value/type pair is returned
/// unchanged (permissive, mirroring `value_matches_type_ref`).
/// `true` when `ty` could require bare-record coercion somewhere inside
/// it — a named union (memoised lookup) or a list that could. Everything
/// else is a guaranteed pass-through, letting `coerce_value_to_type`
/// skip per-call resolution and list rebuilds.
fn type_may_coerce(doc: &Document, ty: &crate::value::TypeRef) -> bool {
    use crate::value::{BuiltinType, TypeRef};
    match ty {
        TypeRef::Named { path, .. } => doc.union_fqn_for_path(path).is_some(),
        TypeRef::List(inner) => type_may_coerce(doc, inner),
        // Strings coerce to identifiers on identifier-declared slots
        // (quoted refs join like bare ones — see `str == id` templates).
        TypeRef::Builtin(BuiltinType::Identifier) => true,
        _ => false,
    }
}

pub(crate) fn coerce_value_to_type(
    doc: &Document,
    value: Value,
    ty: &crate::value::TypeRef,
    span: ast::Span,
) -> Result<Value, EvalError> {
    use crate::value::TypeRef;
    // A literal unit resolves against the declared type — multiply by the
    // type's matching `@unit(name, factor)` decorator — regardless of the
    // union fast-path below. An unresolvable unit is an error here.
    if let Value::PendingUnit { magnitude, unit } = value {
        return resolve_unit_literal(doc, *magnitude, &unit, ty, span);
    }
    // Fast path: when the declared type can't involve a union anywhere
    // (memoised lookup), the value passes through untouched. This runs
    // per function invocation per argument, so without it every closure
    // call re-ran name resolution — and rebuilt entire list arguments —
    // only to find there was nothing to coerce.
    //
    // Exception: a `list<NamedAlias>` may hold unit literals even when the
    // element type isn't a union. A cheap discriminant scan (gated on a
    // named element type, so `list<utf8>` etc. stay on the fast path)
    // detects that case and falls through to the list arm, which resolves
    // each element.
    if !type_may_coerce(doc, ty) {
        let list_needs_units = matches!((&value, ty),
            (Value::List(items), TypeRef::List(inner))
                if matches!(inner.as_ref(), TypeRef::Named { .. })
                    && items.iter().any(|v| matches!(v, Value::PendingUnit { .. })));
        if !list_needs_units {
            return Ok(value);
        }
    }
    match (value, ty) {
        // Identifier-declared slot: a string coerces to the identifier it
        // names, so quoted and bare refs evaluate identically (mirrors the
        // field-eval rule in `views.rs`). Other values pass through.
        (
            Value::Utf8(s) | Value::Ascii(s),
            TypeRef::Builtin(crate::value::BuiltinType::Identifier),
        ) => Ok(Value::Identifier(s)),
        (Value::List(items), TypeRef::List(inner)) => {
            let mut out = Vec::with_capacity(items.len());
            for it in std::sync::Arc::unwrap_or_clone(items) {
                out.push(coerce_value_to_type(doc, it, inner, span)?);
            }
            Ok(Value::List(std::sync::Arc::new(out)))
        }
        (Value::Record { ty: rty, fields }, TypeRef::Named { path, .. }) => {
            // Resolve via the memoised union lookup (namespace-aware:
            // own namespace, then imported library namespaces) so a
            // stdlib field typed `: SomeUnion` under `namespace wdoc`
            // finds `wdoc.SomeUnion`.
            let Some(union_decl) = doc
                .union_fqn_for_path(path)
                .and_then(|fqn| doc.union_decl(&fqn))
            else {
                // Named type that isn't a union — leave the anonymous
                // record untouched.
                return Ok(Value::Record { ty: rty, fields });
            };
            let variant = match_record_variant_by_shape(doc, &fields, &union_decl, span)?;
            // Recurse into the matched variant's declared field types so
            // a bare record nested in a union-typed field also infers.
            let mut coerced: BTreeMap<String, Value> = BTreeMap::new();
            if let ast::VariantBody::Record {
                fields: decl_fields,
                ..
            } = &variant.body
            {
                for (k, v) in std::sync::Arc::unwrap_or_clone(fields) {
                    let nv = match decl_fields.iter().find(|f| f.name == k) {
                        Some(f) => coerce_value_to_type(doc, v, &f.ty, span)?,
                        None => v,
                    };
                    coerced.insert(k, nv);
                }
            } else {
                coerced = std::sync::Arc::unwrap_or_clone(fields);
            }
            Ok(Value::Variant {
                union: union_decl.ast.name.clone(),
                variant: variant.name.clone(),
                payload: VariantPayload::Record(std::sync::Arc::new(coerced)),
            })
        }
        (other, _) => Ok(other),
    }
}

/// Resolve a literal unit (`5MiB`) against its declared type: find the
/// matching `@unit(name, factor)` decorator on the type's alias chain,
/// multiply the magnitude by the factor, and cast the product to the
/// alias's underlying numeric type. Mirrors `schema_check::constraint_violation`'s
/// decorator collection (alias-chain decorators, args evaluated through
/// the document). No matching unit on the type is `UnitNoMatch`.
fn resolve_unit_literal(
    doc: &Document,
    magnitude: Value,
    unit: &str,
    ty: &crate::value::TypeRef,
    span: ast::Span,
) -> Result<Value, EvalError> {
    match doc.unit_factor(ty, unit) {
        Some(factor) => apply_unit_factor(doc, &magnitude, &factor, ty, unit, span),
        None => Err(EvalError::unit_no_match(unit, ty.to_string(), span)),
    }
}

/// Multiply `magnitude` by `factor` and cast to the alias's underlying
/// numeric builtin. Integer products stay exact (i128); a fractional
/// product against an integer target, or one out of range, is an error.
fn apply_unit_factor(
    doc: &Document,
    magnitude: &Value,
    factor: &Value,
    ty: &crate::value::TypeRef,
    unit: &str,
    span: ast::Span,
) -> Result<Value, EvalError> {
    use crate::value::{BuiltinType as B, TypeRef};
    let (Some(mf), Some(ff)) = (magnitude.as_f64(), factor.as_f64()) else {
        // A non-numeric factor means the decorator isn't a real unit.
        return Err(EvalError::unit_no_match(unit, ty.to_string(), span));
    };
    let int_product = magnitude
        .as_i128()
        .zip(factor.as_i128())
        .and_then(|(a, b)| a.checked_mul(b));
    let float_product = mf * ff;

    let target = match doc.resolve_alias(ty) {
        TypeRef::Builtin(b) if b.is_numeric() => Some(b),
        _ => None,
    };

    let frac_err = || {
        EvalError::schema_violation(
            crate::error::SchemaViolationKind::FieldTypeMismatch,
            format!("unit '{unit}' produces a fractional value for integer type '{ty}'"),
            span,
        )
    };
    let range_err = || {
        EvalError::schema_violation(
            crate::error::SchemaViolationKind::FieldTypeMismatch,
            format!("unit '{unit}' product is out of range for type '{ty}'"),
            span,
        )
    };
    // The integer magnitude for an integer target: prefer the exact i128
    // product; fall back to a whole-valued float product.
    let as_int = |i128_opt: Option<i128>| -> Result<i128, EvalError> {
        match i128_opt {
            Some(i) => Ok(i),
            None => {
                if float_product.fract() != 0.0 {
                    Err(frac_err())
                } else {
                    Ok(float_product as i128)
                }
            }
        }
    };
    macro_rules! to_int {
        ($V:ident, $T:ty) => {{
            let i = as_int(int_product)?;
            <$T>::try_from(i).map(Value::$V).map_err(|_| range_err())
        }};
    }
    match target {
        Some(B::I8) => to_int!(I8, i8),
        Some(B::I16) => to_int!(I16, i16),
        Some(B::I32) => to_int!(I32, i32),
        Some(B::I64) => to_int!(I64, i64),
        Some(B::Isize) => to_int!(Isize, isize),
        Some(B::I128) => as_int(int_product).map(Value::I128),
        Some(B::U8) => to_int!(U8, u8),
        Some(B::U16) => to_int!(U16, u16),
        Some(B::U32) => to_int!(U32, u32),
        Some(B::U64) => to_int!(U64, u64),
        Some(B::U128) => {
            let i = as_int(int_product)?;
            u128::try_from(i).map(Value::U128).map_err(|_| range_err())
        }
        Some(B::Usize) => to_int!(Usize, usize),
        Some(B::F32) => Ok(Value::F32(float_product as f32)),
        Some(B::F64) => Ok(Value::F64(float_product)),
        // Non-builtin or non-numeric target alias: emit a default-typed
        // number (integer when exact, else float) and let the ordinary
        // value/type check weigh in.
        _ => Ok(match int_product {
            Some(i) => Value::I64(i as i64),
            None => Value::F64(float_product),
        }),
    }
}

/// Walk `union_decl.effective_variants` and return the unique variant
/// whose record body matches the field map by both *name set* and
/// *field-value types*.
///
/// Returns `VariantNoMatch` (with a near-miss diagnostic when exactly
/// one variant matches by name but not by type), `VariantAmbiguous`
/// (defensive — declaration-time shape collisions catch this).
pub(crate) fn match_record_variant_by_shape<'a>(
    doc: &'a Document,
    fields: &BTreeMap<String, Value>,
    union_decl: &UnionDecl<'a>,
    span: ast::Span,
) -> Result<&'a ast::UnionVariant, EvalError> {
    let effective = doc.effective_variants_of(union_decl.ast)?;
    let mut name_matches: Vec<(&ast::UnionVariant, BTreeMap<String, Value>)> = Vec::new();
    let mut full_matches: Vec<(&ast::UnionVariant, BTreeMap<String, Value>)> = Vec::new();
    for v in &effective {
        let ast::VariantBody::Record {
            fields: decl_fields,
            ..
        } = &v.body
        else {
            continue;
        };
        if !names_equal(decl_fields, fields) {
            continue;
        }
        name_matches.push((*v, fields.clone()));
        if all_field_types_match(decl_fields, fields) {
            full_matches.push((*v, fields.clone()));
        }
    }
    let v = pick_unique_match(*union_decl, full_matches, name_matches, span, None)?;
    // Re-extract the matched variant from the Value::Variant we got back.
    // Both invariants hold by construction (`pick_unique_match` returns a
    // `Value::Variant` built from the effective list); report an internal
    // error rather than panicking mid-validation if they ever drift.
    let Value::Variant { variant: name, .. } = &v else {
        return Err(EvalError::user_error(
            "internal: variant dispatch produced a non-variant value",
            span,
        ));
    };
    let m = effective
        .iter()
        .find(|v| v.name == *name)
        .copied()
        .ok_or_else(|| {
            EvalError::user_error(
                format!("internal: matched variant '{name}' is not in the effective variant list"),
                span,
            )
        })?;
    Ok(m)
}

fn pick_unique_match<'a>(
    union_decl: UnionDecl<'a>,
    full_matches: Vec<(&'a ast::UnionVariant, BTreeMap<String, Value>)>,
    name_matches: Vec<(&'a ast::UnionVariant, BTreeMap<String, Value>)>,
    span: ast::Span,
    argument_spans: Option<DecoratorArgumentSpans<'_>>,
) -> Result<Value, EvalError> {
    if full_matches.len() == 1 {
        let (v, map) = full_matches.into_iter().next().unwrap();
        return Ok(Value::Variant {
            union: union_decl.ast.name.clone(),
            variant: v.name.clone(),
            payload: VariantPayload::Record(std::sync::Arc::new(map)),
        });
    }
    if full_matches.len() > 1 {
        let names: Vec<String> = full_matches.iter().map(|(v, _)| v.name.clone()).collect();
        return Err(EvalError::schema_violation(
            SchemaViolationKind::VariantAmbiguous,
            format!(
                "multiple variants of '{}' match by shape: {}",
                union_decl.ast.name.join("."),
                names.join(", "),
            ),
            span,
        ));
    }
    // No full match — try to give a useful near-miss diagnostic.
    if name_matches.len() == 1 {
        let (v, map) = &name_matches[0];
        let ast::VariantBody::Record {
            fields: decl_fields,
            ..
        } = &v.body
        else {
            unreachable!("name_matches only contains record variants");
        };
        if let Some((field_name, got, expected)) = first_type_mismatch(decl_fields, map) {
            let mismatch_span = argument_spans
                .and_then(|spans| {
                    spans.named.get(field_name).copied().or_else(|| {
                        decl_fields
                            .iter()
                            .position(|field| field.name == field_name)
                            .and_then(|index| spans.positional.get(index).copied())
                    })
                })
                .unwrap_or(span);
            return Err(EvalError::schema_violation(
                SchemaViolationKind::VariantNoMatch,
                format!(
                    "field '{field_name}' of variant {}::{} expects {expected:?}, got {got}",
                    union_decl.ast.name.join("."),
                    v.name,
                ),
                mismatch_span,
            ));
        }
    }
    Err(EvalError::schema_violation(
        SchemaViolationKind::VariantNoMatch,
        format!(
            "no variant of '{}' matches the supplied shape",
            union_decl.ast.name.join("."),
        ),
        span,
    ))
}

fn names_equal(decl_fields: &[ast::TypeField], map: &BTreeMap<String, Value>) -> bool {
    if decl_fields.len() != map.len() {
        return false;
    }
    decl_fields.iter().all(|f| map.contains_key(&f.name))
}

fn all_field_types_match(decl_fields: &[ast::TypeField], map: &BTreeMap<String, Value>) -> bool {
    decl_fields.iter().all(|f| {
        map.get(&f.name)
            .map(|v| super::value_matches_declared(v, &f.ty, f.optional))
            .unwrap_or(false)
    })
}

/// First `(field_name, got_type_name, expected_type)` for a field
/// whose value doesn't match the declared type. Used to drive the
/// near-miss diagnostic.
fn first_type_mismatch<'a>(
    decl_fields: &'a [ast::TypeField],
    map: &'a BTreeMap<String, Value>,
) -> Option<(&'a str, &'a str, &'a crate::value::TypeRef)> {
    for f in decl_fields {
        if let Some(v) = map.get(&f.name)
            && !super::value_matches_declared(v, &f.ty, f.optional)
        {
            return Some((f.name.as_str(), v.type_name(), &f.ty));
        }
    }
    None
}
