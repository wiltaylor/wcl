//! The field rule: what a slot declared with a type makes of a value.
//!
//! A *slot* is anywhere a value meets a declared type — a schema field,
//! and, since function signatures are enforced, a function parameter or
//! return value. They share one rule, and this is it:
//!
//! 1. **Convert.** [`coerce_value_to_type`] resolves a unit literal,
//!    narrows or widens a number that fits the declared numeric type,
//!    turns a string into an `identifier`, and infers a bare record's
//!    union variant by shape.
//! 2. **Judge.** [`slot_refusal`] weighs the converted value against the
//!    alias-resolved type: a union refuses another union's variant and a
//!    record no variant matched, every other type goes through
//!    [`value_matches_type_ref`], then
//!    `symbol_set` membership and the constraint decorators on the alias
//!    chain (`@min(1) type Port = u16`).
//!
//! The schema walk ([`schema_check`](crate::doc::schema_check) and the
//! strict root-field check) calls [`slot_refusal`] and phrases the
//! verdict as a field violation; a call phrases it as an argument or
//! return-type error. Neither owns a type checker of its own.

use crate::ast::{self, TypeRef};
use crate::diagnostics::EvalError;
use crate::doc::Document;
use crate::value::Value;

use super::inhabit::{coerce_value_to_type, numeric_misfit, value_matches_type_ref};

/// Why a slot refused a value. Each caller renders it in its own words.
#[derive(Debug, Clone)]
pub(crate) enum Refusal {
    /// The slot names a union and the value is a variant of another one.
    OtherUnion {
        /// The declared union, fully qualified.
        expected: String,
        /// The value's union, fully qualified.
        union: String,
        /// The value's variant.
        variant: String,
    },
    /// The slot names a union and the value is a bare record no variant's
    /// shape matched.
    UninferredRecord {
        /// The declared union, fully qualified.
        expected: String,
    },
    /// The value does not inhabit the type ([`value_matches_type_ref`]).
    Type,
    /// The slot names a `symbol_set` and the symbol is not a member.
    NotInSet {
        /// The set as written in the declaration.
        set: String,
        /// The symbol, without its leading `:`.
        symbol: String,
    },
    /// A constraint decorator (`@min`, `@max`, `@non_empty`) on the slot
    /// or its alias chain rejected the value. The message is the
    /// constraint's own wording.
    Constraint(String),
}

impl Refusal {
    /// What the slot was handed, phrased to follow "got": the value's
    /// type, or the reason it misses when its type alone does not say.
    /// `resolved` is the alias-resolved slot type.
    pub(crate) fn describe_got(&self, value: &Value, resolved: &TypeRef) -> String {
        match self {
            Self::OtherUnion { union, variant, .. } => format!("{union}::{variant}"),
            Self::UninferredRecord { expected } => {
                format!("a record that matches no variant of '{expected}'")
            }
            Self::Type => describe_misfit(value, resolved),
            Self::NotInSet { set, symbol } => {
                format!(":{symbol}, which is not a member of '{set}'")
            }
            Self::Constraint(message) => format!("{}, but {message}", value_kind(value)),
        }
    }
}

/// What a value refused by [`value_matches_type_ref`] is, to follow
/// "got": a number that does not fit says why (`300, which is out of
/// range for u8`), a list names its first offending element (`a list
/// whose element [1] is utf8`), and anything else gives its type.
fn describe_misfit(value: &Value, resolved: &TypeRef) -> String {
    if let Some((path, number, why)) = numeric_misfit(value, resolved)
        && path.is_empty()
    {
        return format!("{number}, which {why}");
    }
    if let (Value::List(items), TypeRef::List(inner)) = (value, resolved)
        && let Some((i, item)) = items
            .iter()
            .enumerate()
            .find(|(_, item)| !matches!(item, Value::None) && !value_matches_type_ref(item, inner))
    {
        return format!(
            "a list whose element [{i}] is {}",
            describe_misfit(item, inner)
        );
    }
    value_kind(value)
}

/// The value's type as a reader would name it: the declaration a record
/// or variant carries when it has one, otherwise the value's type name.
pub(crate) fn value_kind(value: &Value) -> String {
    match value {
        Value::Record { ty, .. } if !ty.is_empty() => ty.join("."),
        Value::Variant { union, variant, .. } => format!("{}::{variant}", union.join(".")),
        other => other.type_name().to_string(),
    }
}

/// Weigh an already-converted `value` against a slot declared as
/// `declared`, whose alias-resolved form is `resolved`. `decorators` are
/// the slot declaration's own (a schema field's `@min`, say); the alias
/// chain's are found here. `context_ns` is the namespace the declaration
/// was written in. `None` when the slot accepts the value.
///
/// An optional slot handed `none` is the caller's business: there is no
/// value left to judge, so callers skip the call.
pub(crate) fn slot_refusal(
    doc: &Document,
    value: &Value,
    declared: &TypeRef,
    resolved: &TypeRef,
    decorators: &[ast::Decorator],
    context_ns: &[String],
) -> Option<Refusal> {
    // A union accepts its own variants. Anything else that is neither a
    // variant nor a bare record passes: the union check is about which
    // union a variant belongs to, not a scalar type check.
    if let TypeRef::Named { path, .. } = resolved
        && let Some(union_decl) = doc.union_decl(&path.join("."))
    {
        let expected = union_decl.ast.name.join(".");
        return match value {
            Value::Variant { union, variant, .. } if union != &union_decl.ast.name => {
                Some(Refusal::OtherUnion {
                    expected,
                    union: union.join("."),
                    variant: variant.clone(),
                })
            }
            Value::Record { .. } => Some(Refusal::UninferredRecord { expected }),
            _ => None,
        };
    }
    if !value_matches_type_ref(value, resolved) {
        return Some(Refusal::Type);
    }
    if let Some((set, symbol)) = symbol_outside_set(doc, resolved, value, context_ns) {
        return Some(Refusal::NotInSet { set, symbol });
    }
    crate::doc::schema_check::constraint_violation(doc, decorators, declared, context_ns, value)
        .map(Refusal::Constraint)
}

/// When `ty` (already alias-resolved) names a `symbol_set` and `value` is
/// a symbol outside it: the set as written and the symbol.
pub(crate) fn symbol_outside_set(
    doc: &Document,
    ty: &TypeRef,
    value: &Value,
    context_ns: &[String],
) -> Option<(String, String)> {
    let TypeRef::Named { path, .. } = ty else {
        return None;
    };
    let Value::Symbol(sym) = value else {
        return None;
    };
    let resolved = doc
        .resolve_path_in(path, context_ns)
        .unwrap_or_else(|| path.clone());
    let set = doc.symbol_set(&resolved.join("."))?;
    if set.has(sym) {
        return None;
    }
    Some((path.join("."), sym.clone()))
}

/// The field rule end to end: convert `value` for a slot declared `ty`
/// (written in `context_ns`, with no decorators of its own), then judge
/// it. `Ok(Ok(converted))` when the slot accepts it, `Ok(Err(..))` with
/// the alias-resolved type and the refusal when it does not, and `Err`
/// when conversion itself fails (a unit the type does not declare, a
/// record whose shape picks no variant).
pub(crate) fn admit_to_slot(
    doc: &Document,
    value: Value,
    ty: &TypeRef,
    context_ns: &[String],
    span: ast::Span,
) -> Result<Result<Value, (Value, TypeRef, Refusal)>, EvalError> {
    let value = coerce_value_to_type(doc, value, ty, span)?;
    let resolved = match ty {
        // A builtin has nothing to resolve; skip the clone-and-walk.
        TypeRef::Builtin(_) => None,
        _ => Some(doc.resolve_alias_in(ty, context_ns)),
    };
    let resolved_ref = resolved.as_ref().unwrap_or(ty);
    match slot_refusal(doc, &value, ty, resolved_ref, &[], context_ns) {
        None => Ok(Ok(value)),
        Some(refusal) => Ok(Err((
            value,
            resolved.unwrap_or_else(|| ty.clone()),
            refusal,
        ))),
    }
}

/// A schema field's [`Refusal`], as the violation the schema walk reports
/// for field `field_name` written at `span`.
pub(crate) fn field_refusal_error(
    field_name: &str,
    declared: &TypeRef,
    resolved: &TypeRef,
    value: &Value,
    refusal: Refusal,
    span: ast::Span,
) -> EvalError {
    use crate::diagnostics::SchemaViolationKind as Kind;
    match refusal {
        Refusal::OtherUnion {
            expected,
            union,
            variant,
        } => EvalError::schema_violation(
            Kind::VariantUnionMismatch,
            format!(
                "field '{field_name}' declared as union '{expected}' but value is {union}::{variant}"
            ),
            span,
        ),
        Refusal::UninferredRecord { expected } => EvalError::schema_violation(
            Kind::VariantNoMatch,
            format!(
                "field '{field_name}' declared as union '{expected}' but value is an \
                 un-inferred record (no variant matches its shape)"
            ),
            span,
        ),
        Refusal::Type => {
            super::inhabit::field_type_mismatch(field_name, declared, resolved, value, span)
        }
        Refusal::NotInSet { set, symbol } => EvalError::schema_violation(
            Kind::SymbolNotInSet,
            format!(
                "field '{field_name}' declared as symbol_set '{set}' but ':{symbol}' is not one of its members"
            ),
            span,
        ),
        Refusal::Constraint(message) => EvalError::schema_violation(
            Kind::ConstraintViolation,
            format!("field '{field_name}': {message}"),
            span,
        ),
    }
}
