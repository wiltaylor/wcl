use std::fmt::Write as _;

use wcl_lang::{Block, Value};

pub(crate) fn render_page(name: &str, blocks: impl Iterator<Item = String>) -> String {
    let mut body = String::new();
    for b in blocks {
        body.push_str(&b);
        body.push('\n');
    }
    format!(
        "<!DOCTYPE html>\n\
         <html>\n\
         <head><meta charset=\"utf-8\"><title>{title}</title></head>\n\
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

fn render_text(block: &Block<'_>) -> String {
    let spans: String = block
        .blocks()
        .filter(|b| b.kind() == "span")
        .map(|b| render_span(&b))
        .collect();
    format!("<p>{spans}</p>")
}

fn render_span(block: &Block<'_>) -> String {
    let text = label_string(block).unwrap_or_default();
    format!("<span>{}</span>", escape_html(&text))
}

fn render_column(block: &Block<'_>) -> String {
    let widths = field_f64_list(block, "widths");
    let grid_cols: String = widths
        .iter()
        .map(|w| format!("{w}%"))
        .collect::<Vec<_>>()
        .join(" ");
    let children: String = block.blocks().filter_map(|b| render_block(&b)).collect();
    format!("<div style=\"display:grid;grid-template-columns:{grid_cols};\">{children}</div>")
}

fn render_diagram(block: &Block<'_>) -> String {
    let width = field_i64(block, "width").unwrap_or(0);
    let height = field_i64(block, "height").unwrap_or(0);
    let shapes: String = block.blocks().filter_map(|b| render_shape(&b)).collect();
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\">{shapes}</svg>"
    )
}

fn render_shape(block: &Block<'_>) -> Option<String> {
    match block.kind() {
        "rect" => Some(render_rect(block)),
        "circle" => Some(render_circle(block)),
        "line" => Some(render_line(block)),
        "label" => Some(render_label(block)),
        "polygon" => Some(render_polygon(block)),
        _ => None,
    }
}

fn render_rect(block: &Block<'_>) -> String {
    let x = field_f64(block, "x").unwrap_or(0.0);
    let y = field_f64(block, "y").unwrap_or(0.0);
    let w = field_f64(block, "width").unwrap_or(0.0);
    let h = field_f64(block, "height").unwrap_or(0.0);
    let mut out = format!("<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"");
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
    out.push_str(" />");
    out
}

fn render_circle(block: &Block<'_>) -> String {
    let cx = field_f64(block, "cx").unwrap_or(0.0);
    let cy = field_f64(block, "cy").unwrap_or(0.0);
    let r = field_f64(block, "r").unwrap_or(0.0);
    let mut out = format!("<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"");
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
    out.push_str(" />");
    out
}

fn render_line(block: &Block<'_>) -> String {
    let x1 = field_f64(block, "x1").unwrap_or(0.0);
    let y1 = field_f64(block, "y1").unwrap_or(0.0);
    let x2 = field_f64(block, "x2").unwrap_or(0.0);
    let y2 = field_f64(block, "y2").unwrap_or(0.0);
    let mut out = format!("<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"");
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
    out.push_str(" />");
    out
}

fn render_label(block: &Block<'_>) -> String {
    let content = label_string(block).unwrap_or_default();
    let x = field_f64(block, "x").unwrap_or(0.0);
    let y = field_f64(block, "y").unwrap_or(0.0);
    let mut out = format!("<text x=\"{x}\" y=\"{y}\"");
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    write!(out, ">{}</text>", escape_html(&content)).expect("write to String");
    out
}

fn render_polygon(block: &Block<'_>) -> String {
    let points = field_utf8(block, "points").unwrap_or_default();
    let mut out = format!("<polygon points=\"{}\"", escape_html(&points));
    append_attr(&mut out, "fill", field_utf8(block, "fill").as_deref());
    append_attr(&mut out, "stroke", field_utf8(block, "stroke").as_deref());
    out.push_str(" />");
    out
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
