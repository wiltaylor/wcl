use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use wcl_lang::{Block, Document, FnValue, Value, VariantPayload};

use crate::highlight;
use crate::inline::InlinePatterns;
use crate::layered::{self, Direction};
use crate::routing::{self, EdgePath, Obstacle, Side};
use crate::text;

/// Per-shape geometry used by the edge pass: the absolute bounding
/// box (in diagram coords, with all container / grid translates
/// already baked in) plus the resolved list of edge-anchor points
/// the shape exposes. When the source block declares no
/// `connect_points`, the anchors default to the midpoint of each
/// bounding-box side. Each anchor records the `Side` it lives on
/// so the elbow router knows which direction the first / last leg
/// must travel.
#[derive(Clone)]
struct ShapeMetrics {
    bbox: (f64, f64, f64, f64),
    anchors: Vec<(Side, f64, f64)>,
}
type ShapePositions = HashMap<String, ShapeMetrics>;

/// Accumulator passed through the position-collection walk. The
/// `positions` map only stores id'd shapes (it's keyed by id, for
/// the edge pass to resolve `lhs -> rhs`). The `bboxes` Vec stores
/// every shape's absolute bbox regardless of id — used by the
/// fit-to-viewport pass to compute the SVG `viewBox` so content
/// always fills its declared `width` × `height`.
#[derive(Default)]
struct Collector {
    positions: ShapePositions,
    bboxes: Vec<(f64, f64, f64, f64)>,
}

/// Lowering recursion guard. A lowering may emit other custom kinds
/// that themselves lower further; this caps how deep we'll follow
/// before bailing.
const MAX_LOWER_DEPTH: usize = 32;

/// Default styling for `table` blocks, injected into every page's
/// `<style>` (before user `class` rules, so those still override it).
/// Keeps tables legible out of the box without forcing every author
/// to declare a border class.
pub(crate) const TABLE_CSS: &str = "\
table.wdoc-table { border-collapse: collapse; }
.wdoc-table th, .wdoc-table td { border: 1px solid #ccc; padding: 0.3rem 0.6rem; text-align: left; }
.wdoc-table th { background: #f4f4f4; }";

/// Default styling for the bundled `webpage` template's regions.
/// Injected like `TABLE_CSS`; user `class` rules can override it.
pub(crate) const SITE_CSS: &str = "\
.site-header { font-weight: bold; font-size: 1.4rem; padding: 0.5rem 0; }
.site-nav { display: flex; gap: 1rem; padding: 0.5rem 0; border-bottom: 1px solid #ccc; margin-bottom: 1rem; }
.site-nav a { text-decoration: none; }
.site-main { display: block; }";

/// Wrap a page's `body` HTML in the document shell. The `<head>`
/// (title + global stylesheet) is owned here regardless of template;
/// templates control the `<body>` contents via `render_template`.
pub(crate) fn render_page(name: &str, css: &str, body: &str) -> String {
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

/// Find a `@block("template")` instance by its inline name.
pub(crate) fn find_template<'a>(doc: &'a Document, name: &str) -> Option<Block<'a>> {
    doc.blocks()
        .find(|b| b.kind() == "template" && label_string(b).as_deref() == Some(name))
}

/// Render a page through `template`'s `render` function. Builds a
/// `TemplateCtx` record (content + title + page_name + pages) and
/// invokes the WCL function, then renders the returned fundamentals.
/// Best-effort: a missing/failed `render` yields an empty body, like
/// the rest of the lowering pipeline.
pub(crate) fn render_template(
    doc: &Document,
    template: &Block<'_>,
    content: &str,
    title: &str,
    page_name: &str,
    pages: &[(String, String)],
) -> String {
    let Some(field) = template.field("render") else {
        return String::new();
    };
    let Ok(Value::Function(fv)) = field.value() else {
        return String::new();
    };
    let fv = fv.clone();
    let pages_val = Value::List(
        pages
            .iter()
            .map(|(n, h)| {
                let mut m = BTreeMap::new();
                m.insert("name".to_string(), Value::Utf8(n.clone()));
                m.insert("href".to_string(), Value::Utf8(h.clone()));
                Value::Record {
                    ty: vec!["PageRef".to_string()],
                    fields: m,
                }
            })
            .collect(),
    );
    let mut ctx = BTreeMap::new();
    ctx.insert("content".to_string(), Value::Utf8(content.to_string()));
    ctx.insert("title".to_string(), Value::Utf8(title.to_string()));
    ctx.insert("page_name".to_string(), Value::Utf8(page_name.to_string()));
    ctx.insert("pages".to_string(), pages_val);
    let arg = Value::Record {
        ty: vec!["TemplateCtx".to_string()],
        fields: ctx,
    };
    let Ok(Value::List(items)) = doc.call_value(&fv, &[arg]) else {
        return String::new();
    };
    items
        .iter()
        .map(|v| render_html_variant(doc, v, 0))
        .collect()
}

pub(crate) fn render_block(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
) -> Option<String> {
    match block.kind() {
        "text" => Some(render_text(doc, block, patterns)),
        "column" => Some(render_column(doc, block, patterns)),
        "table" => Some(render_table(doc, block, patterns)),
        "diagram" => Some(render_diagram(doc, block)),
        "code" => Some(render_code(block)),
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

fn render_text(doc: &Document, block: &Block<'_>, patterns: &InlinePatterns) -> String {
    let cls = class_attr(block);
    let spans: String = block
        .blocks()
        .filter(|b| b.kind() == "span")
        .map(|b| render_span(doc, &b, patterns))
        .collect();
    let mut out = format!("<p{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, ">{spans}</p>").expect("write to String");
    out
}

fn render_span(doc: &Document, block: &Block<'_>, patterns: &InlinePatterns) -> String {
    let cls = class_attr(block);
    let text = label_string(block).unwrap_or_default();
    let mut out = format!("<span{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, ">{}</span>", patterns.render(doc, &text)).expect("write to String");
    out
}

fn render_column(doc: &Document, block: &Block<'_>, patterns: &InlinePatterns) -> String {
    let cls = class_attr(block);
    let widths = field_f64_list(block, "widths");
    let grid_cols: String = widths
        .iter()
        .map(|w| format!("{w}%"))
        .collect::<Vec<_>>()
        .join(" ");
    let children: String = block
        .blocks()
        .filter_map(|b| render_block(doc, &b, patterns))
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
    let (vw, vh) = (width as f64, height as f64);
    let mut collector = Collector::default();
    collect_layout_children(block, 0.0, 0.0, vw, vh, &mut collector);
    let shapes: String = render_layout_children(doc, block, vw, vh);
    let (edges, edge_bboxes) = render_edges(block, &collector.positions, (vw, vh));
    let viewbox = fit_viewbox(&collector.bboxes, &edge_bboxes, vw, vh);
    let defs = if edges.is_empty() { "" } else { ARROW_MARKER };
    let mut out = format!("<svg{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(
        out,
        " xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"{viewbox}\">{defs}{shapes}{edges}</svg>"
    )
    .expect("write to String");
    out
}

/// Render a `@block("code")` instance to a `<pre><code>` element
/// with syntect-produced `<span class="tok-…">` tokens inside. The
/// `code-block` class is always present so the bundled theme CSS
/// can style the container; user-declared `class` entries are
/// appended after it.
fn render_code(block: &Block<'_>) -> String {
    // `language` is declared `@inline(0)` on @block("code"), so it
    // arrives as the block's label rather than a named field.
    let language = label_string(block).unwrap_or_default();
    let source = field_utf8(block, "source").unwrap_or_default();
    let mut classes: Vec<String> = vec!["code-block".to_string()];
    classes.extend(field_utf8_list(block, "class"));
    let cls = classes_attr_from_names(&classes);
    let inner = highlight::highlight_html(&source, &language);
    let mut out = format!("<pre{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(
        out,
        "><code class=\"language-{}\">{inner}</code></pre>",
        escape_html(&language),
    )
    .expect("write to String");
    out
}

/// Render a `@block("table")` instance. Rows are authored with WCL's
/// pipe-table syntax (`rows: | a | b |`) and read here via
/// `Block::tables()` — a `table` declares no typed `rows` field, so
/// the rows are arbitrary-width and never schema-validated. The first
/// row is the header (`<th>` inside `<thead>`); the rest are body
/// rows (`<td>` inside `<tbody>`). Each cell is rendered by
/// `cell_to_html`, so utf8 cells pick up inline patterns.
fn render_table(doc: &Document, block: &Block<'_>, patterns: &InlinePatterns) -> String {
    // Collect every pipe-row in source order. A `table` normally holds
    // a single `rows:` table; if several are present we concatenate.
    let mut rows: Vec<Vec<String>> = Vec::new();
    for table in block.tables() {
        for row in table.rows() {
            let Ok(values) = row.values() else {
                // A row whose cells fail to evaluate is skipped rather
                // than aborting the whole table.
                continue;
            };
            rows.push(
                values
                    .iter()
                    .map(|v| cell_to_html(doc, patterns, v))
                    .collect(),
            );
        }
    }
    if rows.is_empty() {
        return String::new();
    }
    let mut classes: Vec<String> = vec!["wdoc-table".to_string()];
    classes.extend(field_utf8_list(block, "class"));
    let header = &rows[0];
    let body = &rows[1..];
    table_html(field_id(block, "id").as_deref(), &classes, header, body)
}

/// Render a single table cell to inner HTML. utf8 cells flow through
/// the inline-pattern engine (bold / italic / code / links); every
/// other value kind is stringified via `Value`'s `Display` and
/// HTML-escaped.
fn cell_to_html(doc: &Document, patterns: &InlinePatterns, value: &Value) -> String {
    match value {
        Value::Utf8(s) | Value::Ascii(s) => patterns.render(doc, s),
        other => escape_html(&other.to_string()),
    }
}

/// Shared `<table>` builder. `header` and `body` cells are already
/// rendered inner HTML (not escaped again here). An empty `header`
/// omits the `<thead>` entirely so the lowering path can emit a
/// header-less table.
fn table_html(
    id: Option<&str>,
    classes: &[String],
    header: &[String],
    body: &[Vec<String>],
) -> String {
    let cls = classes_attr_from_names(classes);
    let mut out = format!("<table{cls}");
    append_attr(&mut out, "id", id);
    out.push('>');
    if !header.is_empty() {
        out.push_str("<thead><tr>");
        for cell in header {
            write!(out, "<th>{cell}</th>").expect("write to String");
        }
        out.push_str("</tr></thead>");
    }
    if !body.is_empty() {
        out.push_str("<tbody>");
        for row in body {
            out.push_str("<tr>");
            for cell in row {
                write!(out, "<td>{cell}</td>").expect("write to String");
            }
            out.push_str("</tr>");
        }
        out.push_str("</tbody>");
    }
    out.push_str("</table>");
    out
}

/// Compute the SVG viewBox that wraps every rendered shape and
/// polyline. With default `preserveAspectRatio`, this scales
/// content to fit the declared `width` × `height` while preserving
/// aspect ratio. Empty diagrams fall back to `0 0 W H`.
fn fit_viewbox(
    shape_bboxes: &[(f64, f64, f64, f64)],
    edge_bboxes: &[(f64, f64, f64, f64)],
    width: f64,
    height: f64,
) -> String {
    const PAD: f64 = 10.0;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &(x, y, w, h) in shape_bboxes.iter().chain(edge_bboxes.iter()) {
        if x < min_x {
            min_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if x + w > max_x {
            max_x = x + w;
        }
        if y + h > max_y {
            max_y = y + h;
        }
    }
    if !min_x.is_finite() {
        return format!("0 0 {width} {height}");
    }
    let bx = min_x - PAD;
    let by = min_y - PAD;
    let bw = (max_x - min_x).max(0.0) + 2.0 * PAD;
    let bh = (max_y - min_y).max(0.0) + 2.0 * PAD;
    format!("{bx} {by} {bw} {bh}")
}

/// Iterate `block`'s children under the layout specified by the
/// block's `layout` field, recording each id'd shape's absolute
/// bbox + anchors in `out.positions` and every shape's bbox in
/// `out.bboxes`. Used for both Diagram and Container.
fn collect_layout_children(
    block: &Block<'_>,
    tx: f64,
    ty: f64,
    parent_w: f64,
    parent_h: f64,
    out: &mut Collector,
) {
    let layout = field_symbol(block, "layout").unwrap_or_default();
    match layout.as_str() {
        "grid" => collect_grid_children(block, tx, ty, out),
        "layered" => collect_layered_children(block, tx, ty, out),
        _ => {
            for child in block.blocks() {
                collect_shape_positions(&child, tx, ty, parent_w, parent_h, out);
            }
        }
    }
}

/// Symmetric: render `block`'s children using its `layout`. Caller
/// supplies the parent box (`pw`, `ph`) that propagates to children
/// not assigned an explicit cell.
fn render_layout_children(doc: &Document, block: &Block<'_>, pw: f64, ph: f64) -> String {
    let layout = field_symbol(block, "layout").unwrap_or_default();
    match layout.as_str() {
        "grid" => render_grid_children(doc, block),
        "layered" => render_layered_children(doc, block),
        _ => block
            .blocks()
            .filter_map(|b| render_shape(doc, &b, pw, ph))
            .collect(),
    }
}

fn collect_grid_children(block: &Block<'_>, tx: f64, ty: f64, out: &mut Collector) {
    let cols = field_i64(block, "columns").unwrap_or(1).max(1) as usize;
    let cw = field_f64(block, "cell_width").unwrap_or(0.0);
    let ch = field_f64(block, "cell_height").unwrap_or(0.0);
    let gap = field_f64(block, "gap").unwrap_or(0.0);
    for (i, child) in block.blocks().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let cx_off = col as f64 * (cw + gap);
        let cy_off = row as f64 * (ch + gap);
        collect_shape_positions(&child, tx + cx_off, ty + cy_off, cw, ch, out);
    }
}

fn collect_layered_children(block: &Block<'_>, tx: f64, ty: f64, out: &mut Collector) {
    let children: Vec<Block<'_>> = block.blocks().collect();
    let (offsets, _, _) = compute_layered_plan(block, &children);
    for (child, (cx, cy)) in children.iter().zip(offsets) {
        let pw = field_f64(child, "width").unwrap_or(80.0);
        let ph = field_f64(child, "height").unwrap_or(40.0);
        collect_shape_positions(child, tx + cx, ty + cy, pw, ph, out);
    }
}

/// Compute the layered layout for a container/diagram's children
/// and return (per-child offsets, per-child width, per-child height).
/// The width/height returned reflect the size the renderer should
/// pass downward as `parent_w` / `parent_h`.
fn compute_layered_plan(
    block: &Block<'_>,
    children: &[Block<'_>],
) -> (Vec<(f64, f64)>, Vec<f64>, Vec<f64>) {
    let direction = field_symbol(block, "direction")
        .and_then(|s| Direction::from_symbol(&s))
        .unwrap_or(Direction::TopToBottom);
    let layer_gap = field_f64(block, "layer_gap").unwrap_or(40.0);
    let node_gap = field_f64(block, "node_gap").unwrap_or(40.0);

    let nodes: Vec<layered::Node> = children
        .iter()
        .map(|c| layered::Node {
            id: field_id(c, "id"),
            // Use effective_dims so multi-line text grows the cell
            // the layered solver allocates for the shape.
            size: effective_dims(c),
        })
        .collect();
    let edges: Vec<(String, String)> = edge_id_pairs(block);
    let offsets = layered::assign_layered_offsets(&nodes, &edges, direction, layer_gap, node_gap);
    let widths: Vec<f64> = nodes.iter().map(|n| n.size.0).collect();
    let heights: Vec<f64> = nodes.iter().map(|n| n.size.1).collect();
    (offsets, widths, heights)
}

fn edge_id_pairs(block: &Block<'_>) -> Vec<(String, String)> {
    let Some(dr) = block.typed_field("edges") else {
        return Vec::new();
    };
    let Ok(value) = dr.value() else {
        return Vec::new();
    };
    let Value::List(items) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|v| {
            let Value::Record { fields, .. } = v else {
                return None;
            };
            let s = edge_endpoint_id(fields.get("source")?)?;
            let d = edge_endpoint_id(fields.get("destination")?)?;
            Some((s, d))
        })
        .collect()
}

fn render_layered_children(doc: &Document, block: &Block<'_>) -> String {
    let children: Vec<Block<'_>> = block.blocks().collect();
    let (offsets, widths, heights) = compute_layered_plan(block, &children);
    let mut out = String::new();
    for ((child, (tx, ty)), (cw, ch)) in children
        .iter()
        .zip(offsets)
        .zip(widths.iter().zip(heights.iter()))
    {
        if let Some(rendered) = render_shape(doc, child, *cw, *ch) {
            out.push_str(&format!(
                "<g transform=\"translate({tx} {ty})\">{rendered}</g>"
            ));
        }
    }
    out
}

const ARROW_MARKER: &str = "<defs><marker id=\"wdoc-arrow\" viewBox=\"0 0 10 10\" \
    refX=\"10\" refY=\"5\" markerWidth=\"8\" markerHeight=\"8\" \
    orient=\"auto-start-reverse\">\
    <path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"currentColor\" /></marker></defs>";

/// Walk the diagram subtree once and record each id'd shape's
/// absolute bounding box + resolved anchor positions, plus every
/// shape's bbox (id or not) for the fit-to-viewport pass. Mirrors
/// the geometry computed by `render_*` but without producing SVG.
fn collect_shape_positions(
    block: &Block<'_>,
    tx: f64,
    ty: f64,
    parent_w: f64,
    parent_h: f64,
    out: &mut Collector,
) {
    let record = |block: &Block<'_>, bbox: (f64, f64, f64, f64), out: &mut Collector| {
        out.bboxes.push(bbox);
        if let Some(id) = field_id(block, "id") {
            out.positions.insert(id, build_metrics(block, bbox));
        }
    };
    match block.kind() {
        "rect" => {
            let (x, y, w, h) = resolve_rect_box(block, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
        }
        "circle" => {
            let (cx, cy, r) = resolve_circle(block, parent_w, parent_h);
            record(block, (tx + cx - r, ty + cy - r, 2.0 * r, 2.0 * r), out);
        }
        "label" => {
            let own_x = field_f64(block, "x").unwrap_or(0.0);
            let own_y = field_f64(block, "y").unwrap_or(0.0);
            let (x, y) = resolve_point_anchored(block, parent_w, parent_h, own_x, own_y);
            record(block, (tx + x, ty + y, 0.0, 0.0), out);
        }
        "container" => {
            let (x, y, w, h) = resolve_container_box(block, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
            // Children render inside the container's padded inner
            // region — mirror that translate here so the router's
            // obstacle bboxes and the rendered SVG agree.
            let p = container_padding(block);
            let inner_w = (w - 2.0 * p).max(0.0);
            let inner_h = (h - 2.0 * p).max(0.0);
            collect_layout_children(block, tx + x + p, ty + y + p, inner_w, inner_h, out);
        }
        // Lines and polygons aren't usable as edge endpoints (they
        // have no single anchor point) but their bboxes still
        // matter for the fit-to-viewport pass — push them straight
        // into `out.bboxes` without going through `record`.
        "line" => {
            let x1 = field_f64(block, "x1").unwrap_or(0.0);
            let y1 = field_f64(block, "y1").unwrap_or(0.0);
            let x2 = field_f64(block, "x2").unwrap_or(0.0);
            let y2 = field_f64(block, "y2").unwrap_or(0.0);
            let (ox, oy) = resolve_point_anchor(block, parent_w, parent_h);
            let (min_x, max_x) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
            let (min_y, max_y) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
            out.bboxes.push((
                tx + min_x + ox,
                ty + min_y + oy,
                max_x - min_x,
                max_y - min_y,
            ));
        }
        "polygon" => {
            if let Some(bbox) = polygon_bbox(block) {
                let (ox, oy) = resolve_point_anchor(block, parent_w, parent_h);
                out.bboxes
                    .push((tx + bbox.0 + ox, ty + bbox.1 + oy, bbox.2, bbox.3));
            }
        }
        // Custom shapes (process, decision, terminator, user-defined,
        // …) follow the same convention as fundamentals: x/y/width/
        // height as a bounding box. We read those directly without
        // lowering so connection endpoints land on the visible
        // outline rather than on an arbitrary post-lower coordinate.
        // `effective_dims` grows width/height when the block's
        // `text` field has multiple lines so the bbox reflects what
        // will actually render.
        _ => {
            if let (Some(x), Some(y)) = (field_f64(block, "x"), field_f64(block, "y")) {
                let (w, h) = effective_dims(block);
                record(block, (tx + x, ty + y, w, h), out);
            }
        }
    }
}

/// Compute the effective bbox dimensions of a text-bearing shape.
/// Falls back to the declared `width` / `height` when there's no
/// text field, or returns `max(declared, needed)` so a multi-line
/// label grows the shape rather than overflowing it.
fn effective_dims(block: &Block<'_>) -> (f64, f64) {
    let declared_w = field_f64(block, "width").unwrap_or(80.0);
    let declared_h = field_f64(block, "height").unwrap_or(40.0);
    let Some(text) = text_content(block) else {
        return (declared_w, declared_h);
    };
    let (need_w, need_h) = text::min_shape_dims(&text);
    (declared_w.max(need_w), declared_h.max(need_h))
}

/// Pull a shape's text content for sizing purposes. Looks for a
/// literal `text` or `content` field first, then falls back to the
/// inline label slot — `@inline(0) text: utf8` on Process /
/// Decision / Terminator lives in `block.labels()`, not in a
/// literal field.
fn text_content(block: &Block<'_>) -> Option<String> {
    if let Some(s) = field_utf8(block, "text").or_else(|| field_utf8(block, "content")) {
        return Some(s);
    }
    label_string(block)
}

/// Parse a polygon's `points` field ("x1,y1 x2,y2 …") into a
/// bounding box relative to the block's local origin. Returns
/// `None` when the string is empty or malformed.
fn polygon_bbox(block: &Block<'_>) -> Option<(f64, f64, f64, f64)> {
    let points = field_utf8(block, "points")?;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for pair in points.split_whitespace() {
        let (xs, ys) = pair.split_once(',')?;
        let x: f64 = xs.parse().ok()?;
        let y: f64 = ys.parse().ok()?;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if !min_x.is_finite() {
        return None;
    }
    Some((min_x, min_y, max_x - min_x, max_y - min_y))
}

/// Build the `ShapeMetrics` for a shape given its absolute bounding
/// box. Reads `connect_points` off the block; if the field is absent
/// (or evaluates to `none`), defaults to all four side midpoints.
/// An explicit `connect_points = []` empties the anchor list — the
/// edge pass then falls back to the bbox center.
fn build_metrics(block: &Block<'_>, bbox: (f64, f64, f64, f64)) -> ShapeMetrics {
    let sides = match field_symbol_list_opt(block, "connect_points") {
        Some(list) => list,
        None => vec!["north".into(), "east".into(), "south".into(), "west".into()],
    };
    let anchors = sides
        .iter()
        .filter_map(|side| {
            let s = Side::from_symbol(side)?;
            let (x, y) = anchor_point_for_side(s, bbox);
            Some((s, x, y))
        })
        .collect();
    ShapeMetrics { bbox, anchors }
}

fn anchor_point_for_side(side: Side, bbox: (f64, f64, f64, f64)) -> (f64, f64) {
    let (x, y, w, h) = bbox;
    match side {
        Side::North => (x + w / 2.0, y),
        Side::East => (x + w, y + h / 2.0),
        Side::South => (x + w / 2.0, y + h),
        Side::West => (x, y + h / 2.0),
    }
}

fn bbox_center(bbox: &(f64, f64, f64, f64)) -> (f64, f64) {
    (bbox.0 + bbox.2 / 2.0, bbox.1 + bbox.3 / 2.0)
}

/// `true` if `(px, py)` sits on or inside the closed bounding box.
/// Used to skip ancestor containers from the router's obstacle
/// list — the source/destination anchor sits *strictly inside* any
/// enclosing container. Boundary points (a shape's own anchor on
/// its bbox edge) do not count as contained, so a source / dest
/// shape stays an obstacle while still letting the path leave /
/// enter via its anchor cell (`astar_route` unblocks that cell).
fn bbox_contains(bbox: &(f64, f64, f64, f64), p: (f64, f64)) -> bool {
    let (x, y, w, h) = *bbox;
    p.0 > x && p.0 < x + w && p.1 > y && p.1 < y + h
}

/// Pick the source/destination anchor pair with the smallest
/// Euclidean distance. Returns `None` when either side has no
/// anchors. When `is_self_loop` is true, pairs whose distance is
/// zero are excluded so the rendered arrow has a visible length;
/// the next-shortest pair wins. Returned tuples carry the `Side`
/// the anchor lives on so the router knows the egress / ingress
/// direction.
type SidedAnchor = (Side, f64, f64);

fn pick_closest_pair(
    src: &[SidedAnchor],
    dst: &[SidedAnchor],
    is_self_loop: bool,
) -> Option<(SidedAnchor, SidedAnchor)> {
    let mut best: Option<(f64, SidedAnchor, SidedAnchor)> = None;
    for &s in src {
        for &d in dst {
            let dx = s.1 - d.1;
            let dy = s.2 - d.2;
            let dist2 = dx * dx + dy * dy;
            if is_self_loop && dist2 == 0.0 {
                continue;
            }
            if best.map(|(b, _, _)| dist2 < b).unwrap_or(true) {
                best = Some((dist2, s, d));
            }
        }
    }
    best.map(|(_, s, d)| (s, d))
}

/// Two-step pipeline: first plan every edge into a polyline, then
/// run the separation pass over the whole set, then serialize.
/// Returns the rendered SVG plus a bbox per polyline so the
/// fit-to-viewport pass in `render_diagram` can include edges in
/// the content bbox. Edges are gathered from the diagram block
/// and every nested container so a container's own
/// `@connections(Edge) edges` field participates alongside
/// diagram-level edges.
fn render_edges(
    block: &Block<'_>,
    positions: &ShapePositions,
    viewport: (f64, f64),
) -> (String, Vec<(f64, f64, f64, f64)>) {
    let mut items: Vec<Value> = Vec::new();
    gather_edges_recursive(block, &mut items);
    if items.is_empty() {
        return (String::new(), Vec::new());
    }

    let routing_mode = field_symbol(block, "routing").unwrap_or_default();
    let straight = routing_mode == "straight";
    let separation = field_f64(block, "edge_separation").unwrap_or(4.0);

    // Pre-pass: when a shape participates in multiple edges (as
    // source or destination), pick a single shared anchor for that
    // role so every edge converges at the same point rather than
    // each picking its own closest anchor independently.
    let (source_overrides, dest_overrides) = build_shared_anchors(&items, positions);

    let mut planned: Vec<(EdgePath, Option<String>)> = Vec::new();
    for item in &items {
        if let Some(plan) = plan_edge(
            item,
            positions,
            viewport,
            straight,
            &source_overrides,
            &dest_overrides,
        ) {
            planned.push(plan);
        }
    }
    if !straight {
        let mut paths: Vec<EdgePath> = planned.iter().map(|(p, _)| p.clone()).collect();
        routing::separate_edges(&mut paths, separation);
        for (slot, path) in planned.iter_mut().zip(paths) {
            slot.0 = path;
        }
    }
    let mut out = String::new();
    let mut bboxes: Vec<(f64, f64, f64, f64)> = Vec::new();
    for (path, kind) in planned {
        if let Some(bbox) = polyline_bbox(&path.points) {
            bboxes.push(bbox);
        }
        out.push_str(&serialize_edge(&path, kind.as_deref(), straight));
    }
    (out, bboxes)
}

fn polyline_bbox(points: &[(f64, f64)]) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if !min_x.is_finite() {
        return None;
    }
    Some((min_x, min_y, max_x - min_x, max_y - min_y))
}

/// Walk the block tree depth-first and collect every `edges` field
/// (each emits a `Value::List` of edge records). All edges, no
/// matter how deeply nested, render into the same outer SVG
/// coordinate space — `positions` already holds absolute bboxes.
fn gather_edges_recursive(block: &Block<'_>, out: &mut Vec<Value>) {
    if let Some(dr) = block.typed_field("edges")
        && let Ok(Value::List(items)) = dr.value()
    {
        out.extend(items);
    }
    for child in block.blocks() {
        if child.kind() == "container" {
            gather_edges_recursive(&child, out);
        }
    }
}

/// Build the source / destination anchor overrides. When a shape
/// appears as the source of multiple edges, we pick one shared
/// egress anchor (the one closest to the centroid of the
/// destinations' bbox centers). Same for destinations. Self-loops
/// are excluded. Shapes participating in only one edge get no
/// override and fall back to per-edge `pick_closest_pair`.
type AnchorMap = HashMap<String, SidedAnchor>;

fn build_shared_anchors(items: &[Value], positions: &ShapePositions) -> (AnchorMap, AnchorMap) {
    let mut src_targets: HashMap<String, Vec<(f64, f64)>> = HashMap::new();
    let mut dst_sources: HashMap<String, Vec<(f64, f64)>> = HashMap::new();
    for v in items {
        let Value::Record { fields, .. } = v else {
            continue;
        };
        let Some(s) = fields.get("source").and_then(edge_endpoint_id) else {
            continue;
        };
        let Some(d) = fields.get("destination").and_then(edge_endpoint_id) else {
            continue;
        };
        if s == d {
            continue;
        }
        if let Some(d_metrics) = positions.get(&d) {
            src_targets
                .entry(s.clone())
                .or_default()
                .push(bbox_center(&d_metrics.bbox));
        }
        if let Some(s_metrics) = positions.get(&s) {
            dst_sources
                .entry(d)
                .or_default()
                .push(bbox_center(&s_metrics.bbox));
        }
    }
    let mut sources = AnchorMap::new();
    let mut dests = AnchorMap::new();
    for (id, targets) in src_targets {
        if targets.len() < 2 {
            continue;
        }
        let Some(metrics) = positions.get(&id) else {
            continue;
        };
        let centroid = centroid_of(&targets);
        if let Some(anchor) = pick_anchor_toward(&metrics.anchors, centroid) {
            sources.insert(id, anchor);
        }
    }
    for (id, sources_centers) in dst_sources {
        if sources_centers.len() < 2 {
            continue;
        }
        let Some(metrics) = positions.get(&id) else {
            continue;
        };
        let centroid = centroid_of(&sources_centers);
        if let Some(anchor) = pick_anchor_toward(&metrics.anchors, centroid) {
            dests.insert(id, anchor);
        }
    }
    (sources, dests)
}

fn centroid_of(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    let sx: f64 = points.iter().map(|p| p.0).sum();
    let sy: f64 = points.iter().map(|p| p.1).sum();
    (sx / n, sy / n)
}

fn pick_anchor_toward(anchors: &[SidedAnchor], target: (f64, f64)) -> Option<SidedAnchor> {
    anchors
        .iter()
        .min_by(|a, b| {
            let da = (a.1 - target.0).powi(2) + (a.2 - target.1).powi(2);
            let db = (b.1 - target.0).powi(2) + (b.2 - target.1).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

fn pick_closest_to(anchors: &[SidedAnchor], target: (f64, f64)) -> Option<SidedAnchor> {
    pick_anchor_toward(anchors, target)
}

fn plan_edge(
    value: &Value,
    positions: &ShapePositions,
    viewport: (f64, f64),
    straight: bool,
    source_overrides: &AnchorMap,
    dest_overrides: &AnchorMap,
) -> Option<(EdgePath, Option<String>)> {
    let Value::Record { fields, .. } = value else {
        return None;
    };
    let source_id = edge_endpoint_id(fields.get("source")?)?;
    let dest_id = edge_endpoint_id(fields.get("destination")?)?;
    let src = positions.get(&source_id)?;
    let dst = positions.get(&dest_id)?;
    let is_self_loop = source_id == dest_id;
    // Shared-anchor overrides win over per-edge closest-pair, so
    // edges converging at the same shape end at the same point.
    let src_override = source_overrides.get(&source_id).copied();
    let dst_override = dest_overrides.get(&dest_id).copied();
    let pair = match (src_override, dst_override) {
        (Some(s), Some(d)) => Some((s, d)),
        (Some(s), None) => pick_closest_to(&dst.anchors, (s.1, s.2)).map(|d| (s, d)),
        (None, Some(d)) => pick_closest_to(&src.anchors, (d.1, d.2)).map(|s| (s, d)),
        (None, None) => pick_closest_pair(&src.anchors, &dst.anchors, is_self_loop),
    };
    let kind = match fields.get("kind") {
        Some(Value::Symbol(k)) => Some(k.clone()),
        _ => None,
    };
    let points = if straight {
        match pair {
            Some(((_, x1, y1), (_, x2, y2))) => vec![(x1, y1), (x2, y2)],
            None => {
                let (a, b) = (bbox_center(&src.bbox), bbox_center(&dst.bbox));
                vec![a, b]
            }
        }
    } else {
        let ((src_side, sx, sy), (dst_side, dx, dy)) = pair?;
        // Obstacles: every shape *except* those whose bbox strictly
        // contains an endpoint anchor (ancestor containers — the
        // source / destination sits inside them). Source / dest
        // shapes themselves stay in the list: their own anchors are
        // on the bbox boundary, not strictly inside, so they're
        // treated as obstacles. `astar_route` unblocks the snapped
        // start / goal cells so the path can leave / enter via the
        // anchor without traversing the rest of the shape body —
        // which is exactly what stops the router cutting through a
        // shape just because it's the destination.
        let obstacles: Vec<Obstacle> = positions
            .iter()
            .filter(|(_, m)| !bbox_contains(&m.bbox, (sx, sy)) && !bbox_contains(&m.bbox, (dx, dy)))
            .map(|(_, m)| Obstacle {
                x: m.bbox.0,
                y: m.bbox.1,
                w: m.bbox.2,
                h: m.bbox.3,
            })
            .collect();
        routing::route_elbow((sx, sy), src_side, (dx, dy), dst_side, &obstacles, viewport)
    };
    Some((EdgePath { points }, kind))
}

fn serialize_edge(path: &EdgePath, kind: Option<&str>, straight: bool) -> String {
    let kind_attr = match kind {
        Some(k) => format!(" data-kind=\"{}\"", escape_html(k)),
        None => String::new(),
    };
    if straight && path.points.len() == 2 {
        let (x1, y1) = path.points[0];
        let (x2, y2) = path.points[1];
        return format!(
            "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" \
             stroke=\"currentColor\" marker-end=\"url(#wdoc-arrow)\"{kind_attr} />"
        );
    }
    let points: Vec<String> = path
        .points
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect();
    format!(
        "<polyline points=\"{}\" fill=\"none\" \
         stroke=\"currentColor\" marker-end=\"url(#wdoc-arrow)\"{kind_attr} />",
        points.join(" ")
    )
}

fn edge_endpoint_id(v: &Value) -> Option<String> {
    match v {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
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
    let chrome = container_chrome(block, w, h);
    let padding = container_padding(block);
    // Children lay out against the *interior* area (outer minus
    // 2*padding) so anchored / grid-sized children honor the inset.
    let inner_w = (w - 2.0 * padding).max(0.0);
    let inner_h = (h - 2.0 * padding).max(0.0);
    let raw_inner = render_layout_children(doc, block, inner_w, inner_h);
    let inner = if padding > 0.0 && !raw_inner.is_empty() {
        format!("<g transform=\"translate({padding} {padding})\">{raw_inner}</g>")
    } else {
        raw_inner
    };
    let transform = if x != 0.0 || y != 0.0 {
        format!(" transform=\"translate({x} {y})\"")
    } else {
        String::new()
    };
    let mut out = format!("<g{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, "{transform}>{chrome}{inner}</g>").expect("write to String");
    out
}

fn container_padding(block: &Block<'_>) -> f64 {
    field_f64(block, "padding").unwrap_or(0.0).max(0.0)
}

/// Synthesise a background `<rect>` covering the container's full
/// box when the user set `stroke` or `fill`. Emitted as raw SVG
/// inside `render_container` — it never becomes a `Block`, so the
/// obstacle collector ignores it and cross-container edges still
/// pass through unobstructed. When only `stroke` is set, `fill`
/// defaults to `none` so the chrome doesn't paint over children.
fn container_chrome(block: &Block<'_>, w: f64, h: f64) -> String {
    let stroke = field_utf8(block, "stroke");
    let fill = field_utf8(block, "fill");
    if stroke.is_none() && fill.is_none() {
        return String::new();
    }
    let fill_value = fill.unwrap_or_else(|| "none".to_string());
    let mut out = format!("<rect width=\"{w}\" height=\"{h}\"");
    if let Some(s) = stroke.as_deref() {
        write!(out, " stroke=\"{}\"", escape_html(s)).expect("write to String");
    }
    write!(out, " fill=\"{}\" />", escape_html(&fill_value)).expect("write to String");
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
        "table" => render_table_payload(map),
        "element" => render_element_payload(doc, map, depth),
        "raw" => render_raw_payload(map),
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
    let font_size = resolve_label_font_size(
        &content,
        field_f64(block, "font_size"),
        field_f64(block, "fit_width"),
        field_f64(block, "fit_height"),
    );
    emit_text(
        &content,
        x,
        y,
        font_size,
        &cls,
        field_utf8(block, "fill").as_deref(),
        field_id(block, "id").as_deref(),
    )
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
    let font_size = resolve_label_font_size(
        &content,
        map_f64(map, "font_size"),
        map_f64(map, "fit_width"),
        map_f64(map, "fit_height"),
    );
    emit_text(
        &content,
        x,
        y,
        font_size,
        &cls,
        map_utf8(map, "fill").as_deref(),
        map_id(map, "id").as_deref(),
    )
}

/// Pick the font size for a label: explicit `font_size` overrides
/// everything; otherwise auto-fit to the optional `fit_width` /
/// `fit_height` (with padding applied); otherwise the default.
fn resolve_label_font_size(
    content: &str,
    font_size: Option<f64>,
    fit_w: Option<f64>,
    fit_h: Option<f64>,
) -> f64 {
    if let Some(fs) = font_size {
        return fs;
    }
    match (fit_w, fit_h) {
        (Some(w), Some(h)) => text::fit_font_size(content, w - text::H_PAD, h - text::V_PAD),
        _ => text::DEFAULT_FONT_SIZE,
    }
}

/// Emit a centred multi-line `<text>` element. Each line goes in
/// its own `<tspan x="cx" dy="...">`, the first tspan shifted up
/// by `(lines-1)/2 * 1.2em` so the whole block straddles `(cx, cy)`
/// vertically. `text-anchor="middle"` + `dominant-baseline="middle"`
/// handle horizontal + vertical alignment for each line.
fn emit_text(
    content: &str,
    cx: f64,
    cy: f64,
    font_size: f64,
    class_attr: &str,
    fill: Option<&str>,
    id: Option<&str>,
) -> String {
    let metrics = text::measure(content);
    let mut out = format!(
        "<text{class_attr} x=\"{cx}\" y=\"{cy}\" font-size=\"{font_size}\" \
         text-anchor=\"middle\" dominant-baseline=\"middle\""
    );
    append_attr(&mut out, "fill", fill);
    append_attr(&mut out, "id", id);
    out.push('>');
    let n = metrics.lines.len();
    let first_dy = if n <= 1 {
        0.0
    } else {
        -((n as f64 - 1.0) / 2.0) * text::LINE_HEIGHT
    };
    for (i, line) in metrics.lines.iter().enumerate() {
        let dy = if i == 0 { first_dy } else { text::LINE_HEIGHT };
        write!(
            out,
            "<tspan x=\"{cx}\" dy=\"{dy}em\">{}</tspan>",
            escape_html(line)
        )
        .expect("write to String");
    }
    out.push_str("</text>");
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

/// Render an `HtmlFundamental::Table` variant produced by a custom
/// block's `lower`. `header` is the (optional) heading row and `rows`
/// is `list<list<utf8>>` of body rows. Cells are plain escaped text
/// on this path (no inline patterns), mirroring `Paragraph`'s spans.
fn render_table_payload(map: &BTreeMap<String, Value>) -> String {
    let mut classes: Vec<String> = vec!["wdoc-table".to_string()];
    classes.extend(map_utf8_list(map, "class"));
    let header: Vec<String> = map_utf8_list(map, "header")
        .iter()
        .map(|s| escape_html(s))
        .collect();
    let body: Vec<Vec<String>> = match map.get("rows") {
        Some(Value::List(rows)) => rows
            .iter()
            .map(|row| match row {
                Value::List(cells) => cells
                    .iter()
                    .filter_map(value_as_str)
                    .map(|s| escape_html(&s))
                    .collect(),
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    };
    table_html(map_id(map, "id").as_deref(), &classes, &header, &body)
}

/// Render an `HtmlFundamental::Element` — `<tag id class attrs>…</tag>`
/// with its `children` rendered recursively as fundamentals. Powers
/// template layout (header / nav / main / a / …).
fn render_element_payload(doc: &Document, map: &BTreeMap<String, Value>, depth: usize) -> String {
    let tag = map_utf8(map, "tag").unwrap_or_else(|| "div".to_string());
    // Only allow simple alphanumeric tag names so a stray value can't
    // inject markup; fall back to `div` otherwise.
    let tag = if !tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric()) {
        tag
    } else {
        "div".to_string()
    };
    let cls = class_attr_from_map(map);
    let mut out = format!("<{tag}{cls}");
    append_attr(&mut out, "id", map_id(map, "id").as_deref());
    // `attrs` is a list of `[name, value]` pairs.
    if let Some(Value::List(attrs)) = map.get("attrs") {
        for a in attrs {
            if let Value::List(pair) = a
                && let (Some(name), Some(value)) = (
                    pair.first().and_then(value_as_str),
                    pair.get(1).and_then(value_as_str),
                )
            {
                append_attr(&mut out, &name, Some(&value));
            }
        }
    }
    out.push('>');
    if let Some(Value::List(children)) = map.get("children") {
        for child in children {
            out.push_str(&render_html_variant(doc, child, depth + 1));
        }
    }
    write!(out, "</{tag}>").expect("write to String");
    out
}

/// Render an `HtmlFundamental::Raw` — pre-rendered HTML embedded
/// verbatim (NOT escaped). Used to splice already-rendered content
/// (e.g. a page's body) into a template.
fn render_raw_payload(map: &BTreeMap<String, Value>) -> String {
    map_utf8(map, "html").unwrap_or_default()
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
    // Auto-fit to content. When the container uses :layered or :grid
    // layout, its natural size is the bbox of its laid-out children
    // plus 2*padding on each axis; we honor a declared width/height
    // as a minimum but never as a ceiling. This is what lets
    // `stroke = "..."` chrome hug the contents (with the requested
    // inset, if any) instead of painting a fixed-size frame with
    // empty space inside. :none layout keeps the old behaviour
    // (parent fallback) since children there carry their own coords.
    let padding = container_padding(block);
    let (content_w, content_h) = content_size(block);
    let outer_w = if content_w > 0.0 {
        content_w + 2.0 * padding
    } else {
        0.0
    };
    let outer_h = if content_h > 0.0 {
        content_h + 2.0 * padding
    } else {
        0.0
    };
    let decl_w = field_f64(block, "width");
    let decl_h = field_f64(block, "height");
    let mut w = match decl_w {
        Some(d) => d.max(outer_w),
        None if outer_w > 0.0 => outer_w,
        None => parent_w,
    };
    let mut h = match decl_h {
        Some(d) => d.max(outer_h),
        None if outer_h > 0.0 => outer_h,
        None => parent_h,
    };
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

/// Content bbox (`width`, `height`) implied by a container's layout
/// and children. Returns `(0.0, 0.0)` for `:none` layout; children
/// there carry their own positions so there's no single computed
/// size, and callers fall back to declared / parent dims instead.
fn content_size(block: &Block<'_>) -> (f64, f64) {
    let layout = field_symbol(block, "layout").unwrap_or_default();
    match layout.as_str() {
        "layered" => {
            let children: Vec<Block<'_>> = block.blocks().collect();
            if children.is_empty() {
                return (0.0, 0.0);
            }
            let (offsets, widths, heights) = compute_layered_plan(block, &children);
            let mut max_x = 0.0_f64;
            let mut max_y = 0.0_f64;
            for ((ox, oy), (cw, ch)) in offsets.iter().zip(widths.iter().zip(heights.iter())) {
                max_x = max_x.max(ox + cw);
                max_y = max_y.max(oy + ch);
            }
            (max_x, max_y)
        }
        "grid" => {
            let cols = field_i64(block, "columns").unwrap_or(1).max(1) as usize;
            let cw = field_f64(block, "cell_width").unwrap_or(0.0);
            let ch = field_f64(block, "cell_height").unwrap_or(0.0);
            let gap = field_f64(block, "gap").unwrap_or(0.0);
            let n = block.blocks().count();
            if n == 0 {
                return (0.0, 0.0);
            }
            let rows = n.div_ceil(cols);
            let used_cols = cols.min(n);
            let w = used_cols as f64 * cw + used_cols.saturating_sub(1) as f64 * gap;
            let h = rows as f64 * ch + rows.saturating_sub(1) as f64 * gap;
            (w, h)
        }
        _ => (0.0, 0.0),
    }
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

fn field_bool(block: &Block<'_>, name: &str) -> Option<bool> {
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

fn field_f64(block: &Block<'_>, name: &str) -> Option<f64> {
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

/// Read a `list<symbol>` field, distinguishing "field absent or
/// none" (returns `None`, callers apply their own default) from
/// "explicitly empty list" (returns `Some(vec![])`).
fn field_symbol_list_opt(block: &Block<'_>, name: &str) -> Option<Vec<String>> {
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
