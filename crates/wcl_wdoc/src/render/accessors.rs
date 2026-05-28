//! Field / map / value readers, class-attribute helpers, and HTML escaping.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use wcl_lang::{Block, Value};

pub(crate) fn class_attr(block: &Block<'_>) -> String {
    let names = field_utf8_list(block, "class");
    classes_attr_from_names(&names)
}

pub(crate) fn append_attr(out: &mut String, name: &str, value: Option<&str>) {
    if let Some(v) = value {
        write!(out, " {name}=\"{}\"", escape_html(v)).expect("write to String");
    }
}

pub(crate) fn label_string(block: &Block<'_>) -> Option<String> {
    let labels = block.labels().ok()?;
    value_as_string(labels.into_iter().next()?)
}

pub(crate) fn value_as_string(v: Value) -> Option<String> {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => Some(s),
        other => Some(other.to_string()),
    }
}

pub(crate) fn field_utf8(block: &Block<'_>, name: &str) -> Option<String> {
    let field = block.field(name)?;
    match field.value().ok()? {
        Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

pub(crate) fn field_id(block: &Block<'_>, name: &str) -> Option<String> {
    let field = block.field(name)?;
    match field.value().ok()? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

pub(crate) fn field_bool(block: &Block<'_>, name: &str) -> Option<bool> {
    let field = block.field(name)?;
    match field.value().ok()? {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

pub(crate) fn field_symbol(block: &Block<'_>, name: &str) -> Option<String> {
    let field = block.field(name)?;
    match field.value().ok()? {
        Value::Symbol(s) => Some(s.clone()),
        _ => None,
    }
}

pub(crate) fn field_f64(block: &Block<'_>, name: &str) -> Option<f64> {
    if let Some(field) = block.field(name)
        && let Some(v) = field.value().ok().and_then(value_as_f64)
    {
        return Some(v);
    }
    // Fall back to a schema-declared default (`name = 0.0` inline
    // form or `@default(...)` decorator). This is what lets a
    // layered child render at (x=0, y=0) without forcing every
    // user to write x = 0.0 themselves.
    value_as_f64(&block.schema()?.field(name)?.default_value()?)
}

pub(crate) fn field_i64(block: &Block<'_>, name: &str) -> Option<i64> {
    let field = block.field(name)?;
    value_as_i64(field.value().ok()?)
}

pub(crate) fn field_f64_list(block: &Block<'_>, name: &str) -> Vec<f64> {
    let Some(field) = block.field(name) else {
        return Vec::new();
    };
    let Ok(value) = field.value() else {
        return Vec::new();
    };
    let Value::List(items) = value else {
        return Vec::new();
    };
    items.iter().filter_map(value_as_f64).collect()
}

pub(crate) fn field_utf8_list(block: &Block<'_>, name: &str) -> Vec<String> {
    let Some(field) = block.field(name) else {
        return Vec::new();
    };
    let Ok(value) = field.value() else {
        return Vec::new();
    };
    let Value::List(items) = value else {
        return Vec::new();
    };
    items.iter().filter_map(value_as_str).collect()
}

/// Read a `list<symbol>` field, distinguishing "field absent or
/// none" (returns `None`, callers apply their own default) from
/// "explicitly empty list" (returns `Some(vec![])`).
pub(crate) fn field_symbol_list_opt(block: &Block<'_>, name: &str) -> Option<Vec<String>> {
    let field = block.field(name)?;
    let value = field.value().ok()?;
    let Value::List(items) = value else {
        return None;
    };
    Some(
        items
            .iter()
            .filter_map(|v| match v {
                Value::Symbol(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
    )
}

// ── Map-side accessors (for variant payloads) ─────────────────────

pub(crate) fn class_attr_from_map(map: &BTreeMap<String, Value>) -> String {
    let names = map_utf8_list(map, "class");
    classes_attr_from_names(&names)
}

pub(crate) fn classes_attr_from_names(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let joined = names
        .iter()
        .map(|s| escape_html(s))
        .collect::<Vec<_>>()
        .join(" ");
    format!(" class=\"{joined}\"")
}

pub(crate) fn map_utf8(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    match map.get(name)? {
        Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

pub(crate) fn map_id(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    match map.get(name)? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

pub(crate) fn map_f64(map: &BTreeMap<String, Value>, name: &str) -> Option<f64> {
    value_as_f64(map.get(name)?)
}

pub(crate) fn map_i64(map: &BTreeMap<String, Value>, name: &str) -> Option<i64> {
    value_as_i64(map.get(name)?)
}

pub(crate) fn map_bool(map: &BTreeMap<String, Value>, name: &str) -> Option<bool> {
    match map.get(name)? {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

pub(crate) fn map_symbol(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    match map.get(name)? {
        Value::Symbol(s) => Some(s.clone()),
        _ => None,
    }
}

pub(crate) fn map_utf8_list(map: &BTreeMap<String, Value>, name: &str) -> Vec<String> {
    let Some(Value::List(items)) = map.get(name) else {
        return Vec::new();
    };
    items.iter().filter_map(value_as_str).collect()
}

// ── Value-coercion helpers ────────────────────────────────────────

pub(crate) fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::F64(n) => Some(*n),
        Value::F32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        _ => None,
    }
}

pub(crate) fn value_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::I64(n) => Some(*n),
        Value::I32(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        Value::U64(n) => Some(*n as i64),
        _ => None,
    }
}

pub(crate) fn value_as_str(v: &Value) -> Option<String> {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => {
            Some(s.clone())
        }
        _ => None,
    }
}

pub(crate) fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}
