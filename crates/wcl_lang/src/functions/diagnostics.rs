//! Diagnostic builtins: `error` and `panic` raise an evaluation
//! failure in the document author's own words, and `assert` raises one
//! conditionally.

use super::builtin::from_fn;
use crate::environment::Environment;
use crate::value::Value;

/// Register the diagnostic builtins into `env`.
pub(super) fn register(env: &mut Environment) {
    env.add_builtin(
        "error",
        from_fn(|msg: String| -> Result<Value, String> { Err(msg) })
            .doc("Abort evaluation with an error message.")
            .param("msg", "utf8", "The error message to report.")
            .returns("never", "Never returns — aborts evaluation."),
    );
    env.add_builtin(
        "panic",
        from_fn(|msg: String| -> Result<Value, String> { Err(msg) })
            .doc("Abort evaluation with an unrecoverable failure message.")
            .param("msg", "utf8", "The failure message to report.")
            .returns("never", "Never returns — aborts evaluation."),
    );
    env.add_builtin(
        "assert",
        from_fn(|cond: bool, msg: String| -> Result<Value, String> {
            if cond { Ok(Value::None) } else { Err(msg) }
        })
        .doc("Return `none` when `cond` is true, otherwise abort with `msg`.")
        .param("cond", "bool", "The condition that must hold.")
        .param(
            "msg",
            "utf8",
            "The error message reported when `cond` is false.",
        )
        .returns(
            "none",
            "`none` when the assertion holds (otherwise aborts).",
        ),
    );
}
