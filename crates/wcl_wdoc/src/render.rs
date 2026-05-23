use std::fmt::Write as _;

use wcl_lang::{Block, Value};

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

pub(crate) fn render_block(block: &Block<'_>) -> Option<String> {
    match block.kind() {
        "text" => Some(render_text(block)),
        "column" => Some(render_column(block)),
        "diagram" => Some(render_diagram(block)),
        _ => None,
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

fn render_text(block: &Block<'_>) -> String {
    let cls = class_attr(block);
    let spans: String = block
        .blocks()
        .filter(|b| b.kind() == "span")
        .map(|b| render_span(&b))
        .collect();
    format!("<p{cls}>{spans}</p>")
}

fn render_span(block: &Block<'_>) -> String {
    let cls = class_attr(block);
    let text = label_string(block).unwrap_or_default();
    format!("<span{cls}>{}</span>", escape_html(&text))
}

fn render_column(block: &Block<'_>) -> String {
    let cls = class_attr(block);
    let widths = field_f64_list(block, "widths");
    let grid_cols: String = widths
        .iter()
        .map(|w| format!("{w}%"))
        .collect::<Vec<_>>()
        .join(" ");
    let children: String = block.blocks().filter_map(|b| render_block(&b)).collect();
    format!("<div{cls} style=\"display:grid;grid-template-columns:{grid_cols};\">{children}</div>")
}

fn render_diagram(block: &Block<'_>) -> String {
    let cls = class_attr(block);
    let width = field_i64(block, "width").unwrap_or(0);
    let height = field_i64(block, "height").unwrap_or(0);
    let shapes: String = block
        .blocks()
        .filter_map(|b| render_shape(&b, width as f64, height as f64))
        .collect();
    format!(
        "<svg{cls} xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\">{shapes}</svg>"
    )
}

fn render_shape(block: &Block<'_>, parent_w: f64, parent_h: f64) -> Option<String> {
    match block.kind() {
        "rect" => Some(render_rect(block, parent_w, parent_h)),
        "circle" => Some(render_circle(block, parent_w, parent_h)),
        "line" => Some(render_line(block, parent_w, parent_h)),
        "label" => Some(render_label(block, parent_w, parent_h)),
        "polygon" => Some(render_polygon(block, parent_w, parent_h)),
        "container" => Some(render_container(block, parent_w, parent_h)),
        _ => None,
    }
}

fn render_container(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let cls = class_attr(block);
    let (x, y, w, h) = resolve_container_box(block, parent_w, parent_h);

    let layout = field_symbol(block, "layout").unwrap_or_default();
    let inner = match layout.as_str() {
        "grid" => render_grid_children(block),
        _ => block
            .blocks()
            .filter_map(|b| render_shape(&b, w, h))
            .collect::<String>(),
    };

    let transform = if x != 0.0 || y != 0.0 {
        format!(" transform=\"translate({x} {y})\"")
    } else {
        String::new()
    };
    format!("<g{cls}{transform}>{inner}</g>")
}

fn render_grid_children(block: &Block<'_>) -> String {
    let cols = field_i64(block, "columns").unwrap_or(1).max(1) as usize;
    let cw = field_f64(block, "cell_width").unwrap_or(0.0);
    let ch = field_f64(block, "cell_height").unwrap_or(0.0);
    let gap = field_f64(block, "gap").unwrap_or(0.0);
    block
        .blocks()
        .enumerate()
        .filter_map(|(i, b)| {
            let rendered = render_shape(&b, cw, ch)?;
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

fn render_rect(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let cls = class_attr(block);
    let (x, y, w, h) = resolve_rect_box(block, parent_w, parent_h);
    let mut out = format!("<rect{cls} x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"");
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
    out.push_str(" />");
    out
}

fn render_circle(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let cls = class_attr(block);
    let (cx, cy, r) = resolve_circle(block, parent_w, parent_h);
    let mut out = format!("<circle{cls} cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"");
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
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
    out.push_str(" />");
    out
}

/// Resolve a `(x, y, width, height)` box for shapes that have native
/// width/height fields. Per-axis: opposite anchors pin + stretch, a
/// single anchor pins (preserving the authored size), missing anchors
/// leave the authored value alone.
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

/// Same model as `resolve_rect_box` but for `container`, which uses
/// declared `width`/`height` as the intrinsic interior size.
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

/// Circle resolution. When any anchor is set on either axis, derive
/// a bounding box from anchors + the shape's own radius, then center
/// + shrink to fit.
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

/// Translation-only anchor — used by `line` / `polygon`, which don't
/// have natural size fields. Returns the `(dx, dy)` offset to add to
/// the shape's authored coordinates.
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

/// Anchor resolution for a single `(x, y)` point — used by `label`.
/// If a near anchor is set, it overrides the authored coordinate.
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

fn class_attr(block: &Block<'_>) -> String {
    let names = field_utf8_list(block, "class");
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
    match field.value().ok()? {
        Value::F64(n) => Some(*n),
        Value::F32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        _ => None,
    }
}

fn field_i64(block: &Block<'_>, name: &str) -> Option<i64> {
    let field = block.field(name)?;
    match field.value().ok()? {
        Value::I64(n) => Some(*n),
        Value::I32(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        Value::U64(n) => Some(*n as i64),
        _ => None,
    }
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
    items
        .iter()
        .filter_map(|v| match v {
            Value::F64(n) => Some(*n),
            Value::F32(n) => Some(*n as f64),
            Value::I64(n) => Some(*n as f64),
            Value::I32(n) => Some(*n as f64),
            _ => None,
        })
        .collect()
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
    items
        .iter()
        .filter_map(|v| match v {
            Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => {
                Some(s.clone())
            }
            _ => None,
        })
        .collect()
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
    use super::escape_html;

    #[test]
    fn escapes_html_specials() {
        assert_eq!(
            escape_html("<a href=\"x\">hi & 'bye'</a>"),
            "&lt;a href=&quot;x&quot;&gt;hi &amp; &#39;bye&#39;&lt;/a&gt;"
        );
    }
}
