//! Top-level diagram rendering and layout planning: `render_diagram`
//! (the SVG document + pan/zoom wrapper), the grid / layered / force
//! layout plan + collect + render passes, and the `container` shape.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::Path;

use wcl_lang::{Block, Document, Value};

use crate::force::{self, ForceParams};
use crate::inline::InlinePatterns;
use crate::layered::{self, Direction};
use crate::radial::{self, RadialParams};

use super::*;

pub(crate) fn render_diagram(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    render_diagram_inner(doc, block, patterns, base_dir, false)
}

/// Render a diagram as a self-contained, **static** `<svg>` — no pan/zoom
/// data attributes, no `.wdoc-diagram-viewport` wrapper, no overlay
/// controls. Used by the Markdown target, which writes each diagram to a
/// standalone `.svg` file: interactivity has no meaning there, so a
/// `pan_zoom`/`map` diagram degrades to its base (fully-fitted) view.
pub(crate) fn render_diagram_static(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    render_diagram_inner(doc, block, patterns, base_dir, true)
}

fn render_diagram_inner(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
    static_mode: bool,
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
    // Boundaries are a post-layout overlay: sized from `positions`
    // (now populated), drawn *behind* the shapes, and excluded from the
    // obstacle graph (they never become a `Block`). Their expanded boxes
    // join the viewBox fit so a padded boundary never clips.
    let (boundaries, boundary_bboxes) = render_boundaries(block, &collector.positions);
    let (edges, edge_bboxes) =
        render_edges(block, &collector.positions, &collector.containers, (vw, vh));
    let mut content_bboxes = collector.bboxes.clone();
    content_bboxes.extend(boundary_bboxes);
    let viewbox = fit_viewbox(&content_bboxes, &edge_bboxes, vw, vh);
    let defs = if edges.is_empty() { "" } else { ARROW_MARKER };
    let mut out = format!("<svg{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    // Interactive pan + zoom: carry the fitted view + limits on the
    // `<svg>` so the bundled player can drive its `viewBox`, and wrap
    // it in a viewport that hosts the overlaid controls. A diagram with a
    // `map` is interactive even without an explicit `pan_zoom` (a map is
    // inherently zoomable). Plain diagrams keep the bare-`<svg>` output.
    let interactive =
        !static_mode && (field_bool(block, "pan_zoom") == Some(true) || uses_map(block));
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
         viewBox=\"{viewbox}\">{defs}{boundaries}{shapes}{edges}</svg>"
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
        "layered" | "force" | "radial" => collect_planned_children(block, tx, ty, cctx, out),
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
        "layered" | "force" | "radial" => render_planned_children(block, ctx),
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
    for (i, child) in grid_cells(block).iter().enumerate() {
        let (cx_off, cy_off) = grid_cell_offset(i, cols, cw, ch, gap);
        collect_shape_positions(child, tx + cx_off, ty + cy_off, cw, ch, cctx, out);
    }
}

/// The grid-laid children of a diagram/container: every child *except*
/// a `boundary`, which is a post-layout overlay and must not consume a
/// grid cell (that would shift the real shapes after it).
fn grid_cells<'a>(block: &Block<'a>) -> Vec<Block<'a>> {
    diagram_children(block)
        .into_iter()
        .filter(|c| c.kind() != "boundary")
        .collect()
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
/// A `label` / `line` is a zero-footprint annotation, not a flow node:
/// excluding it from the auto-layout solvers keeps it from being
/// allocated a phantom cell that shoves real shapes aside (and, for a
/// `label`, rendered half-off its own origin). Instead it's positioned
/// at the container origin against the flow content's full extent, so its
/// own `x`/`y` + fractional anchors place it over the whole container.
/// A `boundary` is likewise excluded: it's a post-layout overlay,
/// not a flow node, so the solver must neither place it nor let it
/// shove real shapes aside. (It also produces no SVG in the normal
/// shape pass — see `render_shape` — and is drawn by `render_boundaries`.)
fn is_layout_annotation(kind: &str) -> bool {
    matches!(kind, "label" | "line" | "boundary")
}

/// Reassemble a per-child plan from a solve over the *flow* children only.
/// `flow_idx[k]` is the index in `children` of the k-th solved node, whose
/// offset is `flow_offsets[k]` and size is `flow_nodes[k].size`. Annotation
/// children get offset `(0, 0)` and the flow content's extent as their
/// parent box (so anchors resolve against the whole container) while
/// contributing nothing to that extent.
fn assemble_plan(
    children: &[Block<'_>],
    flow_idx: &[usize],
    flow_nodes: &[layered::Node],
    flow_offsets: &[(f64, f64)],
) -> (Vec<(f64, f64)>, Vec<f64>, Vec<f64>) {
    let mut content_w = 0.0_f64;
    let mut content_h = 0.0_f64;
    for (off, node) in flow_offsets.iter().zip(flow_nodes) {
        content_w = content_w.max(off.0 + node.size.0);
        content_h = content_h.max(off.1 + node.size.1);
    }
    let mut offsets = vec![(0.0, 0.0); children.len()];
    let mut widths = vec![0.0; children.len()];
    let mut heights = vec![0.0; children.len()];
    for (k, &ci) in flow_idx.iter().enumerate() {
        offsets[ci] = flow_offsets[k];
        widths[ci] = flow_nodes[k].size.0;
        heights[ci] = flow_nodes[k].size.1;
    }
    for (ci, child) in children.iter().enumerate() {
        if is_layout_annotation(child.kind()) {
            offsets[ci] = (0.0, 0.0);
            widths[ci] = content_w;
            heights[ci] = content_h;
        }
    }
    (offsets, widths, heights)
}

/// The flow children of a layout container (everything that isn't a
/// zero-footprint annotation), paired with their original indices and
/// solver `Node`s.
fn flow_nodes_of(children: &[Block<'_>]) -> (Vec<usize>, Vec<layered::Node>) {
    let flow_idx: Vec<usize> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| !is_layout_annotation(c.kind()))
        .map(|(i, _)| i)
        .collect();
    let flow_nodes: Vec<layered::Node> = flow_idx
        .iter()
        .map(|&i| layered::Node {
            id: field_id(&children[i], "id"),
            // Use effective_dims so multi-line text grows the cell
            // the layered solver allocates for the shape.
            size: effective_dims(&children[i]),
        })
        .collect();
    (flow_idx, flow_nodes)
}

pub(crate) fn compute_layered_plan(
    block: &Block<'_>,
    children: &[Block<'_>],
) -> (Vec<(f64, f64)>, Vec<f64>, Vec<f64>) {
    let direction = field_symbol(block, "direction")
        .and_then(|s| Direction::from_symbol(&s))
        .unwrap_or(Direction::TopToBottom);
    let layer_gap = field_f64(block, "layer_gap").unwrap_or(40.0);
    let node_gap = field_f64(block, "node_gap").unwrap_or(40.0);

    let (flow_idx, flow_nodes) = flow_nodes_of(children);
    let edges: Vec<(String, String)> = edge_id_pairs(block);
    let flow_offsets =
        layered::assign_layered_offsets(&flow_nodes, &edges, direction, layer_gap, node_gap);
    assemble_plan(children, &flow_idx, &flow_nodes, &flow_offsets)
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

    let (flow_idx, flow_nodes) = flow_nodes_of(children);
    let edges: Vec<(String, String)> = edge_id_pairs(block);
    let flow_offsets = force::assign_force_offsets(&flow_nodes, &edges, params);
    assemble_plan(children, &flow_idx, &flow_nodes, &flow_offsets)
}

/// Compute the radial (hub-and-spoke) layout for a container/diagram's
/// children. Same return shape as `compute_layered_plan`: per-child
/// offsets + widths + heights. The optional knob fields (`hub`, `radius`,
/// `ring_gap`, `start_angle`, `node_gap`) fall back to
/// `RadialParams::default()`.
pub(crate) fn compute_radial_plan(
    block: &Block<'_>,
    children: &[Block<'_>],
) -> (Vec<(f64, f64)>, Vec<f64>, Vec<f64>) {
    let defaults = RadialParams::default();
    let params = RadialParams {
        hub: field_id(block, "hub"),
        radius: field_f64(block, "radius"),
        ring_gap: field_f64(block, "ring_gap").unwrap_or(defaults.ring_gap),
        start_angle: field_f64(block, "start_angle").unwrap_or(defaults.start_angle),
        node_gap: field_f64(block, "node_gap").unwrap_or(defaults.node_gap),
    };

    let (flow_idx, flow_nodes) = flow_nodes_of(children);
    let edges: Vec<(String, String)> = edge_id_pairs(block);
    let flow_offsets = radial::assign_radial_offsets(&flow_nodes, &edges, params);
    assemble_plan(children, &flow_idx, &flow_nodes, &flow_offsets)
}

/// Dispatch to the layout solver named by the block's `layout` field.
/// `:layered`, `:force` and `:radial` all produce offsets + sizes in the
/// same shape, so the collect / render / size paths share one entry point.
pub(crate) fn compute_planned_plan(
    block: &Block<'_>,
    children: &[Block<'_>],
) -> (Vec<(f64, f64)>, Vec<f64>, Vec<f64>) {
    match field_symbol(block, "layout").unwrap_or_default().as_str() {
        "force" => compute_force_plan(block, children),
        "radial" => compute_radial_plan(block, children),
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

/// Read a `boundary`'s `members` list into shape-id strings, resolving
/// each element the same way an edge endpoint does (identifier / utf8 /
/// ascii). Non-id elements are skipped.
fn boundary_member_ids(block: &Block<'_>) -> Vec<String> {
    let Some(field) = block.field("members") else {
        return Vec::new();
    };
    let Ok(Value::List(items)) = field.value() else {
        return Vec::new();
    };
    items.iter().filter_map(edge_endpoint_id).collect()
}

/// Collect every `boundary` block in the diagram tree (top level and
/// inside containers). Mirrors `gather_edges_recursive`: a boundary's
/// members resolve against the *global* `positions` map, so a boundary
/// nested in a container still hugs members anywhere in the diagram.
fn gather_boundaries_recursive<'a>(block: &Block<'a>, out: &mut Vec<Block<'a>>) {
    for child in diagram_children(block) {
        match child.kind() {
            "boundary" => out.push(child),
            "container" => gather_boundaries_recursive(&child, out),
            _ => {}
        }
    }
}

/// Draw every diagram `boundary` as a labelled `<rect>` sized to the
/// post-layout union bbox of its members (plus `padding`). Returns the
/// SVG (to splice *behind* the shapes) plus each box's bbox, so the
/// fit-to-viewport pass keeps a padded boundary from clipping. Like
/// `container_chrome`, the rect is raw SVG — never a `Block` — so it's
/// not an obstacle and edges cross it cleanly. A member id that resolves
/// to no shape is skipped with a non-fatal warning; a boundary whose
/// members all fail to resolve draws nothing.
pub(crate) fn render_boundaries(
    block: &Block<'_>,
    positions: &ShapePositions,
) -> (String, Vec<(f64, f64, f64, f64)>) {
    let mut boundaries: Vec<Block<'_>> = Vec::new();
    gather_boundaries_recursive(block, &mut boundaries);
    let mut out = String::new();
    let mut bboxes: Vec<(f64, f64, f64, f64)> = Vec::new();
    for b in &boundaries {
        if let Some((svg, bbox)) = render_one_boundary(b, positions) {
            out.push_str(&svg);
            bboxes.push(bbox);
        }
    }
    (out, bboxes)
}

fn render_one_boundary(
    block: &Block<'_>,
    positions: &ShapePositions,
) -> Option<(String, (f64, f64, f64, f64))> {
    let label = label_string(block).filter(|s| !s.is_empty());
    let name = label.clone().unwrap_or_else(|| "<unnamed>".to_string());
    let ids = boundary_member_ids(block);

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut found = 0usize;
    for id in &ids {
        let Some(m) = positions.get(id) else {
            crate::render::record_edge_warning(format!(
                "diagram boundary '{name}': member '{id}' matches no shape id"
            ));
            continue;
        };
        let (x, y, w, h) = m.bbox;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
        found += 1;
    }
    if found == 0 {
        crate::render::record_edge_warning(format!(
            "diagram boundary '{name}': no members resolved to a shape — nothing drawn"
        ));
        return None;
    }

    let pad = field_f64(block, "padding").unwrap_or(12.0).max(0.0);
    let x = min_x - pad;
    let y = min_y - pad;
    let w = (max_x - min_x).max(0.0) + 2.0 * pad;
    let h = (max_y - min_y).max(0.0) + 2.0 * pad;
    let bbox = (x, y, w, h);

    let mut svg = boundary_rect(block, bbox);
    if let Some(text) = label.as_deref() {
        svg.push_str(&boundary_label(block, text, bbox));
    }
    Some((svg, bbox))
}

/// The boundary's background `<rect>`. When the author sets no
/// `stroke` / `fill` / `class`, it carries the themed `wdoc-boundary`
/// class (the stylesheet supplies a translucent border). Explicit
/// `stroke` / `fill` paint inline (fill defaults to `none` so the box
/// doesn't cover its members), mirroring `container_chrome`; an explicit
/// `class` overrides the themed default.
fn boundary_rect(block: &Block<'_>, bbox: (f64, f64, f64, f64)) -> String {
    let (x, y, w, h) = bbox;
    let stroke = field_utf8(block, "stroke");
    let fill = field_utf8(block, "fill");
    let user_class = class_attr(block);
    let explicit = stroke.is_some() || fill.is_some() || !user_class.is_empty();
    let class = if explicit {
        user_class
    } else {
        classes_attr_from_names(&["wdoc-boundary".to_string()])
    };
    let mut out = format!("<rect{class} x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    if stroke.is_some() || fill.is_some() {
        append_attr(&mut out, "stroke", stroke.as_deref());
        append_attr(&mut out, "fill", Some(fill.as_deref().unwrap_or("none")));
    }
    out.push_str(" />");
    out
}

/// The boundary's title `<text>`, placed per `label_pos` (default
/// `:top_left`), inset from the box corner. Carries the themed
/// `wdoc-boundary-label` class.
fn boundary_label(block: &Block<'_>, text: &str, bbox: (f64, f64, f64, f64)) -> String {
    const INSET: f64 = 8.0;
    let (x, y, w, h) = bbox;
    let pos = field_symbol(block, "label_pos").unwrap_or_default();
    let (lx, anchor) = match pos.as_str() {
        "top" | "bottom" => (x + w / 2.0, "middle"),
        "top_right" | "bottom_right" => (x + w - INSET, "end"),
        // "top_left" (default) | "bottom_left" | unknown
        _ => (x + INSET, "start"),
    };
    let (ly, baseline) = match pos.as_str() {
        "bottom_left" | "bottom" | "bottom_right" => (y + h - INSET, "auto"),
        _ => (y + INSET, "hanging"),
    };
    let class = classes_attr_from_names(&["wdoc-boundary-label".to_string()]);
    format!(
        "<text{class} x=\"{lx}\" y=\"{ly}\" font-size=\"{fs}\" \
         text-anchor=\"{anchor}\" dominant-baseline=\"{baseline}\">{t}</text>",
        fs = crate::text::DEFAULT_FONT_SIZE,
        t = escape_html(text),
    )
}

pub(crate) fn render_grid_children(block: &Block<'_>, ctx: RenderCtx<'_>) -> String {
    let cols = field_i64(block, "columns").unwrap_or(1).max(1) as usize;
    let cw = field_f64(block, "cell_width").unwrap_or(0.0);
    let ch = field_f64(block, "cell_height").unwrap_or(0.0);
    let gap = field_f64(block, "gap").unwrap_or(0.0);
    grid_cells(block)
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
        "layered" | "force" | "radial" => {
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
            let n = grid_cells(block).len();
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
