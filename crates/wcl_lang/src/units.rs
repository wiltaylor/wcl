//! Literal-unit formatting builtins.
//!
//! Literal units (`5MiB`) are *parsed* into a base-unit number by the
//! evaluator (see `doc::variant_dispatch::resolve_unit_literal`). These
//! builtins go the other way — rendering a stored base-unit number back as
//! a chosen unit:
//!
//! - [`format_unit`] (`format_unit(&field, "MiB")`) reads the field's
//!   declared type's `@unit` factor and divides — the exact inverse of
//!   resolution, with no hard-coded factor.
//! - [`format_unit_value`] (`format_unit_value(n, factor, "MiB")`) is the
//!   lower-level primitive for callers that already hold the factor.

use crate::environment::Environment;
use crate::functions::{BuiltinFn, Caller, FromValue, from_fn};
use crate::value::Value;

/// Register the unit-conversion builtins.
pub(crate) fn register(env: &mut Environment) {
    env.add_builtin(
        "format_unit",
        BuiltinFn::hof(3, format_unit_hof)
            .doc("Render a base-unit value in a chosen unit, looking the factor up from a unit type by name: `format_unit(size, \"std.ByteSize\", \"MiB\")` → `\"5 MiB\"`. The inverse of literal-unit resolution, so it stays correct if the type's `@unit` factor changes.")
            .param("value", "i64", "The stored value, in the type's base unit.")
            .param("type", "utf8", "The unit type's dotted name, e.g. `\"std.ByteSize\"`.")
            .param("unit", "utf8", "The unit to render in, e.g. `\"MiB\"`.")
            .returns("utf8", "The value divided by the unit's factor, suffixed with the unit (e.g. `\"5 MiB\"`)."),
    );
    env.add_builtin(
        "format_unit_value",
        from_fn(format_unit_value)
            .doc("Render a number in a unit given its factor explicitly: `format_unit_value(5242880, 1048576, \"MiB\")` → `\"5 MiB\"`. The primitive behind `format_unit` for callers that already hold the factor.")
            .param("value", "i64", "The stored value, in the type's base unit.")
            .param("factor", "i64", "The unit's multiplier (base units per one unit).")
            .param("unit", "utf8", "The unit label to append.")
            .returns("utf8", "`value / factor` followed by the unit label."),
    );
}

/// `format_unit(value, type_name, unit)` — resolve the unit's factor from
/// the named type (via [`Caller::unit_factor`]) and render.
fn format_unit_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let n = args[0]
        .as_f64()
        .ok_or("format_unit: value must be numeric")?;
    let type_name = String::from_value(&args[1])?;
    let unit = String::from_value(&args[2])?;
    let factor = caller
        .unit_factor(&type_name, &unit)
        .ok_or_else(|| format!("format_unit: '{unit}' is not a unit of type '{type_name}'"))?;
    if factor == 0.0 {
        return Err("format_unit: unit factor must be non-zero".into());
    }
    Ok(Value::Utf8(render_unit(n / factor, &unit)))
}

/// `format_unit_value(value, factor, unit)`. `value` / `factor` accept any
/// numeric variant (the resolved value is whatever the type's base unit
/// is — typically `i64`), so they come in as `Value` and widen here.
fn format_unit_value(value: Value, factor: Value, unit: String) -> Result<String, String> {
    let n = value
        .as_f64()
        .ok_or("format_unit_value: value must be numeric")?;
    let f = factor
        .as_f64()
        .ok_or("format_unit_value: factor must be numeric")?;
    if f == 0.0 {
        return Err("format_unit_value: factor must be non-zero".into());
    }
    Ok(render_unit(n / f, &unit))
}

/// Render `value` followed by `unit`, dropping a trailing `.0` when the
/// value is a whole number (`5 MiB`, not `5.0 MiB`).
pub(crate) fn render_unit(value: f64, unit: &str) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{} {unit}", value as i64)
    } else {
        format!("{value} {unit}")
    }
}
