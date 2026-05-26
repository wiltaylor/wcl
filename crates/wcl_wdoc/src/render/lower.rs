//! Lowering dispatch: invoke a block's `lower` function and recursively
//! render the returned fundamental variants (HTML + SVG).

use std::collections::BTreeMap;

use wcl_lang::{Block, Document, FnValue, Value, VariantPayload};

use crate::inline::InlinePatterns;

use super::*;

/// Lowering recursion guard. A lowering may emit other custom kinds
/// that themselves lower further; this caps how deep we'll follow
/// before bailing.
pub(crate) const MAX_LOWER_DEPTH: usize = 32;

/// Look up the `lower` function for a block kind. Tries the block's
/// own `lower` field first (per-instance override), then the kind's
/// `@block` type's `@default(...)` for `lower`. Returns `None` when
/// neither path produces a callable.
pub(crate) fn lookup_block_lower(doc: &Document, block: &Block<'_>, kind: &str) -> Option<FnValue> {
    if let Some(field) = block.field("lower")
        && let Ok(Value::Function(fv)) = field.value()
    {
        return Some(fv.clone());
    }
    lookup_type_lower(doc, kind)
}

/// Look up the `lower` function declared on a `@block` (or plain
/// `type`) by reading its `lower` field's `@default(...)` value.
/// Used both for block-side dispatch (after the instance check) and
/// for recursive variant dispatch (where no instance is available).
pub(crate) fn lookup_type_lower(doc: &Document, kind: &str) -> Option<FnValue> {
    let schema = doc
        .block_schema(kind)
        .or_else(|| doc.type_decl(&kind_to_typename(kind)))?;
    match schema.field("lower")?.default_value()? {
        Value::Function(fv) => Some(fv),
        _ => None,
    }
}

/// Custom diagram-shape lowering. Resolves the block's `lower`
/// function, calls it with a record built from the block's fields,
/// and renders each returned variant.
pub(crate) fn lower_svg_block(
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let Some(arg) = block_to_record(doc, block, kind) else {
        return String::new();
    };
    let Some(fv) = lookup_block_lower(doc, block, kind) else {
        return String::new();
    };
    let result = match doc.call_value(&fv, &[arg]) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let Value::List(items) = result else {
        return String::new();
    };
    items
        .iter()
        .map(|v| render_svg_variant(doc, v, parent_w, parent_h, 0))
        .collect()
}

/// Custom HTML-block lowering (h1..h6, text, code, callout, and friends).
pub(crate) fn lower_html_block(
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    patterns: &InlinePatterns,
) -> String {
    let Some(arg) = block_to_record(doc, block, kind) else {
        return String::new();
    };
    let Some(fv) = lookup_block_lower(doc, block, kind) else {
        return String::new();
    };
    let result = match doc.call_value(&fv, &[arg]) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let Value::List(items) = result else {
        return String::new();
    };
    items
        .iter()
        .map(|v| render_html_variant(doc, v, 0, patterns))
        .collect()
}

// `_parent_w` / `_parent_h` are threaded through so future variant
// kinds can pick them up; today's fundamentals carry pre-resolved
// geometry in the payload itself.
pub(crate) fn render_svg_variant(
    doc: &Document,
    value: &Value,
    _parent_w: f64,
    _parent_h: f64,
    depth: usize,
) -> String {
    if depth > MAX_LOWER_DEPTH {
        return depth_marker();
    }
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return String::new();
    };
    let kind = kind_for_variant(variant);
    let VariantPayload::Record(map) = payload else {
        return String::new();
    };
    match kind.as_str() {
        "rect" => render_rect_payload(map),
        "circle" => render_circle_payload(map),
        "line" => render_line_payload(map),
        "label" => render_label_payload(map),
        "polygon" => render_polygon_payload(map),
        other => {
            // Custom variant — look up its type's `lower` and recurse
            // with the variant's record payload as the new arg.
            let arg = payload_to_record(map, other);
            let Some(fv) = lookup_type_lower(doc, other) else {
                return String::new();
            };
            let result = match doc.call_value(&fv, &[arg]) {
                Ok(v) => v,
                Err(_) => return String::new(),
            };
            let Value::List(items) = result else {
                return String::new();
            };
            items
                .iter()
                .map(|v| render_svg_variant(doc, v, _parent_w, _parent_h, depth + 1))
                .collect()
        }
    }
}

pub(crate) fn render_html_variant(
    doc: &Document,
    value: &Value,
    depth: usize,
    patterns: &InlinePatterns,
) -> String {
    if depth > MAX_LOWER_DEPTH {
        return depth_marker();
    }
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return String::new();
    };
    let kind = kind_for_variant(variant);
    let VariantPayload::Record(map) = payload else {
        return String::new();
    };
    match kind.as_str() {
        "paragraph" => render_paragraph_payload(map),
        "table" => render_table_payload(map),
        "element" => render_element_payload(doc, map, depth, patterns),
        "raw" => render_raw_payload(map),
        // An icon, resolved against the registry so it records sprite
        // usage. Emitted by the stdlib `callout` lowering (and available
        // to any user HTML lowering).
        "icon" => render_icon_fundamental(map, patterns.icons()),
        // Inline prose run through the inline-pattern engine (bold /
        // italic / link / icon). The Rust regex engine stays a leaf; the
        // `<p>`/`<span>` wrappers around it live in WCL (the `text` lower).
        "inline" => render_inline_fundamental(doc, map, patterns),
        // Syntax-highlighted code body (syntect). Like `inline`, the
        // engine is a leaf; the `<pre><code>` wrapper is the `code` lower.
        "highlighted" => render_highlighted_fundamental(map),
        other => {
            let arg = payload_to_record(map, other);
            let Some(fv) = lookup_type_lower(doc, other) else {
                return String::new();
            };
            let result = match doc.call_value(&fv, &[arg]) {
                Ok(v) => v,
                Err(_) => return String::new(),
            };
            let Value::List(items) = result else {
                return String::new();
            };
            items
                .iter()
                .map(|v| render_html_variant(doc, v, depth + 1, patterns))
                .collect()
        }
    }
}

pub(crate) fn depth_marker() -> String {
    "<!-- wdoc: lowering depth limit reached -->".into()
}

/// Build a `Value::Record` from `block`'s declared fields. Schema is
/// looked up via `doc.block_schema(kind)`. Each declared field is
/// populated from either the matching `@inline(N)` label slot or the
/// literal block field; missing values become `Value::None` so
/// optional fields cleanly reach the lowering function.
pub(crate) fn block_to_record(doc: &Document, block: &Block<'_>, kind: &str) -> Option<Value> {
    let schema = doc.block_schema(kind)?;
    let labels = block.labels().ok().unwrap_or_default();
    let mut map = BTreeMap::new();
    for f in schema.fields() {
        let name = f.name();
        let val = if let Some(slot) = f.inline_slot() {
            labels.get(slot as usize).cloned().unwrap_or(Value::None)
        } else if let Some(field) = block.field(name) {
            field.value().cloned().unwrap_or(Value::None)
        } else if let Some(dr) = block.typed_field(name) {
            // A schema-projected field with no raw AST entry: either a
            // leaf typed projection (e.g. a `@connections` list, which
            // has a `Value`) or a `@children(...)` block list, which has
            // none. Materialise children by recursively converting each
            // child block to a record (using its own kind), so a `lower`
            // can map over them — e.g. `text`'s `@children("span") spans`.
            match dr.value() {
                Ok(v) => v,
                Err(_) => match dr.as_block_list() {
                    Some(blocks) => Value::List(
                        blocks
                            .iter()
                            .filter_map(|b| block_to_record(doc, b, b.kind()))
                            .collect(),
                    ),
                    None => f.default_value().unwrap_or(Value::None),
                },
            }
        } else {
            // Fall back to the schema's declared default
            // (`name = expr` inline-default or `@default(expr)`)
            // so a lowering that consumes `block.x` doesn't crash
            // when the block omits the field but the type
            // declared a value-typed default.
            f.default_value().unwrap_or(Value::None)
        };
        map.insert(name.to_string(), val);
    }
    // Grow width / height in the record so a Process / Decision /
    // Terminator lowering whose text spans multiple lines (or one
    // very long line) sees the same effective dimensions the
    // layered solver did. Without this, the rendered rect would
    // stay at the declared size while the layout reserved a
    // larger cell — the text would spill out of the rect.
    let (eff_w, eff_h) = effective_dims(block);
    if map.contains_key("width") {
        map.insert("width".to_string(), Value::F64(eff_w));
    }
    if map.contains_key("height") {
        map.insert("height".to_string(), Value::F64(eff_h));
    }
    Some(Value::Record {
        ty: vec![kind_to_typename(kind)],
        fields: map,
    })
}

pub(crate) fn payload_to_record(map: &BTreeMap<String, Value>, kind: &str) -> Value {
    Value::Record {
        ty: vec![kind_to_typename(kind)],
        fields: map.clone(),
    }
}

/// Best-effort kind→type-name mapping. With our naming convention
/// (variant name = capitalised kind), `"process"` ↔ `Process`.
pub(crate) fn kind_to_typename(kind: &str) -> String {
    let mut s = String::with_capacity(kind.len());
    let mut up = true;
    for c in kind.chars() {
        if c == '_' {
            up = true;
            continue;
        }
        if up {
            s.extend(c.to_uppercase());
            up = false;
        } else {
            s.push(c);
        }
    }
    s
}

pub(crate) fn kind_for_variant(variant: &str) -> String {
    let mut s = String::with_capacity(variant.len());
    for (i, c) in variant.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            s.push('_');
        }
        s.extend(c.to_lowercase());
    }
    s
}

// ── Fundamental renderers (block-side) ────────────────────────────
