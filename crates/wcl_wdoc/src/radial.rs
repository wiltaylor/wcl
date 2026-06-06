//! Radial (hub-and-spoke) auto-layout for diagram and container children.
//!
//! Places one node — the *hub* — at the centre and arranges every other
//! node on concentric rings around it, ranked by graph distance from the
//! hub. The hub is the explicitly-named `hub` id, else the highest-degree
//! node in the `@connections(Edge)` graph (ties broken by source order),
//! else the first child. A node's ring is its shortest-path (BFS) distance
//! from the hub; nodes unreachable from the hub land on the outermost ring.
//!
//! This suits the "X and everything it talks to" shape — a focus entity
//! wired to many neighbours — where the layered solver would strand the
//! edge-less neighbours in a single line and the force solver gives a less
//! predictable spread. Each ring spaces its nodes evenly by angle, with a
//! radius wide enough that the boxes never overlap.
//!
//! Like the other solvers this only assigns positions; per-shape `width` /
//! `height` are read from the schema and honored as the node size
//! (defaulting to 80×40 upstream when both are omitted). The function is a
//! pure, deterministic function of its inputs — no RNG, fixed iteration
//! order — so the collect and render passes recompute byte-identical
//! layouts. Returned offsets are normalized so the children's bounding box
//! starts at `(0, 0)`.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use crate::layered::Node;

/// Tunable knobs for the radial layout, surfaced on `diagram` /
/// `container` as optional WCL fields. Defaults give a readable ring for
/// typical diagram-scale fan-outs (a handful of neighbours).
pub(crate) struct RadialParams {
    /// Explicit hub id. When `None`, the hub is the highest-degree node
    /// (ties → first in source order), falling back to the first child.
    pub hub: Option<String>,
    /// Radius of the first ring, edge-to-edge from the hub. When `None`
    /// it is derived from the ring's node sizes so boxes never overlap.
    pub radius: Option<f64>,
    /// Added radius per successive ring (ring 2, 3, …).
    pub ring_gap: f64,
    /// Angle (radians) of the first node in each ring. Defaults to
    /// `-PI/2` so the first node sits at the top, going clockwise.
    pub start_angle: f64,
    /// Minimum gap, edge-to-edge, between neighbouring nodes on a ring.
    pub node_gap: f64,
    /// Per-node extra half-extent (SVG units) added to a node's box radius
    /// purely for *clearance* math — never its drawn size. Lets a node wrapped
    /// by a post-layout `boundary` (which inflates its visible footprint by the
    /// boundary `padding` per side) seat ring neighbours outside the boundary
    /// rather than flush against it. Indexed parallel to `nodes`; short/empty ⇒ 0.
    pub inflation: Vec<f64>,
}

impl Default for RadialParams {
    fn default() -> Self {
        RadialParams {
            hub: None,
            radius: None,
            ring_gap: 120.0,
            start_angle: -PI / 2.0,
            node_gap: 24.0,
            inflation: Vec::new(),
        }
    }
}

/// Compute per-child `(tx, ty)` offsets via radial placement. `edges` are
/// `(src_id, dst_id)` pairs; edges whose endpoints don't both name a child
/// `id` in `nodes` are dropped (they belong to a parent scope, exactly as
/// in the layered / force solvers). Returned offsets are normalized so the
/// children's bounding box starts at `(0, 0)`.
pub(crate) fn assign_radial_offsets(
    nodes: &[Node],
    edges: &[(String, String)],
    params: RadialParams,
) -> Vec<(f64, f64)> {
    let n = nodes.len();
    if n == 0 {
        return Vec::new();
    }

    // Per-node "breadth" used both for repulsion-free spacing along a ring
    // and for the centre→top-left conversion. The diagonal half-extent
    // keeps differently-shaped boxes from touching at any rotation.
    let radius_of: Vec<f64> = nodes
        .iter()
        .map(|node| node.size.0.hypot(node.size.1) / 2.0)
        .collect();
    // Clearance-only inflation (e.g. a `boundary` padding the hub's drawn
    // footprint). Added to a node's `radius_of` for ring-radius math, never
    // to its drawn size. Inflating by the raw per-side padding is
    // deliberately conservative: `radius_of` is already the half-*diagonal*,
    // so it over-clears axis-aligned boxes, and radial spokes run along the
    // cardinal directions where keeping the boundary edge clear matters most.
    let infl = |i: usize| params.inflation.get(i).copied().unwrap_or(0.0);

    // ── Undirected adjacency + degree from in-scope edges ────────────
    let id_to_idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| node.id.as_deref().map(|id| (id, i)))
        .collect();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut degree: Vec<usize> = vec![0; n];
    for (s, d) in edges {
        let (Some(&si), Some(&di)) = (id_to_idx.get(s.as_str()), id_to_idx.get(d.as_str())) else {
            continue;
        };
        if si == di {
            continue;
        }
        adj[si].push(di);
        adj[di].push(si);
        degree[si] += 1;
        degree[di] += 1;
    }

    // ── Pick the hub ─────────────────────────────────────────────────
    let hub = params
        .hub
        .as_deref()
        .and_then(|id| id_to_idx.get(id).copied())
        .unwrap_or_else(|| {
            // Highest degree, ties broken by lowest index (source order).
            (0..n).max_by_key(|&i| (degree[i], usize::MAX - i)).unwrap()
        });

    // ── Ring per node = BFS shortest-path distance from the hub ──────
    let mut ring: Vec<Option<usize>> = vec![None; n];
    ring[hub] = Some(0);
    let mut queue = vec![hub];
    let mut head = 0;
    let mut max_ring = 0;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        let next = ring[u].unwrap() + 1;
        for &v in &adj[u] {
            if ring[v].is_none() {
                ring[v] = Some(next);
                max_ring = max_ring.max(next);
                queue.push(v);
            }
        }
    }
    // Nodes unreachable from the hub go on the outermost ring.
    let outer = max_ring + 1;
    let mut has_unreachable = false;
    for r in ring.iter_mut() {
        if r.is_none() {
            *r = Some(outer);
            has_unreachable = true;
        }
    }
    let last_ring = if has_unreachable { outer } else { max_ring };

    // ── Group node indices by ring, preserving source order ──────────
    let mut rings: Vec<Vec<usize>> = vec![Vec::new(); last_ring + 1];
    for (i, r) in ring.iter().enumerate() {
        rings[r.unwrap()].push(i);
    }

    // ── Place each ring ──────────────────────────────────────────────
    let mut centers: Vec<(f64, f64)> = vec![(0.0, 0.0); n];
    for (r, members) in rings.iter().enumerate() {
        if r == 0 {
            // The hub (or whichever node landed at distance 0) is centred.
            for &i in members {
                centers[i] = (0.0, 0.0);
            }
            continue;
        }
        let k = members.len();
        let max_extent = members
            .iter()
            .map(|&i| radius_of[i] + infl(i))
            .fold(0.0_f64, f64::max);
        // Two independent lower bounds on this ring's radius:
        //  • `chord_fit` keeps adjacent nodes on the ring clear of each
        //    other — the chord between two evenly-spaced points must span
        //    both their half-extents plus `node_gap`:
        //      chord = 2 R sin(PI/k)  ⇒  R = chord / (2 sin(PI/k)).
        //  • `hub_clear` keeps every ring node clear of the hub at the
        //    centre. This is the binding constraint for wide boxes on a
        //    sparse ring (e.g. 4 nodes at N/E/S/W), where the chord bound
        //    alone leaves the east/west boxes overlapping the hub.
        let chord_fit = if k <= 1 {
            0.0
        } else {
            (2.0 * max_extent + params.node_gap) / (2.0 * (PI / k as f64).sin())
        };
        let hub_clear = radius_of[hub] + infl(hub) + params.node_gap + max_extent;
        let fit_radius = chord_fit.max(hub_clear);
        let base = params.radius.unwrap_or(fit_radius);
        let ring_radius = base + (r as f64 - 1.0) * params.ring_gap;
        for (j, &i) in members.iter().enumerate() {
            let angle = params.start_angle + TAU * (j as f64) / (k as f64);
            centers[i] = (ring_radius * angle.cos(), ring_radius * angle.sin());
        }
    }

    // ── Normalize to top-left offsets with min corner at (0, 0) ──────
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for (c, node) in centers.iter().zip(nodes) {
        min_x = min_x.min(c.0 - node.size.0 / 2.0);
        min_y = min_y.min(c.1 - node.size.1 / 2.0);
    }
    centers
        .iter()
        .zip(nodes)
        .map(|(c, node)| {
            (
                c.0 - node.size.0 / 2.0 - min_x,
                c.1 - node.size.1 / 2.0 - min_y,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str) -> Node {
        Node {
            id: Some(id.into()),
            size: (80.0, 40.0),
        }
    }

    fn center(off: (f64, f64), size: (f64, f64)) -> (f64, f64) {
        (off.0 + size.0 / 2.0, off.1 + size.1 / 2.0)
    }

    #[test]
    fn empty_input_empty_output() {
        let offsets = assign_radial_offsets(&[], &[], RadialParams::default());
        assert!(offsets.is_empty());
    }

    #[test]
    fn single_node_at_origin() {
        let offsets = assign_radial_offsets(&[node("a")], &[], RadialParams::default());
        assert_eq!(offsets.len(), 1);
        assert!(offsets[0].0.abs() < 1e-9);
        assert!(offsets[0].1.abs() < 1e-9);
    }

    #[test]
    fn hub_sits_at_centroid_of_ring() {
        // a is the hub (connected to b, c, d); the three neighbours form
        // one ring whose centroid is the hub's centre.
        let nodes = vec![node("a"), node("b"), node("c"), node("d")];
        let edges = vec![
            ("a".into(), "b".into()),
            ("a".into(), "c".into()),
            ("a".into(), "d".into()),
        ];
        let offsets = assign_radial_offsets(&nodes, &edges, RadialParams::default());
        let sizes: Vec<_> = nodes.iter().map(|n| n.size).collect();
        let hub_c = center(offsets[0], sizes[0]);
        let mut cx = 0.0;
        let mut cy = 0.0;
        for i in 1..4 {
            let c = center(offsets[i], sizes[i]);
            cx += c.0;
            cy += c.1;
        }
        cx /= 3.0;
        cy /= 3.0;
        assert!(
            (hub_c.0 - cx).abs() < 1e-6,
            "hub x {} vs ring {}",
            hub_c.0,
            cx
        );
        assert!(
            (hub_c.1 - cy).abs() < 1e-6,
            "hub y {} vs ring {}",
            hub_c.1,
            cy
        );
    }

    #[test]
    fn ring_nodes_are_equidistant_from_hub() {
        let nodes = vec![node("a"), node("b"), node("c"), node("d"), node("e")];
        let edges = vec![
            ("a".into(), "b".into()),
            ("a".into(), "c".into()),
            ("a".into(), "d".into()),
            ("a".into(), "e".into()),
        ];
        let offsets = assign_radial_offsets(&nodes, &edges, RadialParams::default());
        let sizes: Vec<_> = nodes.iter().map(|n| n.size).collect();
        let hub_c = center(offsets[0], sizes[0]);
        let r0 = {
            let c = center(offsets[1], sizes[1]);
            (c.0 - hub_c.0).hypot(c.1 - hub_c.1)
        };
        for i in 2..5 {
            let c = center(offsets[i], sizes[i]);
            let r = (c.0 - hub_c.0).hypot(c.1 - hub_c.1);
            assert!((r - r0).abs() < 1e-6, "node {i} radius {r} vs {r0}");
        }
        // Not a single horizontal line: the neighbours span more than one y.
        let ys: Vec<f64> = (1..5).map(|i| center(offsets[i], sizes[i]).1).collect();
        let spread = ys.iter().cloned().fold(f64::MIN, f64::max)
            - ys.iter().cloned().fold(f64::MAX, f64::min);
        assert!(
            spread > 1.0,
            "neighbours collapsed to one row (spread {spread})"
        );
    }

    #[test]
    fn wide_ring_nodes_do_not_overlap_the_hub() {
        // Wide boxes at N/E/S/W: the chord bound alone would seat the
        // east/west nodes on top of the hub. The radius must also clear
        // the hub, so no neighbour box overlaps the hub box.
        let wide = |id: &str| Node {
            id: Some(id.into()),
            size: (190.0, 56.0),
        };
        let nodes = vec![wide("h"), wide("n1"), wide("n2"), wide("n3"), wide("n4")];
        let edges = vec![
            ("h".into(), "n1".into()),
            ("h".into(), "n2".into()),
            ("h".into(), "n3".into()),
            ("h".into(), "n4".into()),
        ];
        let offsets = assign_radial_offsets(&nodes, &edges, RadialParams::default());
        let sizes: Vec<_> = nodes.iter().map(|n| n.size).collect();
        let hub_c = center(offsets[0], sizes[0]);
        for i in 1..5 {
            let c = center(offsets[i], sizes[i]);
            let dx = (c.0 - hub_c.0).abs();
            let dy = (c.1 - hub_c.1).abs();
            // Axis-aligned boxes are disjoint when separated on either axis.
            let clears =
                dx >= (sizes[0].0 + sizes[i].0) / 2.0 || dy >= (sizes[0].1 + sizes[i].1) / 2.0;
            assert!(
                clears,
                "neighbour {i} (dx {dx:.1}, dy {dy:.1}) overlaps the hub"
            );
        }
    }

    #[test]
    fn explicit_hub_overrides_degree() {
        // b has the higher degree, but an explicit hub = "a" wins.
        let nodes = vec![node("a"), node("b"), node("c"), node("d")];
        let edges = vec![
            ("b".into(), "c".into()),
            ("b".into(), "d".into()),
            ("a".into(), "b".into()),
        ];
        let params = RadialParams {
            hub: Some("a".into()),
            ..RadialParams::default()
        };
        let offsets = assign_radial_offsets(&nodes, &edges, params);
        let sizes: Vec<_> = nodes.iter().map(|n| n.size).collect();
        // a is centred: every other node is at distance >= a from a's centre.
        let a_c = center(offsets[0], sizes[0]);
        let b_c = center(offsets[1], sizes[1]);
        assert!((b_c.0 - a_c.0).hypot(b_c.1 - a_c.1) > 1.0);
    }

    #[test]
    fn unreachable_node_lands_on_outer_ring() {
        // a-b-c is a connected chain; d is isolated → outermost ring.
        let nodes = vec![node("a"), node("b"), node("c"), node("d")];
        let edges = vec![("a".into(), "b".into()), ("b".into(), "c".into())];
        let params = RadialParams {
            hub: Some("a".into()),
            ..RadialParams::default()
        };
        let offsets = assign_radial_offsets(&nodes, &edges, params);
        let sizes: Vec<_> = nodes.iter().map(|n| n.size).collect();
        let a_c = center(offsets[0], sizes[0]);
        let c_dist = {
            let c = center(offsets[2], sizes[2]);
            (c.0 - a_c.0).hypot(c.1 - a_c.1)
        };
        let d_dist = {
            let c = center(offsets[3], sizes[3]);
            (c.0 - a_c.0).hypot(c.1 - a_c.1)
        };
        // c is at ring 2 (a→b→c); d is unreachable → ring 3, farther out.
        assert!(d_dist > c_dist, "d {d_dist} should be beyond c {c_dist}");
    }

    #[test]
    fn offsets_normalized_to_origin() {
        let nodes = vec![node("a"), node("b"), node("c"), node("d")];
        let edges = vec![
            ("a".into(), "b".into()),
            ("a".into(), "c".into()),
            ("a".into(), "d".into()),
        ];
        let offsets = assign_radial_offsets(&nodes, &edges, RadialParams::default());
        let min_x = offsets.iter().map(|o| o.0).fold(f64::INFINITY, f64::min);
        let min_y = offsets.iter().map(|o| o.1).fold(f64::INFINITY, f64::min);
        assert!(min_x.abs() < 1e-6, "min_x = {min_x}");
        assert!(min_y.abs() < 1e-6, "min_y = {min_y}");
        assert!(offsets.iter().all(|o| o.0 >= -1e-6 && o.1 >= -1e-6));
    }

    #[test]
    fn deterministic_across_calls() {
        let nodes = vec![node("a"), node("b"), node("c"), node("d")];
        let edges = vec![("a".into(), "b".into()), ("a".into(), "c".into())];
        let first = assign_radial_offsets(&nodes, &edges, RadialParams::default());
        let second = assign_radial_offsets(&nodes, &edges, RadialParams::default());
        assert_eq!(first, second);
    }

    // Wide N/E/S/W star where `hub_clear` is the binding radius bound, so
    // hub inflation moves the ring outward by exactly that amount.
    fn wide_star() -> (Vec<Node>, Vec<(String, String)>) {
        let wide = |id: &str| Node {
            id: Some(id.into()),
            size: (190.0, 56.0),
        };
        let nodes = vec![wide("h"), wide("n1"), wide("n2"), wide("n3"), wide("n4")];
        let edges = vec![
            ("h".into(), "n1".into()),
            ("h".into(), "n2".into()),
            ("h".into(), "n3".into()),
            ("h".into(), "n4".into()),
        ];
        (nodes, edges)
    }

    #[test]
    fn hub_inflation_pushes_ring_outward() {
        let (nodes, edges) = wide_star();
        let sizes: Vec<_> = nodes.iter().map(|n| n.size).collect();
        let bare = assign_radial_offsets(&nodes, &edges, RadialParams::default());
        let inflated = assign_radial_offsets(
            &nodes,
            &edges,
            RadialParams {
                inflation: vec![34.0, 0.0, 0.0, 0.0, 0.0],
                ..RadialParams::default()
            },
        );
        let hub_dist = |offs: &[(f64, f64)], i: usize| {
            let h = center(offs[0], sizes[0]);
            let c = center(offs[i], sizes[i]);
            (c.0 - h.0).hypot(c.1 - h.1)
        };
        for i in 1..5 {
            assert!(
                hub_dist(&inflated, i) > hub_dist(&bare, i) + 1.0,
                "node {i} did not move outward ({} vs {})",
                hub_dist(&inflated, i),
                hub_dist(&bare, i)
            );
        }
    }

    #[test]
    fn inflation_clears_the_boundary_band() {
        // With the hub inflated by `pad`, every ring node's nearest edge must
        // sit beyond the hub's bare box *plus* pad on its facing axis — i.e.
        // outside the boundary rect drawn at hub_box ± pad.
        let pad = 34.0;
        let (nodes, edges) = wide_star();
        let sizes: Vec<_> = nodes.iter().map(|n| n.size).collect();
        let offs = assign_radial_offsets(
            &nodes,
            &edges,
            RadialParams {
                inflation: vec![pad, 0.0, 0.0, 0.0, 0.0],
                ..RadialParams::default()
            },
        );
        let hub_c = center(offs[0], sizes[0]);
        for i in 1..5 {
            let c = center(offs[i], sizes[i]);
            let dx = (c.0 - hub_c.0).abs();
            let dy = (c.1 - hub_c.1).abs();
            // Boundary half-extents = bare hub half-box + pad.
            let bx = sizes[0].0 / 2.0 + pad;
            let by = sizes[0].1 / 2.0 + pad;
            // Disjoint from the boundary rect on at least one axis.
            let clears = dx >= bx + sizes[i].0 / 2.0 || dy >= by + sizes[i].1 / 2.0;
            assert!(
                clears,
                "neighbour {i} (dx {dx:.1}, dy {dy:.1}) intrudes the boundary band"
            );
        }
    }

    #[test]
    fn empty_inflation_is_noop() {
        let (nodes, edges) = wide_star();
        let omitted = assign_radial_offsets(&nodes, &edges, RadialParams::default());
        let explicit_empty = assign_radial_offsets(
            &nodes,
            &edges,
            RadialParams {
                inflation: Vec::new(),
                ..RadialParams::default()
            },
        );
        assert_eq!(omitted, explicit_empty);
    }

    #[test]
    fn ring_member_inflation_widens_chord() {
        // A dense single ring: inflating one ring member widens the ring it
        // sits on (its neighbours sit farther from the hub than without).
        let nodes = vec![
            node("h"),
            node("a"),
            node("b"),
            node("c"),
            node("d"),
            node("e"),
        ];
        let edges = vec![
            ("h".into(), "a".into()),
            ("h".into(), "b".into()),
            ("h".into(), "c".into()),
            ("h".into(), "d".into()),
            ("h".into(), "e".into()),
        ];
        let sizes: Vec<_> = nodes.iter().map(|n| n.size).collect();
        let ring_radius = |offs: &[(f64, f64)]| {
            let h = center(offs[0], sizes[0]);
            let c = center(offs[1], sizes[1]);
            (c.0 - h.0).hypot(c.1 - h.1)
        };
        let bare = assign_radial_offsets(&nodes, &edges, RadialParams::default());
        let inflated = assign_radial_offsets(
            &nodes,
            &edges,
            RadialParams {
                inflation: vec![0.0, 60.0, 0.0, 0.0, 0.0, 0.0],
                ..RadialParams::default()
            },
        );
        assert!(
            ring_radius(&inflated) > ring_radius(&bare) + 1.0,
            "ring did not widen ({} vs {})",
            ring_radius(&inflated),
            ring_radius(&bare)
        );
    }
}
