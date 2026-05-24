use std::collections::BTreeMap;
use std::fmt::Write as _;

use wcl_lang::{Block, Document, FnValue, Value, VariantPayload};

/// Lowering recursion guard. A lowering may emit other custom kinds
/// that themselves lower further; this caps how deep we'll follow
/// before bailing.
const MAX_LOWER_DEPTH: usize = 32;

pub(crate) fn render_page(name: &str, css: &str, blocks: impl Iterator<Item = String>) -> String {
    let mut body = String::new();
    for b in blocks {
        body.push_str(&b);
        body.push('\n');
    }
    format!(
        "<!DOCTYPE html>\n\
         <html>\n\
         <head><meta charset=\"utf-8\"><title>{title}</title>\n\
         <style>{css}</style></head>\n\
         <body>\n\
         {body}</body>\n\
         </html>\n",
        title = escape_html(name),
        body = body,
    )
}

pub(crate) fn render_block(doc: &Document, block: &Block<'_>) -> Option<String> {
    match block.kind() {
        "text" => Some(render_text(doc, block)),
        "column" => Some(render_column(doc, block)),
        "diagram" => Some(render_diagram(doc, block)),
        // Skip the lowering function declarations — they're top-level
        // fields, not blocks, so they don't reach render_block.
        kind => Some(lower_html_block(doc, block, kind)),
    }
}

/// Emit a CSS rule body for a `@block("class")` instance.
/// Returns `None` if the block doesn't have an inline name.
pub(crate) fn render_class(block: &Block<'_>) -> Option<String> {
    let name = label_string(block)?;
    let mut props = String::new();
    push_css(&mut props, "color", field_utf8(block, "color").as_deref());
    push_css(
        &mut props,
        "background",
        field_utf8(block, "background").as_deref(),
    );
    if field_bool(block, "bold") == Some(true) {
        props.push_str("font-weight:bold;");
    }
    if field_bool(block, "italic") == Some(true) {
        props.push_str("font-style:italic;");
    }
    if field_bool(block, "underline") == Some(true) {
        props.push_str("text-decoration:underline;");
    }
    push_css(
        &mut props,
        "font-size",
        field_utf8(block, "font_size").as_deref(),
    );
    push_css(
        &mut props,
        "font-family",
        field_utf8(block, "font_family").as_deref(),
    );
    push_css(
        &mut props,
        "text-align",
        field_utf8(block, "text_align").as_deref(),
    );
    push_css(
        &mut props,
        "padding",
        field_utf8(block, "padding").as_deref(),
    );
    push_css(&mut props, "margin", field_utf8(block, "margin").as_deref());
    push_css(&mut props, "border", field_utf8(block, "border").as_deref());
    Some(format!(".{name} {{ {props} }}"))
}

fn push_css(out: &mut String, prop: &str, value: Option<&str>) {
    if let Some(v) = value {
        write!(out, "{prop}:{v};").expect("write to String");
    }
}

fn render_text(doc: &Document, block: &Block<'_>) -> String {
    let cls = class_attr(block);
    let spans: String = block
        .blocks()
        .filter(|b| b.kind() == "span")
        .map(|b| render_span(&b))
        .collect();
    let _ = doc;
    let mut out = format!("<p{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, ">{spans}</p>").expect("write to String");
    out
}

fn render_span(block: &Block<'_>) -> String {
    let cls = class_attr(block);
    let text = label_string(block).unwrap_or_default();
    let mut out = format!("<span{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, ">{}</span>", escape_html(&text)).expect("write to String");
    out
}

fn render_column(doc: &Document, block: &Block<'_>) -> String {
    let cls = class_attr(block);
    let widths = field_f64_list(block, "widths");
    let grid_cols: String = widths
        .iter()
        .map(|w| format!("{w}%"))
        .collect::<Vec<_>>()
        .join(" ");
    let children: String = block
        .blocks()
        .filter_map(|b| render_block(doc, &b))
        .collect();
    let mut out = format!("<div{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(
        out,
        " style=\"display:grid;grid-template-columns:{grid_cols};\">{children}</div>"
    )
    .expect("write to String");
    out
}

fn render_diagram(doc: &Document, block: &Block<'_>) -> String {
    let cls = class_attr(block);
    let width = field_i64(block, "width").unwrap_or(0);
    let height = field_i64(block, "height").unwrap_or(0);
    let shapes: String = block
        .blocks()
        .filter_map(|b| render_shape(doc, &b, width as f64, height as f64))
        .collect();
    let mut out = format!("<svg{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(
        out,
        " xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\">{shapes}</svg>"
    )
    .expect("write to String");
    out
}

fn render_shape(doc: &Document, block: &Block<'_>, parent_w: f64, parent_h: f64) -> Option<String> {
    match block.kind() {
        "rect" => Some(render_rect(block, parent_w, parent_h)),
        "circle" => Some(render_circle(block, parent_w, parent_h)),
        "line" => Some(render_line(block, parent_w, parent_h)),
        "label" => Some(render_label(block, parent_w, parent_h)),
        "polygon" => Some(render_polygon(block, parent_w, parent_h)),
        "container" => Some(render_container(doc, block, parent_w, parent_h)),
        kind => Some(lower_svg_block(doc, block, kind, parent_w, parent_h)),
    }
}

fn render_container(doc: &Document, block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let cls = class_attr(block);
    let (x, y, w, h) = resolve_container_box(block, parent_w, parent_h);

    let layout = field_symbol(block, "layout").unwrap_or_default();
    let inner = match layout.as_str() {
        "grid" => render_grid_children(doc, block),
        _ => block
            .blocks()
            .filter_map(|b| render_shape(doc, &b, w, h))
            .collect::<String>(),
    };

    let transform = if x != 0.0 || y != 0.0 {
        format!(" transform=\"translate({x} {y})\"")
    } else {
        String::new()
    };
    let mut out = format!("<g{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, "{transform}>{inner}</g>").expect("write to String");
    out
}

fn render_grid_children(doc: &Document, block: &Block<'_>) -> String {
    let cols = field_i64(block, "columns").unwrap_or(1).max(1) as usize;
    let cw = field_f64(block, "cell_width").unwrap_or(0.0);
    let ch = field_f64(block, "cell_height").unwrap_or(0.0);
    let gap = field_f64(block, "gap").unwrap_or(0.0);
    block
        .blocks()
        .enumerate()
        .filter_map(|(i, b)| {
            let rendered = render_shape(doc, &b, cw, ch)?;
            let col = i % cols;
            let row = i / cols;
            let tx = col as f64 * (cw + gap);
            let ty = row as f64 * (ch + gap);
            Some(format!(
                "<g transform=\"translate({tx} {ty})\">{rendered}</g>"
            ))
        })
        .collect()
}

// ── Lowering dispatch ──────────────────────────────────────────────

/// Look up the `lower` function for a block kind. Tries the block's
/// own `lower` field first (per-instance override), then the kind's
/// `@block` type's `@default(...)` for `lower`. Returns `None` when
/// neither path produces a callable.
fn lookup_block_lower(doc: &Document, block: &Block<'_>, kind: &str) -> Option<FnValue> {
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
fn lookup_type_lower(doc: &Document, kind: &str) -> Option<FnValue> {
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
fn lower_svg_block(
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

/// Custom HTML-block lowering (h1..h6 and friends).
fn lower_html_block(doc: &Document, block: &Block<'_>, kind: &str) -> String {
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
        .map(|v| render_html_variant(doc, v, 0))
        .collect()
}

// `_parent_w` / `_parent_h` are threaded through so future variant
// kinds can pick them up; today's fundamentals carry pre-resolved
// geometry in the payload itself.
fn render_svg_variant(
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

fn render_html_variant(doc: &Document, value: &Value, depth: usize) -> String {
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
                .map(|v| render_html_variant(doc, v, depth + 1))
                .collect()
        }
    }
}

fn depth_marker() -> String {
    "<!-- wdoc: lowering depth limit reached -->".into()
}

/// Build a `Value::Record` from `block`'s declared fields. Schema is
/// looked up via `doc.block_schema(kind)`. Each declared field is
/// populated from either the matching `@inline(N)` label slot or the
/// literal block field; missing values become `Value::None` so
/// optional fields cleanly reach the lowering function.
fn block_to_record(doc: &Document, block: &Block<'_>, kind: &str) -> Option<Value> {
    let schema = doc.block_schema(kind)?;
    let labels = block.labels().ok().unwrap_or_default();
    let mut map = BTreeMap::new();
    for f in schema.fields() {
        let name = f.name();
        let val = if let Some(slot) = f.inline_slot() {
            labels.get(slot as usize).cloned().unwrap_or(Value::None)
        } else if let Some(field) = block.field(name) {
            field.value().cloned().unwrap_or(Value::None)
        } else {
            Value::None
        };
        map.insert(name.to_string(), val);
    }
    Some(Value::Record {
        ty: vec![kind_to_typename(kind)],
        fields: map,
    })
}

fn payload_to_record(map: &BTreeMap<String, Value>, kind: &str) -> Value {
    Value::Record {
        ty: vec![kind_to_typename(kind)],
        fields: map.clone(),
    }
}

/// Best-effort kind→type-name mapping. With our naming convention
/// (variant name = capitalised kind), `"process"` ↔ `Process`.
fn kind_to_typename(kind: &str) -> String {
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

fn kind_for_variant(variant: &str) -> String {
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

fn render_rect(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let cls = class_attr(block);
    let (x, y, w, h) = resolve_rect_box(block, parent_w, parent_h);
    let mut out = format!("<rect{cls} x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"");
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    out.push_str(" />");
    out
}

fn render_circle(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let cls = class_attr(block);
    let (cx, cy, r) = resolve_circle(block, parent_w, parent_h);
    let mut out = format!("<circle{cls} cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"");
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    out.push_str(" />");
    out
}

fn render_line(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let cls = class_attr(block);
    let x1 = field_f64(block, "x1").unwrap_or(0.0);
    let y1 = field_f64(block, "y1").unwrap_or(0.0);
    let x2 = field_f64(block, "x2").unwrap_or(0.0);
    let y2 = field_f64(block, "y2").unwrap_or(0.0);
    let (ox, oy) = resolve_point_anchor(block, parent_w, parent_h);
    let mut out = format!(
        "<line{cls} x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"",
        x1 = x1 + ox,
        y1 = y1 + oy,
        x2 = x2 + ox,
        y2 = y2 + oy,
    );
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    out.push_str(" />");
    out
}

fn render_label(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let cls = class_attr(block);
    let content = label_string(block).unwrap_or_default();
    let own_x = field_f64(block, "x").unwrap_or(0.0);
    let own_y = field_f64(block, "y").unwrap_or(0.0);
    let (x, y) = resolve_point_anchored(block, parent_w, parent_h, own_x, own_y);
    let mut out = format!("<text{cls} x=\"{x}\" y=\"{y}\"");
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, ">{}</text>", escape_html(&content)).expect("write to String");
    out
}

fn render_polygon(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let cls = class_attr(block);
    let points = field_utf8(block, "points").unwrap_or_default();
    let (ox, oy) = resolve_point_anchor(block, parent_w, parent_h);
    let mut out = format!("<polygon{cls} points=\"{}\"", escape_html(&points));
    if ox != 0.0 || oy != 0.0 {
        write!(out, " transform=\"translate({ox} {oy})\"").expect("write to String");
    }
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    out.push_str(" />");
    out
}

// ── Fundamental renderers (variant-payload side) ──────────────────
//
// Variant payloads carry pre-resolved geometry. Anchors are not
// honored here — lowering functions are expected to emit final
// coordinates.

fn render_rect_payload(map: &BTreeMap<String, Value>) -> String {
    let cls = class_attr_from_map(map);
    let x = map_f64(map, "x").unwrap_or(0.0);
    let y = map_f64(map, "y").unwrap_or(0.0);
    let w = map_f64(map, "width").unwrap_or(0.0);
    let h = map_f64(map, "height").unwrap_or(0.0);
    let mut out = format!("<rect{cls} x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"");
    append_attr(&mut out, "fill", map_utf8(map, "fill").as_deref());
    append_attr(&mut out, "stroke", map_utf8(map, "stroke").as_deref());
    append_attr(&mut out, "id", map_id(map, "id").as_deref());
    out.push_str(" />");
    out
}

fn render_circle_payload(map: &BTreeMap<String, Value>) -> String {
    let cls = class_attr_from_map(map);
    let cx = map_f64(map, "cx").unwrap_or(0.0);
    let cy = map_f64(map, "cy").unwrap_or(0.0);
    let r = map_f64(map, "r").unwrap_or(0.0);
    let mut out = format!("<circle{cls} cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"");
    append_attr(&mut out, "fill", map_utf8(map, "fill").as_deref());
    append_attr(&mut out, "stroke", map_utf8(map, "stroke").as_deref());
    append_attr(&mut out, "id", map_id(map, "id").as_deref());
    out.push_str(" />");
    out
}

fn render_line_payload(map: &BTreeMap<String, Value>) -> String {
    let cls = class_attr_from_map(map);
    let x1 = map_f64(map, "x1").unwrap_or(0.0);
    let y1 = map_f64(map, "y1").unwrap_or(0.0);
    let x2 = map_f64(map, "x2").unwrap_or(0.0);
    let y2 = map_f64(map, "y2").unwrap_or(0.0);
    let mut out = format!("<line{cls} x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"");
    append_attr(&mut out, "stroke", map_utf8(map, "stroke").as_deref());
    append_attr(&mut out, "id", map_id(map, "id").as_deref());
    out.push_str(" />");
    out
}

fn render_label_payload(map: &BTreeMap<String, Value>) -> String {
    let cls = class_attr_from_map(map);
    let content = map_utf8(map, "content").unwrap_or_default();
    let x = map_f64(map, "x").unwrap_or(0.0);
    let y = map_f64(map, "y").unwrap_or(0.0);
    let mut out = format!("<text{cls} x=\"{x}\" y=\"{y}\"");
    append_attr(&mut out, "fill", map_utf8(map, "fill").as_deref());
    append_attr(&mut out, "id", map_id(map, "id").as_deref());
    write!(out, ">{}</text>", escape_html(&content)).expect("write to String");
    out
}

fn render_polygon_payload(map: &BTreeMap<String, Value>) -> String {
    let cls = class_attr_from_map(map);
    let points = map_utf8(map, "points").unwrap_or_default();
    let mut out = format!("<polygon{cls} points=\"{}\"", escape_html(&points));
    append_attr(&mut out, "fill", map_utf8(map, "fill").as_deref());
    append_attr(&mut out, "stroke", map_utf8(map, "stroke").as_deref());
    append_attr(&mut out, "id", map_id(map, "id").as_deref());
    out.push_str(" />");
    out
}

fn render_paragraph_payload(map: &BTreeMap<String, Value>) -> String {
    let cls = class_attr_from_map(map);
    let spans = map_utf8_list(map, "spans");
    let inner: String = spans
        .iter()
        .map(|s| format!("<span>{}</span>", escape_html(s)))
        .collect();
    let mut out = format!("<p{cls}");
    append_attr(&mut out, "id", map_id(map, "id").as_deref());
    write!(out, ">{inner}</p>").expect("write to String");
    out
}

// ── Resolution helpers (block-side) ───────────────────────────────

fn resolve_rect_box(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64, f64, f64) {
    let mut x = field_f64(block, "x").unwrap_or(0.0);
    let mut y = field_f64(block, "y").unwrap_or(0.0);
    let mut w = field_f64(block, "width").unwrap_or(0.0);
    let mut h = field_f64(block, "height").unwrap_or(0.0);
    apply_axis_anchor(
        &mut x,
        &mut w,
        field_f64(block, "anchor_left"),
        field_f64(block, "anchor_right"),
        parent_w,
    );
    apply_axis_anchor(
        &mut y,
        &mut h,
        field_f64(block, "anchor_top"),
        field_f64(block, "anchor_bottom"),
        parent_h,
    );
    (x, y, w, h)
}

fn resolve_container_box(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64, f64, f64) {
    let mut x = 0.0;
    let mut y = 0.0;
    let mut w = field_f64(block, "width").unwrap_or(parent_w);
    let mut h = field_f64(block, "height").unwrap_or(parent_h);
    apply_axis_anchor(
        &mut x,
        &mut w,
        field_f64(block, "anchor_left"),
        field_f64(block, "anchor_right"),
        parent_w,
    );
    apply_axis_anchor(
        &mut y,
        &mut h,
        field_f64(block, "anchor_top"),
        field_f64(block, "anchor_bottom"),
        parent_h,
    );
    (x, y, w, h)
}

fn apply_axis_anchor(
    pos: &mut f64,
    size: &mut f64,
    near: Option<f64>,
    far: Option<f64>,
    parent: f64,
) {
    match (near, far) {
        (Some(n), Some(f)) => {
            *pos = n;
            *size = parent - n - f;
        }
        (Some(n), None) => {
            *pos = n;
        }
        (None, Some(f)) => {
            *pos = parent - f - *size;
        }
        (None, None) => {}
    }
}

fn resolve_circle(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64, f64) {
    let cx = field_f64(block, "cx").unwrap_or(0.0);
    let cy = field_f64(block, "cy").unwrap_or(0.0);
    let r = field_f64(block, "r").unwrap_or(0.0);
    let al = field_f64(block, "anchor_left");
    let ar = field_f64(block, "anchor_right");
    let at = field_f64(block, "anchor_top");
    let ab = field_f64(block, "anchor_bottom");
    if al.is_none() && ar.is_none() && at.is_none() && ab.is_none() {
        return (cx, cy, r);
    }
    let mut bx = cx - r;
    let mut bw = 2.0 * r;
    let mut by = cy - r;
    let mut bh = 2.0 * r;
    apply_axis_anchor(&mut bx, &mut bw, al, ar, parent_w);
    apply_axis_anchor(&mut by, &mut bh, at, ab, parent_h);
    let new_r = (bw.min(bh) / 2.0).max(0.0);
    (bx + bw / 2.0, by + bh / 2.0, new_r)
}

fn resolve_point_anchor(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64) {
    let dx = match (
        field_f64(block, "anchor_left"),
        field_f64(block, "anchor_right"),
    ) {
        (Some(l), _) => l,
        (None, Some(r)) => parent_w - r,
        _ => 0.0,
    };
    let dy = match (
        field_f64(block, "anchor_top"),
        field_f64(block, "anchor_bottom"),
    ) {
        (Some(t), _) => t,
        (None, Some(b)) => parent_h - b,
        _ => 0.0,
    };
    (dx, dy)
}

fn resolve_point_anchored(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
    own_x: f64,
    own_y: f64,
) -> (f64, f64) {
    let x = match (
        field_f64(block, "anchor_left"),
        field_f64(block, "anchor_right"),
    ) {
        (Some(l), _) => l,
        (None, Some(r)) => parent_w - r,
        _ => own_x,
    };
    let y = match (
        field_f64(block, "anchor_top"),
        field_f64(block, "anchor_bottom"),
    ) {
        (Some(t), _) => t,
        (None, Some(b)) => parent_h - b,
        _ => own_y,
    };
    (x, y)
}

// ── Block-side accessors ──────────────────────────────────────────

fn class_attr(block: &Block<'_>) -> String {
    let names = field_utf8_list(block, "class");
    classes_attr_from_names(&names)
}

fn append_attr(out: &mut String, name: &str, value: Option<&str>) {
    if let Some(v) = value {
        write!(out, " {name}=\"{}\"", escape_html(v)).expect("write to String");
    }
}

fn label_string(block: &Block<'_>) -> Option<String> {
    let labels = block.labels().ok()?;
    value_as_string(labels.into_iter().next()?)
}

fn value_as_string(v: Value) -> Option<String> {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => Some(s),
        other => Some(other.to_string()),
    }
}

fn field_utf8(block: &Block<'_>, name: &str) -> Option<String> {
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

fn field_bool(block: &Block<'_>, name: &str) -> Option<bool> {
    let field = block.field(name)?;
    match field.value().ok()? {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

fn field_symbol(block: &Block<'_>, name: &str) -> Option<String> {
    let field = block.field(name)?;
    match field.value().ok()? {
        Value::Symbol(s) => Some(s.clone()),
        _ => None,
    }
}

fn field_f64(block: &Block<'_>, name: &str) -> Option<f64> {
    let field = block.field(name)?;
    value_as_f64(field.value().ok()?)
}

fn field_i64(block: &Block<'_>, name: &str) -> Option<i64> {
    let field = block.field(name)?;
    value_as_i64(field.value().ok()?)
}

fn field_f64_list(block: &Block<'_>, name: &str) -> Vec<f64> {
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

fn field_utf8_list(block: &Block<'_>, name: &str) -> Vec<String> {
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

// ── Map-side accessors (for variant payloads) ─────────────────────

fn class_attr_from_map(map: &BTreeMap<String, Value>) -> String {
    let names = map_utf8_list(map, "class");
    classes_attr_from_names(&names)
}

fn classes_attr_from_names(names: &[String]) -> String {
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

fn map_utf8(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    match map.get(name)? {
        Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

fn map_id(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    match map.get(name)? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

fn map_f64(map: &BTreeMap<String, Value>, name: &str) -> Option<f64> {
    value_as_f64(map.get(name)?)
}

fn map_utf8_list(map: &BTreeMap<String, Value>, name: &str) -> Vec<String> {
    let Some(Value::List(items)) = map.get(name) else {
        return Vec::new();
    };
    items.iter().filter_map(value_as_str).collect()
}

// ── Value-coercion helpers ────────────────────────────────────────

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::F64(n) => Some(*n),
        Value::F32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        _ => None,
    }
}

fn value_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::I64(n) => Some(*n),
        Value::I32(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        Value::U64(n) => Some(*n as i64),
        _ => None,
    }
}

fn value_as_str(v: &Value) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::{escape_html, kind_for_variant, kind_to_typename};

    #[test]
    fn escapes_html_specials() {
        assert_eq!(
            escape_html("<a href=\"x\">hi & 'bye'</a>"),
            "&lt;a href=&quot;x&quot;&gt;hi &amp; &#39;bye&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn kind_to_typename_capitalises() {
        assert_eq!(kind_to_typename("process"), "Process");
        assert_eq!(kind_to_typename("decision"), "Decision");
        assert_eq!(kind_to_typename("h1"), "H1");
    }

    #[test]
    fn kind_for_variant_lowercases() {
        assert_eq!(kind_for_variant("Process"), "process");
        assert_eq!(kind_for_variant("Paragraph"), "paragraph");
        assert_eq!(kind_for_variant("Rect"), "rect");
    }
}
