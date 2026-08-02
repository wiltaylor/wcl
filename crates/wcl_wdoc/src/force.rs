//! Force-directed auto-layout for diagram and container children.
//!
//! Treats each child as a charged particle that repels every other
//! child, with springs along the `@connections(Edge)` graph pulling
//! connected children together, then relaxes the system over a fixed
//! number of iterations and emits a `(tx, ty)` offset per child.
//! Unlike the layered solver this suits cyclic / undirected graphs
//! (networks, relationship maps, mind-maps) where there is no natural
//! rank order: children start clustered in the middle, push apart, and
//! settle into an organic, cluster-revealing arrangement.
//!
//! The simulation is a pure, deterministic function of its inputs.
//! Initial positions come from a golden-angle spiral seeded by `seed`
//! (no RNG), and every accumulation runs in a fixed order. Both the
//! collect and render passes recompute the layout independently, so
//! identical inputs MUST yield byte-identical output — otherwise the
//! bboxes used to route edges and size containers would drift from the
//! drawn shapes.
//!
//! Like the layered solver, this only assigns positions; per-shape
//! `width` / `height` are read from the schema and honored as the node
//! size (defaulting to 80×40 upstream when both are omitted).

use std::collections::HashMap;

use crate::layered::Node;

/// Tunable knobs for the force simulation, surfaced on `diagram` /
/// `container` as optional WCL fields. Defaults are picked to give a
/// readable spread for typical diagram-scale graphs (a few dozen nodes).
pub(crate) struct ForceParams {
    /// Relaxation steps. More iterations settle larger graphs.
    pub iterations: usize,
    /// Coulomb repulsion constant; larger spreads nodes farther apart.
    pub repulsion: f64,
    /// Ideal edge length, edge-to-edge, between connected node boxes.
    pub link_distance: f64,
    /// Centering pull toward the centroid each step; keeps disconnected
    /// components from drifting apart forever. 0 disables it.
    pub gravity: f64,
    /// Seed for the deterministic spiral initialization. Changing it
    /// reproducibly re-arranges the graph.
    pub seed: i64,
}

impl Default for ForceParams {
    fn default() -> Self {
        ForceParams {
            iterations: 300,
            repulsion: 9000.0,
            link_distance: 60.0,
            gravity: 0.05,
            seed: 1,
        }
    }
}

/// Compute per-child `(tx, ty)` offsets via force-directed relaxation.
/// `edges` are `(src_id, dst_id)` pairs; edges whose endpoints don't
/// both name a child `id` in `nodes` are dropped (they belong to a
/// parent scope, exactly as in the layered solver). Returned offsets
/// are normalized so the children's bounding box starts at `(0, 0)`.
pub(crate) fn assign_force_offsets(
    nodes: &[Node],
    edges: &[(String, String)],
    params: ForceParams,
) -> Vec<(f64, f64)> {
    let n = nodes.len();
    if n == 0 {
        return Vec::new();
    }

    // Per-node radius from the box's diagonal half-extent: larger boxes
    // repel harder and connected boxes settle edge-to-edge rather than
    // overlapping centers.
    let radius: Vec<f64> = nodes
        .iter()
        .map(|node| node.size.0.hypot(node.size.1) / 2.0)
        .collect();

    // Resolve edges to index pairs, dropping self-loops and endpoints
    // that don't name a child here.
    let id_to_idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| node.id.as_deref().map(|id| (id, i)))
        .collect();
    let links: Vec<(usize, usize)> = edges
        .iter()
        .filter_map(|(s, d)| {
            let si = *id_to_idx.get(s.as_str())?;
            let di = *id_to_idx.get(d.as_str())?;
            (si != di).then_some((si, di))
        })
        .collect();

    // Deterministic spiral initialization, clustered near the origin so
    // nodes visibly "start in the middle" but never coincide (which
    // would make the repulsion direction undefined).
    const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;
    let seed = params.seed as f64;
    let mut pos: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let angle = i as f64 * GOLDEN_ANGLE + seed;
            let r = params.link_distance * 0.15 * ((i + 1) as f64).sqrt();
            (r * angle.cos(), r * angle.sin())
        })
        .collect();

    let ideal_base = params.link_distance.max(1.0);
    const SPRING: f64 = 0.1;
    const EPS: f64 = 0.01;
    let mut temperature = params.link_distance.max(1.0) * 2.0;
    let cooling = if params.iterations > 0 {
        temperature / params.iterations as f64
    } else {
        0.0
    };

    for _ in 0..params.iterations {
        let mut disp = vec![(0.0_f64, 0.0_f64); n];

        // Coulomb repulsion between every distinct pair.
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pos[i].0 - pos[j].0;
                let dy = pos[i].1 - pos[j].1;
                let dist = dx.hypot(dy).max(EPS);
                // Distance net of both radii, floored so overlapping
                // boxes push apart hard instead of dividing by ~0.
                let gap = (dist - radius[i] - radius[j]).max(EPS);
                let force = params.repulsion / (gap * gap);
                let (ux, uy) = (dx / dist, dy / dist);
                disp[i].0 += ux * force;
                disp[i].1 += uy * force;
                disp[j].0 -= ux * force;
                disp[j].1 -= uy * force;
            }
        }

        // Hooke spring along each edge toward its ideal length.
        for &(a, b) in &links {
            let dx = pos[a].0 - pos[b].0;
            let dy = pos[a].1 - pos[b].1;
            let dist = dx.hypot(dy).max(EPS);
            let ideal = ideal_base + radius[a] + radius[b];
            let force = (dist - ideal) * SPRING;
            let (ux, uy) = (dx / dist, dy / dist);
            // force > 0 (too far) pulls the pair together.
            disp[a].0 -= ux * force;
            disp[a].1 -= uy * force;
            disp[b].0 += ux * force;
            disp[b].1 += uy * force;
        }

        // Centering gravity toward the current centroid.
        if params.gravity != 0.0 {
            let (mut cx, mut cy) = (0.0, 0.0);
            for p in &pos {
                cx += p.0;
                cy += p.1;
            }
            cx /= n as f64;
            cy /= n as f64;
            for (d, p) in disp.iter_mut().zip(&pos) {
                d.0 += (cx - p.0) * params.gravity;
                d.1 += (cy - p.1) * params.gravity;
            }
        }

        // Apply each displacement, clamped to the current temperature.
        for (p, d) in pos.iter_mut().zip(&disp) {
            let mag = d.0.hypot(d.1);
            if mag > EPS {
                let scale = mag.min(temperature) / mag;
                p.0 += d.0 * scale;
                p.1 += d.1 * scale;
            }
        }
        temperature = (temperature - cooling).max(0.0);
    }

    // Hard collision-resolution. The relaxation above is the only thing
    // spreading nodes, and its repulsion both weakens with distance and is
    // frozen by the cooling schedule — so a dense or large-node graph (many
    // nodes sharing a hub, big boxes) can settle with boxes still
    // overlapping. That leaves the edge router no clear lane and forces
    // lines straight through node boxes. These sweeps project any
    // overlapping pair apart along their centre line until the circles
    // bounding their boxes (the same half-diagonal `radius`, plus a margin
    // for the router's breathing room) are disjoint — which guarantees the
    // boxes are disjoint too. Only overlapping pairs move, so a graph that
    // already relaxed cleanly is left exactly as it was.
    const COLLIDE_MARGIN: f64 = 8.0;
    const COLLIDE_SWEEPS: usize = 200;
    for _ in 0..COLLIDE_SWEEPS {
        let mut moved = false;
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pos[i].0 - pos[j].0;
                let dy = pos[i].1 - pos[j].1;
                let dist = dx.hypot(dy).max(EPS);
                let min_dist = radius[i] + radius[j] + COLLIDE_MARGIN;
                if dist < min_dist {
                    // Split the correction so neither node dominates; many
                    // sweeps relax a jammed cluster the way d3's collide does.
                    let push = (min_dist - dist) / 2.0;
                    let (ux, uy) = (dx / dist, dy / dist);
                    pos[i].0 += ux * push;
                    pos[i].1 += uy * push;
                    pos[j].0 -= ux * push;
                    pos[j].1 -= uy * push;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }

    // Normalize: shift so the children's bounding box top-left is at
    // (0, 0), and return per-child top-left offsets. This keeps the
    // layered-style `content_size` max-loop and container auto-fit
    // valid (all offsets >= 0).
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for (p, node) in pos.iter().zip(nodes) {
        min_x = min_x.min(p.0 - node.size.0 / 2.0);
        min_y = min_y.min(p.1 - node.size.1 / 2.0);
    }
    // Quantize to a micro-pixel grid: cos/sin/hypot round differently
    // across libm builds, and the iteration amplifies that into the
    // printed digits — committed SVG artifacts must not depend on the
    // build machine's libc.
    let quant = |v: f64| (v * 1e6).round() / 1e6;
    pos.iter()
        .zip(nodes)
        .map(|(p, node)| {
            (
                quant(p.0 - node.size.0 / 2.0 - min_x),
                quant(p.1 - node.size.1 / 2.0 - min_y),
            )
        })
        .collect()
}

/// Public seam for the `wcl editor`'s unit graph: deterministic layout of
/// `sizes` boxes connected by index-pair `edges`, returning one top-left
/// `(x, y)` offset per box (bounding box normalized to start at the
/// origin). Same solver as diagram auto-layout — seeded, quantized, so
/// identical inputs give identical output.
pub fn layout_graph(sizes: &[(f64, f64)], edges: &[(usize, usize)]) -> Vec<(f64, f64)> {
    // The solver keys edges by node id; synthesize index ids.
    let nodes: Vec<Node> = sizes
        .iter()
        .enumerate()
        .map(|(i, &size)| Node {
            id: Some(i.to_string()),
            size,
        })
        .collect();
    let edges: Vec<(String, String)> = edges
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    assign_force_offsets(
        &nodes,
        &edges,
        ForceParams {
            // Editor graphs are bigger than typical diagrams; keep linked
            // units close but let clusters breathe.
            link_distance: 110.0,
            ..Default::default()
        },
    )
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

    fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
        (a.0 - b.0).hypot(a.1 - b.1)
    }

    /// Assert no two node AABBs overlap, treating each offset as the
    /// node's top-left corner and allowing exact edge-touching.
    fn assert_no_overlap(nodes: &[Node], offsets: &[(f64, f64)]) {
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let (x1, y1) = offsets[i];
                let (w1, h1) = nodes[i].size;
                let (x2, y2) = offsets[j];
                let (w2, h2) = nodes[j].size;
                let overlaps = x1 + w1 > x2 + 1e-6
                    && x2 + w2 > x1 + 1e-6
                    && y1 + h1 > y2 + 1e-6
                    && y2 + h2 > y1 + 1e-6;
                assert!(
                    !overlaps,
                    "nodes {i} ({x1},{y1} {w1}x{h1}) and {j} ({x2},{y2} {w2}x{h2}) overlap"
                );
            }
        }
    }

    #[test]
    fn empty_input_empty_output() {
        let offsets = assign_force_offsets(&[], &[], ForceParams::default());
        assert!(offsets.is_empty());
    }

    #[test]
    fn single_node_at_origin() {
        let offsets = assign_force_offsets(&[node("a")], &[], ForceParams::default());
        assert_eq!(offsets.len(), 1);
        assert!((offsets[0].0).abs() < 1e-9);
        assert!((offsets[0].1).abs() < 1e-9);
    }

    #[test]
    fn deterministic_across_calls() {
        let nodes = vec![node("a"), node("b"), node("c"), node("d")];
        let edges = vec![
            ("a".into(), "b".into()),
            ("b".into(), "c".into()),
            ("c".into(), "a".into()),
        ];
        let first = assign_force_offsets(&nodes, &edges, ForceParams::default());
        let second = assign_force_offsets(&nodes, &edges, ForceParams::default());
        assert_eq!(first, second);
    }

    #[test]
    fn all_coordinates_finite() {
        let nodes = vec![node("a"), node("b"), node("c"), node("d"), node("e")];
        let edges = vec![("a".into(), "b".into()), ("a".into(), "c".into())];
        let offsets = assign_force_offsets(&nodes, &edges, ForceParams::default());
        for (x, y) in offsets {
            assert!(x.is_finite() && y.is_finite());
        }
    }

    #[test]
    fn offsets_normalized_to_origin() {
        let nodes = vec![node("a"), node("b"), node("c"), node("d")];
        let edges = vec![("a".into(), "b".into())];
        let offsets = assign_force_offsets(&nodes, &edges, ForceParams::default());
        let min_x = offsets.iter().map(|o| o.0).fold(f64::INFINITY, f64::min);
        let min_y = offsets.iter().map(|o| o.1).fold(f64::INFINITY, f64::min);
        // The extreme node's box corner sits exactly at (0, *) / (*, 0).
        assert!(min_x.abs() < 1e-6, "min_x = {min_x}");
        assert!(min_y.abs() < 1e-6, "min_y = {min_y}");
        assert!(offsets.iter().all(|o| o.0 >= -1e-6 && o.1 >= -1e-6));
    }

    #[test]
    fn connected_nodes_settle_closer_than_unconnected() {
        // With gravity disabled, an edge's spring pulls a pair toward
        // the ideal link length while an edge-less pair only repels and
        // drifts much farther apart.
        let params = || ForceParams {
            gravity: 0.0,
            ..ForceParams::default()
        };
        let nodes = vec![node("a"), node("b")];

        let linked = assign_force_offsets(&nodes, &[("a".into(), "b".into())], params());
        let d_linked = dist(linked[0], linked[1]);

        let loose = assign_force_offsets(&nodes, &[], params());
        let d_loose = dist(loose[0], loose[1]);

        assert!(
            d_linked < d_loose,
            "connected {d_linked} should be < unconnected {d_loose}"
        );
        // Spring holds the connected pair in a sane band around the
        // ideal length (link_distance + both radii ≈ 60 + 2*44.7).
        assert!((80.0..220.0).contains(&d_linked), "d_linked = {d_linked}");
    }

    #[test]
    fn no_two_nodes_overlap_in_a_small_graph() {
        // A ring of five plus a tail and an isolated node, mixed sizes:
        // repulsion (with box-radius-aware gaps) should leave every pair
        // of boxes disjoint once the system has relaxed.
        let sized = |id: &str, w: f64, h: f64| Node {
            id: Some(id.into()),
            size: (w, h),
        };
        let nodes = vec![
            sized("a", 80.0, 40.0),
            sized("b", 120.0, 60.0),
            sized("c", 60.0, 30.0),
            sized("d", 100.0, 50.0),
            sized("e", 80.0, 40.0),
            sized("f", 90.0, 45.0),
            sized("g", 70.0, 35.0),
        ];
        let edges: Vec<(String, String)> = vec![
            ("a".into(), "b".into()),
            ("b".into(), "c".into()),
            ("c".into(), "d".into()),
            ("d".into(), "e".into()),
            ("e".into(), "a".into()),
            ("e".into(), "f".into()),
        ];
        let offsets = assign_force_offsets(&nodes, &edges, ForceParams::default());
        assert_no_overlap(&nodes, &offsets);
    }

    #[test]
    fn dense_large_node_hub_graph_has_no_overlap() {
        // Mirrors a dense note graph: many large (120×120) nodes wired
        // into a few hubs. The relaxation alone leaves overlaps at this
        // scale; the collision-resolution pass must still hand back a
        // disjoint layout so the edge router has clean lanes.
        let big = |id: &str| Node {
            id: Some(id.into()),
            size: (120.0, 120.0),
        };
        let nodes: Vec<Node> = (0..24).map(|i| big(&format!("n{i}"))).collect();
        // Hub-and-spoke: n0 and n1 are hubs every other node links to.
        let mut edges: Vec<(String, String)> = Vec::new();
        for i in 2..24 {
            edges.push(("n0".into(), format!("n{i}")));
            if i % 2 == 0 {
                edges.push(("n1".into(), format!("n{i}")));
            }
        }
        let offsets = assign_force_offsets(&nodes, &edges, ForceParams::default());
        assert_no_overlap(&nodes, &offsets);
    }

    #[test]
    fn cycle_self_loop_and_disconnected_terminate() {
        // a→b→c→a is a cycle, d→d a self-loop (dropped), e disconnected.
        let nodes = vec![node("a"), node("b"), node("c"), node("d"), node("e")];
        let edges: Vec<(String, String)> = vec![
            ("a".into(), "b".into()),
            ("b".into(), "c".into()),
            ("c".into(), "a".into()),
            ("d".into(), "d".into()),
        ];
        let offsets = assign_force_offsets(&nodes, &edges, ForceParams::default());
        assert_eq!(offsets.len(), nodes.len());
        assert!(offsets.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
    }

    #[test]
    fn seed_changes_arrangement() {
        let nodes = vec![node("a"), node("b"), node("c"), node("d"), node("e")];
        let edges: Vec<(String, String)> = Vec::new();
        let a = assign_force_offsets(
            &nodes,
            &edges,
            ForceParams {
                seed: 1,
                ..ForceParams::default()
            },
        );
        let b = assign_force_offsets(
            &nodes,
            &edges,
            ForceParams {
                seed: 7,
                ..ForceParams::default()
            },
        );
        assert_ne!(a, b);
    }
}
