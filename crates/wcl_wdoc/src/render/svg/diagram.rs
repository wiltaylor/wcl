//! Top-level diagram rendering and layout planning: `render_diagram`
//! (the SVG document + pan/zoom wrapper), the grid / layered / force
//! layout plan + collect + render passes, and the `container` shape.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::Path;

use wcl_lang::{Block, Document};

use crate::force::{self, ForceParams};
use crate::inline::InlinePatterns;
use crate::layered::{self, Direction};

use super::*;

pub(crate) fn render_diagram(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    // Pin cards render to HTML that must sit outside the `<svg>`; collect
    // them here and splice them into the viewport wrapper below.
    let overlays = RefCell::new(Vec::new());
    let ctx = RenderCtx {
        doc,
        icons: patterns.icons(),
        tilesets: patterns.tilesets(),
        images: patterns.images(),
        patterns,
        base_dir,
        overlays: &overlays,
    };
    let cctx = CollectCtx {
        tilesets: patterns.tilesets(),
        images: patterns.images(),
    };
    let cls = class_attr(block);
    let width = field_i64(block, "width").unwrap_or(0);
    let height = field_i64(block, "height").unwrap_or(0);
    let (vw, vh) = (width as f64, height as f64);
    let mut collector = Collector::default();
    collect_layout_children(block, 0.0, 0.0, vw, vh, cctx, &mut collector);
    let shapes: String = render_layout_children(block, vw, vh, ctx);
    let (edges, edge_bboxes) = render_edges(block, &collector.positions, (vw, vh));
    let viewbox = fit_viewbox(&collector.bboxes, &edge_bboxes, vw, vh);
    let defs = if edges.is_empty() { "" } else { ARROW_MARKER };
    let mut out = format!("<svg{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    // Interactive pan + zoom: carry the fitted view + limits on the
    // `<svg>` so the bundled player can drive its `viewBox`, and wrap
    // it in a viewport that hosts the overlaid controls. A diagram with a
    // `map` is interactive even without an explicit `pan_zoom` (a map is
    // inherently zoomable). Plain diagrams keep the bare-`<svg>` output.
    let interactive = field_bool(block, "pan_zoom") == Some(true) || uses_map(block);
    if interactive {
        let zoom_min = field_f64(block, "zoom_min").unwrap_or(1.0);
        let zoom_max = field_f64(block, "zoom_max").unwrap_or(4.0);
        let pan_margin = field_f64(block, "pan_margin").unwrap_or(0.0);
        write!(
            out,
            " data-pan-zoom=\"1\" data-base-viewbox=\"{viewbox}\" \
             data-zoom-min=\"{zoom_min}\" data-zoom-max=\"{zoom_max}\" \
             data-pan-margin=\"{pan_margin}\""
        )
        .expect("write to String");
    }
    write!(
        out,
        " xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"{viewbox}\">{defs}{shapes}{edges}</svg>"
    )
    .expect("write to String");
    if interactive {
        // Pin cards (collected while rendering shapes) sit after the SVG,
        // inside the same relatively-positioned viewport, so the map player
        // can position them as popups over the map.
        let cards: String = overlays.borrow().concat();
        return format!(
            "<div class=\"wdoc-diagram-viewport\">{out}{DIAGRAM_CONTROLS}{cards}</div>"
        );
    }
    out
}

/// Iterate `block`'s children under the layout specified by the
/// block's `layout` field, recording each id'd shape's absolute
/// bbox + anchors in `out.positions` and every shape's bbox in
/// `out.bboxes`. Used for both Diagram and Container.
pub(crate) fn collect_layout_children(
    block: &Block<'_>,
    tx: f64,
    ty: f64,
    parent_w: f64,
    parent_h: f64,
    cctx: CollectCtx<'_>,
    out: &mut Collector,
) {
    let layout = field_symbol(block, "layout").unwrap_or_default();
    match layout.as_str() {
        "grid" => collect_grid_children(block, tx, ty, cctx, out),
        "layered" | "force" => collect_planned_children(block, tx, ty, cctx, out),
        _ => {
            for child in diagram_children(block) {
                collect_shape_positions(&child, tx, ty, parent_w, parent_h, cctx, out);
            }
        }
    }
}

/// Symmetric: render `block`'s children using its `layout`. Caller
/// supplies the parent box (`pw`, `ph`) that propagates to children
/// not assigned an explicit cell.
pub(crate) fn render_layout_children(
    block: &Block<'_>,
    pw: f64,
    ph: f64,
    ctx: RenderCtx<'_>,
) -> String {
    let layout = field_symbol(block, "layout").unwrap_or_default();
    match layout.as_str() {
        "grid" => render_grid_children(block, ctx),
        "layered" | "force" => render_planned_children(block, ctx),
        _ => diagram_children(block)
            .iter()
            .filter_map(|b| render_shape(b, pw, ph, ctx))
            .collect(),
    }
}

pub(crate) fn collect_grid_children(
    block: &Block<'_>,
    tx: f64,
    ty: f64,
    cctx: CollectCtx<'_>,
    out: &mut Collector,
) {
    let cols = field_i64(block, "columns").unwrap_or(1).max(1) as usize;
    let cw = field_f64(block, "cell_width").unwrap_or(0.0);
    let ch = field_f64(block, "cell_height").unwrap_or(0.0);
    let gap = field_f64(block, "gap").unwrap_or(0.0);
    for (i, child) in diagram_children(block).iter().enumerate() {
        let (cx_off, cy_off) = grid_cell_offset(i, cols, cw, ch, gap);
        collect_shape_positions(child, tx + cx_off, ty + cy_off, cw, ch, cctx, out);
    }
}

/// Collect positions for a `:layered` or `:force` container/diagram.
/// Both layouts produce per-child `(tx, ty)` offsets via
/// `compute_planned_plan`; only the offset solver differs.
pub(crate) fn collect_planned_children(
    block: &Block<'_>,
    tx: f64,
    ty: f64,
    cctx: CollectCtx<'_>,
    out: &mut Collector,
) {
    let children: Vec<Block<'_>> = diagram_children(block);
    let (offsets, widths, heights) = compute_planned_plan(block, &children);
    // Size each child's parent box from the plan (effective_dims), not
    // the raw width/height, so collect and render agree on circles
    // (sized by diameter) and text-grown shapes alike.
    for ((child, (cx, cy)), (pw, ph)) in children
        .iter()
        .zip(offsets)
        .zip(widths.iter().zip(heights.iter()))
    {
        collect_shape_positions(child, tx + cx, ty + cy, *pw, *ph, cctx, out);
    }
}

/// Compute the layered layout for a container/diagram's children
/// and return (per-child offsets, per-child width, per-child height).
/// The width/height returned reflect the size the renderer should
/// pass downward as `parent_w` / `parent_h`.
pub(crate) fn compute_layered_plan(
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

/// Compute the force-directed layout for a container/diagram's children.
/// Same return shape as `compute_layered_plan`: per-child offsets +
/// widths + heights. The optional knob fields (`iterations`,
/// `repulsion`, `link_distance`, `gravity`, `seed`) fall back to
/// `ForceParams::default()`.
pub(crate) fn compute_force_plan(
    block: &Block<'_>,
    children: &[Block<'_>],
) -> (Vec<(f64, f64)>, Vec<f64>, Vec<f64>) {
    let defaults = ForceParams::default();
    let params = ForceParams {
        iterations: field_i64(block, "iterations")
            .map(|v| v.max(0) as usize)
            .unwrap_or(defaults.iterations),
        repulsion: field_f64(block, "repulsion").unwrap_or(defaults.repulsion),
        link_distance: field_f64(block, "link_distance").unwrap_or(defaults.link_distance),
        gravity: field_f64(block, "gravity").unwrap_or(defaults.gravity),
        seed: field_i64(block, "seed").unwrap_or(defaults.seed),
    };

    let nodes: Vec<layered::Node> = children
        .iter()
        .map(|c| layered::Node {
            id: field_id(c, "id"),
            size: effective_dims(c),
        })
        .collect();
    let edges: Vec<(String, String)> = edge_id_pairs(block);
    let offsets = force::assign_force_offsets(&nodes, &edges, params);
    let widths: Vec<f64> = nodes.iter().map(|n| n.size.0).collect();
    let heights: Vec<f64> = nodes.iter().map(|n| n.size.1).collect();
    (offsets, widths, heights)
}

/// Dispatch to the layout solver named by the block's `layout` field.
/// Both `:layered` and `:force` produce offsets + sizes in the same
/// shape, so the collect / render / size paths share one entry point.
pub(crate) fn compute_planned_plan(
    block: &Block<'_>,
    children: &[Block<'_>],
) -> (Vec<(f64, f64)>, Vec<f64>, Vec<f64>) {
    match field_symbol(block, "layout").unwrap_or_default().as_str() {
        "force" => compute_force_plan(block, children),
        _ => compute_layered_plan(block, children),
    }
}

pub(crate) fn render_planned_children(block: &Block<'_>, ctx: RenderCtx<'_>) -> String {
    let children: Vec<Block<'_>> = diagram_children(block);
    let (offsets, widths, heights) = compute_planned_plan(block, &children);
    let mut out = String::new();
    for ((child, (tx, ty)), (cw, ch)) in children
        .iter()
        .zip(offsets)
        .zip(widths.iter().zip(heights.iter()))
    {
        if let Some(rendered) = render_shape(child, *cw, *ch, ctx) {
            out.push_str(&format!(
                "<g transform=\"translate({tx} {ty})\">{rendered}</g>"
            ));
        }
    }
    out
}

pub(crate) fn render_container(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
    ctx: RenderCtx<'_>,
) -> String {
    let cls = class_attr(block);
    let (x, y, w, h) = resolve_container_box(block, parent_w, parent_h);
    let chrome = container_chrome(block, w, h);
    let padding = container_padding(block);
    // Children lay out against the *interior* area (outer minus
    // 2*padding) so anchored / grid-sized children honor the inset.
    let inner_w = (w - 2.0 * padding).max(0.0);
    let inner_h = (h - 2.0 * padding).max(0.0);
    let raw_inner = render_layout_children(block, inner_w, inner_h, ctx);
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

pub(crate) fn container_padding(block: &Block<'_>) -> f64 {
    field_f64(block, "padding").unwrap_or(0.0).max(0.0)
}

/// Synthesise a background `<rect>` covering the container's full
/// box when the user set `stroke` or `fill`. Emitted as raw SVG
/// inside `render_container` — it never becomes a `Block`, so the
/// obstacle collector ignores it and cross-container edges still
/// pass through unobstructed. When only `stroke` is set, `fill`
/// defaults to `none` so the chrome doesn't paint over children.
pub(crate) fn container_chrome(block: &Block<'_>, w: f64, h: f64) -> String {
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

pub(crate) fn render_grid_children(block: &Block<'_>, ctx: RenderCtx<'_>) -> String {
    let cols = field_i64(block, "columns").unwrap_or(1).max(1) as usize;
    let cw = field_f64(block, "cell_width").unwrap_or(0.0);
    let ch = field_f64(block, "cell_height").unwrap_or(0.0);
    let gap = field_f64(block, "gap").unwrap_or(0.0);
    diagram_children(block)
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let rendered = render_shape(b, cw, ch, ctx)?;
            let (tx, ty) = grid_cell_offset(i, cols, cw, ch, gap);
            Some(format!(
                "<g transform=\"translate({tx} {ty})\">{rendered}</g>"
            ))
        })
        .collect()
}

/// Content bbox (`width`, `height`) implied by a container's layout
/// and children. Returns `(0.0, 0.0)` for `:none` layout; children
/// there carry their own positions so there's no single computed
/// size, and callers fall back to declared / parent dims instead.
pub(crate) fn content_size(block: &Block<'_>) -> (f64, f64) {
    let layout = field_symbol(block, "layout").unwrap_or_default();
    match layout.as_str() {
        "layered" | "force" => {
            let children: Vec<Block<'_>> = diagram_children(block);
            if children.is_empty() {
                return (0.0, 0.0);
            }
            let (offsets, widths, heights) = compute_planned_plan(block, &children);
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
            let n = diagram_children(block).len();
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
