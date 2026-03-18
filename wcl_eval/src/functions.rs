use crate::value::Value;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub type BuiltinFn = fn(&[Value]) -> Result<Value, String>;

pub fn builtin_registry() -> HashMap<&'static str, BuiltinFn> {
    let mut m: HashMap<&'static str, BuiltinFn> = HashMap::new();

    // String functions (Section 14.1)
    m.insert("upper", upper as BuiltinFn);
    m.insert("lower", lower as BuiltinFn);
    m.insert("trim", trim as BuiltinFn);
    m.insert("trim_prefix", trim_prefix as BuiltinFn);
    m.insert("trim_suffix", trim_suffix as BuiltinFn);
    m.insert("replace", fn_replace as BuiltinFn);
    m.insert("split", split as BuiltinFn);
    m.insert("join", join as BuiltinFn);
    m.insert("starts_with", starts_with as BuiltinFn);
    m.insert("ends_with", ends_with as BuiltinFn);
    m.insert("contains", fn_contains as BuiltinFn);
    m.insert("length", length as BuiltinFn);
    m.insert("substr", substr as BuiltinFn);
    m.insert("format", fn_format as BuiltinFn);
    m.insert("regex_match", regex_match as BuiltinFn);
    m.insert("regex_capture", regex_capture as BuiltinFn);

    // Math functions (Section 14.2)
    m.insert("abs", abs as BuiltinFn);
    m.insert("min", fn_min as BuiltinFn);
    m.insert("max", fn_max as BuiltinFn);
    m.insert("floor", floor as BuiltinFn);
    m.insert("ceil", ceil as BuiltinFn);
    m.insert("round", fn_round as BuiltinFn);
    m.insert("sqrt", sqrt as BuiltinFn);
    m.insert("pow", pow as BuiltinFn);

    // Collection functions (Section 14.3)
    m.insert("len", len as BuiltinFn);
    m.insert("keys", keys as BuiltinFn);
    m.insert("values", fn_values as BuiltinFn);
    m.insert("flatten", flatten as BuiltinFn);
    m.insert("concat", fn_concat as BuiltinFn);
    m.insert("distinct", distinct as BuiltinFn);
    m.insert("sort", fn_sort as BuiltinFn);
    m.insert("reverse", fn_reverse as BuiltinFn);
    m.insert("index_of", index_of as BuiltinFn);
    m.insert("range", range as BuiltinFn);
    m.insert("zip", zip as BuiltinFn);

    // Higher-order functions (Section 14.4) — require special evaluator support
    m.insert("map", higher_order_placeholder as BuiltinFn);
    m.insert("filter", higher_order_placeholder as BuiltinFn);
    m.insert("every", higher_order_placeholder as BuiltinFn);
    m.insert("some", higher_order_placeholder as BuiltinFn);
    m.insert("reduce", higher_order_placeholder as BuiltinFn);

    // Aggregate functions (Section 14.5)
    m.insert("sum", sum as BuiltinFn);
    m.insert("avg", avg as BuiltinFn);
    m.insert("min_of", min_of as BuiltinFn);
    m.insert("max_of", max_of as BuiltinFn);
    m.insert("count", higher_order_placeholder as BuiltinFn);

    // Hash/encoding (Section 14.6)
    m.insert("sha256", fn_sha256 as BuiltinFn);
    m.insert("base64_encode", base64_encode as BuiltinFn);
    m.insert("base64_decode", base64_decode as BuiltinFn);
    m.insert("json_encode", json_encode as BuiltinFn);

    // Type coercion (Section 14.7)
    m.insert("to_string", to_string as BuiltinFn);
    m.insert("to_int", to_int as BuiltinFn);
    m.insert("to_float", to_float as BuiltinFn);
    m.insert("to_bool", to_bool as BuiltinFn);
    m.insert("type_of", type_of as BuiltinFn);

    // Reference and Query Functions (Section 14.9)
    m.insert("has", fn_has as BuiltinFn);
    m.insert("has_decorator", fn_has_decorator as BuiltinFn);

    m
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expect_args(args: &[Value], n: usize, name: &str) -> Result<(), String> {
    if args.len() != n {
        Err(format!(
            "{}: expected {} argument(s), got {}",
            name,
            n,
            args.len()
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(args: &[Value], n: usize, name: &str) -> Result<(), String> {
    if args.len() < n {
        Err(format!(
            "{}: expected at least {} argument(s), got {}",
            name,
            n,
            args.len()
        ))
    } else {
        Ok(())
    }
}

fn get_string<'a>(v: &'a Value, pos: usize, fn_name: &str) -> Result<&'a str, String> {
    match v {
        Value::String(s) => Ok(s.as_str()),
        other => Err(format!(
            "{}: argument {} must be string, got {}",
            fn_name,
            pos,
            other.type_name()
        )),
    }
}

fn get_int(v: &Value, pos: usize, fn_name: &str) -> Result<i64, String> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(format!(
            "{}: argument {} must be int, got {}",
            fn_name,
            pos,
            other.type_name()
        )),
    }
}

fn get_list<'a>(v: &'a Value, pos: usize, fn_name: &str) -> Result<&'a [Value], String> {
    match v {
        Value::List(l) => Ok(l.as_slice()),
        other => Err(format!(
            "{}: argument {} must be list, got {}",
            fn_name,
            pos,
            other.type_name()
        )),
    }
}

/// Coerce Int or Float to f64 for numeric operations.
fn coerce_to_float(v: &Value, pos: usize, fn_name: &str) -> Result<f64, String> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        other => Err(format!(
            "{}: argument {} must be int or float, got {}",
            fn_name,
            pos,
            other.type_name()
        )),
    }
}

// ---------------------------------------------------------------------------
// Section 14.1 — String Functions
// ---------------------------------------------------------------------------

fn upper(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "upper")?;
    let s = get_string(&args[0], 1, "upper")?;
    Ok(Value::String(s.to_uppercase()))
}

fn lower(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "lower")?;
    let s = get_string(&args[0], 1, "lower")?;
    Ok(Value::String(s.to_lowercase()))
}

fn trim(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "trim")?;
    let s = get_string(&args[0], 1, "trim")?;
    Ok(Value::String(s.trim().to_string()))
}

fn trim_prefix(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "trim_prefix")?;
    let s = get_string(&args[0], 1, "trim_prefix")?;
    let prefix = get_string(&args[1], 2, "trim_prefix")?;
    Ok(Value::String(
        s.strip_prefix(prefix).unwrap_or(s).to_string(),
    ))
}

fn trim_suffix(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "trim_suffix")?;
    let s = get_string(&args[0], 1, "trim_suffix")?;
    let suffix = get_string(&args[1], 2, "trim_suffix")?;
    Ok(Value::String(
        s.strip_suffix(suffix).unwrap_or(s).to_string(),
    ))
}

fn fn_replace(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 3, "replace")?;
    let s = get_string(&args[0], 1, "replace")?;
    let old = get_string(&args[1], 2, "replace")?;
    let new = get_string(&args[2], 3, "replace")?;
    Ok(Value::String(s.replace(old, new)))
}

fn split(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "split")?;
    let sep = get_string(&args[0], 1, "split")?;
    let s = get_string(&args[1], 2, "split")?;
    let parts: Vec<Value> = s
        .split(sep)
        .map(|p: &str| Value::String(p.to_string()))
        .collect();
    Ok(Value::List(parts))
}

fn join(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "join")?;
    let sep = get_string(&args[0], 1, "join")?;
    let list = get_list(&args[1], 2, "join")?;
    let mut parts = Vec::with_capacity(list.len());
    for (i, v) in list.iter().enumerate() {
        match v {
            Value::String(s) => parts.push(s.as_str().to_string()),
            other => {
                return Err(format!(
                    "join: list element {} must be string, got {}",
                    i,
                    other.type_name()
                ))
            }
        }
    }
    Ok(Value::String(parts.join(sep)))
}

fn starts_with(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "starts_with")?;
    let s = get_string(&args[0], 1, "starts_with")?;
    let prefix = get_string(&args[1], 2, "starts_with")?;
    Ok(Value::Bool(s.starts_with(prefix)))
}

fn ends_with(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "ends_with")?;
    let s = get_string(&args[0], 1, "ends_with")?;
    let suffix = get_string(&args[1], 2, "ends_with")?;
    Ok(Value::Bool(s.ends_with(suffix)))
}

/// Overloaded: `contains(string, string)` or `contains(list, value)`.
fn fn_contains(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "contains")?;
    match &args[0] {
        Value::String(s) => {
            let substr = get_string(&args[1], 2, "contains")?;
            Ok(Value::Bool(s.contains(substr)))
        }
        Value::List(list) => Ok(Value::Bool(list.contains(&args[1]))),
        other => Err(format!(
            "contains: argument 1 must be string or list, got {}",
            other.type_name()
        )),
    }
}

fn length(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "length")?;
    let s = get_string(&args[0], 1, "length")?;
    Ok(Value::Int(s.chars().count() as i64))
}

fn substr(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err(format!(
            "substr: expected 2 or 3 arguments, got {}",
            args.len()
        ));
    }
    let s = get_string(&args[0], 1, "substr")?;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;

    let start = get_int(&args[1], 2, "substr")?;
    let end = if args.len() == 3 {
        get_int(&args[2], 3, "substr")?
    } else {
        len
    };

    // Clamp to valid range
    let start = start.max(0).min(len) as usize;
    let end = end.max(0).min(len) as usize;
    let end = end.max(start);

    Ok(Value::String(chars[start..end].iter().collect()))
}

/// `format(fmt, args...)` — replace `{}` placeholders positionally.
fn fn_format(args: &[Value]) -> Result<Value, String> {
    expect_min_args(args, 1, "format")?;
    let fmt = get_string(&args[0], 1, "format")?;
    let fmt_args = &args[1..];

    let mut result = String::new();
    let mut arg_idx = 0;
    let mut chars = fmt.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'}') {
                chars.next(); // consume '}'
                if arg_idx >= fmt_args.len() {
                    return Err(format!(
                        "format: not enough arguments (placeholder {} but only {} args)",
                        arg_idx,
                        fmt_args.len()
                    ));
                }
                result.push_str(&fmt_args[arg_idx].to_string());
                arg_idx += 1;
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    Ok(Value::String(result))
}

fn regex_match(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "regex_match")?;
    let s = get_string(&args[0], 1, "regex_match")?;
    let pattern = get_string(&args[1], 2, "regex_match")?;
    let re = regex::Regex::new(pattern)
        .map_err(|e| format!("regex_match: invalid pattern: {}", e))?;
    Ok(Value::Bool(re.is_match(s)))
}

fn regex_capture(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "regex_capture")?;
    let s = get_string(&args[0], 1, "regex_capture")?;
    let pattern = get_string(&args[1], 2, "regex_capture")?;
    let re = regex::Regex::new(pattern)
        .map_err(|e| format!("regex_capture: invalid pattern: {}", e))?;

    let captures: Vec<Value> = match re.captures(s) {
        None => vec![],
        Some(caps) => caps
            .iter()
            .skip(1) // skip full match, return capture groups only
            .map(|m| match m {
                Some(m) => Value::String(m.as_str().to_string()),
                None => Value::Null,
            })
            .collect(),
    };
    Ok(Value::List(captures))
}

// ---------------------------------------------------------------------------
// Section 14.2 — Math Functions
// ---------------------------------------------------------------------------

fn abs(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "abs")?;
    match &args[0] {
        Value::Int(i) => Ok(Value::Int(i.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        other => Err(format!(
            "abs: argument must be int or float, got {}",
            other.type_name()
        )),
    }
}

fn fn_min(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "min")?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.min(b))),
        _ => {
            let a = coerce_to_float(&args[0], 1, "min")?;
            let b = coerce_to_float(&args[1], 2, "min")?;
            Ok(Value::Float(a.min(b)))
        }
    }
}

fn fn_max(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "max")?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.max(b))),
        _ => {
            let a = coerce_to_float(&args[0], 1, "max")?;
            let b = coerce_to_float(&args[1], 2, "max")?;
            Ok(Value::Float(a.max(b)))
        }
    }
}

fn floor(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "floor")?;
    let f = coerce_to_float(&args[0], 1, "floor")?;
    Ok(Value::Int(f.floor() as i64))
}

fn ceil(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "ceil")?;
    let f = coerce_to_float(&args[0], 1, "ceil")?;
    Ok(Value::Int(f.ceil() as i64))
}

fn fn_round(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "round")?;
    let f = coerce_to_float(&args[0], 1, "round")?;
    Ok(Value::Int(f.round() as i64))
}

fn sqrt(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "sqrt")?;
    let f = coerce_to_float(&args[0], 1, "sqrt")?;
    Ok(Value::Float(f.sqrt()))
}

fn pow(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "pow")?;
    let base = coerce_to_float(&args[0], 1, "pow")?;
    let exp = coerce_to_float(&args[1], 2, "pow")?;
    Ok(Value::Float(base.powf(exp)))
}

// ---------------------------------------------------------------------------
// Section 14.3 — Collection Functions
// ---------------------------------------------------------------------------

fn len(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "len")?;
    match &args[0] {
        Value::List(l) => Ok(Value::Int(l.len() as i64)),
        Value::Map(m) => Ok(Value::Int(m.len() as i64)),
        Value::Set(s) => Ok(Value::Int(s.len() as i64)),
        other => Err(format!(
            "len: argument must be list, map, or set, got {}",
            other.type_name()
        )),
    }
}

fn keys(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "keys")?;
    match &args[0] {
        Value::Map(m) => {
            let ks: Vec<Value> = m.keys().map(|k| Value::String(k.clone())).collect();
            Ok(Value::List(ks))
        }
        other => Err(format!(
            "keys: argument must be map, got {}",
            other.type_name()
        )),
    }
}

fn fn_values(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "values")?;
    match &args[0] {
        Value::Map(m) => {
            let vs: Vec<Value> = m.values().cloned().collect();
            Ok(Value::List(vs))
        }
        other => Err(format!(
            "values: argument must be map, got {}",
            other.type_name()
        )),
    }
}

fn flatten(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "flatten")?;
    let list = get_list(&args[0], 1, "flatten")?;
    let mut result = Vec::new();
    for item in list {
        match item {
            Value::List(inner) => result.extend(inner.iter().cloned()),
            other => result.push(other.clone()),
        }
    }
    Ok(Value::List(result))
}

fn fn_concat(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "concat")?;
    let l1 = get_list(&args[0], 1, "concat")?;
    let l2 = get_list(&args[1], 2, "concat")?;
    let mut result = l1.to_vec();
    result.extend_from_slice(l2);
    Ok(Value::List(result))
}

fn distinct(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "distinct")?;
    let list = get_list(&args[0], 1, "distinct")?;
    let mut seen: Vec<Value> = Vec::new();
    for item in list {
        if !seen.contains(item) {
            seen.push(item.clone());
        }
    }
    Ok(Value::List(seen))
}

/// Compare two Values for sorting purposes. Returns None if not comparable.
fn value_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.partial_cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        _ => Some(Ordering::Equal), // fallback: treat as equal for mixed types
    }
}

fn fn_sort(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "sort")?;
    let list = get_list(&args[0], 1, "sort")?;
    let mut result = list.to_vec();
    result.sort_by(|a, b| value_cmp(a, b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(Value::List(result))
}

fn fn_reverse(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "reverse")?;
    let list = get_list(&args[0], 1, "reverse")?;
    let mut result = list.to_vec();
    result.reverse();
    Ok(Value::List(result))
}

fn index_of(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "index_of")?;
    let list = get_list(&args[0], 1, "index_of")?;
    let needle = &args[1];
    for (i, v) in list.iter().enumerate() {
        if v == needle {
            return Ok(Value::Int(i as i64));
        }
    }
    Ok(Value::Int(-1))
}

fn range(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err(format!(
            "range: expected 2 or 3 arguments, got {}",
            args.len()
        ));
    }
    let start = get_int(&args[0], 1, "range")?;
    let end = get_int(&args[1], 2, "range")?;
    let step = if args.len() == 3 {
        let s = get_int(&args[2], 3, "range")?;
        if s == 0 {
            return Err("range: step must not be zero".to_string());
        }
        s
    } else {
        1
    };

    let mut result = Vec::new();
    if step > 0 {
        let mut i = start;
        while i < end {
            result.push(Value::Int(i));
            i += step;
        }
    } else {
        let mut i = start;
        while i > end {
            result.push(Value::Int(i));
            i += step;
        }
    }
    Ok(Value::List(result))
}

fn zip(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "zip")?;
    let l1 = get_list(&args[0], 1, "zip")?;
    let l2 = get_list(&args[1], 2, "zip")?;
    let result: Vec<Value> = l1
        .iter()
        .zip(l2.iter())
        .map(|(a, b): (&Value, &Value)| Value::List(vec![a.clone(), b.clone()]))
        .collect();
    Ok(Value::List(result))
}

// ---------------------------------------------------------------------------
// Section 14.4 — Higher-Order Functions (placeholder)
// ---------------------------------------------------------------------------

fn higher_order_placeholder(_args: &[Value]) -> Result<Value, String> {
    Err("higher-order functions require special evaluation".to_string())
}

// ---------------------------------------------------------------------------
// Section 14.5 — Aggregate Functions
// ---------------------------------------------------------------------------

fn sum(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "sum")?;
    let list = get_list(&args[0], 1, "sum")?;
    if list.is_empty() {
        return Ok(Value::Int(0));
    }
    let mut has_float = false;
    for v in list {
        if matches!(v, Value::Float(_)) {
            has_float = true;
            break;
        }
    }
    if has_float {
        let mut acc = 0.0f64;
        for (i, v) in list.iter().enumerate() {
            acc += coerce_to_float(v, i + 1, "sum")?;
        }
        Ok(Value::Float(acc))
    } else {
        let mut acc = 0i64;
        for (i, v) in list.iter().enumerate() {
            acc += get_int(v, i + 1, "sum")?;
        }
        Ok(Value::Int(acc))
    }
}

fn avg(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "avg")?;
    let list = get_list(&args[0], 1, "avg")?;
    if list.is_empty() {
        return Err("avg: cannot average empty list".to_string());
    }
    let mut acc = 0.0f64;
    for (i, v) in list.iter().enumerate() {
        acc += coerce_to_float(v, i + 1, "avg")?;
    }
    Ok(Value::Float(acc / list.len() as f64))
}

fn min_of(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "min_of")?;
    let list = get_list(&args[0], 1, "min_of")?;
    if list.is_empty() {
        return Err("min_of: cannot find minimum of empty list".to_string());
    }
    let mut result = list[0].clone();
    for v in &list[1..] {
        if value_cmp(v, &result) == Some(std::cmp::Ordering::Less) {
            result = v.clone();
        }
    }
    Ok(result)
}

fn max_of(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "max_of")?;
    let list = get_list(&args[0], 1, "max_of")?;
    if list.is_empty() {
        return Err("max_of: cannot find maximum of empty list".to_string());
    }
    let mut result = list[0].clone();
    for v in &list[1..] {
        if value_cmp(v, &result) == Some(std::cmp::Ordering::Greater) {
            result = v.clone();
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Section 14.6 — Hash and Encoding Functions
// ---------------------------------------------------------------------------

fn fn_sha256(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "sha256")?;
    let s = get_string(&args[0], 1, "sha256")?;
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();
    let hex = result
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    Ok(Value::String(hex))
}

fn base64_encode(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "base64_encode")?;
    let s = get_string(&args[0], 1, "base64_encode")?;
    Ok(Value::String(BASE64_STANDARD.encode(s.as_bytes())))
}

fn base64_decode(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "base64_decode")?;
    let s = get_string(&args[0], 1, "base64_decode")?;
    let bytes = BASE64_STANDARD
        .decode(s)
        .map_err(|e| format!("base64_decode: invalid base64: {}", e))?;
    let decoded = String::from_utf8(bytes)
        .map_err(|e| format!("base64_decode: decoded bytes are not valid UTF-8: {}", e))?;
    Ok(Value::String(decoded))
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Null => serde_json::Value::Null,
        Value::Identifier(s) => serde_json::Value::String(s.clone()),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::String(format!("{}", v)),
    }
}

fn json_encode(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "json_encode")?;
    let j = value_to_json(&args[0]);
    let s = serde_json::to_string(&j)
        .map_err(|e| format!("json_encode: serialization failed: {}", e))?;
    Ok(Value::String(s))
}

// ---------------------------------------------------------------------------
// Section 14.7 — Type Coercion Functions
// ---------------------------------------------------------------------------

fn to_string(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "to_string")?;
    Ok(Value::String(args[0].to_string()))
}

fn to_int(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "to_int")?;
    match &args[0] {
        Value::Int(i) => Ok(Value::Int(*i)),
        Value::Float(f) => Ok(Value::Int(*f as i64)),
        Value::Bool(b) => Ok(Value::Int(if *b { 1 } else { 0 })),
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| format!("to_int: cannot convert string {:?} to int", s)),
        other => Err(format!(
            "to_int: cannot convert {} to int",
            other.type_name()
        )),
    }
}

fn to_float(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "to_float")?;
    match &args[0] {
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Int(i) => Ok(Value::Float(*i as f64)),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| format!("to_float: cannot convert string {:?} to float", s)),
        other => Err(format!(
            "to_float: cannot convert {} to float",
            other.type_name()
        )),
    }
}

fn to_bool(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "to_bool")?;
    match &args[0] {
        Value::Bool(b) => Ok(Value::Bool(*b)),
        Value::Int(0) => Ok(Value::Bool(false)),
        Value::Int(1) => Ok(Value::Bool(true)),
        Value::Int(i) => Err(format!(
            "to_bool: int {} cannot be converted to bool (only 0 or 1)",
            i
        )),
        Value::String(s) => match s.trim() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            other => Err(format!(
                "to_bool: string {:?} cannot be converted to bool (expected \"true\" or \"false\")",
                other
            )),
        },
        other => Err(format!(
            "to_bool: cannot convert {} to bool",
            other.type_name()
        )),
    }
}

fn type_of(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "type_of")?;
    Ok(Value::String(args[0].type_name().to_string()))
}

// ---------------------------------------------------------------------------
// Section 14.9 — Reference and Query Functions
// ---------------------------------------------------------------------------

fn fn_has(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "has")?;
    let block_ref = match &args[0] {
        Value::BlockRef(br) => br,
        other => return Err(format!("has: argument 1 must be block_ref, got {}", other.type_name())),
    };
    let attr_name = get_string(&args[1], 2, "has")?;
    // Check attributes AND child blocks by kind
    let has_attr = block_ref.attributes.contains_key(attr_name);
    let has_child = block_ref.children.iter().any(|c| c.kind == attr_name);
    Ok(Value::Bool(has_attr || has_child))
}

fn fn_has_decorator(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "has_decorator")?;
    let block_ref = match &args[0] {
        Value::BlockRef(br) => br,
        other => return Err(format!("has_decorator: argument 1 must be block_ref, got {}", other.type_name())),
    };
    let deco_name = get_string(&args[1], 2, "has_decorator")?;
    let found = block_ref.decorators.iter().any(|d| d.name == deco_name);
    Ok(Value::Bool(found))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn s(v: &str) -> Value {
        Value::String(v.to_string())
    }

    fn i(v: i64) -> Value {
        Value::Int(v)
    }

    fn f(v: f64) -> Value {
        Value::Float(v)
    }

    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    // --- String ---

    #[test]
    fn test_upper() {
        assert_eq!(upper(&[s("hello")]).unwrap(), s("HELLO"));
        assert_eq!(upper(&[s("Hello World")]).unwrap(), s("HELLO WORLD"));
        assert!(upper(&[]).is_err());
        assert!(upper(&[i(1)]).is_err());
    }

    #[test]
    fn test_lower() {
        assert_eq!(lower(&[s("HELLO")]).unwrap(), s("hello"));
        assert_eq!(lower(&[s("MiXeD")]).unwrap(), s("mixed"));
    }

    #[test]
    fn test_trim() {
        assert_eq!(trim(&[s("  hello  ")]).unwrap(), s("hello"));
        assert_eq!(trim(&[s("\t\n foo \n")]).unwrap(), s("foo"));
    }

    #[test]
    fn test_trim_prefix() {
        assert_eq!(
            trim_prefix(&[s("hello world"), s("hello ")]).unwrap(),
            s("world")
        );
        // no match → unchanged
        assert_eq!(
            trim_prefix(&[s("hello world"), s("xyz")]).unwrap(),
            s("hello world")
        );
    }

    #[test]
    fn test_trim_suffix() {
        assert_eq!(
            trim_suffix(&[s("hello world"), s(" world")]).unwrap(),
            s("hello")
        );
    }

    #[test]
    fn test_replace() {
        assert_eq!(
            fn_replace(&[s("aabbcc"), s("bb"), s("XX")]).unwrap(),
            s("aaXXcc")
        );
    }

    #[test]
    fn test_split() {
        let result = split(&[s(","), s("a,b,c")]).unwrap();
        assert_eq!(result, list(vec![s("a"), s("b"), s("c")]));
    }

    #[test]
    fn test_join() {
        let result = join(&[s(", "), list(vec![s("a"), s("b"), s("c")])]).unwrap();
        assert_eq!(result, s("a, b, c"));
    }

    #[test]
    fn test_starts_with() {
        assert_eq!(starts_with(&[s("hello"), s("he")]).unwrap(), Value::Bool(true));
        assert_eq!(starts_with(&[s("hello"), s("lo")]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_ends_with() {
        assert_eq!(ends_with(&[s("hello"), s("lo")]).unwrap(), Value::Bool(true));
        assert_eq!(ends_with(&[s("hello"), s("he")]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_contains_string() {
        assert_eq!(fn_contains(&[s("foobar"), s("oba")]).unwrap(), Value::Bool(true));
        assert_eq!(fn_contains(&[s("foobar"), s("xyz")]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_contains_list() {
        assert_eq!(
            fn_contains(&[list(vec![i(1), i(2), i(3)]), i(2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            fn_contains(&[list(vec![i(1), i(2), i(3)]), i(5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_length() {
        assert_eq!(length(&[s("hello")]).unwrap(), i(5));
        assert_eq!(length(&[s("caf\u{00e9}")]).unwrap(), i(4)); // "café" is 4 chars
    }

    #[test]
    fn test_substr() {
        assert_eq!(substr(&[s("hello"), i(1), i(4)]).unwrap(), s("ell"));
        assert_eq!(substr(&[s("hello"), i(0), i(5)]).unwrap(), s("hello"));
        assert_eq!(substr(&[s("hello"), i(2)]).unwrap(), s("llo"));
    }

    #[test]
    fn test_format() {
        assert_eq!(
            fn_format(&[s("Hello, {}!"), s("world")]).unwrap(),
            s("Hello, world!")
        );
        assert_eq!(
            fn_format(&[s("{} + {} = {}"), i(1), i(2), i(3)]).unwrap(),
            s("1 + 2 = 3")
        );
    }

    #[test]
    fn test_regex_match() {
        assert_eq!(
            regex_match(&[s("hello123"), s(r"\d+")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            regex_match(&[s("hello"), s(r"^\d+$")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_regex_capture() {
        let result = regex_capture(&[s("2024-03-15"), s(r"(\d{4})-(\d{2})-(\d{2})")]).unwrap();
        assert_eq!(result, list(vec![s("2024"), s("03"), s("15")]));
    }

    // --- Math ---

    #[test]
    fn test_abs() {
        assert_eq!(abs(&[i(-5)]).unwrap(), i(5));
        assert_eq!(abs(&[i(5)]).unwrap(), i(5));
        assert_eq!(abs(&[f(-3.14)]).unwrap(), f(3.14));
    }

    #[test]
    fn test_min() {
        assert_eq!(fn_min(&[i(3), i(7)]).unwrap(), i(3));
        assert_eq!(fn_min(&[f(3.5), f(2.1)]).unwrap(), f(2.1));
        // int + float → float promotion
        assert_eq!(fn_min(&[i(5), f(3.0)]).unwrap(), f(3.0));
    }

    #[test]
    fn test_max() {
        assert_eq!(fn_max(&[i(3), i(7)]).unwrap(), i(7));
        assert_eq!(fn_max(&[f(3.5), f(2.1)]).unwrap(), f(3.5));
        assert_eq!(fn_max(&[i(5), f(6.0)]).unwrap(), f(6.0));
    }

    #[test]
    fn test_floor_ceil_round() {
        assert_eq!(floor(&[f(3.7)]).unwrap(), i(3));
        assert_eq!(ceil(&[f(3.2)]).unwrap(), i(4));
        assert_eq!(fn_round(&[f(3.5)]).unwrap(), i(4));
        assert_eq!(fn_round(&[f(3.4)]).unwrap(), i(3));
    }

    #[test]
    fn test_sqrt() {
        assert_eq!(sqrt(&[i(4)]).unwrap(), f(2.0));
        assert_eq!(sqrt(&[f(9.0)]).unwrap(), f(3.0));
    }

    #[test]
    fn test_pow() {
        assert_eq!(pow(&[i(2), i(10)]).unwrap(), f(1024.0));
        assert_eq!(pow(&[f(2.0), f(0.5)]).unwrap(), f(2.0f64.powf(0.5)));
    }

    // --- Collections ---

    #[test]
    fn test_len() {
        assert_eq!(len(&[list(vec![i(1), i(2), i(3)])]).unwrap(), i(3));
        assert_eq!(len(&[list(vec![])]).unwrap(), i(0));

        let mut m = IndexMap::new();
        m.insert("a".to_string(), i(1));
        m.insert("b".to_string(), i(2));
        assert_eq!(len(&[Value::Map(m)]).unwrap(), i(2));

        assert_eq!(len(&[Value::Set(vec![i(1), i(2)])]).unwrap(), i(2));
    }

    #[test]
    fn test_keys() {
        let mut m = IndexMap::new();
        m.insert("x".to_string(), i(1));
        m.insert("y".to_string(), i(2));
        let result = keys(&[Value::Map(m)]).unwrap();
        assert_eq!(result, list(vec![s("x"), s("y")]));
    }

    #[test]
    fn test_values_fn() {
        let mut m = IndexMap::new();
        m.insert("x".to_string(), i(10));
        m.insert("y".to_string(), i(20));
        let result = fn_values(&[Value::Map(m)]).unwrap();
        assert_eq!(result, list(vec![i(10), i(20)]));
    }

    #[test]
    fn test_flatten() {
        let nested = list(vec![
            list(vec![i(1), i(2)]),
            list(vec![i(3)]),
            list(vec![i(4), i(5)]),
        ]);
        assert_eq!(
            flatten(&[nested]).unwrap(),
            list(vec![i(1), i(2), i(3), i(4), i(5)])
        );
    }

    #[test]
    fn test_concat() {
        let result = fn_concat(&[list(vec![i(1), i(2)]), list(vec![i(3), i(4)])]).unwrap();
        assert_eq!(result, list(vec![i(1), i(2), i(3), i(4)]));
    }

    #[test]
    fn test_distinct() {
        let result =
            distinct(&[list(vec![i(1), i(2), i(1), i(3), i(2)])]).unwrap();
        assert_eq!(result, list(vec![i(1), i(2), i(3)]));
    }

    #[test]
    fn test_sort() {
        let result = fn_sort(&[list(vec![i(3), i(1), i(2)])]).unwrap();
        assert_eq!(result, list(vec![i(1), i(2), i(3)]));

        let result = fn_sort(&[list(vec![s("banana"), s("apple"), s("cherry")])]).unwrap();
        assert_eq!(result, list(vec![s("apple"), s("banana"), s("cherry")]));
    }

    #[test]
    fn test_reverse() {
        let result = fn_reverse(&[list(vec![i(1), i(2), i(3)])]).unwrap();
        assert_eq!(result, list(vec![i(3), i(2), i(1)]));
    }

    #[test]
    fn test_index_of() {
        assert_eq!(
            index_of(&[list(vec![i(10), i(20), i(30)]), i(20)]).unwrap(),
            i(1)
        );
        assert_eq!(
            index_of(&[list(vec![i(10), i(20), i(30)]), i(99)]).unwrap(),
            i(-1)
        );
    }

    #[test]
    fn test_range() {
        assert_eq!(range(&[i(0), i(5)]).unwrap(), list(vec![i(0), i(1), i(2), i(3), i(4)]));
        assert_eq!(range(&[i(0), i(10), i(2)]).unwrap(), list(vec![i(0), i(2), i(4), i(6), i(8)]));
        assert_eq!(range(&[i(5), i(0), i(-1)]).unwrap(), list(vec![i(5), i(4), i(3), i(2), i(1)]));
        assert!(range(&[i(0), i(5), i(0)]).is_err());
    }

    #[test]
    fn test_zip() {
        let result = zip(&[list(vec![i(1), i(2)]), list(vec![s("a"), s("b")])]).unwrap();
        assert_eq!(
            result,
            list(vec![list(vec![i(1), s("a")]), list(vec![i(2), s("b")])])
        );
    }

    // --- Aggregate ---

    #[test]
    fn test_sum() {
        assert_eq!(sum(&[list(vec![i(1), i(2), i(3)])]).unwrap(), i(6));
        assert_eq!(sum(&[list(vec![f(1.5), f(2.5)])]).unwrap(), f(4.0));
        assert_eq!(sum(&[list(vec![])]).unwrap(), i(0));
    }

    #[test]
    fn test_avg() {
        assert_eq!(avg(&[list(vec![i(1), i(2), i(3)])]).unwrap(), f(2.0));
        assert!(avg(&[list(vec![])]).is_err());
    }

    #[test]
    fn test_min_of_max_of() {
        assert_eq!(min_of(&[list(vec![i(3), i(1), i(4), i(1), i(5)])]).unwrap(), i(1));
        assert_eq!(max_of(&[list(vec![i(3), i(1), i(4), i(1), i(5)])]).unwrap(), i(5));
        assert!(min_of(&[list(vec![])]).is_err());
        assert!(max_of(&[list(vec![])]).is_err());
    }

    // --- Hash / encoding ---

    #[test]
    fn test_sha256() {
        let result = fn_sha256(&[s("hello")]).unwrap();
        // known SHA-256 of "hello"
        assert_eq!(
            result,
            s("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
    }

    #[test]
    fn test_base64_encode() {
        let result = base64_encode(&[s("hello")]).unwrap();
        assert_eq!(result, s("aGVsbG8="));
    }

    #[test]
    fn test_base64_roundtrip() {
        let encoded = base64_encode(&[s("Hello, World!")]).unwrap();
        let decoded = base64_decode(&[encoded]).unwrap();
        assert_eq!(decoded, s("Hello, World!"));
    }

    #[test]
    fn test_base64_decode_invalid() {
        assert!(base64_decode(&[s("not valid base64!!!")]).is_err());
    }

    #[test]
    fn test_json_encode() {
        assert_eq!(json_encode(&[i(42)]).unwrap(), s("42"));
        assert_eq!(json_encode(&[s("hello")]).unwrap(), s("\"hello\""));
        assert_eq!(json_encode(&[Value::Bool(true)]).unwrap(), s("true"));
        assert_eq!(json_encode(&[Value::Null]).unwrap(), s("null"));

        let result = json_encode(&[list(vec![i(1), i(2)])]).unwrap();
        assert_eq!(result, s("[1,2]"));

        let mut m = IndexMap::new();
        m.insert("k".to_string(), s("v"));
        let result = json_encode(&[Value::Map(m)]).unwrap();
        assert_eq!(result, s("{\"k\":\"v\"}"));
    }

    // --- Type coercion ---

    #[test]
    fn test_to_string() {
        assert_eq!(to_string(&[i(42)]).unwrap(), s("42"));
        assert_eq!(to_string(&[f(3.14)]).unwrap(), s("3.14"));
        assert_eq!(to_string(&[Value::Bool(true)]).unwrap(), s("true"));
        assert_eq!(to_string(&[Value::Null]).unwrap(), s("null"));
        assert_eq!(to_string(&[s("already")]).unwrap(), s("already"));
    }

    #[test]
    fn test_to_int() {
        assert_eq!(to_int(&[s("42")]).unwrap(), i(42));
        assert_eq!(to_int(&[f(3.9)]).unwrap(), i(3));
        assert_eq!(to_int(&[Value::Bool(true)]).unwrap(), i(1));
        assert_eq!(to_int(&[Value::Bool(false)]).unwrap(), i(0));
        assert!(to_int(&[s("not a number")]).is_err());
    }

    #[test]
    fn test_to_float() {
        assert_eq!(to_float(&[s("3.14")]).unwrap(), f(3.14));
        assert_eq!(to_float(&[i(7)]).unwrap(), f(7.0));
        assert!(to_float(&[s("abc")]).is_err());
    }

    #[test]
    fn test_to_bool() {
        assert_eq!(to_bool(&[s("true")]).unwrap(), Value::Bool(true));
        assert_eq!(to_bool(&[s("false")]).unwrap(), Value::Bool(false));
        assert_eq!(to_bool(&[i(1)]).unwrap(), Value::Bool(true));
        assert_eq!(to_bool(&[i(0)]).unwrap(), Value::Bool(false));
        assert!(to_bool(&[i(2)]).is_err());
        assert!(to_bool(&[s("yes")]).is_err());
    }

    #[test]
    fn test_type_of() {
        assert_eq!(type_of(&[s("hello")]).unwrap(), s("string"));
        assert_eq!(type_of(&[i(1)]).unwrap(), s("int"));
        assert_eq!(type_of(&[f(1.0)]).unwrap(), s("float"));
        assert_eq!(type_of(&[Value::Bool(true)]).unwrap(), s("bool"));
        assert_eq!(type_of(&[Value::Null]).unwrap(), s("null"));
        assert_eq!(type_of(&[list(vec![])]).unwrap(), s("list"));
        assert_eq!(type_of(&[Value::Map(IndexMap::new())]).unwrap(), s("map"));
    }

    // --- Builtin registry ---

    #[test]
    fn test_registry_completeness() {
        let registry = builtin_registry();
        let expected = [
            "upper", "lower", "trim", "trim_prefix", "trim_suffix", "replace",
            "split", "join", "starts_with", "ends_with", "contains", "length",
            "substr", "format", "regex_match", "regex_capture",
            "abs", "min", "max", "floor", "ceil", "round", "sqrt", "pow",
            "len", "keys", "values", "flatten", "concat", "distinct", "sort",
            "reverse", "index_of", "range", "zip",
            "map", "filter", "every", "some", "reduce", "count",
            "sum", "avg", "min_of", "max_of",
            "sha256", "base64_encode", "base64_decode", "json_encode",
            "to_string", "to_int", "to_float", "to_bool", "type_of",
            "has", "has_decorator",
        ];
        for name in &expected {
            assert!(registry.contains_key(name), "missing builtin: {}", name);
        }
    }

    #[test]
    fn test_higher_order_placeholder() {
        let err = higher_order_placeholder(&[]).unwrap_err();
        assert!(err.contains("higher-order functions require special evaluation"));
    }

    // --- Reference and Query Functions ---

    #[test]
    fn test_has_attribute_present() {
        let mut attrs = IndexMap::new();
        attrs.insert("port".to_string(), i(8080));
        attrs.insert("tls".to_string(), Value::Bool(true));
        let br = Value::BlockRef(crate::value::BlockRef {
            kind: "service".to_string(),
            id: Some("svc-api".to_string()),
            labels: vec![],
            attributes: attrs,
            children: vec![],
            decorators: vec![],
            span: wcl_core::Span::dummy(),
        });
        assert_eq!(fn_has(&[br.clone(), s("port")]).unwrap(), Value::Bool(true));
        assert_eq!(fn_has(&[br.clone(), s("tls")]).unwrap(), Value::Bool(true));
        assert_eq!(fn_has(&[br, s("missing")]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_has_child_block() {
        let child = crate::value::BlockRef {
            kind: "monitoring".to_string(),
            id: None,
            labels: vec![],
            attributes: IndexMap::new(),
            children: vec![],
            decorators: vec![],
            span: wcl_core::Span::dummy(),
        };
        let br = Value::BlockRef(crate::value::BlockRef {
            kind: "service".to_string(),
            id: Some("svc-api".to_string()),
            labels: vec![],
            attributes: IndexMap::new(),
            children: vec![child],
            decorators: vec![],
            span: wcl_core::Span::dummy(),
        });
        assert_eq!(fn_has(&[br.clone(), s("monitoring")]).unwrap(), Value::Bool(true));
        assert_eq!(fn_has(&[br, s("logging")]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_has_wrong_arg_type() {
        assert!(fn_has(&[s("not a block"), s("attr")]).is_err());
        assert!(fn_has(&[i(42), s("attr")]).is_err());
    }

    #[test]
    fn test_has_decorator_present() {
        let br = Value::BlockRef(crate::value::BlockRef {
            kind: "service".to_string(),
            id: Some("svc-api".to_string()),
            labels: vec![],
            attributes: IndexMap::new(),
            children: vec![],
            decorators: vec![
                crate::value::DecoratorValue {
                    name: "deprecated".to_string(),
                    args: IndexMap::new(),
                },
                crate::value::DecoratorValue {
                    name: "sensitive".to_string(),
                    args: IndexMap::new(),
                },
            ],
            span: wcl_core::Span::dummy(),
        });
        assert_eq!(fn_has_decorator(&[br.clone(), s("deprecated")]).unwrap(), Value::Bool(true));
        assert_eq!(fn_has_decorator(&[br.clone(), s("sensitive")]).unwrap(), Value::Bool(true));
        assert_eq!(fn_has_decorator(&[br, s("optional")]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_has_decorator_wrong_arg_type() {
        assert!(fn_has_decorator(&[s("not a block"), s("deco")]).is_err());
    }
}
