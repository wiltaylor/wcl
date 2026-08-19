//! Record builtins: reading a record's `keys` and `values`, combining
//! two with `merge`, and rewriting one with `map_values`.

use std::collections::BTreeMap;

use super::builtin::{BuiltinFn, Caller, from_fn};
use super::expect_function;
use crate::environment::Environment;
use crate::value::{Value, VariantPayload};

/// Register every record builtin into `env`.
pub(super) fn register(env: &mut Environment) {
    env.add_builtin(
        "keys",
        from_fn(|r: Value| -> Result<Value, String> {
            let fields = record_fields("keys", &r)?;
            Ok(Value::list(
                fields.keys().map(|k| Value::Utf8(k.clone())).collect(),
            ))
        })
        .doc("The field names of a record, in deterministic (sorted) order.")
        .param(
            "r",
            "record",
            "A record value (or a union variant with a record body).",
        )
        .returns("[utf8]", "The field names."),
    );
    env.add_builtin(
        "values",
        from_fn(|r: Value| -> Result<Value, String> {
            let fields = record_fields("values", &r)?;
            Ok(Value::list(fields.values().cloned().collect()))
        })
        .doc("The field values of a record, in the same order as `keys`.")
        .param(
            "r",
            "record",
            "A record value (or a union variant with a record body).",
        )
        .returns("[T]", "The field values."),
    );
    env.add_builtin(
        "merge",
        from_fn(|a: Value, b: Value| -> Result<Value, String> {
            let (a_ty, a_fields) = match a {
                Value::Record { ty, fields } => (ty, fields),
                other => {
                    return Err(format!(
                        "merge: first argument must be a record, got {}",
                        other.type_name()
                    ));
                }
            };
            let b_fields = match b {
                Value::Record { fields, .. } => fields,
                other => {
                    return Err(format!(
                        "merge: second argument must be a record, got {}",
                        other.type_name()
                    ));
                }
            };
            let mut fields = std::sync::Arc::unwrap_or_clone(a_fields);
            fields.extend(std::sync::Arc::unwrap_or_clone(b_fields));
            Ok(Value::record(a_ty, fields))
        })
        .doc("Combine two records into one; fields of `b` win on a name clash.")
        .param("a", "record", "The base record.")
        .param("b", "record", "The overriding record.")
        .returns("record", "A record with the union of both field sets."),
    );
    env.add_builtin(
        "map_values",
        BuiltinFn::hof(2, map_values_hof)
            .doc("Apply a function to every field value of a record, keeping the keys.")
            .param("r", "record", "The record to transform.")
            .param("f", "fn (T) -> U", "Function applied to each field value.")
            .returns(
                "record",
                "A record with the same keys and transformed values.",
            ),
    );
}

/// Borrow the field map out of a record-shaped value: a record literal /
/// projected record, or a union variant with a record body.
fn record_fields<'a>(who: &str, v: &'a Value) -> Result<&'a BTreeMap<String, Value>, String> {
    match v {
        Value::Record { fields, .. } => Ok(fields),
        Value::Variant {
            payload: VariantPayload::Record(fields),
            ..
        } => Ok(fields),
        other => Err(format!(
            "{who}: expected a record, got {}",
            other.type_name()
        )),
    }
}

/// `map_values(record, f)` — apply `f` to each value, keeping keys.
fn map_values_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("map_values", "second argument", &args[1])?.clone();
    let (ty, fields) = match &args[0] {
        Value::Record { ty, fields } => (ty.clone(), fields),
        Value::Variant {
            payload: VariantPayload::Record(fields),
            ..
        } => (Vec::new(), fields),
        other => {
            return Err(format!(
                "map_values: first argument must be a record, got {}",
                other.type_name()
            ));
        }
    };
    let mut out = BTreeMap::new();
    for (k, v) in fields.iter() {
        out.insert(k.clone(), caller.call_fn(&f, std::slice::from_ref(v))?);
    }
    Ok(Value::Record {
        ty,
        fields: std::sync::Arc::new(out),
    })
}
