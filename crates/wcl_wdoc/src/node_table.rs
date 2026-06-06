//! The `node_table` shape: a titled diagram box made of a stack of rows,
//! each row holding arbitrary wdoc content and exposing its own per-row
//! connection point(s). Ideal for database / ER tables and UML class
//! diagrams, where an edge attaches to a specific row (a foreign-key
//! column, a class field) rather than the whole box.
//!
//! Like `card`, it is special-cased in the renderer (its WCL `lower` is a
//! stub): each row's body is HTML — produced by the block renderer +
//! inline engine ([`render_block`]) — wrapped in an SVG `<foreignObject>`.
//! The frame, row separators and per-row port markers are SVG primitives
//! drawn around those rows.
//!
//! Per-row connectivity reuses the existing edge machinery: each row with
//! an `id` is registered as its own sub-shape in `collect_shape_positions`
//! (see `src/render/svg/shapes.rs`), so an edge can target the row id and
//! the standard west/east anchor logic lands it on the row's edge.

use std::fmt::Write as _;

use wcl_lang::Block;

use crate::render::{
    MAX_LOWER_DEPTH, RenderCtx, escape_html, expand_component_children, expand_instance_children,
    expand_repeater_children, field_f64, field_id, field_symbol_list_opt, field_utf8,
    field_utf8_list, render_block, resolve_rect_box,
};

/// Default per-row height when `row_height` is unset.
const ROW_HEIGHT: f64 = 30.0;
/// Default header (title) row height when `header_height` is unset.
const HEADER_HEIGHT: f64 = 28.0;
/// Radius of a per-row connection-point marker.
const PORT_R: f64 = 3.0;

fn row_height(block: &Block<'_>) -> f64 {
    field_f64(block, "row_height").unwrap_or(ROW_HEIGHT)
}

/// Height reserved for the title header — `header_height` when the table
/// has a `title`, else zero (a header-less table).
fn header_offset(block: &Block<'_>) -> f64 {
    if field_utf8(block, "title").is_some() {
        field_f64(block, "header_height").unwrap_or(HEADER_HEIGHT)
    } else {
        0.0
    }
}

/// The `node_row` children of a `node_table`, in source order, with
/// `wdoc_repeater` / `wdoc_component` children expanded the same way the
/// diagram/container path expands shapes (see `render::expand`), so a
/// table's rows can be data-driven (the headline ER / class-diagram case)
/// rather than only literal.
fn rows<'a>(block: &Block<'a>) -> Vec<Block<'a>> {
    let mut out = Vec::new();
    for child in block.blocks() {
        collect_rows(child, &mut out);
    }
    out
}

/// Recursively collect `node_row`s, expanding repeaters, `wdoc_instance`s,
/// and component instances in place — mirrors `expand_container_children`.
fn collect_rows<'a>(child: Block<'a>, out: &mut Vec<Block<'a>>) {
    // Stop runaway self-referential expansion (mirrors the diagram guard).
    if child.binding_scope_depth() > MAX_LOWER_DEPTH {
        return;
    }
    match child.kind() {
        "node_row" => out.push(child),
        "wdoc_repeater" => {
            for c in expand_repeater_children(&child) {
                collect_rows(c, out);
            }
        }
        "wdoc_instance" => {
            for c in expand_instance_children(&child) {
                collect_rows(c, out);
            }
        }
        kind => {
            if let Some(def) = child.doc().component_def(kind) {
                for c in expand_component_children(&child, &def) {
                    collect_rows(c, out);
                }
            }
            // Other non-row kinds are ignored, as before.
        }
    }
}

/// The connection sides a row exposes (edge-attach points + visible
/// markers). Honors the row's `connect_points`; defaults to west + east
/// (left / right), the natural sides for side-by-side tables. Shared by
/// the marker renderer and the position collector so they can't drift.
pub(crate) fn row_sides(row: &Block<'_>) -> Vec<String> {
    field_symbol_list_opt(row, "connect_points")
        .unwrap_or_else(|| vec!["west".to_string(), "east".to_string()])
}

/// Resolve the table's absolute-local box. Width comes from `width` /
/// anchors (like `rect`); height is *derived* — header + one `row_height`
/// per `node_row` — because the renderer can't measure HTML row content.
pub(crate) fn node_table_bbox(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
    let (x, y, w, _) = resolve_rect_box(block, parent_w, parent_h);
    let h = header_offset(block) + rows(block).len() as f64 * row_height(block);
    (x, y, w, h)
}

/// Each `node_row` paired with its absolute-local bbox (`x` / `y` are the
/// table's local origin; the caller adds any container translate). Shared
/// by the renderer and the position collector so SVG and anchors agree.
pub(crate) fn row_boxes<'a>(
    block: &Block<'a>,
    x: f64,
    y: f64,
    w: f64,
) -> Vec<(Block<'a>, (f64, f64, f64, f64))> {
    let rh = row_height(block);
    let top = y + header_offset(block);
    rows(block)
        .into_iter()
        .enumerate()
        .map(|(i, row)| (row, (x, top + i as f64 * rh, w, rh)))
        .collect()
}

/// The (x, y) of a cardinal side's midpoint on a box — mirrors the SVG
/// edge router's `anchor_point_for_side`, so a rendered marker sits
/// exactly where an edge attaches.
fn side_point(side: &str, (x, y, w, h): (f64, f64, f64, f64)) -> Option<(f64, f64)> {
    Some(match side {
        "north" => (x + w / 2.0, y),
        "east" => (x + w, y + h / 2.0),
        "south" => (x + w / 2.0, y + h),
        "west" => (x, y + h / 2.0),
        _ => return None,
    })
}

/// Render a `@block("node_table")`: an SVG frame + a per-row
/// `<foreignObject>` body + per-row connection-point markers.
pub(crate) fn render_node_table(
    block: &Block<'_>,
    ctx: RenderCtx<'_>,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let (x, y, w, h) = node_table_bbox(block, parent_w, parent_h);
    let mut svg = String::new();

    // Frame: background + outline behind the rows.
    let mut frame_class = String::from("wdoc-node-table-frame");
    for c in field_utf8_list(block, "class") {
        frame_class.push(' ');
        frame_class.push_str(&escape_html(&c));
    }
    let id_attr = field_id(block, "id")
        .map(|i| format!(" data-node-table-id=\"{}\"", escape_html(&i)))
        .unwrap_or_default();
    let _ = write!(
        svg,
        "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"6\" \
         class=\"{frame_class}\"{id_attr} />"
    );

    // Header (title) drawn as a foreignObject so it themes like the rows,
    // with a separator underneath it.
    let head = header_offset(block);
    if let Some(title) = field_utf8(block, "title") {
        let _ = write!(
            svg,
            "<foreignObject x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{head}\">\
             <div xmlns=\"http://www.w3.org/1999/xhtml\" class=\"wdoc-node-table-title\">{}</div>\
             </foreignObject>",
            escape_html(&title)
        );
        let sy = y + head;
        let _ = write!(
            svg,
            "<line x1=\"{x}\" y1=\"{sy}\" x2=\"{}\" y2=\"{sy}\" class=\"wdoc-node-table-sep\" />",
            x + w
        );
    }

    // Rows: a top separator (between rows), the content foreignObject, and
    // the per-row connection markers.
    for (i, (row, rb)) in row_boxes(block, x, y, w).into_iter().enumerate() {
        let (rx, ry, rw, rh) = rb;
        // Separator above every row except the first (the header already
        // drew its own divider when present).
        if i > 0 {
            let _ = write!(
                svg,
                "<line x1=\"{rx}\" y1=\"{ry}\" x2=\"{}\" y2=\"{ry}\" \
                 class=\"wdoc-node-table-sep\" />",
                rx + rw
            );
        }
        // Row content: arbitrary wdoc blocks, in a foreignObject.
        let mut inner = String::new();
        for child in row.blocks() {
            if let Some(html) = render_block(ctx.doc, &child, ctx.patterns, ctx.base_dir) {
                inner.push_str(&html);
            }
        }
        if !inner.is_empty() {
            let mut row_class = String::from("wdoc-node-row");
            for c in field_utf8_list(&row, "class") {
                row_class.push(' ');
                row_class.push_str(&escape_html(&c));
            }
            let rid = field_id(&row, "id")
                .map(|i| format!(" data-node-row-id=\"{}\"", escape_html(&i)))
                .unwrap_or_default();
            let _ = write!(
                svg,
                "<foreignObject x=\"{rx}\" y=\"{ry}\" width=\"{rw}\" height=\"{rh}\">\
                 <div xmlns=\"http://www.w3.org/1999/xhtml\" class=\"{row_class}\"{rid}>{inner}</div>\
                 </foreignObject>"
            );
        }
        // Per-row connection-point markers (left / right by default).
        for side in row_sides(&row) {
            if let Some((px, py)) = side_point(&side, rb) {
                let _ = write!(
                    svg,
                    "<circle cx=\"{px}\" cy=\"{py}\" r=\"{PORT_R}\" \
                     class=\"wdoc-node-table-port\" />"
                );
            }
        }
    }

    svg
}
