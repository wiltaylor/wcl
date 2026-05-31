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

/// Register a single-argument `f64 -> f64` builtin under `name`, with help text.
fn add_unary(env: &mut Environment, name: &'static str, doc: &'static str, f: fn(f64) -> f64) {
    env.add_builtin(
        name,
        from_fn(move |x: Value| -> Result<Value, String> { Ok(Value::F64(f(num(&x, name)?))) })
            .doc(doc)
            .param(
                "x",
                "number",
                "The input value (any number, widened to f64).",
            )
            .returns("f64"),
    );
}

/// Register a two-argument `(f64, f64) -> f64` builtin under `name`, with help text.
fn add_binary(
    env: &mut Environment,
    name: &'static str,
    doc: &'static str,
    f: fn(f64, f64) -> f64,
) {
    env.add_builtin(
        name,
        from_fn(move |a: Value, b: Value| -> Result<Value, String> {
            Ok(Value::F64(f(num(&a, name)?, num(&b, name)?)))
        })
        .doc(doc)
        .param("a", "number", "The first operand.")
        .param("b", "number", "The second operand.")
        .returns("f64"),
    );
}

/// Register every math builtin into `env`.
pub(crate) fn register(env: &mut Environment) {
    // Trigonometry (radians).
    add_unary(env, "sin", "Sine of an angle in radians.", f64::sin);
    add_unary(env, "cos", "Cosine of an angle in radians.", f64::cos);
    add_unary(env, "tan", "Tangent of an angle in radians.", f64::tan);
    add_unary(
        env,
        "asin",
        "Arcsine, in radians, of a value in [-1, 1].",
        f64::asin,
    );
    add_unary(
        env,
        "acos",
        "Arccosine, in radians, of a value in [-1, 1].",
        f64::acos,
    );
    add_unary(env, "atan", "Arctangent, in radians.", f64::atan);
    add_binary(
        env,
        "atan2",
        "Arctangent of `a/b` in radians, using the signs of both to pick the quadrant.",
        f64::atan2,
    );
    add_unary(
        env,
        "radians",
        "Convert an angle from degrees to radians.",
        f64::to_radians,
    );
    add_unary(
        env,
        "degrees",
        "Convert an angle from radians to degrees.",
        f64::to_degrees,
    );

    // Powers, roots, exponentials, logarithms.
    add_unary(env, "sqrt", "Square root.", f64::sqrt);
    add_unary(env, "cbrt", "Cube root.", f64::cbrt);
    add_binary(env, "pow", "Raise `a` to the power `b`.", f64::powf);
    add_binary(
        env,
        "hypot",
        "Length of the hypotenuse `sqrt(a² + b²)`.",
        f64::hypot,
    );
    add_unary(env, "exp", "`e` raised to the power `x`.", f64::exp);
    add_unary(env, "ln", "Natural (base-`e`) logarithm.", f64::ln);
    add_unary(env, "log10", "Base-10 logarithm.", f64::log10);
    add_unary(env, "log2", "Base-2 logarithm.", f64::log2);

    // Rounding.
    add_unary(
        env,
        "floor",
        "Round down to the nearest integer.",
        f64::floor,
    );
    add_unary(env, "ceil", "Round up to the nearest integer.", f64::ceil);
    add_unary(
        env,
        "round",
        "Round to the nearest integer (ties away from zero).",
        f64::round,
    );
    add_unary(
        env,
        "trunc",
        "Discard the fractional part, rounding toward zero.",
        f64::trunc,
    );

    // Sign / magnitude / comparison.
    add_unary(env, "abs", "Absolute value.", f64::abs);
    add_unary(env, "sign", "The sign of `x`: `1`, `-1`, or `0`.", |x| {
        if x > 0.0 {
            1.0
        } else if x < 0.0 {
            -1.0
        } else {
            0.0
        }
    });
    add_binary(env, "min", "The smaller of two numbers.", f64::min);
    add_binary(env, "max", "The larger of two numbers.", f64::max);
    env.add_builtin(
        "clamp",
        from_fn(|x: Value, lo: Value, hi: Value| -> Result<Value, String> {
            let x = num(&x, "clamp")?;
            let lo = num(&lo, "clamp")?;
            let hi = num(&hi, "clamp")?;
            Ok(Value::F64(x.max(lo).min(hi)))
        })
        .doc("Constrain `x` to the range `[lo, hi]`.")
        .param("x", "number", "The value to clamp.")
        .param("lo", "number", "The lower bound.")
        .param("hi", "number", "The upper bound.")
        .returns("f64"),
    );

    // Constants (nullary).
    env.add_builtin(
        "pi",
        from_fn(|| std::f64::consts::PI)
            .doc("The constant π (≈ 3.14159).")
            .returns("f64"),
    );
    env.add_builtin(
        "tau",
        from_fn(|| std::f64::consts::TAU)
            .doc("The constant τ = 2π (≈ 6.28319).")
            .returns("f64"),
    );
    env.add_builtin(
        "e",
        from_fn(|| std::f64::consts::E)
            .doc("Euler's number e (≈ 2.71828).")
            .returns("f64"),
    );
}
