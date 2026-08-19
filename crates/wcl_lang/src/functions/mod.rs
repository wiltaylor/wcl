//! Callable functions: the Rust↔WCL bridge, and the builtins built on it.
//!
//! Two halves. [`builtin`] and [`convert`] are the *machinery* — how a
//! Rust function becomes something a WCL `Call` expression can invoke,
//! and how values cross that boundary in each direction. Everything
//! else is a *family* of builtins registered through it, one module per
//! family: [`list`], [`string`], [`record`], [`tensor`] and
//! [`diagnostics`].
//!
//! Each family owns its own `register` and the implementations behind
//! it, so adding a builtin means touching one file. [`register`] here
//! is only the fan-out, and is called from
//! [`Environment::new`](crate::Environment::new).
//!
//! The other builtin families live outside this module because they are
//! bound to a subsystem rather than to a value shape: `math`, `units`,
//! `paths` and `reflect` each register from their own module.

pub mod builtin;
mod convert;
mod diagnostics;
mod list;
mod record;
mod string;
mod tensor;

pub use builtin::{
    BuiltinFn, BuiltinSignature, Caller, FromValue, IntoBuiltin, IntoValue, IntoValueResult,
    from_fn,
};
pub use convert::DataPath;

pub(crate) use builtin::BuiltinKind;

pub(crate) use string::format_value;

use crate::environment::Environment;
use crate::value::Value;

/// Register every builtin this module owns into `env`, family by
/// family.
pub(crate) fn register(env: &mut Environment) {
    list::register(env);
    string::register(env);
    record::register(env);
    tensor::register(env);
    diagnostics::register(env);
}

/// Borrow a `&FnValue` from `value`, producing a uniform diagnostic
/// `"{builtin}: {what} must be a function, got {ty}"` on mismatch.
/// `what` is interpolated as-is, so prefer phrases like
/// `"second argument"` (the callsite reads naturally with or without
/// "the").
fn expect_function<'a>(
    builtin: &str,
    what: &str,
    value: &'a Value,
) -> Result<&'a crate::value::FnValue, String> {
    match value {
        Value::Function(fv) => Ok(fv),
        other => Err(format!(
            "{builtin}: {what} must be a function, got {}",
            other.type_name()
        )),
    }
}
