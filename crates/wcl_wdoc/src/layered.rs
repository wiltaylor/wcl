//! Layered auto-layout for diagram and container children.
//!
//! Topologically ranks shapes against the @connections(Edge) graph
//! and emits a `(tx, ty)` offset per child. Connected children sit
//! on consecutive ranks; isolated or unreferenced children land at
//! rank 0 (top of a top-to-bottom layout, left of a left-to-right
//! layout) in source order.
//!
//! The layout only assigns positions. Per-shape `width` / `height`
//! are still read from the schema and honored as the node size; if
//! a child omits both, it defaults to 80×40 so the layout never
//! collapses to zero.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Which way a layered graph flows.
pub(crate) enum Direction {
    /// Layers stack downwards.
    TopToBottom,
    /// Layers advance rightwards.
    LeftToRight,
}

impl Direction {
    /// Parse an author-written direction symbol.
    pub(crate) fn from_symbol(s: &str) -> Option<Direction> {
        match s {
            "top_to_bottom" => Some(Direction::TopToBottom),
            "left_to_right" => Some(Direction::LeftToRight),
            _ => None,
        }
    }
}

/// Description of one child as input to the layout solver. `id` is
/// `None` for children that have no `id` field — they're laid out
/// in source order but never participate in adjacency. `size` is
/// the child's declared (width, height) defaulting to (80, 40) when
/// missing.
#[derive(Clone)]
pub(crate) struct Node {
    /// Node id, when the block declares one.
    pub id: Option<String>,
    /// Measured `(width, height)` of the node.
    pub size: (f64, f64),
}

/// Compute per-child `(tx, ty)` offsets. `edges` are `(src_id, dst_id)`
/// pairs; edges whose endpoints don't both name a child id in
/// `nodes` are dropped here (they belong to a parent scope).
pub(crate) fn assign_layered_offsets(
    nodes: &[Node],
    edges: &[(String, String)],
    direction: Direction,
    layer_gap: f64,
    node_gap: f64,
) -> Vec<(f64, f64)> {
    let n = nodes.len();
    if n == 0 {
        return Vec::new();
    }

    // ── Build the DAG ────────────────────────────────────────────
    let id_to_idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| node.id.as_deref().map(|id| (id, i)))
        .collect();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_degree: Vec<usize> = vec![0; n];
    for (s, d) in edges {
        let (Some(&si), Some(&di)) = (id_to_idx.get(s.as_str()), id_to_idx.get(d.as_str())) else {
            continue;
        };
        if si == di {
            continue;
        }
        succ[si].push(di);
        in_degree[di] += 1;
    }

    // ── Kahn-style topological rank assignment ───────────────────
    let mut rank: Vec<usize> = vec![0; n];
    let mut visited: Vec<bool> = vec![false; n];
    let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut head = 0;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        if visited[u] {
            continue;
        }
        visited[u] = true;
        for &v in &succ[u] {
            let r = rank[u] + 1;
            if r > rank[v] {
                rank[v] = r;
            }
            in_degree[v] = in_degree[v].saturating_sub(1);
            if in_degree[v] == 0 {
                queue.push(v);
            }
        }
    }
    // Any nodes left unvisited belong to a cycle; assign them by
    // source-declaration order at the deepest rank seen so far + 1.
    let max_rank = rank.iter().copied().max().unwrap_or(0);
    for i in 0..n {
        if !visited[i] {
            rank[i] = max_rank + 1;
        }
    }

    // ── Group nodes by rank, keeping source order within rank ────
    let mut layers: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, r) in rank.iter().enumerate() {
        layers.entry(*r).or_default().push(i);
    }
    let mut sorted_ranks: Vec<usize> = layers.keys().copied().collect();
    sorted_ranks.sort_unstable();

    // ── Compute layer dimensions ─────────────────────────────────
    // For top-to-bottom: layer "depth" = max(child.height) in the
    // layer; layer "width" = sum(child.width) + node_gap * (k - 1).
    // For left-to-right swap axes.
    type AxisFn = fn(&Node) -> f64;
    let (axis_depth, axis_breadth): (AxisFn, AxisFn) = match direction {
        Direction::TopToBottom => (|n| n.size.1, |n| n.size.0),
        Direction::LeftToRight => (|n| n.size.0, |n| n.size.1),
    };

    let layer_depths: Vec<f64> = sorted_ranks
        .iter()
        .map(|r| {
            layers[r]
                .iter()
                .map(|&i| axis_depth(&nodes[i]))
                .fold(0.0_f64, f64::max)
        })
        .collect();
    let layer_breadths: Vec<f64> = sorted_ranks
        .iter()
        .map(|r| {
            let ids = &layers[r];
            let widths: f64 = ids.iter().map(|&i| axis_breadth(&nodes[i])).sum();
            let gaps = node_gap * (ids.len().saturating_sub(1) as f64);
            widths + gaps
        })
        .collect();
    let widest = layer_breadths.iter().copied().fold(0.0_f64, f64::max);

    // ── Position each child ──────────────────────────────────────
    let mut offsets: Vec<(f64, f64)> = vec![(0.0, 0.0); n];
    let mut depth_cursor = 0.0_f64;
    for (li, &r) in sorted_ranks.iter().enumerate() {
        let ids = &layers[&r];
        let breadth = layer_breadths[li];
        let mut breadth_cursor = (widest - breadth) / 2.0;
        for &i in ids {
            let (tx, ty) = match direction {
                Direction::TopToBottom => (breadth_cursor, depth_cursor),
                Direction::LeftToRight => (depth_cursor, breadth_cursor),
            };
            offsets[i] = (tx, ty);
            breadth_cursor += axis_breadth(&nodes[i]) + node_gap;
        }
        depth_cursor += layer_depths[li] + layer_gap;
    }

    offsets
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
        let offsets = assign_layered_offsets(&[], &[], Direction::TopToBottom, 30.0, 20.0);
        assert!(offsets.is_empty());
    }

    #[test]
    fn single_node_at_origin() {
        let offsets = assign_layered_offsets(&[node("a")], &[], Direction::TopToBottom, 30.0, 20.0);
        assert_eq!(offsets, vec![(0.0, 0.0)]);
    }

    #[test]
    fn two_connected_nodes_stack_along_primary_axis() {
        let nodes = vec![node("a"), node("b")];
        let edges = vec![("a".into(), "b".into())];
        let offsets = assign_layered_offsets(&nodes, &edges, Direction::TopToBottom, 30.0, 20.0);
        // Same x (each layer has one node), b strictly below a.
        assert!((offsets[0].0 - offsets[1].0).abs() < 1e-6);
        assert!(offsets[1].1 > offsets[0].1);
        assert!(offsets.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
    }

    #[test]
    fn linear_chain_top_to_bottom() {
        // a -> b -> c.
        let nodes = vec![node("a"), node("b"), node("c")];
        let edges = vec![("a".into(), "b".into()), ("b".into(), "c".into())];
        let offsets = assign_layered_offsets(&nodes, &edges, Direction::TopToBottom, 30.0, 20.0);
        // All on the same x (single-node layers); y stacks downward.
        assert_eq!(offsets[0].0, offsets[1].0);
        assert_eq!(offsets[1].0, offsets[2].0);
        assert!(offsets[0].1 < offsets[1].1 && offsets[1].1 < offsets[2].1);
        // Each layer spaced by node height (40) + layer_gap (30) = 70.
        assert!((offsets[1].1 - offsets[0].1 - 70.0).abs() < 1e-6);
        assert!((offsets[2].1 - offsets[1].1 - 70.0).abs() < 1e-6);
    }

    #[test]
    fn linear_chain_left_to_right() {
        let nodes = vec![node("a"), node("b"), node("c")];
        let edges = vec![("a".into(), "b".into()), ("b".into(), "c".into())];
        let offsets = assign_layered_offsets(&nodes, &edges, Direction::LeftToRight, 30.0, 20.0);
        // Same y; x increases.
        assert_eq!(offsets[0].1, offsets[1].1);
        assert!(offsets[0].0 < offsets[1].0 && offsets[1].0 < offsets[2].0);
        // Spacing = width(80) + layer_gap(30) = 110.
        assert!((offsets[1].0 - offsets[0].0 - 110.0).abs() < 1e-6);
    }

    #[test]
    fn isolated_nodes_go_to_rank_zero() {
        let nodes = vec![node("a"), node("b"), node("c")];
        let edges: Vec<(String, String)> = Vec::new();
        let offsets = assign_layered_offsets(&nodes, &edges, Direction::TopToBottom, 30.0, 20.0);
        // All on same y (rank 0), increasing x.
        assert!((offsets[0].1 - offsets[1].1).abs() < 1e-6);
        assert!((offsets[1].1 - offsets[2].1).abs() < 1e-6);
        assert!(offsets[0].0 < offsets[1].0 && offsets[1].0 < offsets[2].0);
    }

    #[test]
    fn no_two_nodes_overlap_in_a_branching_dag() {
        // Mixed sizes: a fans out to b/c/d, which converge on e; f and g
        // are isolated and share rank 0 with a.
        let sized = |id: &str, w: f64, h: f64| Node {
            id: Some(id.into()),
            size: (w, h),
        };
        let nodes = vec![
            sized("a", 80.0, 40.0),
            sized("b", 140.0, 60.0),
            sized("c", 60.0, 30.0),
            sized("d", 100.0, 50.0),
            sized("e", 80.0, 40.0),
            sized("f", 120.0, 20.0),
            sized("g", 40.0, 40.0),
        ];
        let edges: Vec<(String, String)> = vec![
            ("a".into(), "b".into()),
            ("a".into(), "c".into()),
            ("a".into(), "d".into()),
            ("b".into(), "e".into()),
            ("c".into(), "e".into()),
            ("d".into(), "e".into()),
        ];
        for direction in [Direction::TopToBottom, Direction::LeftToRight] {
            let offsets = assign_layered_offsets(&nodes, &edges, direction, 30.0, 20.0);
            assert_no_overlap(&nodes, &offsets);
        }
    }

    #[test]
    fn cycle_self_loop_and_disconnected_terminate() {
        // a→b→c→a is a cycle, d→d a self-loop, e is disconnected.
        let nodes = vec![node("a"), node("b"), node("c"), node("d"), node("e")];
        let edges: Vec<(String, String)> = vec![
            ("a".into(), "b".into()),
            ("b".into(), "c".into()),
            ("c".into(), "a".into()),
            ("d".into(), "d".into()),
        ];
        let offsets = assign_layered_offsets(&nodes, &edges, Direction::TopToBottom, 30.0, 20.0);
        assert_eq!(offsets.len(), nodes.len());
        assert!(offsets.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
        assert_no_overlap(&nodes, &offsets);
    }

    #[test]
    fn deterministic_across_calls() {
        let nodes = vec![node("a"), node("b"), node("c"), node("d")];
        let edges: Vec<(String, String)> = vec![
            ("a".into(), "b".into()),
            ("a".into(), "c".into()),
            ("c".into(), "d".into()),
        ];
        let first = assign_layered_offsets(&nodes, &edges, Direction::TopToBottom, 30.0, 20.0);
        let second = assign_layered_offsets(&nodes, &edges, Direction::TopToBottom, 30.0, 20.0);
        assert_eq!(first, second);
    }

    #[test]
    fn dag_ranks_are_monotone_along_the_primary_axis() {
        // a→b→c: each successive layer sits strictly deeper on the
        // primary axis (y for top-to-bottom, x for left-to-right).
        let nodes = vec![node("a"), node("b"), node("c")];
        let edges: Vec<(String, String)> = vec![("a".into(), "b".into()), ("b".into(), "c".into())];

        let ttb = assign_layered_offsets(&nodes, &edges, Direction::TopToBottom, 30.0, 20.0);
        assert!(ttb[0].1 < ttb[1].1 && ttb[1].1 < ttb[2].1);

        let ltr = assign_layered_offsets(&nodes, &edges, Direction::LeftToRight, 30.0, 20.0);
        assert!(ltr[0].0 < ltr[1].0 && ltr[1].0 < ltr[2].0);
    }
}
