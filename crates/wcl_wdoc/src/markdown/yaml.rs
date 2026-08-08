//! Serialize a page's schemaless `frontmatter` block to a YAML header.
//!
//! The `frontmatter` block (see `lib/core.wcl`) is open: the author writes
//! arbitrary `key = value` entries. We read them back in source order via
//! [`Block::fields`] and emit block-style YAML between `---` fences. Scalars
//! (strings, numbers, bools, symbols), lists, and nested records are
//! covered; anything more exotic (tensors, closures) falls back to a quoted
//! one-line form.

use wcl_lang::{Block, Value};

use crate::build::BuildError;

/// Build the `---`-fenced YAML front matter for `page` from its
/// `frontmatter` child block, in author (source) order. Returns `Ok(None)`
/// when the page has no `frontmatter` block or it is empty (nothing to
/// emit).
///
/// Reading arbitrary keys requires the block to be `@schemaless` (WCL's
/// strict field-membership check otherwise rejects undeclared fields). A
/// `frontmatter` block missing that marker would silently drop every field,
/// so we surface it as an actionable build error instead.
pub(crate) fn front_matter(page: &Block<'_>) -> Result<Option<String>, BuildError> {
    let Some(fm) = page.blocks().find(|b| b.kind() == "frontmatter") else {
        return Ok(None);
    };
    let mut body = String::new();
    for field in fm.fields() {
        match field.value() {
            Ok(value) => {
                body.push_str(&yaml_key(field.name()));
                emit_value(&mut body, value, 0);
            }
            Err(_) => {
                return Err(BuildError::BadPage(format!(
                    "frontmatter field '{}' could not be read — mark the block `@schemaless` \
                     so it accepts arbitrary keys (`@schemaless frontmatter {{ … }}`)",
                    field.name()
                )));
            }
        }
    }
    if body.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!("---\n{body}---\n")))
    }
}

/// Append the value half of a `key:` entry (the key text is already in
/// `out`), formatted `indent` spaces deep. Scalars stay on the key's line;
/// lists and records open onto following, more-indented lines.
fn emit_value(out: &mut String, value: &Value, indent: usize) {
    if let Some(s) = scalar(value) {
        out.push_str(&format!(": {s}\n"));
        return;
    }
    match value {
        Value::List(items) if items.is_empty() => out.push_str(": []\n"),
        Value::List(items) => {
            out.push_str(":\n");
            let pad = " ".repeat(indent + 2);
            for item in items.iter() {
                if let Some(s) = scalar(item) {
                    out.push_str(&format!("{pad}- {s}\n"));
                } else {
                    // A nested collection inside a list item is rare in front
                    // matter; emit a quoted one-line fallback rather than risk
                    // malformed block YAML.
                    out.push_str(&format!("{pad}- {}\n", quote(&item.to_string())));
                }
            }
        }
        Value::Record { fields, .. } if fields.is_empty() => out.push_str(": {}\n"),
        Value::Record { fields, .. } => {
            out.push_str(":\n");
            let pad = " ".repeat(indent + 2);
            for (k, v) in fields.iter() {
                out.push_str(&format!("{pad}{}", yaml_key(k)));
                emit_value(out, v, indent + 2);
            }
        }
        // Tensors / closures / variants / data-paths have no clean YAML
        // shape — fall back to a quoted one-line rendering.
        other => out.push_str(&format!(": {}\n", quote(&other.to_string()))),
    }
}

/// Render a scalar `Value` as a YAML scalar, or `None` for a collection /
/// non-scalar that needs block formatting.
fn scalar(v: &Value) -> Option<String> {
    Some(match v {
        Value::Bool(b) => b.to_string(),
        Value::None => "null".to_string(),
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => {
            scalar_string(s)
        }
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
        Value::F32(n) => fmt_float(f64::from(*n)),
        Value::F64(n) => fmt_float(*n),
        _ => return None,
    })
}

fn fmt_float(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// A YAML mapping key: bare when plain-safe, double-quoted otherwise.
fn yaml_key(k: &str) -> String {
    scalar_string(k)
}

/// A YAML string scalar: bare when it can't be misread as another type or
/// carry significant punctuation, double-quoted (with escapes) otherwise.
fn scalar_string(s: &str) -> String {
    if is_plain_safe(s) {
        s.to_string()
    } else {
        quote(s)
    }
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Whether `s` is safe to emit as a bare YAML scalar: non-empty, not a
/// reserved word, not number-like, no leading/trailing space, and built
/// only from unambiguous characters.
fn is_plain_safe(s: &str) -> bool {
    if s.is_empty() || s.trim() != s {
        return false;
    }
    let lower = s.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~"
    ) {
        return false;
    }
    if s.parse::<f64>().is_ok() {
        return false;
    }
    let first = s.chars().next().expect("non-empty checked above");
    if !(first.is_ascii_alphanumeric() || first == '_' || first == '/') {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.' | '/'))
}
