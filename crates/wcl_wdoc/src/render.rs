use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use wcl_lang::{Block, Document, FnValue, Value, VariantPayload};

use crate::layered::{self, Direction};
use crate::routing::{self, EdgePath, Obstacle, Side};

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
    let (vw, vh) = (width as f64, height as f64);
    let mut positions: ShapePositions = HashMap::new();
    collect_layout_children(block, 0.0, 0.0, vw, vh, &mut positions);
    let shapes: String = render_layout_children(doc, block, vw, vh);
    let edges = render_edges(block, &positions, (vw, vh));
    let defs = if edges.is_empty() { "" } else { ARROW_MARKER };
    let mut out = format!("<svg{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(
        out,
        " xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\">{defs}{shapes}{edges}</svg>"
    )
    .expect("write to String");
    out
}

/// Iterate `block`'s children under the layout specified by the
/// block's `layout` field, recording each id'd shape's absolute
/// bbox + anchors in `out`. Used for both Diagram and Container.
fn collect_layout_children(
    block: &Block<'_>,
    tx: f64,
    ty: f64,
    parent_w: f64,
    parent_h: f64,
    out: &mut ShapePositions,
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

fn collect_grid_children(block: &Block<'_>, tx: f64, ty: f64, out: &mut ShapePositions) {
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

fn collect_layered_children(block: &Block<'_>, tx: f64, ty: f64, out: &mut ShapePositions) {
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
    let layer_gap = field_f64(block, "layer_gap").unwrap_or(30.0);
    let node_gap = field_f64(block, "node_gap").unwrap_or(20.0);

    let nodes: Vec<layered::Node> = children
        .iter()
        .map(|c| layered::Node {
            id: field_id(c, "id"),
            size: (
                field_f64(c, "width").unwrap_or(80.0),
                field_f64(c, "height").unwrap_or(40.0),
            ),
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
/// absolute bounding box + resolved anchor positions. Mirrors the
/// geometry computed by `render_*` but without producing SVG — so
/// the edge pass can join shapes by id regardless of how deeply they
/// nest inside containers / grids.
fn collect_shape_positions(
    block: &Block<'_>,
    tx: f64,
    ty: f64,
    parent_w: f64,
    parent_h: f64,
    out: &mut ShapePositions,
) {
    let record = |block: &Block<'_>, bbox: (f64, f64, f64, f64), out: &mut ShapePositions| {
        if let Some(id) = field_id(block, "id") {
            out.insert(id, build_metrics(block, bbox));
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
            collect_layout_children(block, tx + x, ty + y, w, h, out);
        }
        // `line` and `polygon` have no single anchor point usable as
        // an edge endpoint; they're skipped.
        "line" | "polygon" => {}
        // Custom shapes (process, decision, terminator, user-defined,
        // …) follow the same convention as fundamentals: x/y/width/
        // height as a bounding box. We read those directly without
        // lowering so connection endpoints land on the visible
        // outline rather than on an arbitrary post-lower coordinate.
        _ => {
            if let (Some(x), Some(y), Some(w), Some(h)) = (
                field_f64(block, "x"),
                field_f64(block, "y"),
                field_f64(block, "width"),
                field_f64(block, "height"),
            ) {
                record(block, (tx + x, ty + y, w, h), out);
            }
        }
    }
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
/// list — the source/destination anchor sits on the source shape's
/// outline, which is *inside* any enclosing container.
fn bbox_contains(bbox: &(f64, f64, f64, f64), p: (f64, f64)) -> bool {
    let (x, y, w, h) = *bbox;
    p.0 >= x && p.0 <= x + w && p.1 >= y && p.1 <= y + h
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
fn render_edges(block: &Block<'_>, positions: &ShapePositions, viewport: (f64, f64)) -> String {
    let Some(dr) = block.typed_field("edges") else {
        return String::new();
    };
    let Ok(value) = dr.value() else {
        return String::new();
    };
    let Value::List(items) = value else {
        return String::new();
    };

    let routing_mode = field_symbol(block, "routing").unwrap_or_default();
    let straight = routing_mode == "straight";
    let separation = field_f64(block, "edge_separation").unwrap_or(4.0);

    let mut planned: Vec<(EdgePath, Option<String>)> = Vec::new();
    for item in &items {
        if let Some(plan) = plan_edge(item, positions, viewport, straight) {
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
    for (path, kind) in planned {
        out.push_str(&serialize_edge(&path, kind.as_deref(), straight));
    }
    out
}

fn plan_edge(
    value: &Value,
    positions: &ShapePositions,
    viewport: (f64, f64),
    straight: bool,
) -> Option<(EdgePath, Option<String>)> {
    let Value::Record { fields, .. } = value else {
        return None;
    };
    let source_id = edge_endpoint_id(fields.get("source")?)?;
    let dest_id = edge_endpoint_id(fields.get("destination")?)?;
    let src = positions.get(&source_id)?;
    let dst = positions.get(&dest_id)?;
    let is_self_loop = source_id == dest_id;
    let pair = pick_closest_pair(&src.anchors, &dst.anchors, is_self_loop);
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
        // Obstacles: every other shape, excluding the source and
        // destination shapes themselves, and excluding any shape
        // whose bbox *contains* either endpoint anchor. The
        // containing-shape exclusion handles nested containers:
        // if the source anchor sits inside group_left's bbox,
        // group_left is an ancestor scope rather than an obstacle
        // the router must avoid.
        let obstacles: Vec<Obstacle> = positions
            .iter()
            .filter(|(id, _)| id.as_str() != source_id.as_str() && id.as_str() != dest_id.as_str())
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
    let inner = render_layout_children(doc, block, w, h);
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
            // Fall back to the schema's declared default
            // (`name = expr` inline-default or `@default(expr)`)
            // so a lowering that consumes `block.x` doesn't crash
            // when the block omits the field but the type
            // declared a value-typed default.
            f.default_value().unwrap_or(Value::None)
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
