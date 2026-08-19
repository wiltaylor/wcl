//! Pure helpers used by the document evaluator: operator naming, unary /
//! binary application, value comparison, expression description.
//!
//! Split out of `doc.rs` so the main module owns the document/cell machinery
//! while same-shape arithmetic lives next to the numeric macros that drive
//! it. Nothing here touches `Document` state — every entry takes the inputs
//! it needs by reference.

use crate::ast::{self, Span};
use crate::diagnostics::{ArithmeticFault, EvalError};
use crate::numeric::{
    for_each_float_numeric_variant, for_each_integer_numeric_variant, for_each_numeric_variant,
    for_each_signed_integer_numeric_variant,
};
use crate::value::Value;

/// Render a member chain back to source form, for diagnostics.
pub(super) fn format_member_path(expr: &ast::Expr) -> String {
    use ast::Expr as E;
    fn walk(e: &ast::Expr, out: &mut String) {
        match e {
            E::Identifier(s, _) => out.push_str(s),
            E::SelfKw(_) => out.push_str("self"),
            E::ParentKw(_) => out.push_str("parent"),
            E::Member { recv, name, .. } => {
                walk(recv, out);
                out.push('.');
                out.push_str(name);
            }
            _ => out.push_str("<expr>"),
        }
    }
    let mut s = String::new();
    walk(expr, &mut s);
    s
}

/// Name an expression's form, as diagnostics spell it.
pub(super) fn describe_expr(expr: &ast::Expr) -> &'static str {
    use ast::Expr as E;
    match expr {
        E::Bool(_) => "bool",
        E::I8(_) | E::I16(_) | E::I32(_) | E::I64(_) | E::I128(_) | E::Isize(_) => "integer",
        E::U8(_) | E::U16(_) | E::U32(_) | E::U64(_) | E::U128(_) | E::Usize(_) => "integer",
        E::F32(_) | E::F64(_) => "float",
        E::Utf8(_) | E::Ascii(_) | E::Utf16(_) | E::Utf32(_) => "string",
        E::InterpolatedString { .. } => "interpolated string",
        E::Identifier(..) => "identifier",
        E::Symbol(_) => "symbol",
        E::None => "none",
        E::Function(_) => "function literal",
        E::Call { .. } => "call",
        E::Binary { .. } => "binary expression",
        E::Unary { .. } => "unary expression",
        E::Block { .. } => "block expression",
        E::Paren { .. } => "parenthesised expression",
        E::ListLit { .. } => "list literal",
        E::Member { .. } => "member access",
        E::SelfKw(_) => "self",
        E::ParentKw(_) => "parent",
        E::If { .. } => "if expression",
        E::IfLet { .. } => "if-let expression",
        E::Match { .. } => "match expression",
        E::Try { .. } => "try expression",
        E::Variant { .. } => "variant constructor",
        E::Record { .. } => "record literal",
        E::UnitLiteral { .. } => "unit literal",
    }
}

/// Read a value as a `bool`, or report a type mismatch naming `op`.
pub(super) fn as_bool(v: &Value, op: ast::BinOp, span: Span) -> Result<bool, EvalError> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(EvalError::type_mismatch(
            op_name(op),
            other.type_name(),
            "—",
            span,
        )),
    }
}

/// The operator's source spelling, for diagnostics.
pub(super) fn op_name(op: ast::BinOp) -> &'static str {
    match op {
        ast::BinOp::Add => "+",
        ast::BinOp::Sub => "-",
        ast::BinOp::Mul => "*",
        ast::BinOp::Div => "/",
        ast::BinOp::Mod => "%",
        ast::BinOp::Eq => "==",
        ast::BinOp::Ne => "!=",
        ast::BinOp::Lt => "<",
        ast::BinOp::Le => "<=",
        ast::BinOp::Gt => ">",
        ast::BinOp::Ge => ">=",
        ast::BinOp::And => "&&",
        ast::BinOp::Or => "||",
        ast::BinOp::Coalesce => "??",
    }
}

/// Apply a prefix operator.
pub(super) fn apply_unary(op: ast::UnaryOp, v: Value, span: Span) -> Result<Value, EvalError> {
    match op {
        ast::UnaryOp::Neg => {
            // Integers negate through `checked_neg` — `-i8::MIN` has no
            // answer in `i8` and must be a diagnostic, not a debug panic.
            macro_rules! int_arm {
                ($t:ty, $variant:ident) => {
                    if let Value::$variant(n) = &v {
                        return n.checked_neg().map(Value::$variant).ok_or_else(|| {
                            EvalError::arithmetic(
                                "-",
                                ArithmeticFault::overflow(v.type_name()),
                                span,
                            )
                        });
                    }
                };
            }
            for_each_signed_integer_numeric_variant!(int_arm);
            macro_rules! float_arm {
                ($t:ty, $variant:ident) => {
                    if let Value::$variant(n) = &v {
                        return Ok(Value::$variant(-*n));
                    }
                };
            }
            for_each_float_numeric_variant!(float_arm);
            Err(EvalError::type_mismatch("-", v.type_name(), "—", span))
        }
        ast::UnaryOp::Not => match v {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            other => Err(EvalError::type_mismatch("!", other.type_name(), "—", span)),
        },
    }
}

/// Define one same-variant arithmetic function: the integer half of the
/// numeric ladder runs `$int`, which answers or names the fault, while the
/// float half takes the raw `$op` — floats have neither problem, IEEE
/// already answers `1.0 / 0.0`. Stating each op's integer rule in full is
/// the point: what a fallible integer operation *means* differs per
/// operator, and `checked_*` alone gets `%` wrong.
///
/// `$int` is written as a closure-like `|a, b, overflow|` over the two
/// operands (by value, already the same variant) and a constructor for the
/// overflow fault, already carrying the variant's name.
///
/// The result reads on three levels: `Ok(Some(v))` is the answer,
/// `Ok(None)` means the operands weren't the same numeric variant and the
/// caller should promote and retry, and `Err` means they were but the
/// operation has no result.
macro_rules! arith_fn {
    ($name:ident, $op:tt, |$a:ident, $b:ident, $overflow:ident| $int:expr) => {
        fn $name(l: &Value, r: &Value) -> Result<Option<Value>, ArithmeticFault> {
            let $overflow = || ArithmeticFault::overflow(l.type_name());
            macro_rules! int_arm {
                ($t:ty, $variant:ident) => {
                    if let (Value::$variant(a), Value::$variant(b)) = (l, r) {
                        let ($a, $b) = (*a, *b);
                        return $int.map(|n| Some(Value::$variant(n)));
                    }
                };
            }
            for_each_integer_numeric_variant!(int_arm);
            macro_rules! float_arm {
                                ($t:ty, $variant:ident) => {
                                    if let (Value::$variant(a), Value::$variant(b)) = (l, r) {
                                        return Ok(Some(Value::$variant(*a $op *b)));
                                    }
                                };
                            }
            for_each_float_numeric_variant!(float_arm);
            Ok(None)
        }
    };
}

// A zero operand can't make `+`/`-`/`*` miss, so a `checked_*` miss there
// is an overflow and nothing else.
arith_fn!(arith_add, +, |a, b, overflow| a.checked_add(b).ok_or_else(overflow));
arith_fn!(arith_sub, -, |a, b, overflow| a.checked_sub(b).ok_or_else(overflow));
arith_fn!(arith_mul, *, |a, b, overflow| a.checked_mul(b).ok_or_else(overflow));

// `/` misses on two counts, so the divisor is guarded before `checked_div`
// to tell them apart: what's left is `MIN / -1`, a genuine overflow.
arith_fn!(arith_div, /, |a, b, overflow| if b == 0 {
    Err(ArithmeticFault::DivideByZero)
} else {
    a.checked_div(b).ok_or_else(overflow)
});

// `%` can't use `checked_rem` at all: it reports `MIN % -1` as an overflow
// because the matching *division* overflows, but the remainder itself is
// plainly `0`. Calling that "no representable result" would be a lie, so
// `%` guards the divisor and takes the wrapping result — which, for a
// remainder, is the mathematically correct one for every operand pair.
arith_fn!(arith_mod, %, |a, b, _overflow| if b == 0 {
    Err(ArithmeticFault::DivideByZero)
} else {
    Ok(a.wrapping_rem(b))
});

/// Promote two numeric values to a common variant for arithmetic /
/// comparison. Returns `None` when either operand is non-numeric.
/// Ladder: any float → `f64`; otherwise both → `i128`. Unsigned
/// values that don't fit in `i128` (i.e. `u128` magnitudes above
/// `i128::MAX`) return `None` — the caller falls back to a
/// type-mismatch error.
fn promote_pair(l: &Value, r: &Value) -> Option<(Value, Value)> {
    if !l.is_numeric() || !r.is_numeric() {
        return None;
    }
    let any_float =
        matches!(l, Value::F32(_) | Value::F64(_)) || matches!(r, Value::F32(_) | Value::F64(_));
    if any_float {
        return Some((Value::F64(l.as_f64()?), Value::F64(r.as_f64()?)));
    }
    Some((Value::I128(l.as_i128()?), Value::I128(r.as_i128()?)))
}

/// Apply a binary operator, promoting numeric operands to a common
/// type first.
pub(super) fn apply_binary(
    op: ast::BinOp,
    l: Value,
    r: Value,
    span: Span,
) -> Result<Value, EvalError> {
    use ast::BinOp as B;
    let mismatch = || EvalError::type_mismatch(op_name(op), l.type_name(), r.type_name(), span);

    // Helper: try same-typed dispatch first (fast path, preserves
    // the operand's numeric variant); on miss, fall back to a
    // promoted pair and retry. A fault (zero divisor, overflow) is
    // final — the operands matched, so promoting would only re-run the
    // same impossible operation in a wider type.
    type Arith = fn(&Value, &Value) -> Result<Option<Value>, ArithmeticFault>;
    let arith = |same: Arith| -> Result<Value, EvalError> {
        let fault = |f: ArithmeticFault| EvalError::arithmetic(op_name(op), f, span);
        if let Some(v) = same(&l, &r).map_err(fault)? {
            return Ok(v);
        }
        if let Some((pl, pr)) = promote_pair(&l, &r)
            && let Some(v) = same(&pl, &pr).map_err(fault)?
        {
            return Ok(v);
        }
        Err(mismatch())
    };

    match op {
        B::Add => arith(arith_add),
        B::Sub => arith(arith_sub),
        B::Mul => arith(arith_mul),
        B::Div => arith(arith_div),
        B::Mod => arith(arith_mod),
        B::Eq => Ok(Value::Bool(values_eq(&l, &r))),
        B::Ne => Ok(Value::Bool(!values_eq(&l, &r))),
        B::Lt => compare(&l, &r, span, |c| c == std::cmp::Ordering::Less),
        B::Le => compare(&l, &r, span, |c| c != std::cmp::Ordering::Greater),
        B::Gt => compare(&l, &r, span, |c| c == std::cmp::Ordering::Greater),
        B::Ge => compare(&l, &r, span, |c| c != std::cmp::Ordering::Less),
        B::And | B::Or | B::Coalesce => unreachable!("handled with short-circuit eval"),
    }
}

/// Structural equality across values, comparing numbers by magnitude
/// rather than by variant.
pub(super) fn values_eq(l: &Value, r: &Value) -> bool {
    // Fast path: same-typed structural equality.
    if l == r {
        return true;
    }
    // For mixed numeric types, promote to a common form and compare.
    if let Some((pl, pr)) = promote_pair(l, r) {
        return pl == pr;
    }
    false
}

/// Order two numeric values, or `None` when either is not numeric.
fn numeric_cmp(l: &Value, r: &Value) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    macro_rules! arm {
        ($t:ty, $variant:ident) => {
            if let (Value::$variant(a), Value::$variant(b)) = (l, r) {
                return Some(a.partial_cmp(b).unwrap_or(Ordering::Equal));
            }
        };
    }
    for_each_numeric_variant!(arm);
    // Cross-type numeric comparison: promote to a common variant
    // and retry. `promote_pair` returns either two `F64`s or two
    // `I128`s, both of which have well-defined ordering.
    if let Some((pl, pr)) = promote_pair(l, r) {
        macro_rules! arm2 {
            ($t:ty, $variant:ident) => {
                if let (Value::$variant(a), Value::$variant(b)) = (&pl, &pr) {
                    return Some(a.partial_cmp(b).unwrap_or(Ordering::Equal));
                }
            };
        }
        for_each_numeric_variant!(arm2);
    }
    None
}

/// Shared implementation of the ordering operators: compare, then let
/// `pick` turn the ordering into the operator's answer.
fn compare<F>(l: &Value, r: &Value, span: Span, pick: F) -> Result<Value, EvalError>
where
    F: Fn(std::cmp::Ordering) -> bool,
{
    if let Some(ord) = numeric_cmp(l, r) {
        return Ok(Value::Bool(pick(ord)));
    }
    let ord = match (l, r) {
        (Value::Utf8(a), Value::Utf8(b)) | (Value::Ascii(a), Value::Ascii(b)) => a.cmp(b),
        _ => {
            return Err(EvalError::type_mismatch(
                "<>",
                l.type_name(),
                r.type_name(),
                span,
            ));
        }
    };
    Ok(Value::Bool(pick(ord)))
}
