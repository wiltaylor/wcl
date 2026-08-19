//! Does this value inhabit this type?
//!
//! Two questions that look alike and are not.
//!
//! *Matching* asks whether a value already belongs to a declared type —
//! the check the schema layer runs. It is deliberately one-sided: where
//! deciding would need a declaration it wasn't handed, it answers yes and
//! leaves the narrower judgement to someone holding more (a `Symbol`
//! against a named type passes here and is settled by
//! [`symbol_set_membership_error_in`]).
//!
//! *Coercion* asks what the value would have to become in order to fit: a
//! unit literal multiplied by its type's `@unit` factor, a bare record
//! rewritten as the union variant whose shape it matches. *Which* variant
//! that is comes from [`variant_dispatch`](super::variant_dispatch),
//! which owns shape matching; the rest of the rewrite is here.

use std::collections::BTreeMap;

use crate::ast::{self, TypeRef};
use crate::error::EvalError;
use crate::value::{Value, VariantPayload};

use crate::doc::Document;

use super::unions::path_matches_suffix;
use super::variant_dispatch::match_record_variant_by_shape;

/// Whether `value` satisfies a *declared field*: [`value_matches_type_ref`]
/// plus the optional rule — a `T?` field accepts the `none` literal, so
/// writing absence out (`note = none`) is as legal as omitting the field.
///
/// The `?` lives on the declaration (`ast::TypeField::optional`), not in
/// `TypeRef`, so `value_matches_type_ref` cannot see it: on its own it
/// answers `false` for `none` against every concrete type. Every
/// value-vs-declared-type check goes through here so the two stay in step.
pub(crate) fn value_matches_declared(value: &Value, ty: &TypeRef, optional: bool) -> bool {
    (optional && matches!(value, Value::None)) || value_matches_type_ref(value, ty)
}

/// Conservative check that `value` could inhabit `ty`.
///
/// Deliberately one-sided: it returns `true` when it cannot decide, so
/// the schema check never rejects a value it merely failed to
/// understand.
pub(crate) fn value_matches_type_ref(value: &Value, ty: &TypeRef) -> bool {
    use crate::ast::BuiltinType as B;
    match (value, ty) {
        (Value::Bool(_), TypeRef::Builtin(B::Bool)) => true,
        (Value::I8(_), TypeRef::Builtin(B::I8)) => true,
        (Value::I16(_), TypeRef::Builtin(B::I16)) => true,
        (Value::I32(_), TypeRef::Builtin(B::I32)) => true,
        (Value::I64(_), TypeRef::Builtin(B::I64)) => true,
        (Value::I128(_), TypeRef::Builtin(B::I128)) => true,
        (Value::Isize(_), TypeRef::Builtin(B::Isize)) => true,
        (Value::U8(_), TypeRef::Builtin(B::U8)) => true,
        (Value::U16(_), TypeRef::Builtin(B::U16)) => true,
        (Value::U32(_), TypeRef::Builtin(B::U32)) => true,
        (Value::U64(_), TypeRef::Builtin(B::U64)) => true,
        (Value::U128(_), TypeRef::Builtin(B::U128)) => true,
        (Value::Usize(_), TypeRef::Builtin(B::Usize)) => true,
        (Value::F32(_), TypeRef::Builtin(B::F32)) => true,
        (Value::F64(_), TypeRef::Builtin(B::F64)) => true,
        (Value::Utf8(_), TypeRef::Builtin(B::Utf8)) => true,
        (Value::Ascii(_), TypeRef::Builtin(B::Ascii)) => true,
        (Value::Utf16(_), TypeRef::Builtin(B::Utf16)) => true,
        (Value::Utf32(_), TypeRef::Builtin(B::Utf32)) => true,
        (Value::Symbol(_), TypeRef::Builtin(B::Symbol)) => true,
        (Value::Identifier(_), TypeRef::Builtin(B::Identifier)) => true,
        // A string for an `identifier` field is a tolerated authoring
        // form (`set = "platformer"`): consumers read Identifier and
        // Utf8/Ascii interchangeably for id-typed fields.
        (Value::Utf8(_) | Value::Ascii(_), TypeRef::Builtin(B::Identifier)) => true,
        // A symbol against a named type is (typically) a `symbol_set`
        // member — checking membership would need the declaration, which
        // `Value` doesn't carry, so stay permissive.
        (Value::Symbol(_), TypeRef::Named { .. }) => true,
        (Value::None, _) => false, // None doesn't satisfy any concrete type
        // Numeric values satisfy any numeric builtin type: the evaluator
        // promotes numerics (an `f64` field authored as `520` holds an
        // i64 literal), so an exact-variant check here would flag values
        // the eval path accepts.
        (v, TypeRef::Builtin(b)) if v.is_numeric() && b.is_numeric() => true,
        // Variant value against a named union type: compare FQN.
        (Value::Variant { union, .. }, TypeRef::Named { path, .. }) => {
            path_matches_suffix(path, union)
        }
        // Record value against a named (non-union) type. Builtin-produced
        // records (e.g. `@connections` projections) carry the producing
        // declaration's FQN in `ty` — compare it. Bare record literals
        // (`ty` empty) stay permissive: matching them by shape would need
        // the declaration, which `Value` doesn't carry (mirrors the
        // tensor / function pass-through below).
        (Value::Record { ty, .. }, TypeRef::Named { path, .. }) => {
            ty.is_empty() || path_matches_suffix(path, ty)
        }
        // Lists check element type recursively — except that a `none`
        // *element* is always legal. An else-less `if` in a list literal
        // (`["base", if e.current { "current" }]`) contributes one, and
        // consumers drop it; the rule that a `none` never satisfies a
        // concrete type bounds a field's own value, not what a list may
        // hold on the way to being filtered.
        (Value::List(items), TypeRef::List(inner)) => items
            .iter()
            .all(|el| matches!(el, Value::None) || value_matches_type_ref(el, inner)),
        // Tensors / functions / references stay permissive — strict
        // checks here would need richer type information than we
        // currently carry on `Value`.
        (Value::Tensor { .. }, TypeRef::Tensor { .. }) => true,
        (Value::Function(_), TypeRef::Function { .. }) => true,
        // `&T` fields evaluate to a `Value::DataPath` (lazy navigator).
        (Value::DataPath { .. }, TypeRef::Reference(_)) => true,
        _ => false,
    }
}

/// When `ty` (already alias-resolved) names a `symbol_set` and `value`
/// is a symbol that isn't one of its members, return the membership
/// violation; otherwise `None`. Mirrors the connection-kind membership
/// check (`schema_check::connection_errors`) so a `status: SomeSet`
/// field rejects an out-of-set symbol identically whether the block is a
/// document child or nested — `value_matches_type_ref` stays permissive
/// for `(Symbol, Named)` because it lacks the declaration to check.
pub(crate) fn symbol_set_membership_error_in(
    doc: &Document,
    ty: &TypeRef,
    value: &Value,
    field_name: &str,
    span: crate::ast::Span,
    context_ns: &[String],
) -> Option<EvalError> {
    let TypeRef::Named { path, .. } = ty else {
        return None;
    };
    let resolved = doc
        .resolve_path_in(path, context_ns)
        .unwrap_or_else(|| path.clone());
    let ss = doc.symbol_set(&resolved.join("."))?;
    let Value::Symbol(sym) = value else {
        return None;
    };
    if ss.has(sym) {
        return None;
    }
    Some(EvalError::schema_violation(
        crate::error::SchemaViolationKind::SymbolNotInSet,
        format!(
            "field '{field_name}' declared as symbol_set '{}' but ':{sym}' is not one of its members",
            path.join(".")
        ),
        span,
    ))
}

/// `true` when `ty` could require bare-record coercion somewhere inside
/// it — a named union (memoised lookup) or a list that could. Everything
/// else is a guaranteed pass-through, letting `coerce_value_to_type`
/// skip per-call resolution and list rebuilds.
fn type_may_coerce(doc: &Document, ty: &crate::ast::TypeRef) -> bool {
    use crate::ast::{BuiltinType, TypeRef};
    match ty {
        TypeRef::Named { path, .. } => doc.union_fqn_for_path(path).is_some(),
        TypeRef::List(inner) => type_may_coerce(doc, inner),
        // Strings coerce to identifiers on identifier-declared slots
        // (quoted refs join like bare ones — see `str == id` templates).
        TypeRef::Builtin(BuiltinType::Identifier) => true,
        _ => false,
    }
}

/// Coerce a value towards a declared type: resolve a pending unit,
/// shape-infer a bare record into a union variant, and recurse into
/// list elements.
///
/// A bare `Value::Record` becomes a `Value::Variant` when the declared
/// type names a union, recursing through `list<…>` element types and
/// through the matched variant's own union-typed fields. Any other
/// value/type pair comes back unchanged — permissive in exactly the way
/// [`value_matches_type_ref`] is.
pub(crate) fn coerce_value_to_type(
    doc: &Document,
    value: Value,
    ty: &crate::ast::TypeRef,
    span: ast::Span,
) -> Result<Value, EvalError> {
    use crate::ast::TypeRef;
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
            TypeRef::Builtin(crate::ast::BuiltinType::Identifier),
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
    ty: &crate::ast::TypeRef,
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
    ty: &crate::ast::TypeRef,
    unit: &str,
    span: ast::Span,
) -> Result<Value, EvalError> {
    use crate::ast::{BuiltinType as B, TypeRef};
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
