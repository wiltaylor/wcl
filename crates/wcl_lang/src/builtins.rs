//! Built-in functions exposed from Rust to WCL.
//!
//! Hosts register Rust closures with an [`Environment`](crate::Environment) and
//! WCL code can invoke them through `Call` expressions:
//!
//! ```ignore
//! let mut env = Environment::new();
//! env.add_builtin("upper", from_fn(|s: String| s.to_uppercase()));
//! ```
//!
//! `from_fn` accepts both infallible (`-> R`) and fallible
//! (`-> Result<R, String>`) closures via the [`IntoValueResult`] trait.
//! Closures with 0..=8 typed parameters are supported.

use std::sync::Arc;

use crate::numeric::for_each_numeric_variant;
use crate::value::Value;

/// Convert a [`Value`] into a concrete Rust type for use as a built-in
/// function argument. Returning `Err(String)` produces a
/// [`BuiltinTypeMismatch`](crate::EvalError::BuiltinTypeMismatch)
/// at the call site.
pub trait FromValue: Sized {
    fn from_value(v: &Value) -> Result<Self, String>;
}

/// Convert a Rust value back into a [`Value`] for return from a built-in.
pub trait IntoValue {
    fn into_value(self) -> Value;
}

/// Adapter trait used internally by [`from_fn`] so that both
/// `-> R where R: IntoValue` and `-> Result<R, String>` returns work
/// through the same registration path.
pub trait IntoValueResult {
    fn into_value_result(self) -> Result<Value, String>;
}

impl<T> IntoValueResult for T
where
    T: IntoValue,
{
    fn into_value_result(self) -> Result<Value, String> {
        Ok(self.into_value())
    }
}

impl<T> IntoValueResult for Result<T, String>
where
    T: IntoValue,
{
    fn into_value_result(self) -> Result<Value, String> {
        self.map(IntoValue::into_value)
    }
}

/// Internal type of the boxed dispatcher held by [`BuiltinFn`].
pub(crate) type BuiltinBody = Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

/// A registered built-in. Carries its arity for fast call-site validation
/// plus a boxed dispatch closure that handles arg unmarshalling, the
/// host call, and return marshalling.
#[derive(Clone)]
pub struct BuiltinFn {
    pub(crate) arity: usize,
    pub(crate) body: BuiltinBody,
}

impl BuiltinFn {
    pub fn arity(&self) -> usize {
        self.arity
    }
}

impl std::fmt::Debug for BuiltinFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuiltinFn")
            .field("arity", &self.arity)
            .finish()
    }
}

/// Marker trait wired up per-arity by the `impl_into_builtin!` macro
/// below. Hosts don't implement this directly — they call [`from_fn`].
pub trait IntoBuiltin<Args> {
    fn into_builtin(self) -> BuiltinFn;
}

/// Wrap any closure of arity 0..=8 with appropriately-typed parameters
/// into a [`BuiltinFn`]. The closure's parameter types must each
/// implement [`FromValue`] and its return type must implement
/// [`IntoValueResult`] (which covers both `R: IntoValue` and
/// `Result<R, String> where R: IntoValue`).
pub fn from_fn<Func, Args>(f: Func) -> BuiltinFn
where
    Func: IntoBuiltin<Args>,
{
    f.into_builtin()
}

// ─── FromValue impls ─────────────────────────────────────────────────

macro_rules! from_value_scalar {
    ($t:ty, $variant:ident) => {
        impl FromValue for $t {
            fn from_value(v: &Value) -> Result<Self, String> {
                match v {
                    Value::$variant(n) => Ok(*n),
                    other => Err(format!(
                        "expected {}, found {}",
                        stringify!($t),
                        other.type_name()
                    )),
                }
            }
        }
    };
}

from_value_scalar!(bool, Bool);
for_each_numeric_variant!(from_value_scalar);

impl FromValue for String {
    fn from_value(v: &Value) -> Result<Self, String> {
        match v {
            Value::Utf8(s) | Value::Ascii(s) => Ok(s.clone()),
            other => Err(format!("expected utf8 string, found {}", other.type_name())),
        }
    }
}

impl FromValue for Value {
    fn from_value(v: &Value) -> Result<Self, String> {
        Ok(v.clone())
    }
}

impl<T> FromValue for Option<T>
where
    T: FromValue,
{
    fn from_value(v: &Value) -> Result<Self, String> {
        if matches!(v, Value::None) {
            Ok(None)
        } else {
            T::from_value(v).map(Some)
        }
    }
}

impl<T> FromValue for Vec<T>
where
    T: FromValue,
{
    fn from_value(v: &Value) -> Result<Self, String> {
        match v {
            Value::List(items) => items.iter().map(T::from_value).collect(),
            other => Err(format!("expected list, found {}", other.type_name())),
        }
    }
}

// ─── IntoValue impls ─────────────────────────────────────────────────

macro_rules! into_value_scalar {
    ($t:ty, $variant:ident) => {
        impl IntoValue for $t {
            fn into_value(self) -> Value {
                Value::$variant(self)
            }
        }
    };
}

into_value_scalar!(bool, Bool);
for_each_numeric_variant!(into_value_scalar);

impl IntoValue for String {
    fn into_value(self) -> Value {
        Value::Utf8(self)
    }
}

impl IntoValue for &str {
    fn into_value(self) -> Value {
        Value::Utf8(self.to_string())
    }
}

impl IntoValue for () {
    fn into_value(self) -> Value {
        Value::None
    }
}

impl IntoValue for Value {
    fn into_value(self) -> Value {
        self
    }
}

impl<T> IntoValue for Option<T>
where
    T: IntoValue,
{
    fn into_value(self) -> Value {
        match self {
            Some(t) => t.into_value(),
            None => Value::None,
        }
    }
}

impl<T> IntoValue for Vec<T>
where
    T: IntoValue,
{
    fn into_value(self) -> Value {
        Value::List(self.into_iter().map(T::into_value).collect())
    }
}

// ─── IntoBuiltin impls (per arity, 0..=8) ────────────────────────────

macro_rules! impl_into_builtin {
    ($($name:ident: $T:ident),*) => {
        impl<Func, R, $($T,)*> IntoBuiltin<($($T,)*)> for Func
        where
            Func: Fn($($T),*) -> R + Send + Sync + 'static,
            $($T: FromValue,)*
            R: IntoValueResult,
        {
            #[allow(non_snake_case, unused_assignments, unused_mut, unused_variables)]
            fn into_builtin(self) -> BuiltinFn {
                let arity: usize = 0usize $(+ { let _ = stringify!($T); 1 })*;
                let body: BuiltinBody =
                    Arc::new(move |args: &[Value]| -> Result<Value, String> {
                        if args.len() != arity {
                            return Err(format!(
                                "arity mismatch: expected {arity}, got {}",
                                args.len()
                            ));
                        }
                        let mut idx = 0usize;
                        $(
                            let $name: $T = <$T as FromValue>::from_value(&args[idx])?;
                            idx += 1;
                        )*
                        (self)($($name),*).into_value_result()
                    });
                BuiltinFn { arity, body }
            }
        }
    };
}

impl_into_builtin!();
impl_into_builtin!(a: A);
impl_into_builtin!(a: A, b: B);
impl_into_builtin!(a: A, b: B, c: C);
impl_into_builtin!(a: A, b: B, c: C, d: D);
impl_into_builtin!(a: A, b: B, c: C, d: D, e: E);
impl_into_builtin!(a: A, b: B, c: C, d: D, e: E, g: G);
impl_into_builtin!(a: A, b: B, c: C, d: D, e: E, g: G, h: H);
impl_into_builtin!(a: A, b: B, c: C, d: D, e: E, g: G, h: H, i: I);

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_fn_zero_arity_infallible() {
        let b = from_fn(|| 42i64);
        assert_eq!(b.arity, 0);
        assert_eq!((b.body)(&[]).unwrap(), Value::I64(42));
    }

    #[test]
    fn from_fn_one_arg_string_to_string() {
        let b = from_fn(|s: String| s.to_uppercase());
        assert_eq!(b.arity, 1);
        let out = (b.body)(&[Value::Utf8("hi".into())]).unwrap();
        assert_eq!(out, Value::Utf8("HI".into()));
    }

    #[test]
    fn from_fn_two_args_add() {
        let b = from_fn(|x: i64, y: i64| x + y);
        let out = (b.body)(&[Value::I64(2), Value::I64(3)]).unwrap();
        assert_eq!(out, Value::I64(5));
    }

    #[test]
    fn from_fn_fallible_result() {
        let b = from_fn(|n: i64| -> Result<i64, String> {
            if n == 0 {
                Err("divide by zero".into())
            } else {
                Ok(10 / n)
            }
        });
        assert_eq!((b.body)(&[Value::I64(2)]).unwrap(), Value::I64(5));
        let err = (b.body)(&[Value::I64(0)]).unwrap_err();
        assert_eq!(err, "divide by zero");
    }

    #[test]
    fn from_fn_arity_mismatch() {
        let b = from_fn(|s: String| s);
        let err = (b.body)(&[]).unwrap_err();
        assert!(err.contains("arity mismatch"), "{err}");
    }

    #[test]
    fn from_fn_type_mismatch() {
        let b = from_fn(|s: String| s);
        let err = (b.body)(&[Value::I64(1)]).unwrap_err();
        assert!(err.contains("expected utf8 string"), "{err}");
    }

    #[test]
    fn from_fn_unit_return() {
        let b = from_fn(|_n: i64| {});
        let out = (b.body)(&[Value::I64(1)]).unwrap();
        assert_eq!(out, Value::None);
    }

    #[test]
    fn option_round_trip() {
        let b = from_fn(|x: Option<i64>| match x {
            Some(n) => Value::I64(n * 2),
            None => Value::None,
        });
        assert_eq!((b.body)(&[Value::I64(3)]).unwrap(), Value::I64(6));
        assert_eq!((b.body)(&[Value::None]).unwrap(), Value::None);
    }

    #[test]
    fn vec_from_value_round_trip() {
        let b = from_fn(|v: Vec<i64>| v.iter().sum::<i64>());
        let args = vec![Value::List(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
        ])];
        assert_eq!((b.body)(&args).unwrap(), Value::I64(6));
    }

    #[test]
    fn vec_from_value_type_mismatch() {
        let b = from_fn(|v: Vec<i64>| v.len() as i64);
        // Passing a non-list value should error.
        let err = (b.body)(&[Value::I64(42)]).unwrap_err();
        assert!(err.contains("expected list"), "{err}");
    }

    #[test]
    fn vec_into_value_round_trip() {
        let b = from_fn(|_n: i64| -> Vec<i64> { vec![1, 2, 3] });
        let out = (b.body)(&[Value::I64(0)]).unwrap();
        assert_eq!(
            out,
            Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
        );
    }

    #[test]
    fn vec_of_string_round_trip() {
        let b = from_fn(|parts: Vec<String>| parts.join(","));
        let out = (b.body)(&[Value::List(vec![
            Value::Utf8("a".into()),
            Value::Utf8("b".into()),
        ])])
        .unwrap();
        assert_eq!(out, Value::Utf8("a,b".into()));
    }
}
