//! Collection builtins: map, filter, fold, len, sum, range, head, tail,
//! plus the tensor constructor/accessors. Registered in
//! [`Environment::new`](crate::Environment::new).

use crate::builtins::{BuiltinFn, Caller, from_fn};
use crate::environment::Environment;
use crate::numeric::for_each_numeric_variant;
use crate::value::Value;

/// Register every collection builtin into `env`.
pub(crate) fn register(env: &mut Environment) {
    env.add_builtin("map", BuiltinFn::hof(2, map_hof));
    env.add_builtin("filter", BuiltinFn::hof(2, filter_hof));
    env.add_builtin("fold", BuiltinFn::hof(3, fold_hof));

    env.add_builtin("len", from_fn(len_pure));
    env.add_builtin("sum", from_fn(sum_pure));
    env.add_builtin("range", from_fn(range_pure));
    env.add_builtin("head", from_fn(head_pure));
    env.add_builtin("tail", from_fn(tail_pure));

    env.add_builtin("tensor", from_fn(tensor_pure));
    env.add_builtin("tensor_data", from_fn(tensor_data_pure));
    env.add_builtin("tensor_shape", from_fn(tensor_shape_pure));

    // `error(msg)` — the evaluator intercepts this name in the pure-builtin
    // dispatch path and raises `EvalError::UserError` directly so the
    // diagnostic carries a structured user_error code. The closure here is
    // a fallback that should never run; if it does (e.g. a host calls the
    // body directly), it still surfaces the message as an error string.
    env.add_builtin(
        "error",
        from_fn(|msg: String| -> Result<Value, String> { Err(msg) }),
    );
}

// ── Higher-order ─────────────────────────────────────────────────────

fn map_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = match &args[1] {
        Value::Function(fv) => fv.clone(),
        other => {
            return Err(format!(
                "map: second argument must be a function, got {}",
                other.type_name()
            ));
        }
    };
    match &args[0] {
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for elem in items {
                out.push(caller.call_fn(&f, std::slice::from_ref(elem))?);
            }
            Ok(Value::List(out))
        }
        Value::Tensor { shape, data } => {
            let shape = shape.clone();
            let mut out = Vec::with_capacity(data.len());
            for elem in data {
                out.push(caller.call_fn(&f, std::slice::from_ref(elem))?);
            }
            Ok(Value::Tensor { shape, data: out })
        }
        other => Err(format!(
            "map: first argument must be a list or tensor, got {}",
            other.type_name()
        )),
    }
}

fn filter_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let items = match &args[0] {
        Value::List(items) => items,
        Value::Tensor { .. } => {
            return Err(
                "filter: tensors are not supported (would invalidate shape); use map or convert to a list"
                    .into(),
            );
        }
        other => {
            return Err(format!(
                "filter: first argument must be a list, got {}",
                other.type_name()
            ));
        }
    };
    let f = match &args[1] {
        Value::Function(fv) => fv.clone(),
        other => {
            return Err(format!(
                "filter: second argument must be a function, got {}",
                other.type_name()
            ));
        }
    };
    let mut out = Vec::new();
    for elem in items {
        match caller.call_fn(&f, std::slice::from_ref(elem))? {
            Value::Bool(true) => out.push(elem.clone()),
            Value::Bool(false) => {}
            other => {
                return Err(format!(
                    "filter: predicate must return bool, got {}",
                    other.type_name()
                ));
            }
        }
    }
    Ok(Value::List(out))
}

fn fold_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let items = match &args[0] {
        Value::List(items) => items.clone(),
        Value::Tensor { data, .. } => data.clone(),
        other => {
            return Err(format!(
                "fold: first argument must be a list or tensor, got {}",
                other.type_name()
            ));
        }
    };
    let f = match &args[2] {
        Value::Function(fv) => fv.clone(),
        other => {
            return Err(format!(
                "fold: third argument must be a function, got {}",
                other.type_name()
            ));
        }
    };
    let mut acc = args[1].clone();
    for elem in items {
        acc = caller.call_fn(&f, &[acc, elem])?;
    }
    Ok(acc)
}

// ── Pure ─────────────────────────────────────────────────────────────

fn len_pure(v: Value) -> Result<i64, String> {
    match v {
        Value::List(items) => Ok(items.len() as i64),
        Value::Tensor { data, .. } => Ok(data.len() as i64),
        Value::Utf8(s) | Value::Ascii(s) => Ok(s.chars().count() as i64),
        other => Err(format!(
            "len: expected list, tensor, or string, got {}",
            other.type_name()
        )),
    }
}

fn range_pure(start: i64, end: i64) -> Result<Vec<i64>, String> {
    if end < start {
        return Err(format!(
            "range: end ({end}) must be greater than or equal to start ({start})"
        ));
    }
    Ok((start..end).collect())
}

fn head_pure(v: Value) -> Result<Value, String> {
    match v {
        Value::List(items) => Ok(items.into_iter().next().unwrap_or(Value::None)),
        Value::Tensor { data, .. } => Ok(data.into_iter().next().unwrap_or(Value::None)),
        other => Err(format!(
            "head: expected list or tensor, got {}",
            other.type_name()
        )),
    }
}

fn tail_pure(v: Value) -> Result<Vec<Value>, String> {
    match v {
        Value::List(items) => {
            if items.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(items.into_iter().skip(1).collect())
            }
        }
        Value::Tensor { data, .. } => Ok(data.into_iter().skip(1).collect()),
        other => Err(format!(
            "tail: expected list or tensor, got {}",
            other.type_name()
        )),
    }
}

/// `sum(list)` — reduces a non-empty homogeneous numeric list. Picks the
/// variant of the first element; every subsequent element must match.
fn sum_pure(v: Value) -> Result<Value, String> {
    let items = match v {
        Value::List(items) => items,
        Value::Tensor { data, .. } => data,
        other => {
            return Err(format!(
                "sum: expected list or tensor, got {}",
                other.type_name()
            ));
        }
    };
    let first = items.first().ok_or("sum: empty list".to_string())?;

    macro_rules! sum_variant {
        ($t:ty, $variant:ident) => {
            if let Value::$variant(first_v) = first {
                let mut acc: $t = *first_v;
                for elem in items.iter().skip(1) {
                    match elem {
                        Value::$variant(n) => {
                            acc += *n;
                        }
                        other => {
                            return Err(format!(
                                "sum: mixed element types ({} and {})",
                                first.type_name(),
                                other.type_name()
                            ));
                        }
                    }
                }
                return Ok(Value::$variant(acc));
            }
        };
    }
    for_each_numeric_variant!(sum_variant);
    Err(format!(
        "sum: list elements must be numeric, got {}",
        first.type_name()
    ))
}

// ── Tensor primitives ────────────────────────────────────────────────

fn tensor_pure(data: Value, shape: Value) -> Result<Value, String> {
    let data = match data {
        Value::List(items) => items,
        other => {
            return Err(format!(
                "tensor: first argument must be a list, got {}",
                other.type_name()
            ));
        }
    };
    let shape_vals = match shape {
        Value::List(items) => items,
        other => {
            return Err(format!(
                "tensor: second argument must be a list of dimensions, got {}",
                other.type_name()
            ));
        }
    };
    if shape_vals.is_empty() {
        return Err("tensor: shape must have at least one dimension".into());
    }
    let mut dims: Vec<u64> = Vec::with_capacity(shape_vals.len());
    for s in &shape_vals {
        let d = s.as_u64().ok_or_else(|| {
            format!(
                "tensor: shape entries must be non-negative integers, got {}",
                s.type_name()
            )
        })?;
        dims.push(d);
    }
    let expected: u64 = dims.iter().copied().fold(1u64, u64::saturating_mul);
    if (data.len() as u64) != expected {
        return Err(format!(
            "tensor: data length {} does not match shape product {}",
            data.len(),
            expected
        ));
    }
    Ok(Value::Tensor { shape: dims, data })
}

fn tensor_data_pure(v: Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Tensor { data, .. } => Ok(data),
        other => Err(format!(
            "tensor_data: expected tensor, got {}",
            other.type_name()
        )),
    }
}

fn tensor_shape_pure(v: Value) -> Result<Vec<i64>, String> {
    match v {
        Value::Tensor { shape, .. } => Ok(shape.into_iter().map(|d| d as i64).collect()),
        other => Err(format!(
            "tensor_shape: expected tensor, got {}",
            other.type_name()
        )),
    }
}
