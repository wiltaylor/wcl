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
        out.extend(std::sync::Arc::unwrap_or_clone(items));
    }
    // Computed edges: a literal `edges = <expr>` field (a list of records).
    if let Some(f) = block.field("edges")
        && let Ok(Value::List(items)) = f.value()
    {
        out.extend(items.iter().cloned());
    }
    out
}

/// The `(source, destination)` id pairs every edge in this block
/// connects.
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
    borders: &[(f64, f64, f64, f64)],
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
    // each picking its own closest anchor independently. This forms a
    // clean branching trunk for `:elbow` routing — but for `:straight`
    // routing convergence to one point IS the defect (every spoke
    // would leave the same side and cross the shape body), so straight
    // edges skip it and each picks its own facing anchor below.
    let (source_overrides, dest_overrides) = if straight {
        (AnchorMap::new(), DestAnchorMap::new())
    } else {
        build_shared_anchors(&items, positions)
    };

    let mut planned: Vec<(EdgePath, EdgeStyle)> = Vec::new();
    for item in &items {
        if let Some(plan) = plan_edge(
            item,
            positions,
            borders,
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
    for (path, style) in planned {
        if let Some(bbox) = polyline_bbox(&path.points) {
            bboxes.push(bbox);
        }
        // The label contributes to the fit too — a label wider than the
        // shapes around it would otherwise clip at the viewBox edge.
        if let Some(label) = style.label.as_deref()
            && let Some((x, y)) = edge_label_point(&path.points, label)
        {
            let (w, h) = edge_label_extent(label);
            bboxes.push((x - w / 2.0, y - h / 2.0, w, h));
        }
        out.push_str(&serialize_edge(&path, &style, straight));
    }
    (out, bboxes)
}

/// Estimated rendered extent of an edge label (the midpoint `<text>` at
/// font-size 11, anchor middle).
pub(crate) fn edge_label_extent(label: &str) -> (f64, f64) {
    const FONT: f64 = 11.0;
    (
        label.chars().count() as f64 * FONT * crate::text::CHAR_RATIO,
        FONT * crate::text::LINE_HEIGHT,
    )
}

/// The text a connection kind renders as, or `None` when the kind is purely
/// presentational. Only the answer-shaped kinds label themselves.
fn kind_label(kind: &str) -> Option<&'static str> {
    match kind {
        "yes" => Some("yes"),
        "no" => Some("no"),
        _ => None,
    }
}

/// Per-edge presentation read off the edge record. `kind` comes from
/// the `a -> b : kind` connection syntax or a computed record's `kind`
/// field; `label` / `dash` are reachable only through computed
/// `edges = [...]` records today — the `->` statement grammar carries
/// no payload beyond the kind symbol.
pub(crate) struct EdgeStyle {
    /// Edge kind, when the statement names one.
    pub(crate) kind: Option<String>,
    /// Text drawn along the edge.
    pub(crate) label: Option<String>,
    /// Dash pattern, when the style asks for one.
    pub(crate) dash: Option<String>,
}

/// Bounding box of a polyline; `None` when it has no points.
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
/// appears as the source of multiple edges that face the *same* side,
/// we pick one shared egress anchor (the one closest to the centroid
/// of those destinations' bbox centers) so they converge into a clean
/// branching trunk. Crucially the grouping is **per facing side**, not
/// per shape: edges radiating in opposing directions (a radial hub's
/// spokes) land in different side-groups and so are *not* bundled —
/// each falls back to its own natural facing anchor via
/// `pick_closest_pair`. Self-loops are excluded. A `(shape, side)`
/// group with only one edge gets no override.
///
/// Destinations get the opposite treatment: multiple edges *arriving*
/// on one side are SPREAD into per-edge slots along that side instead
/// of converging — arrowheads stacked on one point read as a single
/// edge (an A→B→C chain's through-edge A→C would vanish under its
/// neighbours at both ends). The dest map is therefore keyed by the
/// arriving edge's source id as well.
pub(crate) type AnchorMap = HashMap<(String, Side), SidedAnchor>;
/// Anchor chosen for each `(shape, side, destination)` triple, so
/// edges sharing an endpoint stay aligned with one another.
pub(crate) type DestAnchorMap = HashMap<(String, Side, String), SidedAnchor>;

/// Gap between neighbouring arrival slots on a destination side.
const ARRIVAL_SPREAD: f64 = 12.0;

/// Dominant cardinal direction from `from` toward `to` (SVG y grows
/// downward). Used both to group a shape's edges into facing-side
/// buckets here and to look the resulting override back up per edge in
/// `plan_edge` — computing it the same way in both places keeps the
/// grouping key and the lookup key in lockstep.
pub(crate) fn facing_side(from: (f64, f64), to: (f64, f64)) -> Side {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 { Side::East } else { Side::West }
    } else if dy >= 0.0 {
        Side::South
    } else {
        Side::North
    }
}

/// One edge arriving at a destination group: its source shape id and
/// that shape's bbox center.
type Arrival = (String, (f64, f64));

/// Choose one anchor per shape side shared by several edges, so they
/// fan from a common point rather than crossing.
pub(crate) fn build_shared_anchors(
    items: &[Value],
    positions: &ShapePositions,
) -> (AnchorMap, DestAnchorMap) {
    let mut src_targets: HashMap<(String, Side), Vec<(f64, f64)>> = HashMap::new();
    let mut dst_sources: HashMap<(String, Side), Vec<Arrival>> = HashMap::new();
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
        let (Some(s_metrics), Some(d_metrics)) = (positions.get(&s), positions.get(&d)) else {
            continue;
        };
        let s_center = bbox_center(&s_metrics.bbox);
        let d_center = bbox_center(&d_metrics.bbox);
        // Group each edge under the side of its own shape that faces
        // the other endpoint, so only co-directional edges converge.
        src_targets
            .entry((s.clone(), facing_side(s_center, d_center)))
            .or_default()
            .push(d_center);
        dst_sources
            .entry((d, facing_side(d_center, s_center)))
            .or_default()
            .push((s, s_center));
    }
    let mut sources = AnchorMap::new();
    let mut dests = DestAnchorMap::new();
    for ((id, side), targets) in src_targets {
        if targets.len() < 2 {
            continue;
        }
        let Some(metrics) = positions.get(&id) else {
            continue;
        };
        let centroid = centroid_of(&targets);
        if let Some(anchor) = pick_anchor_toward(&metrics.anchors, centroid) {
            sources.insert((id, side), anchor);
        }
    }
    for ((id, side), mut arrivals) in dst_sources {
        if arrivals.len() < 2 {
            continue;
        }
        let Some(metrics) = positions.get(&id) else {
            continue;
        };
        let centers: Vec<(f64, f64)> = arrivals.iter().map(|(_, c)| *c).collect();
        let centroid = centroid_of(&centers);
        let Some(base) = pick_anchor_toward(&metrics.anchors, centroid) else {
            continue;
        };
        // One slot per arriving edge, centred on the base anchor and
        // spread along the side's tangent axis (clamped inside the
        // shape's extent), ordered by source id for determinism.
        arrivals.sort_by(|a, b| a.0.cmp(&b.0));
        let k = arrivals.len();
        let (bx, by, bw, bh) = metrics.bbox;
        for (i, (src_id, _)) in arrivals.into_iter().enumerate() {
            let off = (i as f64 - (k as f64 - 1.0) / 2.0) * ARRIVAL_SPREAD;
            let (_, ax, ay) = base;
            let slot = match base.0 {
                // Vertical sides spread along y, horizontal along x.
                Side::East | Side::West => (base.0, ax, (ay + off).clamp(by + 8.0, by + bh - 8.0)),
                Side::North | Side::South => {
                    (base.0, (ax + off).clamp(bx + 8.0, bx + bw - 8.0), ay)
                }
            };
            dests.insert((id.clone(), side, src_id), slot);
        }
    }
    (sources, dests)
}

/// Average of a point set.
pub(crate) fn centroid_of(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    let sx: f64 = points.iter().map(|p| p.0).sum();
    let sy: f64 = points.iter().map(|p| p.1).sum();
    (sx / n, sy / n)
}

/// Choose the anchor facing a target point.
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

/// Choose the anchor nearest a target point; `None` when the shape
/// offers none.
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

/// Choose the anchor pair minimising the distance between two shapes.
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

/// Plan one edge: pick its anchors, then route between them.
pub(crate) fn plan_edge(
    value: &Value,
    positions: &ShapePositions,
    borders: &[(f64, f64, f64, f64)],
    viewport: (f64, f64),
    straight: bool,
    source_overrides: &AnchorMap,
    dest_overrides: &DestAnchorMap,
) -> Option<(EdgePath, EdgeStyle)> {
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
    // edges converging at the same shape end at the same point. The
    // override is keyed by the facing side toward the other endpoint —
    // computed exactly as in `build_shared_anchors` so this lookup and
    // that grouping agree by construction.
    let src_center = bbox_center(&src.bbox);
    let dst_center = bbox_center(&dst.bbox);
    let src_override = source_overrides
        .get(&(source_id.clone(), facing_side(src_center, dst_center)))
        .copied();
    let dst_override = dest_overrides
        .get(&(
            dest_id.clone(),
            facing_side(dst_center, src_center),
            source_id.clone(),
        ))
        .copied();
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
    // A `->` statement can carry a kind but no label, so a labelling kind
    // (anything but the neutral `:default`/`:flow`/`:data`) doubles as the
    // label. An explicit `label` on a record form always wins.
    let label = fields
        .get("label")
        .and_then(edge_endpoint_id)
        .or_else(|| kind.as_deref().and_then(kind_label).map(str::to_string));
    let style = EdgeStyle {
        kind,
        label,
        dash: fields.get("dash").and_then(edge_endpoint_id),
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
        let Some(points) = routing::route_elbow(
            (sx, sy),
            src_side,
            (dx, dy),
            dst_side,
            &obstacles,
            borders,
            viewport,
        ) else {
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
    Some((EdgePath { points }, style))
}

/// Emit an edge as SVG — a straight line or a routed polyline.
pub(crate) fn serialize_edge(path: &EdgePath, style: &EdgeStyle, straight: bool) -> String {
    let kind_attr = match style.kind.as_deref() {
        Some(k) => format!(" data-kind=\"{}\"", escape_html(k)),
        None => String::new(),
    };
    let dash_attr = match style.dash.as_deref() {
        Some(d) => format!(" stroke-dasharray=\"{}\"", escape_html(d)),
        None => String::new(),
    };
    let mut out = if straight && path.points.len() == 2 {
        let (x1, y1) = path.points[0];
        let (x2, y2) = path.points[1];
        format!(
            "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" \
             stroke=\"currentColor\" marker-end=\"url(#wdoc-arrow)\"{dash_attr}{kind_attr} />"
        )
    } else {
        let points: Vec<String> = path
            .points
            .iter()
            .map(|(x, y)| format!("{x},{y}"))
            .collect();
        format!(
            "<polyline points=\"{}\" fill=\"none\" \
             stroke=\"currentColor\" marker-end=\"url(#wdoc-arrow)\"{dash_attr}{kind_attr} />",
            points.join(" ")
        )
    };
    if let Some(label) = style.label.as_deref()
        && let Some((x, y)) = edge_label_point(&path.points, label)
    {
        out.push_str(&format!(
            "<text class=\"wdoc-edge-label\" x=\"{x}\" y=\"{y}\" \
             text-anchor=\"middle\" dominant-baseline=\"middle\" \
             font-size=\"11\">{}</text>",
            escape_html(label)
        ));
    }
    out
}

/// Label anchor for an edge: the polyline's arc-length midpoint, nudged
/// along the local normal (flipped to sit above a mostly-horizontal run
/// and left of a mostly-vertical one) far enough for the *whole*
/// anchor-middle text to clear the stroke — half the label's extent
/// along the normal direction, plus a fixed gap. A fixed 8px nudge left
/// vertical-edge labels straddling the line.
pub(crate) fn edge_label_point(points: &[(f64, f64)], label: &str) -> Option<(f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let total: f64 = points
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .sum();
    if total <= f64::EPSILON {
        return Some(points[0]);
    }
    let mut remaining = total / 2.0;
    for w in points.windows(2) {
        let (dx, dy) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
        let seg = dx.hypot(dy);
        if seg < remaining || seg <= f64::EPSILON {
            remaining -= seg;
            continue;
        }
        let t = remaining / seg;
        let (mx, my) = (w[0].0 + dx * t, w[0].1 + dy * t);
        // Unit normal, flipped toward "up / left" so the label sits on
        // the same side regardless of segment direction.
        let (mut nx, mut ny) = (-dy / seg, dx / seg);
        if ny > 0.0 || (ny == 0.0 && nx > 0.0) {
            nx = -nx;
            ny = -ny;
        }
        // Clearance: half of the label's extent projected onto the
        // normal (width for a horizontal normal, height for a vertical
        // one), plus a 4px gap off the stroke.
        let (w_ext, h_ext) = edge_label_extent(label);
        let clear = (nx.abs() * w_ext / 2.0) + (ny.abs() * h_ext / 2.0) + 4.0;
        return Some((mx + nx * clear, my + ny * clear));
    }
    Some(points[points.len() - 1])
}

/// Read an edge endpoint as a shape id.
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

/// The midpoint of one side of a bounding box.
pub(crate) fn anchor_point_for_side(side: Side, bbox: (f64, f64, f64, f64)) -> (f64, f64) {
    let (x, y, w, h) = bbox;
    match side {
        Side::North => (x + w / 2.0, y),
        Side::East => (x + w, y + h / 2.0),
        Side::South => (x + w / 2.0, y + h),
        Side::West => (x, y + h / 2.0),
    }
}

/// Centre of a bounding box.
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
