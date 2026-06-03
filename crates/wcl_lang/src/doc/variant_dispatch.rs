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
        payload: VariantPayload::Record(field_map),
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
    named: &BTreeMap<String, Value>,
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
    pick_unique_match(union_decl, full_matches, name_matches, span)
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
        payload: VariantPayload::Record(map),
    })
}

/// Type-directed coercion of a freshly-evaluated value against its
/// declared type. Rewrites a bare `Value::Record` into a
/// `Value::Variant` when the declared type names a union, recursing
/// through `list<…>` element types and through the matched variant's
/// own union-typed fields. Any other value/type pair is returned
/// unchanged (permissive, mirroring `value_matches_type_ref`).
pub(crate) fn coerce_value_to_type(
    doc: &Document,
    value: Value,
    ty: &crate::value::TypeRef,
    span: ast::Span,
) -> Result<Value, EvalError> {
    use crate::value::TypeRef;
    match (value, ty) {
        (Value::List(items), TypeRef::List(inner)) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                out.push(coerce_value_to_type(doc, it, inner, span)?);
            }
            Ok(Value::List(out))
        }
        (Value::Record { ty: rty, fields }, TypeRef::Named(path)) => {
            // Resolve the named type namespace-aware (own namespace, then
            // imported library namespaces) so a stdlib field typed
            // `: SomeUnion` under `namespace wdoc` finds `wdoc.SomeUnion`.
            let resolved = doc
                .resolve_path_in(path, doc.file_ns())
                .map(|p| p.join("."))
                .unwrap_or_else(|| path.join("."));
            let Some(union_decl) = doc
                .union_decl(&resolved)
                .or_else(|| doc.union_decl(&path.join(".")))
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
                for (k, v) in fields {
                    let nv = match decl_fields.iter().find(|f| f.name == k) {
                        Some(f) => coerce_value_to_type(doc, v, &f.ty, span)?,
                        None => v,
                    };
                    coerced.insert(k, nv);
                }
            } else {
                coerced = fields;
            }
            Ok(Value::Variant {
                union: union_decl.ast.name.clone(),
                variant: variant.name.clone(),
                payload: VariantPayload::Record(coerced),
            })
        }
        (other, _) => Ok(other),
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
    let v = pick_unique_match(*union_decl, full_matches, name_matches, span)?;
    // Re-extract the matched variant from the Value::Variant we got back.
    let Value::Variant { variant: name, .. } = &v else {
        unreachable!("pick_unique_match returns Value::Variant on success");
    };
    let m = effective
        .iter()
        .find(|v| v.name == *name)
        .copied()
        .expect("matched variant is in effective list");
    Ok(m)
}

fn pick_unique_match<'a>(
    union_decl: UnionDecl<'a>,
    full_matches: Vec<(&'a ast::UnionVariant, BTreeMap<String, Value>)>,
    name_matches: Vec<(&'a ast::UnionVariant, BTreeMap<String, Value>)>,
    span: ast::Span,
) -> Result<Value, EvalError> {
    if full_matches.len() == 1 {
        let (v, map) = full_matches.into_iter().next().unwrap();
        return Ok(Value::Variant {
            union: union_decl.ast.name.clone(),
            variant: v.name.clone(),
            payload: VariantPayload::Record(map),
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
            return Err(EvalError::schema_violation(
                SchemaViolationKind::VariantNoMatch,
                format!(
                    "field '{field_name}' of variant {}::{} expects {expected:?}, got {got}",
                    union_decl.ast.name.join("."),
                    v.name,
                ),
                span,
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
            .map(|v| super::value_matches_type_ref(v, &f.ty))
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
            && !super::value_matches_type_ref(v, &f.ty)
        {
            return Some((f.name.as_str(), v.type_name(), &f.ty));
        }
    }
    None
}
