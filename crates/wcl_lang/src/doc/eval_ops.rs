//! Pure helpers used by the document evaluator: operator naming, unary /
//! binary application, value comparison, expression description.
//!
//! Split out of `doc.rs` so the main module owns the document/cell machinery
//! while same-shape arithmetic lives next to the numeric macros that drive
//! it. Nothing here touches `Document` state — every entry takes the inputs
//! it needs by reference.

use crate::ast::{self, Span};
use crate::error::EvalError;
use crate::numeric::{for_each_numeric_variant, for_each_signed_numeric_variant};
use crate::value::Value;

pub(super) fn format_member_path(expr: &ast::Expr) -> String {
    use ast::Expr as E;
    fn walk(e: &ast::Expr, out: &mut String) {
        match e {
            E::Identifier(s) => out.push_str(s),
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

pub(super) fn describe_expr(expr: &ast::Expr) -> &'static str {
    use ast::Expr as E;
    match expr {
        E::Bool(_) => "bool",
        E::I8(_) | E::I16(_) | E::I32(_) | E::I64(_) | E::I128(_) | E::Isize(_) => "integer",
        E::U8(_) | E::U16(_) | E::U32(_) | E::U64(_) | E::U128(_) | E::Usize(_) => "integer",
        E::F32(_) | E::F64(_) => "float",
        E::Utf8(_) | E::Ascii(_) | E::Utf16(_) | E::Utf32(_) => "string",
        E::Identifier(_) => "identifier",
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
    }
}

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
    }
}

pub(super) fn apply_unary(op: ast::UnaryOp, v: Value, span: Span) -> Result<Value, EvalError> {
    match op {
        ast::UnaryOp::Neg => {
            macro_rules! arm {
                ($t:ty, $variant:ident) => {
                    if let Value::$variant(n) = &v {
                        return Ok(Value::$variant(-*n));
                    }
                };
            }
            for_each_signed_numeric_variant!(arm);
            Err(EvalError::type_mismatch("-", v.type_name(), "—", span))
        }
        ast::UnaryOp::Not => match v {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            other => Err(EvalError::type_mismatch("!", other.type_name(), "—", span)),
        },
    }
}

macro_rules! arith_fn {
    ($name:ident, $op:tt) => {
        fn $name(l: &Value, r: &Value) -> Option<Value> {
            macro_rules! arm {
                                ($t:ty, $variant:ident) => {
                                    if let (Value::$variant(a), Value::$variant(b)) = (l, r) {
                                        return Some(Value::$variant(*a $op *b));
                                    }
                                };
                            }
            for_each_numeric_variant!(arm);
            None
        }
    };
}

arith_fn!(arith_add, +);
arith_fn!(arith_sub, -);
arith_fn!(arith_mul, *);
arith_fn!(arith_div, /);
arith_fn!(arith_mod, %);

pub(super) fn apply_binary(
    op: ast::BinOp,
    l: Value,
    r: Value,
    span: Span,
) -> Result<Value, EvalError> {
    use ast::BinOp as B;
    let mismatch = || EvalError::type_mismatch(op_name(op), l.type_name(), r.type_name(), span);

    match op {
        B::Add => arith_add(&l, &r).ok_or_else(mismatch),
        B::Sub => arith_sub(&l, &r).ok_or_else(mismatch),
        B::Mul => arith_mul(&l, &r).ok_or_else(mismatch),
        B::Div => arith_div(&l, &r).ok_or_else(mismatch),
        B::Mod => arith_mod(&l, &r).ok_or_else(mismatch),
        B::Eq => Ok(Value::Bool(values_eq(&l, &r))),
        B::Ne => Ok(Value::Bool(!values_eq(&l, &r))),
        B::Lt => compare(&l, &r, span, |c| c == std::cmp::Ordering::Less),
        B::Le => compare(&l, &r, span, |c| c != std::cmp::Ordering::Greater),
        B::Gt => compare(&l, &r, span, |c| c == std::cmp::Ordering::Greater),
        B::Ge => compare(&l, &r, span, |c| c != std::cmp::Ordering::Less),
        B::And | B::Or => unreachable!("handled with short-circuit eval"),
    }
}

pub(super) fn values_eq(l: &Value, r: &Value) -> bool {
    // Same-typed equality. Cross-type comparisons (e.g. i32 vs i64) are
    // not equal — there is no implicit numeric coercion.
    l == r
}

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
    None
}

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
