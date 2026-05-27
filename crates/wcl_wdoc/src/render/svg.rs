//! SVG diagram rendering: layout (grid / layered), edge routing, shape
//! geometry, the fundamental shape emitters, and pan/zoom.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use wcl_lang::{Block, Document, Value};

use crate::icons::{IconRegistry, ShapeOverride};
use crate::image::{self, ImageRegistry};
use crate::inline::InlinePatterns;
use crate::layered::{self, Direction};
use crate::map;
use crate::routing::{self, EdgePath, Obstacle, Side};
use crate::text;
use crate::tileset::{self, TilesetRegistry};

use super::*;

/// Per-shape geometry used by the edge pass: the absolute bounding
/// box (in diagram coords, with all container / grid translates
/// already baked in) plus the resolved list of edge-anchor points
/// the shape exposes. When the source block declares no
/// `connect_points`, the anchors default to the midpoint of each
/// bounding-box side. Each anchor records the `Side` it lives on
/// so the elbow router knows which direction the first / last leg
/// must travel.
#[derive(Clone)]
pub(crate) struct ShapeMetrics {
    bbox: (f64, f64, f64, f64),
    anchors: Vec<(Side, f64, f64)>,
}
pub(crate) type ShapePositions = HashMap<String, ShapeMetrics>;

/// Accumulator passed through the position-collection walk. The
/// `positions` map only stores id'd shapes (it's keyed by id, for
/// the edge pass to resolve `lhs -> rhs`). The `bboxes` Vec stores
/// every shape's absolute bbox regardless of id — used by the
/// fit-to-viewport pass to compute the SVG `viewBox` so content
/// always fills its declared `width` × `height`.
#[derive(Default)]
pub(crate) struct Collector {
    positions: ShapePositions,
    bboxes: Vec<(f64, f64, f64, f64)>,
}

/// Read-only context threaded through the diagram render pass. Bundles
/// the document (for lowering calls) and the two sprite registries so
/// each `render_*` shape function takes one `&RenderCtx` instead of the
/// repeated `doc` / `icons` / `tilesets` trio.
#[derive(Clone, Copy)]
pub(crate) struct RenderCtx<'a> {
    pub(crate) doc: &'a Document,
    pub(crate) icons: &'a IconRegistry,
    pub(crate) tilesets: &'a TilesetRegistry,
    pub(crate) images: &'a ImageRegistry,
    /// The full inline-pattern set + source base dir, so a `map`'s pin
    /// cards can render arbitrary wdoc content via `render_block`.
    pub(crate) patterns: &'a InlinePatterns,
    pub(crate) base_dir: Option<&'a Path>,
    /// Sink for HTML that must sit *outside* the `<svg>` (a `map`'s pin
    /// cards). `render_diagram` drains it into the viewport wrapper.
    pub(crate) overlays: &'a RefCell<Vec<String>>,
}

/// Counterpart to [`RenderCtx`] for the geometry-collection pass, which
/// produces no SVG and so needs only the registries that size a shape's
/// bbox (a `tilemap`'s sheet, an `image`'s natural dimensions).
#[derive(Clone, Copy)]
pub(crate) struct CollectCtx<'a> {
    tilesets: &'a TilesetRegistry,
    images: &'a ImageRegistry,
}

/// The bundled pan + zoom player, written to `_wdoc/` and loaded once
/// per page when any diagram sets `pan_zoom`. Mirrors the terminal
/// player asset pattern.
pub(crate) const DIAGRAM_PAN_ZOOM_JS: &str = include_str!("../../assets/diagram-pan-zoom.js");

/// The bundled map player (layer level-of-detail + popup cards), written
/// to `_wdoc/` and loaded once per page when any diagram contains a `map`.
/// Mirrors the pan/zoom + terminal player asset pattern.
pub(crate) const WDOC_MAP_JS: &str = include_str!("../../assets/wdoc-map.js");

/// `true` when `block` is an interactive (`pan_zoom`) diagram or
/// contains one anywhere in its subtree. Drives the conditional asset
/// write + per-page script injection (mirrors `terminal::uses_terminal`).
pub(crate) fn uses_pan_zoom(block: &Block<'_>) -> bool {
    (block.kind() == "diagram" && field_bool(block, "pan_zoom") == Some(true))
        || block.blocks().any(|b| uses_pan_zoom(&b))
}

/// `true` when `block` is, or contains, a `map`. Drives the map asset
/// write + script injection, and (in `render_diagram`) makes a diagram
/// holding a map interactive even without an explicit `pan_zoom`.
pub(crate) fn uses_map(block: &Block<'_>) -> bool {
    block.kind() == "map" || block.blocks().any(|b| uses_map(&b))
}

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

/// The +/−/reset control cluster overlaid on an interactive diagram.
/// The player binds the buttons by their `data-zoom` value.
pub(crate) const DIAGRAM_CONTROLS: &str = "<div class=\"wdoc-diagram-controls\">\
<button type=\"button\" data-zoom=\"in\" aria-label=\"Zoom in\">+</button>\
<button type=\"button\" data-zoom=\"out\" aria-label=\"Zoom out\">\u{2212}</button>\
<button type=\"button\" data-zoom=\"reset\" aria-label=\"Reset view\">\u{27f2}</button>\
</div>";

/// Compute the SVG viewBox that wraps every rendered shape and
/// polyline. With default `preserveAspectRatio`, this scales
/// content to fit the declared `width` × `height` while preserving
/// aspect ratio. Empty diagrams fall back to `0 0 W H`.
pub(crate) fn fit_viewbox(
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
        "layered" => collect_layered_children(block, tx, ty, cctx, out),
        _ => {
            for child in block.blocks() {
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
        "layered" => render_layered_children(block, ctx),
        _ => block
            .blocks()
            .filter_map(|b| render_shape(&b, pw, ph, ctx))
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
    for (i, child) in block.blocks().enumerate() {
        let (cx_off, cy_off) = grid_cell_offset(i, cols, cw, ch, gap);
        collect_shape_positions(&child, tx + cx_off, ty + cy_off, cw, ch, cctx, out);
    }
}

pub(crate) fn collect_layered_children(
    block: &Block<'_>,
    tx: f64,
    ty: f64,
    cctx: CollectCtx<'_>,
    out: &mut Collector,
) {
    let children: Vec<Block<'_>> = block.blocks().collect();
    let (offsets, _, _) = compute_layered_plan(block, &children);
    for (child, (cx, cy)) in children.iter().zip(offsets) {
        let pw = field_f64(child, "width").unwrap_or(80.0);
        let ph = field_f64(child, "height").unwrap_or(40.0);
        collect_shape_positions(child, tx + cx, ty + cy, pw, ph, cctx, out);
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

pub(crate) fn edge_id_pairs(block: &Block<'_>) -> Vec<(String, String)> {
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

pub(crate) fn render_layered_children(block: &Block<'_>, ctx: RenderCtx<'_>) -> String {
    let children: Vec<Block<'_>> = block.blocks().collect();
    let (offsets, widths, heights) = compute_layered_plan(block, &children);
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

pub(crate) const ARROW_MARKER: &str = "<defs><marker id=\"wdoc-arrow\" viewBox=\"0 0 10 10\" \
    refX=\"10\" refY=\"5\" markerWidth=\"8\" markerHeight=\"8\" \
    orient=\"auto-start-reverse\">\
    <path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"currentColor\" /></marker></defs>";

/// Resolve the local (pre-`tx`/`ty`) bounding box of a "box-like"
/// shape — `rect` / `circle` / `container` / `polygon` — the single
/// source of truth shared by the position collector and the icon-badge
/// placer. Returns `None` both for a malformed `polygon` and for kinds
/// each caller handles itself (`line` / `label` / `icon` / `tilemap` /
/// custom shapes), so callers fall back as appropriate.
pub(crate) fn resolve_shape_bbox(
    block: &Block<'_>,
    kind: &str,
    parent_w: f64,
    parent_h: f64,
) -> Option<(f64, f64, f64, f64)> {
    match kind {
        "rect" => Some(resolve_rect_box(block, parent_w, parent_h)),
        "circle" => {
            let (cx, cy, r) = resolve_circle(block, parent_w, parent_h);
            Some((cx - r, cy - r, 2.0 * r, 2.0 * r))
        }
        "container" => Some(resolve_container_box(block, parent_w, parent_h)),
        "polygon" => polygon_bbox(block),
        _ => None,
    }
}

/// Top-left offset of the `i`th child in a grid of `cols` columns with
/// `cw`×`ch` cells and `gap` between them. Shared by the grid render +
/// collect passes so their cell placement can't drift.
pub(crate) fn grid_cell_offset(i: usize, cols: usize, cw: f64, ch: f64, gap: f64) -> (f64, f64) {
    let col = i % cols;
    let row = i / cols;
    (col as f64 * (cw + gap), row as f64 * (ch + gap))
}

/// Walk the diagram subtree once and record each id'd shape's
/// absolute bounding box + resolved anchor positions, plus every
/// shape's bbox (id or not) for the fit-to-viewport pass. Mirrors
/// the geometry computed by `render_*` but without producing SVG.
pub(crate) fn collect_shape_positions(
    block: &Block<'_>,
    tx: f64,
    ty: f64,
    parent_w: f64,
    parent_h: f64,
    cctx: CollectCtx<'_>,
    out: &mut Collector,
) {
    let record = |block: &Block<'_>, bbox: (f64, f64, f64, f64), out: &mut Collector| {
        out.bboxes.push(bbox);
        if let Some(id) = field_id(block, "id") {
            out.positions.insert(id, build_metrics(block, bbox));
        }
    };
    match block.kind() {
        "rect" | "circle" => {
            let (x, y, w, h) = resolve_shape_bbox(block, block.kind(), parent_w, parent_h)
                .expect("rect/circle always resolve a bbox");
            record(block, (tx + x, ty + y, w, h), out);
        }
        "label" => {
            let own_x = field_f64(block, "x").unwrap_or(0.0);
            let own_y = field_f64(block, "y").unwrap_or(0.0);
            let (x, y) = resolve_point_anchored(block, parent_w, parent_h, own_x, own_y);
            record(block, (tx + x, ty + y, 0.0, 0.0), out);
        }
        "icon" => {
            // Mirror render_icon's geometry exactly (resolved box × scale)
            // so the icon contributes a correct bbox for edges + viewBox
            // fit. Read directly rather than via the `_` arm so the icon
            // name (its inline label) doesn't drive `effective_dims`.
            let (x, y, mut w, mut h) = resolve_rect_box(block, parent_w, parent_h);
            if let Some(scale) = field_f64(block, "scale") {
                w *= scale;
                h *= scale;
            }
            record(block, (tx + x, ty + y, w, h), out);
        }
        "tilemap" => {
            // The tilemap's box comes from its grid dimensions × the
            // referenced tileset's tile size; mirror render_tilemap so
            // overlays / edges and the viewBox fit see its true extent.
            let (x, y, w, h) = tileset::tilemap_bbox(block, cctx.tilesets, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
        }
        "image" => {
            // Mirror image::render_svg's geometry (declared or natural
            // size × scale, anchored) so edges + the viewBox fit see it.
            let (x, y, w, h) = image::image_bbox(block, cctx.images, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
        }
        "map" => {
            // The map's box is its declared coordinate space (width ×
            // height), positioned via the shared anchor helper — so the
            // viewBox fits the whole map and pins land in-frame.
            let (x, y, w, h) = map::map_bbox(block, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
        }
        "container" => {
            let (x, y, w, h) = resolve_shape_bbox(block, "container", parent_w, parent_h)
                .expect("container always resolves a bbox");
            record(block, (tx + x, ty + y, w, h), out);
            // Children render inside the container's padded inner
            // region — mirror that translate here so the router's
            // obstacle bboxes and the rendered SVG agree.
            let p = container_padding(block);
            let inner_w = (w - 2.0 * p).max(0.0);
            let inner_h = (h - 2.0 * p).max(0.0);
            collect_layout_children(block, tx + x + p, ty + y + p, inner_w, inner_h, cctx, out);
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
            if let Some(bbox) = resolve_shape_bbox(block, "polygon", parent_w, parent_h) {
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
pub(crate) fn effective_dims(block: &Block<'_>) -> (f64, f64) {
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
pub(crate) fn text_content(block: &Block<'_>) -> Option<String> {
    if let Some(s) = field_utf8(block, "text").or_else(|| field_utf8(block, "content")) {
        return Some(s);
    }
    label_string(block)
}

/// Parse a polygon's `points` field ("x1,y1 x2,y2 …") into a
/// bounding box relative to the block's local origin. Returns
/// `None` when the string is empty or malformed.
pub(crate) fn polygon_bbox(block: &Block<'_>) -> Option<(f64, f64, f64, f64)> {
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
pub(crate) fn build_metrics(block: &Block<'_>, bbox: (f64, f64, f64, f64)) -> ShapeMetrics {
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

pub(crate) fn anchor_point_for_side(side: Side, bbox: (f64, f64, f64, f64)) -> (f64, f64) {
    let (x, y, w, h) = bbox;
    match side {
        Side::North => (x + w / 2.0, y),
        Side::East => (x + w, y + h / 2.0),
        Side::South => (x + w / 2.0, y + h),
        Side::West => (x, y + h / 2.0),
    }
}

pub(crate) fn bbox_center(bbox: &(f64, f64, f64, f64)) -> (f64, f64) {
    (bbox.0 + bbox.2 / 2.0, bbox.1 + bbox.3 / 2.0)
}

/// `true` if `(px, py)` sits on or inside the closed bounding box.
/// Used to skip ancestor containers from the router's obstacle
/// list — the source/destination anchor sits *strictly inside* any
/// enclosing container. Boundary points (a shape's own anchor on
/// its bbox edge) do not count as contained, so a source / dest
/// shape stays an obstacle while still letting the path leave /
/// enter via its anchor cell (`astar_route` unblocks that cell).
pub(crate) fn bbox_contains(bbox: &(f64, f64, f64, f64), p: (f64, f64)) -> bool {
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
pub(crate) type SidedAnchor = (Side, f64, f64);

pub(crate) fn pick_closest_pair(
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
pub(crate) fn render_edges(
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

pub(crate) fn polyline_bbox(points: &[(f64, f64)]) -> Option<(f64, f64, f64, f64)> {
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
pub(crate) fn gather_edges_recursive(block: &Block<'_>, out: &mut Vec<Value>) {
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
pub(crate) type AnchorMap = HashMap<String, SidedAnchor>;

pub(crate) fn build_shared_anchors(
    items: &[Value],
    positions: &ShapePositions,
) -> (AnchorMap, AnchorMap) {
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

pub(crate) fn centroid_of(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    let sx: f64 = points.iter().map(|p| p.0).sum();
    let sy: f64 = points.iter().map(|p| p.1).sum();
    (sx / n, sy / n)
}

pub(crate) fn pick_anchor_toward(
    anchors: &[SidedAnchor],
    target: (f64, f64),
) -> Option<SidedAnchor> {
    anchors
        .iter()
        .min_by(|a, b| {
            let da = (a.1 - target.0).powi(2) + (a.2 - target.1).powi(2);
            let db = (b.1 - target.0).powi(2) + (b.2 - target.1).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

pub(crate) fn pick_closest_to(anchors: &[SidedAnchor], target: (f64, f64)) -> Option<SidedAnchor> {
    pick_anchor_toward(anchors, target)
}

pub(crate) fn plan_edge(
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

pub(crate) fn serialize_edge(path: &EdgePath, kind: Option<&str>, straight: bool) -> String {
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

pub(crate) fn edge_endpoint_id(v: &Value) -> Option<String> {
    match v {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

pub(crate) fn render_shape(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
    ctx: RenderCtx<'_>,
) -> Option<String> {
    let kind = block.kind();
    let base = match kind {
        "rect" => render_rect(block, parent_w, parent_h),
        "circle" => render_circle(block, parent_w, parent_h),
        "line" => render_line(block, parent_w, parent_h),
        "label" => render_label(block, parent_w, parent_h),
        "polygon" => render_polygon(block, parent_w, parent_h),
        "container" => render_container(block, parent_w, parent_h, ctx),
        // The `icon` block is itself an icon — never give it an icon badge.
        "icon" => return Some(render_icon(block, parent_w, parent_h, ctx.icons)),
        // The tilemap crops tiles out of an external spritesheet — like
        // `icon`, it's special-cased (not expressible in WCL).
        "tilemap" => {
            return Some(tileset::render_tilemap(
                block,
                ctx.tilesets,
                parent_w,
                parent_h,
            ));
        }
        // An `image` embeds an external raster as an SVG `<image>`; the
        // asset copy + path rewrite is special-cased like `tilemap`.
        "image" => return Some(image::render_svg(block, ctx.images, parent_w, parent_h)),
        // A `map` is a zoomable image + pins + popup cards — special-cased
        // like `tilemap` (its tiles, icon `<use>`s, and HTML cards aren't
        // expressible in WCL). It pushes its cards into `ctx.overlays`.
        "map" => return Some(map::render_map(block, ctx, parent_w, parent_h)),
        kind => lower_svg_block(ctx.doc, block, kind, parent_w, parent_h),
    };
    Some(with_shape_icon(
        block, kind, parent_w, parent_h, ctx.icons, base,
    ))
}

/// If `block` carries an `icon` field, draw it as a badge over the
/// shape's bounding box (default a small `:top_left` inset) and append
/// it to the already-rendered `base`. Box-like shapes (rect / circle /
/// container / process / decision / terminator, and any custom shape
/// that declares the same fields) opt in by setting `icon`; shapes
/// without a usable box (line / label) are skipped.
pub(crate) fn with_shape_icon(
    block: &Block<'_>,
    kind: &str,
    parent_w: f64,
    parent_h: f64,
    icons: &IconRegistry,
    mut base: String,
) -> String {
    let Some(name) = field_utf8(block, "icon") else {
        return base;
    };
    let Some((bx, by, bw, bh)) = shape_icon_box(block, kind, parent_w, parent_h) else {
        return base;
    };
    let size = field_f64(block, "icon_size").unwrap_or_else(|| bw.min(bh) * 0.4);
    let pos = field_symbol(block, "icon_pos").unwrap_or_else(|| "top_left".to_string());
    let (ix, iy) = place_icon(&pos, bx, by, bw, bh, size);
    let over = ShapeOverride {
        classes: field_utf8_list(block, "icon_class"),
        ..ShapeOverride::default()
    };
    if let Some(svg) = icons.resolve_shape(&name, None, (ix, iy, size, size), &over) {
        base.push_str(&svg);
    }
    base
}

/// The bounding box (in the shape's local frame) used to place an icon
/// badge. Mirrors `collect_shape_positions`'s per-kind geometry.
pub(crate) fn shape_icon_box(
    block: &Block<'_>,
    kind: &str,
    parent_w: f64,
    parent_h: f64,
) -> Option<(f64, f64, f64, f64)> {
    match kind {
        // No single box to anchor a badge to.
        "line" | "label" => None,
        "circle" | "container" | "polygon" => resolve_shape_bbox(block, kind, parent_w, parent_h),
        // rect / process / decision / terminator / custom shapes — none
        // of which `resolve_shape_bbox` claims, so resolve the box here.
        _ => Some(resolve_rect_box(block, parent_w, parent_h)),
    }
}

/// Top-left corner of an `size`×`size` badge within box `(bx,by,bw,bh)`
/// for a given `IconPos`. Corners are inset by `pad`.
pub(crate) fn place_icon(pos: &str, bx: f64, by: f64, bw: f64, bh: f64, size: f64) -> (f64, f64) {
    let pad = (bw.min(bh) * 0.1).max(0.0);
    let cx = bx + (bw - size) / 2.0;
    let cy = by + (bh - size) / 2.0;
    let right = bx + bw - size - pad;
    let bottom = by + bh - size - pad;
    match pos {
        "center" => (cx, cy),
        "top_right" => (right, by + pad),
        "bottom_left" => (bx + pad, bottom),
        "bottom_right" => (right, bottom),
        "left" => (bx + pad, cy),
        "right" => (right, cy),
        // "top_left" and any unrecognised value.
        _ => (bx + pad, by + pad),
    }
}

/// Render a diagram `icon` block: resolve its name against the icon
/// registry and emit a `<use>` of the shared sprite, sized by the
/// resolved box (× `scale`). A miss renders nothing (best-effort, like
/// a failed lowering).
pub(crate) fn render_icon(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
    icons: &IconRegistry,
) -> String {
    let name = label_string(block).unwrap_or_default();
    let set = field_id(block, "set");
    let (x, y, mut w, mut h) = resolve_rect_box(block, parent_w, parent_h);
    if let Some(scale) = field_f64(block, "scale") {
        w *= scale;
        h *= scale;
    }
    let over = ShapeOverride {
        color: field_utf8(block, "color"),
        fill: field_utf8(block, "fill"),
        background: field_utf8(block, "background"),
        classes: field_utf8_list(block, "class"),
    };
    icons
        .resolve_shape(&name, set.as_deref(), (x, y, w, h), &over)
        .unwrap_or_default()
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
    block
        .blocks()
        .enumerate()
        .filter_map(|(i, b)| {
            let rendered = render_shape(&b, cw, ch, ctx)?;
            let (tx, ty) = grid_cell_offset(i, cols, cw, ch, gap);
            Some(format!(
                "<g transform=\"translate({tx} {ty})\">{rendered}</g>"
            ))
        })
        .collect()
}

// ── Lowering dispatch ──────────────────────────────────────────────

pub(crate) fn render_rect(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let (x, y, w, h) = resolve_rect_box(block, parent_w, parent_h);
    emit_rect(
        &class_attr(block),
        x,
        y,
        w,
        h,
        field_utf8(block, "fill").as_deref(),
        field_utf8(block, "stroke").as_deref(),
        field_id(block, "id").as_deref(),
    )
}

pub(crate) fn render_circle(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let (cx, cy, r) = resolve_circle(block, parent_w, parent_h);
    emit_circle(
        &class_attr(block),
        cx,
        cy,
        r,
        field_utf8(block, "fill").as_deref(),
        field_utf8(block, "stroke").as_deref(),
        field_id(block, "id").as_deref(),
    )
}

pub(crate) fn render_line(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let x1 = field_f64(block, "x1").unwrap_or(0.0);
    let y1 = field_f64(block, "y1").unwrap_or(0.0);
    let x2 = field_f64(block, "x2").unwrap_or(0.0);
    let y2 = field_f64(block, "y2").unwrap_or(0.0);
    let (ox, oy) = resolve_point_anchor(block, parent_w, parent_h);
    emit_line(
        &class_attr(block),
        x1 + ox,
        y1 + oy,
        x2 + ox,
        y2 + oy,
        field_utf8(block, "stroke").as_deref(),
        field_id(block, "id").as_deref(),
    )
}

pub(crate) fn render_label(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
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

pub(crate) fn render_polygon(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let points = field_utf8(block, "points").unwrap_or_default();
    let (ox, oy) = resolve_point_anchor(block, parent_w, parent_h);
    emit_polygon(
        &class_attr(block),
        &points,
        ox,
        oy,
        field_utf8(block, "fill").as_deref(),
        field_utf8(block, "stroke").as_deref(),
        field_id(block, "id").as_deref(),
    )
}

// ── Fundamental renderers (variant-payload side) ──────────────────
//
// Variant payloads carry pre-resolved geometry. Anchors are not
// honored here — lowering functions are expected to emit final
// coordinates.

pub(crate) fn render_rect_payload(map: &BTreeMap<String, Value>) -> String {
    emit_rect(
        &class_attr_from_map(map),
        map_f64(map, "x").unwrap_or(0.0),
        map_f64(map, "y").unwrap_or(0.0),
        map_f64(map, "width").unwrap_or(0.0),
        map_f64(map, "height").unwrap_or(0.0),
        map_utf8(map, "fill").as_deref(),
        map_utf8(map, "stroke").as_deref(),
        map_id(map, "id").as_deref(),
    )
}

pub(crate) fn render_circle_payload(map: &BTreeMap<String, Value>) -> String {
    emit_circle(
        &class_attr_from_map(map),
        map_f64(map, "cx").unwrap_or(0.0),
        map_f64(map, "cy").unwrap_or(0.0),
        map_f64(map, "r").unwrap_or(0.0),
        map_utf8(map, "fill").as_deref(),
        map_utf8(map, "stroke").as_deref(),
        map_id(map, "id").as_deref(),
    )
}

pub(crate) fn render_line_payload(map: &BTreeMap<String, Value>) -> String {
    emit_line(
        &class_attr_from_map(map),
        map_f64(map, "x1").unwrap_or(0.0),
        map_f64(map, "y1").unwrap_or(0.0),
        map_f64(map, "x2").unwrap_or(0.0),
        map_f64(map, "y2").unwrap_or(0.0),
        map_utf8(map, "stroke").as_deref(),
        map_id(map, "id").as_deref(),
    )
}

pub(crate) fn render_label_payload(map: &BTreeMap<String, Value>) -> String {
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
pub(crate) fn resolve_label_font_size(
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
pub(crate) fn emit_text(
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

// ── Shared SVG emitters ───────────────────────────────────────────
//
// The single production site for each fundamental's SVG string. The
// block-side renderers (which resolve anchors against parent dims) and
// the variant-payload renderers (which carry pre-resolved geometry)
// both read their own source — `field_*` vs `map_*` — then hand the
// resolved primitives here, so the element markup lives in one place.

#[allow(clippy::too_many_arguments)] // cohesive <rect> attributes
pub(crate) fn emit_rect(
    cls: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: Option<&str>,
    stroke: Option<&str>,
    id: Option<&str>,
) -> String {
    let mut out = format!("<rect{cls} x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"");
    append_attr(&mut out, "fill", fill);
    append_attr(&mut out, "stroke", stroke);
    append_attr(&mut out, "id", id);
    out.push_str(" />");
    out
}

pub(crate) fn emit_circle(
    cls: &str,
    cx: f64,
    cy: f64,
    r: f64,
    fill: Option<&str>,
    stroke: Option<&str>,
    id: Option<&str>,
) -> String {
    let mut out = format!("<circle{cls} cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"");
    append_attr(&mut out, "fill", fill);
    append_attr(&mut out, "stroke", stroke);
    append_attr(&mut out, "id", id);
    out.push_str(" />");
    out
}

pub(crate) fn emit_line(
    cls: &str,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: Option<&str>,
    id: Option<&str>,
) -> String {
    let mut out = format!("<line{cls} x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"");
    append_attr(&mut out, "stroke", stroke);
    append_attr(&mut out, "id", id);
    out.push_str(" />");
    out
}

/// `points` is escaped here. A non-zero `(ox, oy)` becomes a
/// `transform="translate(ox oy)"` (the block-side anchor offset);
/// pre-resolved payloads pass `(0.0, 0.0)` and so emit no transform.
pub(crate) fn emit_polygon(
    cls: &str,
    points: &str,
    ox: f64,
    oy: f64,
    fill: Option<&str>,
    stroke: Option<&str>,
    id: Option<&str>,
) -> String {
    let mut out = format!("<polygon{cls} points=\"{}\"", escape_html(points));
    if ox != 0.0 || oy != 0.0 {
        write!(out, " transform=\"translate({ox} {oy})\"").expect("write to String");
    }
    append_attr(&mut out, "fill", fill);
    append_attr(&mut out, "stroke", stroke);
    append_attr(&mut out, "id", id);
    out.push_str(" />");
    out
}

pub(crate) fn render_polygon_payload(map: &BTreeMap<String, Value>) -> String {
    emit_polygon(
        &class_attr_from_map(map),
        &map_utf8(map, "points").unwrap_or_default(),
        0.0,
        0.0,
        map_utf8(map, "fill").as_deref(),
        map_utf8(map, "stroke").as_deref(),
        map_id(map, "id").as_deref(),
    )
}

pub(crate) fn resolve_rect_box(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
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

pub(crate) fn resolve_container_box(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
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
pub(crate) fn content_size(block: &Block<'_>) -> (f64, f64) {
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

pub(crate) fn apply_axis_anchor(
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

pub(crate) fn resolve_circle(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64, f64) {
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

pub(crate) fn resolve_point_anchor(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64) {
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

pub(crate) fn resolve_point_anchored(
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
