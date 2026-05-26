//! Math / geometry builtins: trigonometry, powers and roots, rounding,
//! sign / comparison helpers, and the `pi` / `tau` / `e` constants.
//!
//! Every function works in `f64`. Arguments are accepted as any numeric
//! `Value` and lossily widened via [`Value::as_f64`], so `sin(1)` and
//! `sin(1.0)` both evaluate. Results are always `f64` (a `bool` for the
//! handful of predicates), which composes with WCL's implicit numeric
//! promotion in arithmetic. Registered in
//! [`Environment::new`](crate::Environment::new).

use crate::builtins::from_fn;
use crate::environment::Environment;
use crate::value::Value;

/// Widen a numeric argument to `f64`, or describe the mismatch for the
/// `BuiltinTypeMismatch` surfaced at the call site.
fn num(v: &Value, fname: &str) -> Result<f64, String> {
    v.as_f64()
        .ok_or_else(|| format!("{fname}: expected a number, found {}", v.type_name()))
}

/// Register a single-argument `f64 -> f64` builtin under `name`.
fn add_unary(env: &mut Environment, name: &'static str, f: fn(f64) -> f64) {
    env.add_builtin(
        name,
        from_fn(move |x: Value| -> Result<Value, String> { Ok(Value::F64(f(num(&x, name)?))) })
            .with_signature("fn (number) -> f64"),
    );
}

/// Register a two-argument `(f64, f64) -> f64` builtin under `name`.
fn add_binary(env: &mut Environment, name: &'static str, f: fn(f64, f64) -> f64) {
    env.add_builtin(
        name,
        from_fn(move |a: Value, b: Value| -> Result<Value, String> {
            Ok(Value::F64(f(num(&a, name)?, num(&b, name)?)))
        })
        .with_signature("fn (number, number) -> f64"),
    );
}

/// Register every math builtin into `env`.
pub(crate) fn register(env: &mut Environment) {
    // Trigonometry (radians).
    add_unary(env, "sin", f64::sin);
    add_unary(env, "cos", f64::cos);
    add_unary(env, "tan", f64::tan);
    add_unary(env, "asin", f64::asin);
    add_unary(env, "acos", f64::acos);
    add_unary(env, "atan", f64::atan);
    add_binary(env, "atan2", f64::atan2);
    add_unary(env, "radians", f64::to_radians);
    add_unary(env, "degrees", f64::to_degrees);

    // Powers, roots, exponentials, logarithms.
    add_unary(env, "sqrt", f64::sqrt);
    add_unary(env, "cbrt", f64::cbrt);
    add_binary(env, "pow", f64::powf);
    add_binary(env, "hypot", f64::hypot);
    add_unary(env, "exp", f64::exp);
    add_unary(env, "ln", f64::ln);
    add_unary(env, "log10", f64::log10);
    add_unary(env, "log2", f64::log2);

    // Rounding.
    add_unary(env, "floor", f64::floor);
    add_unary(env, "ceil", f64::ceil);
    add_unary(env, "round", f64::round);
    add_unary(env, "trunc", f64::trunc);

    // Sign / magnitude / comparison.
    add_unary(env, "abs", f64::abs);
    add_unary(env, "sign", |x| {
        if x > 0.0 {
            1.0
        } else if x < 0.0 {
            -1.0
        } else {
            0.0
        }
    });
    add_binary(env, "min", f64::min);
    add_binary(env, "max", f64::max);
    env.add_builtin(
        "clamp",
        from_fn(|x: Value, lo: Value, hi: Value| -> Result<Value, String> {
            let x = num(&x, "clamp")?;
            let lo = num(&lo, "clamp")?;
            let hi = num(&hi, "clamp")?;
            Ok(Value::F64(x.max(lo).min(hi)))
        })
        .with_signature("fn (number, number, number) -> f64"),
    );

    // Constants (nullary).
    env.add_builtin(
        "pi",
        from_fn(|| std::f64::consts::PI).with_signature("fn () -> f64"),
    );
    env.add_builtin(
        "tau",
        from_fn(|| std::f64::consts::TAU).with_signature("fn () -> f64"),
    );
    env.add_builtin(
        "e",
        from_fn(|| std::f64::consts::E).with_signature("fn () -> f64"),
    );
}
