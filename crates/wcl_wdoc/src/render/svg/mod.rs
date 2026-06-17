//! SVG diagram rendering: layout (grid / layered), edge routing, shape
//! geometry, the fundamental shape emitters, and pan/zoom.
//!
//! Split into focused submodules behind this re-exporting `mod`:
//! [`diagram`] (the top-level render + layout planning), [`edges`] (edge
//! gathering, anchor selection, and elbow routing), [`shapes`] (per-kind
//! shape dispatch, position collection, and icon badges), and
//! [`primitives`] (geometry resolution + the fundamental SVG emitters).
//! The shared geometry types + bundled-asset consts live here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use wcl_lang::{Block, Document};

use crate::icons::IconRegistry;
use crate::image::ImageRegistry;
use crate::inline::InlinePatterns;
use crate::routing::Side;
use crate::tileset::TilesetRegistry;

use super::*;

mod diagram;
mod edges;
mod primitives;
mod shapes;
mod standalone;

pub(crate) use diagram::*;
pub(crate) use edges::*;
pub(crate) use primitives::*;
pub(crate) use shapes::*;
pub(crate) use standalone::*;

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
    pub(crate) bbox: (f64, f64, f64, f64),
    pub(crate) anchors: Vec<(Side, f64, f64)>,
    /// Whether the shape renders as a circle (`circle` / `node`). Round
    /// shapes attach straight edges at their boundary along the
    /// center-to-center line rather than at a cardinal anchor point.
    pub(crate) round: bool,
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
    pub(crate) positions: ShapePositions,
    pub(crate) bboxes: Vec<(f64, f64, f64, f64)>,
    /// Absolute boxes of *visibly bordered* containers (those with a
    /// `stroke` / `fill` chrome rect). The elbow router penalises routing
    /// flush along these so an edge doesn't merge into a boundary line.
    pub(crate) containers: Vec<(f64, f64, f64, f64)>,
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
    pub(crate) tilesets: &'a TilesetRegistry,
    pub(crate) images: &'a ImageRegistry,
}

/// The bundled pan + zoom player, written to `_wdoc/` and loaded once
/// per page when any diagram sets `pan_zoom`. Mirrors the terminal
/// player asset pattern.
pub(crate) const DIAGRAM_PAN_ZOOM_JS: &str = include_str!("../../../assets/diagram-pan-zoom.js");

/// The bundled map player (layer level-of-detail + popup cards), written
/// to `_wdoc/` and loaded once per page when any diagram contains a `map`.
/// Mirrors the pan/zoom + terminal player asset pattern.
pub(crate) const WDOC_MAP_JS: &str = include_str!("../../../assets/wdoc-map.js");

/// The bundled dopesheet player (advances the frame `viewBox` at the
/// authored fps), written to `_wdoc/` and loaded once per page when any
/// diagram contains a `dopesheet`. Mirrors the terminal player pattern.
pub(crate) const DOPESHEET_PLAYER_JS: &str = include_str!("../../../assets/dopesheet-player.js");

/// The bundled video player (click-to-play facade → real `<video>` /
/// `<iframe>`), written to `_wdoc/` and loaded once per page when any
/// `video` block is present. Mirrors the dopesheet/terminal player pattern.
pub(crate) const WDOC_VIDEO_JS: &str = include_str!("../../../assets/wdoc-video.js");

/// The bundled presentation (deck) navigation player, written to
/// `_wdoc/` and loaded on a `presentation`-template site's single deck
/// page. Drives the arrow-key slide grid. Mirrors the player pattern.
pub(crate) const PRESENTATION_PLAYER_JS: &str = include_str!("../../../assets/presentation.js");

/// The +/−/reset control cluster overlaid on an interactive diagram.
/// The player binds the buttons by their `data-zoom` value.
pub(crate) const DIAGRAM_CONTROLS: &str = "<div class=\"wdoc-diagram-controls\">\
<button type=\"button\" data-zoom=\"in\" aria-label=\"Zoom in\">+</button>\
<button type=\"button\" data-zoom=\"out\" aria-label=\"Zoom out\">\u{2212}</button>\
<button type=\"button\" data-zoom=\"reset\" aria-label=\"Reset view\">\u{27f2}</button>\
</div>";

pub(crate) const ARROW_MARKER: &str = "<defs><marker id=\"wdoc-arrow\" viewBox=\"0 0 10 10\" \
    refX=\"10\" refY=\"5\" markerWidth=\"8\" markerHeight=\"8\" \
    orient=\"auto-start-reverse\">\
    <path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"currentColor\" /></marker></defs>";

/// `true` when `block` is an interactive (`pan_zoom`) diagram or
/// contains one anywhere in its subtree. Drives the conditional asset
/// write + per-page script injection (mirrors `terminal::uses_terminal`).
pub(crate) fn uses_pan_zoom(block: &Block<'_>) -> bool {
    crate::render::block_tree_any(block, &|b| {
        b.kind() == "diagram" && field_bool(b, "pan_zoom") == Some(true)
    })
}

/// `true` when `block` is, or contains, a `map`. Drives the map asset
/// write + script injection, and (in `render_diagram`) makes a diagram
/// holding a map interactive even without an explicit `pan_zoom`.
pub(crate) fn uses_map(block: &Block<'_>) -> bool {
    crate::render::block_tree_any(block, &|b| b.kind() == "map")
}

/// Top-left offset of the `i`th child in a grid of `cols` columns with
/// `cw`×`ch` cells and `gap` between them. Shared by the grid render +
/// collect passes so their cell placement can't drift.
pub(crate) fn grid_cell_offset(i: usize, cols: usize, cw: f64, ch: f64, gap: f64) -> (f64, f64) {
    let col = i % cols;
    let row = i / cols;
    (col as f64 * (cw + gap), row as f64 * (ch + gap))
}

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
