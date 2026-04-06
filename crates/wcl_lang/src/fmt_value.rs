//! Formatters for evaluated `Value`s.
//!
//! - `value_to_json` / `blockref_to_json`: convert to `serde_json::Value`
//!   for JSON output.
//! - `value_to_wcl`: render a `Value` as WCL source text (block/attribute
//!   syntax where applicable).

use crate::eval::value::{BlockRef, Value};

// ── JSON ────────────────────────────────────────────────────────────────────

pub fn value_to_json(val: &Value) -> serde_json::Value {
    match val {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Null => serde_json::Value::Null,
        Value::Identifier(s) => serde_json::Value::String(s.clone()),
        Value::Symbol(s) => serde_json::Value::String(s.clone()),
        Value::List(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::BlockRef(br) => blockref_to_json(br),
        _ => serde_json::Value::String(format!("{}", val)),
    }
}

pub fn blockref_to_json(br: &BlockRef) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for (k, v) in &br.attributes {
        obj.insert(k.clone(), value_to_json(v));
    }
    for child in &br.children {
        let key = child.id.as_deref().unwrap_or(&child.kind);
        obj.insert(key.to_string(), blockref_to_json(child));
    }
    serde_json::Value::Object(obj)
}

// ── WCL ─────────────────────────────────────────────────────────────────────

/// Format a `Value` as WCL source text.
pub fn value_to_wcl(val: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, val, 0);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn write_value(out: &mut String, val: &Value, indent: usize) {
    let pad = "    ".repeat(indent);
    match val {
        Value::BlockRef(br) => write_block(out, br, indent),
        Value::Map(m) => {
            for (key, value) in m {
                out.push_str(&pad);
                out.push_str(key);
                out.push_str(" = ");
                write_inline(out, value, indent);
                out.push('\n');
            }
        }
        Value::List(items) => {
            // Top-level list: emit each item on its own. Blocks become blocks,
            // other values become bracketed list syntax.
            let all_blocks =
                !items.is_empty() && items.iter().all(|v| matches!(v, Value::BlockRef(_)));
            if all_blocks {
                for item in items {
                    write_value(out, item, indent);
                }
            } else {
                out.push_str(&pad);
                write_inline(out, val, indent);
                out.push('\n');
            }
        }
        other => {
            out.push_str(&pad);
            write_inline(out, other, indent);
            out.push('\n');
        }
    }
}

fn write_block(out: &mut String, br: &BlockRef, indent: usize) {
    let pad = "    ".repeat(indent);
    out.push_str(&pad);
    out.push_str(&br.kind);
    if let Some(id) = &br.id {
        out.push(' ');
        out.push_str(id);
    }
    out.push_str(" {\n");
    let inner = "    ".repeat(indent + 1);
    for (key, value) in &br.attributes {
        out.push_str(&inner);
        out.push_str(key);
        out.push_str(" = ");
        write_inline(out, value, indent + 1);
        out.push('\n');
    }
    for child in &br.children {
        write_block(out, child, indent + 1);
    }
    out.push_str(&pad);
    out.push_str("}\n");
}

fn write_inline(out: &mut String, val: &Value, indent: usize) {
    match val {
        Value::String(s) => {
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    '\r' => out.push_str("\\r"),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(f) => out.push_str(&f.to_string()),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
        Value::Identifier(s) => out.push_str(s),
        Value::Symbol(s) => {
            out.push(':');
            out.push_str(s);
        }
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_inline(out, item, indent);
            }
            out.push(']');
        }
        Value::Map(m) => {
            out.push('{');
            for (i, (k, v)) in m.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(k);
                out.push_str(": ");
                write_inline(out, v, indent);
            }
            out.push('}');
        }
        Value::BlockRef(br) => {
            // Blocks inside inline contexts — fall back to block form on new lines.
            out.push('\n');
            write_block(out, br, indent);
        }
        other => out.push_str(&format!("{}", other)),
    }
}

/// Format the whole evaluated document as WCL source text.
pub fn document_values_to_wcl(values: &indexmap::IndexMap<String, Value>) -> String {
    let mut out = String::new();
    for (key, val) in values {
        match val {
            Value::BlockRef(br) => write_block(&mut out, br, 0),
            Value::List(items) if items.iter().all(|v| matches!(v, Value::BlockRef(_))) => {
                for item in items {
                    if let Value::BlockRef(br) = item {
                        write_block(&mut out, br, 0);
                    }
                }
            }
            other => {
                out.push_str(key);
                out.push_str(" = ");
                write_inline(&mut out, other, 0);
                out.push('\n');
            }
        }
    }
    out
}
