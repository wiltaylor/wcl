//! Per-kind shape dispatch (`render_shape`), the geometry-collection
//! walk (`collect_shape_positions` + the bbox/metrics helpers it shares
//! with the icon-badge placer), and the icon-badge / diagram-`icon`
//! rendering.

use wcl_lang::Block;

use crate::dopesheet;
use crate::icons::{IconRegistry, ShapeOverride};
use crate::image;
use crate::map;
use crate::text;
use crate::tileset;

use super::*;

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
        // A `card` is a positioned box (anchor-aware, like `rect`); read
        // it directly so its bbox feeds viewBox-fit + edge anchoring
        // without `effective_dims` (it has no text-driven growth).
        "card" => {
            let (x, y, w, h) = resolve_rect_box(block, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
        }
        // A `node_table` registers its whole-box anchors *and* each id'd
        // row as its own sub-shape — so an edge can target a single row
        // (`fk -> users_id`) and the standard anchor logic lands it on the
        // row's west/east edge. The row's exposed sides come from
        // `node_table::row_sides` (the same source the markers use), so the
        // anchor and its visible marker always coincide.
        "node_table" => {
            let (x, y, w, h) = crate::node_table::node_table_bbox(block, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
            for (row, (rx, ry, rw, rh)) in crate::node_table::row_boxes(block, x, y, w) {
                let abs = (tx + rx, ty + ry, rw, rh);
                out.bboxes.push(abs);
                if let Some(id) = field_id(&row, "id") {
                    let anchors = crate::node_table::row_sides(&row)
                        .iter()
                        .filter_map(|s| {
                            let side = Side::from_symbol(s)?;
                            let (ax, ay) = anchor_point_for_side(side, abs);
                            Some((side, ax, ay))
                        })
                        .collect();
                    out.positions.insert(
                        id,
                        ShapeMetrics {
                            bbox: abs,
                            anchors,
                            round: false,
                        },
                    );
                }
            }
        }
        // A `tree` registers its whole-box anchors *and* each id'd node as
        // its own sub-shape — so an edge can target a single node and the
        // standard west/east anchor logic lands it on the node's row.
        "tree" => {
            let (x, y, w, h) = crate::tree::tree_bbox(block, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
            for (node, (rx, ry, rw, rh)) in crate::tree::node_rows(block, x, y, w) {
                let abs = (tx + rx, ty + ry, rw, rh);
                out.bboxes.push(abs);
                if let Some(id) = field_id(&node, "id") {
                    let anchors = ["west", "east"]
                        .iter()
                        .filter_map(|s| {
                            let side = Side::from_symbol(s)?;
                            let (ax, ay) = anchor_point_for_side(side, abs);
                            Some((side, ax, ay))
                        })
                        .collect();
                    out.positions.insert(
                        id,
                        ShapeMetrics {
                            bbox: abs,
                            anchors,
                            round: false,
                        },
                    );
                }
            }
        }
        "label" => {
            let own_x = field_f64(block, "x").unwrap_or(0.0);
            let own_y = field_f64(block, "y").unwrap_or(0.0);
            let (x, y) = resolve_point_anchored(block, parent_w, parent_h, own_x, own_y);
            record(block, (tx + x, ty + y, 0.0, 0.0), out);
        }
        // A `boundary` is a post-layout overlay sized to its members'
        // bboxes — it has no box of its own here. Record nothing (no
        // bbox, no position): it's not a member of itself, and it must
        // not appear in `positions` for the edge pass. The actual rect
        // is computed by `render_boundaries` once `positions` is filled.
        "boundary" => {}
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
        "dopesheet" => {
            // A single frame's display size (frame_width/height × scale),
            // anchored — mirror render_dopesheet so edges + the viewBox
            // fit see its true extent.
            let (x, y, w, h) = dopesheet::dopesheet_bbox(block, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
        }
        "map" => {
            // The map's box is its declared coordinate space (width ×
            // height), positioned via the shared anchor helper — so the
            // viewBox fits the whole map and pins land in-frame.
            let (x, y, w, h) = map::map_bbox(block, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
        }
        // A wireframe widget: `x`/`y` from anchors, size from the measured
        // widget tree — mirror render_wireframe_shape so edges can target it
        // and the viewBox fits it. (doc-free; size is theme-independent.)
        kind if crate::wireframe::is_wireframe_kind(kind) => {
            let (x, y, w, h) = crate::wireframe::wireframe_bbox(block, parent_w, parent_h);
            record(block, (tx + x, ty + y, w, h), out);
        }
        "container" => {
            let (x, y, w, h) = resolve_shape_bbox(block, "container", parent_w, parent_h)
                .expect("container always resolves a bbox");
            record(block, (tx + x, ty + y, w, h), out);
            // Record the box for the router's border-avoidance only when the
            // container actually draws a border (mirrors `container_chrome`'s
            // gate) — an invisible grouping box has no line to merge into.
            if field_utf8(block, "stroke").is_some() || field_utf8(block, "fill").is_some() {
                out.containers.push((tx + x, ty + y, w, h));
            }
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
    // A wireframe widget is sized by its measured content, so a layout
    // solver (`:layered` / `:force`) allocates a correct cell instead of
    // the default 80×40. Measurement is doc-free (size is theme-independent).
    if crate::wireframe::is_wireframe_kind(block.kind()) {
        return crate::wireframe::measured_size(block);
    }
    // A circle is sized by its diameter (it carries `r`, not
    // width/height), so a layout solver allocates a correct square
    // cell for it instead of the default 80×40.
    if block.kind() == "circle"
        && let Some(r) = field_f64(block, "r")
    {
        return (2.0 * r, 2.0 * r);
    }
    // A container is sized by its laid-out children — the same box the
    // renderer draws (`resolve_container_box`): the content bbox plus
    // `padding` on every side. Reporting that footprint to the layout
    // solver makes it reserve the container's true extent instead of
    // the default 80×40, so a sibling on the next rank / breadth slot
    // no longer overlaps the border. Falls through to declared /
    // default dims for a `:none` or empty container (content_size → 0).
    if block.kind() == "container" {
        let (content_w, content_h) = content_size(block);
        if content_w > 0.0 || content_h > 0.0 {
            let pad = 2.0 * container_padding(block);
            // Honor a declared width/height as a minimum, matching
            // `resolve_container_box` (never a ceiling).
            let decl_w = field_f64(block, "width").unwrap_or(0.0);
            let decl_h = field_f64(block, "height").unwrap_or(0.0);
            return ((content_w + pad).max(decl_w), (content_h + pad).max(decl_h));
        }
    }
    // A node_table has no `height` field — its height is derived (header +
    // one row_height per `node_row`). Report that true extent so a layout
    // solver reserves the box's real footprint instead of the default 40px,
    // preventing the next rank from overlapping it. (`width` is honored by
    // node_table_bbox via resolve_rect_box; parent_w/parent_h = 0 is fine
    // for the absolute-`width` case these diagrams use.)
    if block.kind() == "node_table" {
        let (_, _, w, h) = crate::node_table::node_table_bbox(block, 0.0, 0.0);
        return (w, h);
    }
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
    // The stdlib's round shapes: the `circle` fundamental and the
    // `node` graph shape (which lowers to a circle of radius
    // min(w,h)/2). Edges to these attach on the circle boundary.
    let round = matches!(block.kind(), "circle" | "node");
    ShapeMetrics {
        bbox,
        anchors,
        round,
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
        // A `boundary` is drawn behind the shapes by `render_boundaries`
        // (it needs the post-layout `positions` map), never in the normal
        // shape flow — emit nothing here.
        "boundary" => return None,
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
        // A `dopesheet` windows an animation frame out of a sprite sheet
        // and is driven by a bundled JS player — special-cased like
        // `tilemap` (it reuses the image registry to copy the sheet).
        "dopesheet" => {
            return Some(dopesheet::render_dopesheet(
                block, ctx.images, parent_w, parent_h,
            ));
        }
        // A `node_table` is a titled box of rows whose bodies are HTML
        // (foreignObject) and whose rows expose per-row connection points
        // — special-cased like `card`.
        "node_table" => {
            return Some(crate::node_table::render_node_table(
                block, ctx, parent_w, parent_h,
            ));
        }
        // A `tree` is an indented file-tree: one row per node, with
        // connector guides + icons + labels drawn as SVG primitives —
        // special-cased like `node_table` (the indent / connector math
        // isn't expressible in WCL).
        "tree" => {
            return Some(crate::tree::render_tree(block, ctx, parent_w, parent_h));
        }
        // An `image` embeds an external raster as an SVG `<image>`; the
        // asset copy + path rewrite is special-cased like `tilemap`.
        "image" => return Some(image::render_svg(block, ctx.images, parent_w, parent_h)),
        // A `map` is a zoomable image + pins + popup cards — special-cased
        // like `tilemap` (its tiles, icon `<use>`s, and HTML cards aren't
        // expressible in WCL). It pushes its cards into `ctx.overlays`.
        "map" => return Some(map::render_map(block, ctx, parent_w, parent_h)),
        // A `card` holds rich wdoc content drawn as an HTML
        // `<foreignObject>` — special-cased like `map` because the body
        // comes from the block renderer + inline engine, not WCL.
        "card" => return Some(crate::card::render_card(block, ctx, parent_w, parent_h)),
        // A `timeline` lowers to SVG fundamentals in WCL, but its event
        // `card` children render as `<foreignObject>`s in Rust — so it's
        // special-cased to thread the `RenderCtx` through.
        "timeline" => {
            return Some(crate::timeline::render_timeline(
                block, ctx, parent_w, parent_h,
            ));
        }
        // The wireframe widgets (`wf_window`, `wf_button`, …) are special-
        // cased like `card`: the whole family measures + lays out and emits
        // one positioned `<g>`, so it returns directly (no `with_shape_icon`,
        // which would clash with `wf_button`'s own `icon`).
        kind if crate::wireframe::is_wireframe_kind(kind) => {
            return Some(crate::wireframe::render_wireframe_shape(
                block, ctx, parent_w, parent_h,
            ));
        }
        kind => lower_svg_block(ctx.doc, block, kind, parent_w, parent_h, Some(ctx.patterns)),
    };
    let svg = with_shape_icon(block, kind, parent_w, parent_h, ctx.icons, base);
    Some(wrap_shape_link(block, ctx, svg))
}

/// Wrap a shape's SVG in an `<a href>` when it carries a `link` (a page
/// name / href resolved by the inline link resolver, so an unknown page
/// raises the same build error as a prose link). With no `link` the SVG
/// passes through unchanged, so unlinked shapes render exactly as before.
///
/// Note: SVG/HTML can't nest `<a>` inside `<a>`. A `container` carrying a
/// `link` wraps its children's SVG, so authors should link a container's
/// title `label` rather than both the container and its linked children.
fn wrap_shape_link(block: &Block<'_>, ctx: RenderCtx<'_>, svg: String) -> String {
    match field_utf8(block, "link") {
        Some(link) => format!(
            "<a href=\"{}\">{}</a>",
            escape_html(&ctx.patterns.resolve_href(&link)),
            svg
        ),
        None => svg,
    }
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
    // Default to a leading badge (`:left`, vertically centred): the
    // label-bearing flowchart shapes reserve a matching left strip so
    // the icon and the centred text sit side by side instead of
    // overlapping (see `icon_lead` in lib/flowchart.wcl). `:left` also
    // keeps the badge inside a rounded / oval outline, where a corner
    // would poke past the curve.
    let pos = field_symbol(block, "icon_pos").unwrap_or_else(|| "left".to_string());
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
