//! Collection builtins: map, filter, fold, len, sum, range, head, tail,
//! plus the tensor constructor/accessors. Registered in
//! [`Environment::new`](crate::Environment::new).

use crate::builtins::{BuiltinFn, Caller, from_fn};
use crate::environment::Environment;
use crate::numeric::for_each_numeric_variant;
use crate::value::Value;

/// Register every collection builtin into `env`.
pub(crate) fn register(env: &mut Environment) {
    env.add_builtin(
        "map",
        BuiltinFn::hof(2, map_hof).with_signature("fn ([T], fn (T) -> U) -> [U]"),
    );
    env.add_builtin(
        "filter",
        BuiltinFn::hof(2, filter_hof).with_signature("fn ([T], fn (T) -> bool) -> [T]"),
    );
    env.add_builtin(
        "fold",
        BuiltinFn::hof(3, fold_hof).with_signature("fn ([T], U, fn (U, T) -> U) -> U"),
    );

    env.add_builtin("len", from_fn(len_pure).with_signature("fn ([T]) -> usize"));
    env.add_builtin(
        "sum",
        from_fn(sum_pure).with_signature("fn ([number]) -> number"),
    );
    env.add_builtin(
        "range",
        from_fn(range_pure).with_signature("fn (i64, i64) -> [i64]"),
    );
    env.add_builtin("head", from_fn(head_pure).with_signature("fn ([T]) -> T"));
    env.add_builtin("tail", from_fn(tail_pure).with_signature("fn ([T]) -> [T]"));

    env.add_builtin(
        "tensor",
        from_fn(tensor_pure).with_signature("fn ([number], [usize]) -> tensor<T>"),
    );
    env.add_builtin(
        "tensor_data",
        from_fn(tensor_data_pure).with_signature("fn (tensor<T>) -> [T]"),
    );
    env.add_builtin(
        "tensor_shape",
        from_fn(tensor_shape_pure).with_signature("fn (tensor<T>) -> [usize]"),
    );

    env.add_builtin(
        "error",
        from_fn(|msg: String| -> Result<Value, String> { Err(msg) })
            .with_signature("fn (utf8) -> never"),
    );

    env.add_builtin(
        "panic",
        from_fn(|msg: String| -> Result<Value, String> { Err(msg) })
            .with_signature("fn (utf8) -> never"),
    );
    env.add_builtin(
        "assert",
        from_fn(|cond: bool, msg: String| -> Result<Value, String> {
            if cond { Ok(Value::None) } else { Err(msg) }
        })
        .with_signature("fn (bool, utf8) -> none"),
    );

    env.add_builtin(
        "concat",
        from_fn(|a: String, b: String| -> String { format!("{a}{b}") })
            .with_signature("fn (utf8, utf8) -> utf8"),
    );
    env.add_builtin(
        "format",
        BuiltinFn::hof(0, format_hof).with_signature("fn (utf8, ...args) -> utf8"),
    );

    env.add_builtin(
        "flatten",
        from_fn(flatten_pure).with_signature("fn ([[T]]) -> [T]"),
    );
    env.add_builtin(
        "zip",
        from_fn(zip_pure).with_signature("fn ([A], [B]) -> [(A, B)]"),
    );

    env.add_builtin(
        "tensor_reshape",
        from_fn(tensor_reshape_pure).with_signature("fn (tensor<T>, [usize]) -> tensor<T>"),
    );

    // ── String builtins ─────────────────────────────────────────────
    env.add_builtin(
        "split",
        from_fn(|s: String, sep: String| -> Value {
            Value::List(s.split(&sep).map(|p| Value::Utf8(p.to_string())).collect())
        })
        .with_signature("fn (utf8, utf8) -> [utf8]"),
    );
    env.add_builtin(
        "join",
        from_fn(|parts: Vec<String>, sep: String| -> String { parts.join(&sep) })
            .with_signature("fn ([utf8], utf8) -> utf8"),
    );
    env.add_builtin(
        "replace",
        from_fn(|s: String, old: String, new: String| -> String { s.replace(&old, &new) })
            .with_signature("fn (utf8, utf8, utf8) -> utf8"),
    );
    env.add_builtin(
        "contains",
        from_fn(|s: String, needle: String| -> bool { s.contains(&needle) })
            .with_signature("fn (utf8, utf8) -> bool"),
    );
    env.add_builtin(
        "starts_with",
        from_fn(|s: String, prefix: String| -> bool { s.starts_with(&prefix) })
            .with_signature("fn (utf8, utf8) -> bool"),
    );
    env.add_builtin(
        "ends_with",
        from_fn(|s: String, suffix: String| -> bool { s.ends_with(&suffix) })
            .with_signature("fn (utf8, utf8) -> bool"),
    );
    env.add_builtin(
        "to_upper",
        from_fn(|s: String| -> String { s.to_uppercase() }).with_signature("fn (utf8) -> utf8"),
    );
    env.add_builtin(
        "to_lower",
        from_fn(|s: String| -> String { s.to_lowercase() }).with_signature("fn (utf8) -> utf8"),
    );
    env.add_builtin(
        "trim",
        from_fn(|s: String| -> String { s.trim().to_string() }).with_signature("fn (utf8) -> utf8"),
    );

    // ── List builtins ───────────────────────────────────────────────
    env.add_builtin(
        "list_contains",
        from_fn(list_contains_pure).with_signature("fn ([T], T) -> bool"),
    );
    env.add_builtin(
        "reverse",
        from_fn(|xs: Vec<Value>| -> Value {
            let mut xs = xs;
            xs.reverse();
            Value::List(xs)
        })
        .with_signature("fn ([T]) -> [T]"),
    );
    env.add_builtin("sort", from_fn(sort_pure).with_signature("fn ([T]) -> [T]"));
    env.add_builtin(
        "unique",
        from_fn(|xs: Vec<Value>| -> Value {
            let mut seen = Vec::with_capacity(xs.len());
            for x in xs {
                if !seen.contains(&x) {
                    seen.push(x);
                }
            }
            Value::List(seen)
        })
        .with_signature("fn ([T]) -> [T]"),
    );
    env.add_builtin(
        "index_of",
        from_fn(|xs: Vec<Value>, x: Value| -> i64 {
            xs.iter()
                .position(|v| v == &x)
                .map(|i| i as i64)
                .unwrap_or(-1)
        })
        .with_signature("fn ([T], T) -> i64"),
    );
    env.add_builtin(
        "take",
        from_fn(|xs: Vec<Value>, n: i64| -> Value {
            let n = n.max(0) as usize;
            Value::List(xs.into_iter().take(n).collect())
        })
        .with_signature("fn ([T], i64) -> [T]"),
    );
    env.add_builtin(
        "drop",
        from_fn(|xs: Vec<Value>, n: i64| -> Value {
            let n = n.max(0) as usize;
            Value::List(xs.into_iter().skip(n).collect())
        })
        .with_signature("fn ([T], i64) -> [T]"),
    );
}

fn list_contains_pure(xs: Vec<Value>, needle: Value) -> bool {
    xs.iter().any(|v| v == &needle)
}

fn sort_pure(xs: Vec<Value>) -> Result<Value, String> {
    // Numeric lists sort numerically; string lists sort lexicographically.
    // Mixed-type lists and unsupported types fail loudly rather than
    // silently producing a partial ordering.
    if xs.is_empty() {
        return Ok(Value::List(xs));
    }
    let first = &xs[0];
    if first.is_numeric() && xs.iter().all(|v| v.is_numeric()) {
        let mut keyed: Vec<(f64, Value)> = xs
            .iter()
            .map(|v| (v.as_f64().unwrap_or(f64::NAN), v.clone()))
            .collect();
        keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        return Ok(Value::List(keyed.into_iter().map(|(_, v)| v).collect()));
    }
    let all_strings = xs
        .iter()
        .all(|v| matches!(v, Value::Utf8(_) | Value::Ascii(_)));
    if all_strings {
        let mut keyed: Vec<(String, Value)> = xs
            .iter()
            .map(|v| {
                let key = match v {
                    Value::Utf8(s) | Value::Ascii(s) => s.clone(),
                    _ => unreachable!(),
                };
                (key, v.clone())
            })
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        return Ok(Value::List(keyed.into_iter().map(|(_, v)| v).collect()));
    }
    Err(format!(
        "sort: list must be all numeric or all strings, found mixed (first element type: {})",
        first.type_name()
    ))
}

// ── Higher-order ─────────────────────────────────────────────────────

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

fn map_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("map", "second argument", &args[1])?.clone();
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
    let f = expect_function("filter", "second argument", &args[1])?.clone();
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
    let f = expect_function("fold", "third argument", &args[2])?.clone();
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

/// Convert a `Value::List` of integer shape entries into `(dims, product)`,
/// rejecting empty shapes (when `allow_empty` is false), non-integer
/// entries, and shape products that overflow `u64`. `builtin` is
/// interpolated into every error message so the source-level builtin
/// name (`tensor`, `tensor_reshape`) appears in diagnostics.
fn validate_tensor_shape(
    builtin: &str,
    shape_vals: &[Value],
    allow_empty: bool,
) -> Result<(Vec<u64>, u64), String> {
    if !allow_empty && shape_vals.is_empty() {
        return Err(format!("{builtin}: shape must have at least one dimension"));
    }
    let mut dims: Vec<u64> = Vec::with_capacity(shape_vals.len());
    for s in shape_vals {
        let d = s.as_u64().ok_or_else(|| {
            format!(
                "{builtin}: shape entries must be non-negative integers, got {}",
                s.type_name()
            )
        })?;
        dims.push(d);
    }
    let mut product: u64 = 1;
    for d in &dims {
        product = product
            .checked_mul(*d)
            .ok_or_else(|| format!("{builtin}: shape product overflows u64"))?;
    }
    Ok((dims, product))
}

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
    let (dims, expected) = validate_tensor_shape("tensor", &shape_vals, false)?;
    if (data.len() as u64) != expected {
        return Err(format!(
            "tensor: data length {} does not match shape product {expected}",
            data.len(),
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

fn tensor_reshape_pure(t: Value, new_shape: Value) -> Result<Value, String> {
    let (data, _old_shape) = match t {
        Value::Tensor { shape, data } => (data, shape),
        other => {
            return Err(format!(
                "tensor_reshape: expected tensor, got {}",
                other.type_name()
            ));
        }
    };
    let shape_vals = match new_shape {
        Value::List(items) => items,
        other => {
            return Err(format!(
                "tensor_reshape: shape must be a list of u64, got {}",
                other.type_name()
            ));
        }
    };
    let (dims, expected) = validate_tensor_shape("tensor_reshape", &shape_vals, true)?;
    if (data.len() as u64) != expected {
        return Err(format!(
            "tensor_reshape: data length {} does not match new shape product {expected}",
            data.len(),
        ));
    }
    Ok(Value::Tensor { shape: dims, data })
}

fn flatten_pure(v: Value) -> Result<Vec<Value>, String> {
    let items = match v {
        Value::List(items) => items,
        other => {
            return Err(format!(
                "flatten: expected list of lists, got {}",
                other.type_name()
            ));
        }
    };
    let mut out: Vec<Value> = Vec::new();
    for inner in items {
        match inner {
            Value::List(xs) => out.extend(xs),
            other => {
                return Err(format!(
                    "flatten: outer list element must be a list, got {}",
                    other.type_name()
                ));
            }
        }
    }
    Ok(out)
}

fn zip_pure(a: Value, b: Value) -> Result<Vec<Value>, String> {
    let av = match a {
        Value::List(xs) => xs,
        other => {
            return Err(format!(
                "zip: first arg must be a list, got {}",
                other.type_name()
            ));
        }
    };
    let bv = match b {
        Value::List(xs) => xs,
        other => {
            return Err(format!(
                "zip: second arg must be a list, got {}",
                other.type_name()
            ));
        }
    };
    let n = av.len().min(bv.len());
    let mut out: Vec<Value> = Vec::with_capacity(n);
    for i in 0..n {
        out.push(Value::List(vec![av[i].clone(), bv[i].clone()]));
    }
    Ok(out)
}

/// `format(template, ...args)` — `{}` positional substitution.
fn format_hof(_caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let Some((template_v, rest)) = args.split_first() else {
        return Err("format: missing template argument".into());
    };
    let template = match template_v {
        Value::Utf8(s) | Value::Ascii(s) => s.clone(),
        other => {
            return Err(format!(
                "format: template must be a string, got {}",
                other.type_name()
            ));
        }
    };
    let mut out = String::with_capacity(template.len());
    let mut idx = 0usize;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                out.push('{');
                continue;
            }
            // Expect `}`.
            if chars.next() != Some('}') {
                return Err("format: expected '{}' placeholder".into());
            }
            let Some(arg) = rest.get(idx) else {
                return Err(format!(
                    "format: not enough arguments for placeholder #{idx}"
                ));
            };
            idx += 1;
            out.push_str(&format_value(arg));
        } else if c == '}' {
            if chars.peek() == Some(&'}') {
                chars.next();
                out.push('}');
                continue;
            }
            return Err("format: unmatched '}' in template".into());
        } else {
            out.push(c);
        }
    }
    if idx < rest.len() {
        return Err(format!(
            "format: {} extra arguments after template",
            rest.len() - idx
        ));
    }
    Ok(Value::Utf8(out))
}

/// Render a `Value` for inclusion in a `format` substitution. Stays
/// compact and predictable — host CLIs render richer forms.
pub(crate) fn format_value(v: &Value) -> String {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s.clone(),
        Value::Symbol(s) => format!(":{s}"),
        Value::Bool(b) => b.to_string(),
        Value::None => "none".to_string(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::Isize(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::Usize(n) => n.to_string(),
        Value::F32(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        Value::Utf16(units) => String::from_utf16_lossy(units),
        Value::Utf32(chars) => chars.iter().collect(),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(format_value).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Tensor { shape, data } => {
            let dims: Vec<String> = shape.iter().map(u64::to_string).collect();
            let elems: Vec<String> = data.iter().map(format_value).collect();
            format!("tensor[{}]({})", dims.join("x"), elems.join(", "))
        }
        Value::Variant {
            union,
            variant,
            payload,
        } => {
            use crate::value::VariantPayload;
            let path = format!("{}::{}", union.join("."), variant);
            match payload {
                VariantPayload::Unit => path,
                VariantPayload::Positional(v) => format!("{path}({})", format_value(v)),
                VariantPayload::Record(map) => {
                    let parts: Vec<String> = map
                        .iter()
                        .map(|(k, v)| format!("{k}: {}", format_value(v)))
                        .collect();
                    format!("{path} {{ {} }}", parts.join(", "))
                }
            }
        }
        Value::Function(_) => "<fn>".to_string(),
        Value::Record { ty, fields } => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", format_value(v)))
                .collect();
            format!("{} {{ {} }}", ty.join("."), parts.join(", "))
        }
        Value::DataPath { kind, segments } => {
            format!("&{}<{kind}>", segments.join("."))
        }
    }
}
