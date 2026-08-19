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

use crate::data::DataRef;
use crate::value::{FnValue, Value};

/// Convert a [`Value`] into a concrete Rust type for use as a built-in
/// function argument. Returning `Err(String)` produces a
/// [`BuiltinTypeMismatch`](crate::EvalError::BuiltinTypeMismatch)
/// at the call site.
pub trait FromValue: Sized {
    /// Convert a WCL value into this Rust type, or explain why not.
    fn from_value(v: &Value) -> Result<Self, String>;
}

/// Convert a Rust value back into a [`Value`] for return from a built-in.
pub trait IntoValue {
    /// Convert this Rust type into a WCL value.
    fn into_value(self) -> Value;
}

/// Adapter trait used internally by [`from_fn`] so that both
/// `-> R where R: IntoValue` and `-> Result<R, String>` returns work
/// through the same registration path.
pub trait IntoValueResult {
    /// Convert into a WCL value, allowing the conversion to fail — the
    /// return path for builtins that are themselves fallible.
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

/// Internal type of the boxed dispatcher held by a pure [`BuiltinFn`].
pub(crate) type BuiltinBody = Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

/// Internal type of the boxed dispatcher held by a higher-order [`BuiltinFn`].
/// Receives a [`Caller`] so the builtin can invoke `Value::Function` callbacks
/// back through the evaluator.
pub(crate) type BuiltinHofBody =
    Arc<dyn Fn(&mut dyn Caller, &[Value]) -> Result<Value, String> + Send + Sync>;

/// Invokes WCL function values from inside a higher-order builtin.
///
/// The evaluator constructs a `Caller` at every call site that dispatches to
/// an HOF builtin; the builtin receives it as `&mut dyn Caller` and uses
/// [`Caller::call_fn`] to apply user-supplied callbacks (the `fn` argument
/// to `map`, `filter`, `fold`, …) without needing to know anything about
/// the evaluator internals.
pub trait Caller {
    /// Invoke a function value with the supplied arguments. Returns the
    /// callee's result, or a string describing the failure (arity mismatch,
    /// runtime evaluation error, recursion-depth exceeded, …) which is
    /// surfaced through `EvalError::BuiltinTypeMismatch` at the original
    /// call site.
    fn call_fn(&mut self, f: &FnValue, args: &[Value]) -> Result<Value, String>;

    /// Resolve a dotted FQN into a navigator. The first segment is
    /// looked up at document root; subsequent segments walk via
    /// `DataRef::child`. Returns `None` if any segment is missing.
    /// Lets reflective builtins (`decorator_names`, `decorator_arg`,
    /// …) rediscover the target carried in a `Value::DataPath`.
    fn resolve<'r>(&'r self, path: &[String]) -> Option<DataRef<'r>>;

    /// Every top-level declaration (`type` / `interface` / `union` /
    /// `symbol_set`) whose namespace exactly equals `ns`, as resolvable
    /// navigators — backs the `namespace_decls` reflection builtin so a
    /// documentation generator can enumerate a namespace's schema.
    /// Imported declarations are included. Defaults to empty so hosts that
    /// don't wire it up aren't forced to.
    fn decls_in_namespace<'r>(&'r self, _ns: &[String]) -> Vec<DataRef<'r>> {
        Vec::new()
    }

    /// Decorator-schema declarations applicable to the named block kind,
    /// as resolvable navigators. Backs `decorators_for_kind`; defaults to
    /// empty for hosts without a document-backed evaluator.
    fn decorator_schemas_for_kind<'r>(&'r self, _kind: &str) -> Vec<DataRef<'r>> {
        Vec::new()
    }

    /// Structured documentation + signature of a built-in by `name`, for
    /// the `fn_signature` reflection builtin. `None` when no built-in of
    /// that name is registered. Defaults to `None` so hosts that don't
    /// wire it up aren't forced to.
    fn builtin_info(&self, _name: &str) -> Option<BuiltinSignature> {
        None
    }

    /// Names of every registered built-in, for the `builtin_names`
    /// reflection builtin. Defaults to empty so hosts that don't wire it
    /// up aren't forced to.
    fn builtin_names(&self) -> Vec<String> {
        Vec::new()
    }

    /// The multiplier of `unit` on the type named by `type_name` (dotted,
    /// e.g. `"std.ByteSize"`), via its `@unit(unit, factor)` decorator —
    /// the inverse of literal-unit resolution. Backs the `format_unit`
    /// builtin. `None` when the type or unit is unknown. Defaults to `None`
    /// so hosts that don't wire it up aren't forced to.
    fn unit_factor(&self, _type_name: &str, _unit: &str) -> Option<f64> {
        None
    }
}

/// One documented parameter of a built-in function: its name, a printable
/// type (informal notation like `[T]` / `fn (T) -> U` is allowed), and a
/// help string.
#[derive(Debug, Clone)]
pub struct BuiltinParam {
    /// Parameter name, as documentation renders it.
    pub name: String,
    /// Parameter type, as WCL spells it.
    pub ty: String,
    /// One-line description.
    pub doc: String,
}

/// Owned snapshot of a built-in's documentation, returned to reflection
/// (`fn_signature`). `signature` is the resolved printable form.
#[derive(Debug, Clone)]
pub struct BuiltinSignature {
    /// One-line description.
    pub doc: String,
    /// Parameters, in order.
    pub params: Vec<BuiltinParam>,
    /// Return type, as WCL spells it.
    pub return_type: String,
    /// What the return value means.
    pub return_doc: String,
    /// The full signature, pre-rendered for display.
    pub signature: String,
}

/// Body kind for a registered builtin: either a pure function over
/// [`Value`]s, or a higher-order one that needs access to the evaluator
/// via [`Caller`].
#[derive(Clone)]
pub(crate) enum BuiltinKind {
    /// A builtin over values alone.
    Pure(BuiltinBody),
    /// A builtin that takes a function argument, so it needs a `Caller`
    /// to invoke it.
    Hof(BuiltinHofBody),
}

/// A registered built-in. Carries its arity for fast call-site validation
/// plus a boxed dispatch closure that handles arg unmarshalling, the
/// host call, and return marshalling. Optionally carries a printable
/// signature (e.g. `"fn (utf8) -> utf8"`) that tooling can surface in
/// completion and hover popups.
#[derive(Clone)]
pub struct BuiltinFn {
    /// How many arguments the builtin expects.
    pub(crate) arity: usize,
    /// The implementation, and whether it needs a `Caller`.
    pub(crate) kind: BuiltinKind,
    /// Explicit printable signature override. When `None`, [`signature`] is
    /// derived from the structured `params` + `returns`. Kept for the rare
    /// builtin whose shape isn't structurally expressible (e.g. the
    /// variadic `format`).
    pub(crate) signature: Option<String>,
    /// Function-level help text.
    pub(crate) doc: Option<String>,
    /// Documented parameters (name / type / help), in order.
    pub(crate) params: Vec<BuiltinParam>,
    /// Printable return type.
    pub(crate) returns: Option<String>,
    /// Help text describing the return value.
    pub(crate) return_doc: Option<String>,
}

impl BuiltinFn {
    /// How many arguments this builtin expects.
    pub fn arity(&self) -> usize {
        self.arity
    }

    /// Human-readable signature (e.g. `"fn(a: utf8, b: utf8) -> utf8"`).
    /// Uses the explicit override when one was set, otherwise derives it
    /// from the structured `params` + `returns`. `None` when neither is
    /// available.
    pub fn signature(&self) -> Option<String> {
        if let Some(sig) = &self.signature {
            return Some(sig.clone());
        }
        if self.params.is_empty() && self.returns.is_none() {
            return None;
        }
        let params = self
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.ty))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = self.returns.as_deref().unwrap_or("none");
        Some(format!("fn({params}) -> {ret}"))
    }

    /// Owned snapshot of this builtin's documentation, for reflection.
    pub fn signature_info(&self) -> BuiltinSignature {
        BuiltinSignature {
            doc: self.doc.clone().unwrap_or_default(),
            params: self.params.clone(),
            return_type: self.returns.clone().unwrap_or_default(),
            return_doc: self.return_doc.clone().unwrap_or_default(),
            signature: self.signature().unwrap_or_default(),
        }
    }

    /// Attach an explicit printable signature, overriding the derived one.
    /// Use only when the structured form can't express the shape (e.g. a
    /// variadic builtin); prefer `.param(...)` / `.returns(...)`.
    pub fn with_signature(mut self, sig: impl Into<String>) -> Self {
        self.signature = Some(sig.into());
        self
    }

    /// Attach function-level help text. Builder-style:
    /// `from_fn(...).doc("…").param(...).returns(...)`.
    pub fn doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    /// Append a documented parameter (name, printable type, help text).
    pub fn param(
        mut self,
        name: impl Into<String>,
        ty: impl Into<String>,
        doc: impl Into<String>,
    ) -> Self {
        self.params.push(BuiltinParam {
            name: name.into(),
            ty: ty.into(),
            doc: doc.into(),
        });
        self
    }

    /// Set the printable return type and a description of the return value.
    pub fn returns(mut self, ty: impl Into<String>, doc: impl Into<String>) -> Self {
        self.returns = Some(ty.into());
        self.return_doc = Some(doc.into());
        self
    }

    /// Construct a higher-order builtin: one that receives a [`Caller`]
    /// and can invoke `Value::Function` callbacks. The closure handles
    /// arg unmarshalling itself.
    pub fn hof<F>(arity: usize, f: F) -> Self
    where
        F: Fn(&mut dyn Caller, &[Value]) -> Result<Value, String> + Send + Sync + 'static,
    {
        Self {
            arity,
            kind: BuiltinKind::Hof(Arc::new(f)),
            signature: None,
            doc: None,
            params: Vec::new(),
            returns: None,
            return_doc: None,
        }
    }
}

impl std::fmt::Debug for BuiltinFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match &self.kind {
            BuiltinKind::Pure(_) => "pure",
            BuiltinKind::Hof(_) => "hof",
        };
        f.debug_struct("BuiltinFn")
            .field("arity", &self.arity)
            .field("kind", &kind)
            .finish()
    }
}

/// Marker trait wired up per-arity by the `impl_into_builtin!` macro
/// below. Hosts don't implement this directly — they call [`from_fn`].
pub trait IntoBuiltin<Args> {
    /// Wrap a Rust function as a builtin, deriving its arity and
    /// argument conversions from its signature.
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

// ─── IntoBuiltin impls (per arity, 0..=8) ────────────────────────────

/// Implement [`IntoBuiltin`] for functions of one particular arity.
/// Invoked once per supported argument count.
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
                BuiltinFn {
                    arity,
                    kind: BuiltinKind::Pure(body),
                    signature: None,
                    doc: None,
                    params: Vec::new(),
                    returns: None,
                    return_doc: None,
                }
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

    fn pure(b: &BuiltinFn) -> &BuiltinBody {
        match &b.kind {
            BuiltinKind::Pure(body) => body,
            BuiltinKind::Hof(_) => panic!("expected pure builtin"),
        }
    }

    #[test]
    fn signature_derives_from_structured_params() {
        let b = from_fn(|_a: i64| 0i64)
            .doc("Doubles a number.")
            .param("a", "i64", "the input")
            .returns("i64", "the doubled value");
        assert_eq!(b.signature().as_deref(), Some("fn(a: i64) -> i64"));
        let info = b.signature_info();
        assert_eq!(info.doc, "Doubles a number.");
        assert_eq!(info.return_type, "i64");
        assert_eq!(info.return_doc, "the doubled value");
        assert_eq!(info.params.len(), 1);
        assert_eq!(info.params[0].name, "a");
        assert_eq!(info.params[0].doc, "the input");
    }

    #[test]
    fn explicit_signature_overrides_derivation() {
        let b = from_fn(|_a: i64| 0i64)
            .with_signature("fn (utf8, ...args) -> utf8")
            .param("a", "i64", "ignored for display");
        assert_eq!(b.signature().as_deref(), Some("fn (utf8, ...args) -> utf8"));
    }

    #[test]
    fn no_signature_when_undocumented() {
        let b = from_fn(|_a: i64| 0i64);
        assert_eq!(b.signature(), None);
    }

    #[test]
    fn from_fn_zero_arity_infallible() {
        let b = from_fn(|| 42i64);
        assert_eq!(b.arity, 0);
        assert_eq!((pure(&b))(&[]).unwrap(), Value::I64(42));
    }

    #[test]
    fn from_fn_one_arg_string_to_string() {
        let b = from_fn(|s: String| s.to_uppercase());
        assert_eq!(b.arity, 1);
        let out = (pure(&b))(&[Value::Utf8("hi".into())]).unwrap();
        assert_eq!(out, Value::Utf8("HI".into()));
    }

    #[test]
    fn from_fn_two_args_add() {
        let b = from_fn(|x: i64, y: i64| x + y);
        let out = (pure(&b))(&[Value::I64(2), Value::I64(3)]).unwrap();
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
        assert_eq!((pure(&b))(&[Value::I64(2)]).unwrap(), Value::I64(5));
        let err = (pure(&b))(&[Value::I64(0)]).unwrap_err();
        assert_eq!(err, "divide by zero");
    }

    #[test]
    fn from_fn_arity_mismatch() {
        let b = from_fn(|s: String| s);
        let err = (pure(&b))(&[]).unwrap_err();
        assert!(err.contains("arity mismatch"), "{err}");
    }

    #[test]
    fn from_fn_type_mismatch() {
        let b = from_fn(|s: String| s);
        let err = (pure(&b))(&[Value::I64(1)]).unwrap_err();
        assert!(err.contains("expected utf8 string"), "{err}");
    }

    #[test]
    fn from_fn_unit_return() {
        let b = from_fn(|_n: i64| {});
        let out = (pure(&b))(&[Value::I64(1)]).unwrap();
        assert_eq!(out, Value::None);
    }

    #[test]
    fn option_round_trip() {
        let b = from_fn(|x: Option<i64>| match x {
            Some(n) => Value::I64(n * 2),
            None => Value::None,
        });
        assert_eq!((pure(&b))(&[Value::I64(3)]).unwrap(), Value::I64(6));
        assert_eq!((pure(&b))(&[Value::None]).unwrap(), Value::None);
    }

    #[test]
    fn vec_from_value_round_trip() {
        let b = from_fn(|v: Vec<i64>| v.iter().sum::<i64>());
        let args = vec![Value::List(std::sync::Arc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
        ]))];
        assert_eq!((pure(&b))(&args).unwrap(), Value::I64(6));
    }

    #[test]
    fn vec_from_value_type_mismatch() {
        let b = from_fn(|v: Vec<i64>| v.len() as i64);
        // Passing a non-list value should error.
        let err = (pure(&b))(&[Value::I64(42)]).unwrap_err();
        assert!(err.contains("expected list"), "{err}");
    }

    #[test]
    fn vec_into_value_round_trip() {
        let b = from_fn(|_n: i64| -> Vec<i64> { vec![1, 2, 3] });
        let out = (pure(&b))(&[Value::I64(0)]).unwrap();
        assert_eq!(
            out,
            Value::List(std::sync::Arc::new(vec![
                Value::I64(1),
                Value::I64(2),
                Value::I64(3)
            ]))
        );
    }

    #[test]
    fn vec_of_string_round_trip() {
        let b = from_fn(|parts: Vec<String>| parts.join(","));
        let out = (pure(&b))(&[Value::List(std::sync::Arc::new(vec![
            Value::Utf8("a".into()),
            Value::Utf8("b".into()),
        ]))])
        .unwrap();
        assert_eq!(out, Value::Utf8("a,b".into()));
    }
}
