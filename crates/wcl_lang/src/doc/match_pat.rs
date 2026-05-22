//! Pattern matching for `match` and `if let` expressions.
//!
//! Pure helper: takes a [`Pattern`] and a [`Value`] and returns the
//! bindings to push when the pattern matches, or `None` otherwise. No
//! `Document` or `EvalCtx` state — the caller installs the bindings
//! into its own `locals` stack.

use crate::ast::{Pattern, VariantPatArgs};
use crate::lexer::NumberLit;
use crate::value::{Value, VariantPayload};

use super::eval_ops::values_eq;

/// Try to match `pat` against `val`. On success, returns the list of
/// `(name, value)` bindings to introduce into the arm's scope. On
/// failure (the pattern doesn't fit), returns `None`.
pub(super) fn match_pattern(pat: &Pattern, val: &Value) -> Option<Vec<(String, Value)>> {
    match pat {
        Pattern::Wildcard(_) => Some(Vec::new()),
        Pattern::Binding { name, .. } => Some(vec![(name.clone(), val.clone())]),
        Pattern::At { name, inner, .. } => {
            let mut bindings = match_pattern(inner, val)?;
            // Outer name wins on conflict — pushed last, looked up rev.
            bindings.push((name.clone(), val.clone()));
            Some(bindings)
        }
        Pattern::LiteralBool(b, _) => {
            if values_eq(val, &Value::Bool(*b)) {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::LiteralNumber { lit, .. } => {
            let lit_val = number_lit_to_value(lit);
            if values_eq(val, &lit_val) {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::LiteralUtf8(s, _) => {
            if let Value::Utf8(other) = val
                && other == s
            {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::LiteralAscii(s, _) => {
            if let Value::Ascii(other) = val
                && other == s
            {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::LiteralSymbol(s, _) => {
            if let Value::Symbol(other) = val
                && other == s
            {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::LiteralNone(_) => {
            if matches!(val, Value::None) {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::Variant {
            type_path,
            variant,
            args,
            ..
        } => match val {
            Value::Variant {
                union,
                variant: v_name,
                payload,
            } => {
                // Compare the pattern's type path against the value's
                // resolved union FQN. A *non-empty* `type_path` must
                // match as a suffix (e.g. `Shape::Circle` matches
                // `["company", "Shape"]`). An *empty* `type_path` is
                // the unqualified-variant form: skip the union check
                // and bind purely on variant name + payload shape.
                if !type_path.is_empty() && !path_matches(type_path, union) {
                    return None;
                }
                if v_name != variant {
                    return None;
                }
                match_variant_payload(args, payload)
            }
            _ => None,
        },
    }
}

fn path_matches(pat_path: &[String], union_fqn: &[String]) -> bool {
    if pat_path.len() > union_fqn.len() {
        return false;
    }
    let offset = union_fqn.len() - pat_path.len();
    union_fqn[offset..] == *pat_path
}

fn match_variant_payload(
    args: &VariantPatArgs,
    payload: &VariantPayload,
) -> Option<Vec<(String, Value)>> {
    match (args, payload) {
        (VariantPatArgs::Unit, VariantPayload::Unit) => Some(Vec::new()),
        (VariantPatArgs::Positional(inner), VariantPayload::Positional(v)) => {
            match_pattern(inner, v)
        }
        (VariantPatArgs::Record { fields, rest }, VariantPayload::Record(map)) => {
            // If `rest` is false, the parser already enforced that every
            // declared field appears, so we don't need an exhaustiveness
            // check here — just match each pattern against the named
            // value. Unmatched fields (allowed when `rest == true`) are
            // ignored.
            let _ = rest;
            let mut all = Vec::new();
            for (field_name, inner_pat) in fields {
                let field_val = map.get(field_name)?;
                let inner = match_pattern(inner_pat, field_val)?;
                all.extend(inner);
            }
            Some(all)
        }
        _ => None,
    }
}

fn number_lit_to_value(lit: &NumberLit) -> Value {
    match lit {
        NumberLit::I8(n) => Value::I8(*n),
        NumberLit::I16(n) => Value::I16(*n),
        NumberLit::I32(n) => Value::I32(*n),
        NumberLit::I64(n) => Value::I64(*n),
        NumberLit::I128(n) => Value::I128(*n),
        NumberLit::Isize(n) => Value::Isize(*n),
        NumberLit::U8(n) => Value::U8(*n),
        NumberLit::U16(n) => Value::U16(*n),
        NumberLit::U32(n) => Value::U32(*n),
        NumberLit::U64(n) => Value::U64(*n),
        NumberLit::U128(n) => Value::U128(*n),
        NumberLit::Usize(n) => Value::Usize(*n),
        NumberLit::F32(n) => Value::F32(*n),
        NumberLit::F64(n) => Value::F64(*n),
    }
}
