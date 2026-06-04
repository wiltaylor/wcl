//! Edge handling: gathering `edges` records from the diagram tree,
//! selecting shared / closest anchors, planning each edge into a
//! polyline (straight or elbow-routed), and serialising to SVG. Also
//! holds the small bbox / anchor geometry helpers the edge pass needs.

use std::collections::HashMap;

use wcl_lang::{Block, Value};

use crate::routing::{self, EdgePath, Obstacle, Side};

use super::*;

/// Every edge record of a diagram/container: the `@connections`-projected
/// `a -> b` statements PLUS a computed `edges = <list>` field (data-driven
/// connections, e.g. `map`ped from the data's relationships). Both yield
/// `{ source, destination, kind? }` records. The two forms may coexist
/// (concatenated); a computed endpoint matches a shape whose `id` equals
/// the endpoint string.
pub(crate) fn all_edges(block: &Block<'_>) -> Vec<Value> {
    let mut out = Vec::new();
    // `@connections`-projected `a -> b` statements.
    if let Some(dr) = block.typed_field("edges")
        && let Ok(Value::List(items)) = dr.value()
    {
        out.extend(items);
    }
    // Computed edges: a literal `edges = <expr>` field (a list of records).
    if let Some(f) = block.field("edges")
        && let Ok(Value::List(items)) = f.value()
    {
        out.extend(items.iter().cloned());
    }
    out
}

pub(crate) fn edge_id_pairs(block: &Block<'_>) -> Vec<(String, String)> {
    all_edges(block)
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
    out.extend(all_edges(block));
    for child in diagram_children(block) {
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
    // An endpoint that names no rendered shape can't be drawn. Rather than
    // drop it silently, record a non-fatal warning naming the missing id —
    // the common cause after dynamic-id resolution is a typo. Drop the edge
    // either way (matching prior behaviour).
    let (Some(src), Some(dst)) = (positions.get(&source_id), positions.get(&dest_id)) else {
        let missing = if positions.contains_key(&source_id) {
            &dest_id
        } else {
            &source_id
        };
        crate::render::record_edge_warning(format!(
            "diagram edge {source_id} → {dest_id}: endpoint '{missing}' matches no shape id"
        ));
        return None;
    };
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
        let ca = bbox_center(&src.bbox);
        let cb = bbox_center(&dst.bbox);
        // Anchor pair gives the default endpoints (cardinal side
        // midpoints); fall back to centers when no anchors exist.
        let (mut p1, mut p2) = match pair {
            Some(((_, x1, y1), (_, x2, y2))) => ((x1, y1), (x2, y2)),
            None => (ca, cb),
        };
        // A round shape (circle / node) instead attaches on its circle
        // boundary along the center-to-center line, so the arrow points
        // radially at the node and touches its edge — not one of the
        // four cardinal anchors. Self-loops keep the anchor behaviour
        // (a radial point would collapse to the center).
        if !is_self_loop {
            if src.round {
                p1 = round_boundary_point(&src.bbox, cb);
            }
            if dst.round {
                p2 = round_boundary_point(&dst.bbox, ca);
            }
        }
        vec![p1, p2]
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
        let Some(points) =
            routing::route_elbow((sx, sy), src_side, (dx, dy), dst_side, &obstacles, viewport)
        else {
            // No obstacle-free orthogonal path exists even at zero padding —
            // the layout is too tightly packed. Record a diagnostic (surfaced
            // as a hard `BuildError`) and drop this edge rather than draw a
            // line straight through the shapes in between.
            crate::render::record_route_error(format!(
                "diagram edge {source_id} → {dest_id} could not be routed around \
                 intervening shapes — the layout is too tightly packed. Increase \
                 the diagram's spacing (node_gap / layer_gap) or size, or set \
                 routing: \"straight\"."
            ));
            return None;
        };
        points
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

/// Point on a round shape's circle boundary where a ray from the
/// center toward `target` exits. The radius mirrors the lowering of
/// `circle` / `node` (`min(w, h) / 2`), so the arrow lands exactly on
/// the drawn outline. Returns the center when `target` coincides with
/// it (a degenerate, zero-length edge).
pub(crate) fn round_boundary_point(bbox: &(f64, f64, f64, f64), target: (f64, f64)) -> (f64, f64) {
    let (cx, cy) = bbox_center(bbox);
    let r = bbox.2.min(bbox.3) / 2.0;
    let dx = target.0 - cx;
    let dy = target.1 - cy;
    let dist = dx.hypot(dy);
    if r <= 0.0 || dist <= f64::EPSILON {
        return (cx, cy);
    }
    (cx + dx / dist * r, cy + dy / dist * r)
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
