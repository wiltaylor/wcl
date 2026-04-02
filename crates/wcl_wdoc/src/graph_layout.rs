//! Graph-aware layout engines for diagrams.
//!
//! Each engine positions shapes based on their connections.
//! All engines share the same signature and operate on `ShapeNode` slices.

use std::collections::{HashMap, HashSet, VecDeque};

use indexmap::IndexMap;

use crate::shapes::{Bounds, Connection, ShapeNode};

// ---------------------------------------------------------------------------
// Layered (Sugiyama-style) — directed graphs, flowcharts, pipelines
// ---------------------------------------------------------------------------

pub fn layout_layered(
    children: &mut [ShapeNode],
    connections: &[Connection],
    parent: &Bounds,
    gap: f64,
    options: &IndexMap<String, String>,
) {
    let horizontal = options.get("direction").map(|s| s.as_str()) == Some("horizontal");
    let n = children.len();
    if n == 0 {
        return;
    }

    // Build ID → index mapping
    let id_map: HashMap<&str, usize> = children
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.id.as_deref().map(|id| (id, i)))
        .collect();

    // Build adjacency (outgoing edges)
    let mut outgoing: Vec<Vec<usize>> = vec![vec![]; n];
    let mut incoming: Vec<Vec<usize>> = vec![vec![]; n];
    for conn in connections {
        if let (Some(&from), Some(&to)) = (
            id_map.get(conn.from_id.as_str()),
            id_map.get(conn.to_id.as_str()),
        ) {
            outgoing[from].push(to);
            incoming[to].push(from);
        }
    }

    // Assign layers via longest-path from sources (nodes with no incoming edges)
    let mut layer: Vec<usize> = vec![0; n];
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut in_degree: Vec<usize> = incoming.iter().map(|v| v.len()).collect();

    for (i, &deg) in in_degree.iter().enumerate() {
        if deg == 0 {
            queue.push_back(i);
        }
    }
    // If no sources (cycle), start from node 0
    if queue.is_empty() {
        queue.push_back(0);
        layer[0] = 0;
    }

    while let Some(node) = queue.pop_front() {
        for &next in &outgoing[node] {
            layer[next] = layer[next].max(layer[node] + 1);
            in_degree[next] = in_degree[next].saturating_sub(1);
            if in_degree[next] == 0 {
                queue.push_back(next);
            }
        }
    }

    let max_layer = layer.iter().copied().max().unwrap_or(0);

    // Group nodes by layer
    let mut layers: Vec<Vec<usize>> = vec![vec![]; max_layer + 1];
    for (i, &l) in layer.iter().enumerate() {
        layers[l].push(i);
    }

    // Barycenter ordering within each layer (minimize crossings)
    for l in 1..=max_layer {
        let mut positions: Vec<(usize, f64)> = layers[l]
            .iter()
            .map(|&node| {
                let parents: Vec<f64> = incoming[node]
                    .iter()
                    .filter_map(|&p| {
                        layers[layer[p]]
                            .iter()
                            .position(|&x| x == p)
                            .map(|pos| pos as f64)
                    })
                    .collect();
                let bary = if parents.is_empty() {
                    0.0
                } else {
                    parents.iter().sum::<f64>() / parents.len() as f64
                };
                (node, bary)
            })
            .collect();
        positions.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        layers[l] = positions.into_iter().map(|(node, _)| node).collect();
    }

    // Assign positions
    let num_layers = (max_layer + 1) as f64;
    let (main_size, cross_size) = if horizontal {
        (parent.width, parent.height)
    } else {
        (parent.height, parent.width)
    };
    let layer_spacing = if num_layers > 1.0 {
        (main_size - gap) / num_layers
    } else {
        main_size
    };

    for (l, layer_nodes) in layers.iter().enumerate() {
        let count = layer_nodes.len() as f64;
        let node_spacing = if count > 0.0 {
            cross_size / count
        } else {
            cross_size
        };

        for (order, &node_idx) in layer_nodes.iter().enumerate() {
            let main_pos = parent.y + l as f64 * layer_spacing + gap / 2.0;
            let cross_pos = parent.x
                + order as f64 * node_spacing
                + (node_spacing - children[node_idx].resolved.width) / 2.0;

            if horizontal {
                children[node_idx].resolved.x = main_pos;
                children[node_idx].resolved.y = cross_pos;
            } else {
                children[node_idx].resolved.x = cross_pos;
                children[node_idx].resolved.y = main_pos;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Force-directed — network diagrams, organic layouts
// ---------------------------------------------------------------------------

pub fn layout_force(
    children: &mut [ShapeNode],
    connections: &[Connection],
    parent: &Bounds,
    _gap: f64,
    _options: &IndexMap<String, String>,
) {
    let n = children.len();
    if n == 0 {
        return;
    }

    let id_map: HashMap<&str, usize> = children
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.id.as_deref().map(|id| (id, i)))
        .collect();

    // Build edge list
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for conn in connections {
        if let (Some(&from), Some(&to)) = (
            id_map.get(conn.from_id.as_str()),
            id_map.get(conn.to_id.as_str()),
        ) {
            edges.push((from, to));
        }
    }

    // Initialize positions in a circle
    let cx = parent.width / 2.0;
    let cy = parent.height / 2.0;
    let radius = parent.width.min(parent.height) / 3.0;

    let mut pos: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let angle = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            (cx + radius * angle.cos(), cy + radius * angle.sin())
        })
        .collect();

    let mut vel: Vec<(f64, f64)> = vec![(0.0, 0.0); n];

    // Simulation parameters
    let repulsion = 5000.0;
    let attraction = 0.01;
    let damping = 0.85;
    let iterations = 120;

    for _ in 0..iterations {
        // Repulsive forces (all pairs)
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pos[i].0 - pos[j].0;
                let dy = pos[i].1 - pos[j].1;
                let dist_sq = (dx * dx + dy * dy).max(1.0);
                let force = repulsion / dist_sq;
                let dist = dist_sq.sqrt();
                let fx = force * dx / dist;
                let fy = force * dy / dist;
                vel[i].0 += fx;
                vel[i].1 += fy;
                vel[j].0 -= fx;
                vel[j].1 -= fy;
            }
        }

        // Attractive forces (along edges)
        for &(from, to) in &edges {
            let dx = pos[to].0 - pos[from].0;
            let dy = pos[to].1 - pos[from].1;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let force = attraction * dist;
            let fx = force * dx / dist;
            let fy = force * dy / dist;
            vel[from].0 += fx;
            vel[from].1 += fy;
            vel[to].0 -= fx;
            vel[to].1 -= fy;
        }

        // Apply velocity with damping
        for i in 0..n {
            pos[i].0 += vel[i].0;
            pos[i].1 += vel[i].1;
            vel[i].0 *= damping;
            vel[i].1 *= damping;
        }
    }

    // Scale and center to fit parent bounds
    let (min_x, max_x, min_y, max_y) = bounding_box(&pos);
    let scale_x = if max_x > min_x {
        (parent.width * 0.8) / (max_x - min_x)
    } else {
        1.0
    };
    let scale_y = if max_y > min_y {
        (parent.height * 0.8) / (max_y - min_y)
    } else {
        1.0
    };
    let scale = scale_x.min(scale_y);

    let offset_x = parent.x + (parent.width - (max_x - min_x) * scale) / 2.0;
    let offset_y = parent.y + (parent.height - (max_y - min_y) * scale) / 2.0;

    for i in 0..n {
        let w = children[i].resolved.width;
        let h = children[i].resolved.height;
        children[i].resolved.x = (pos[i].0 - min_x) * scale + offset_x - w / 2.0;
        children[i].resolved.y = (pos[i].1 - min_y) * scale + offset_y - h / 2.0;
    }
}

// ---------------------------------------------------------------------------
// Radial — tree hierarchies from a root node
// ---------------------------------------------------------------------------

pub fn layout_radial(
    children: &mut [ShapeNode],
    connections: &[Connection],
    parent: &Bounds,
    gap: f64,
    options: &IndexMap<String, String>,
) {
    let n = children.len();
    if n == 0 {
        return;
    }

    let id_map: HashMap<&str, usize> = children
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.id.as_deref().map(|id| (id, i)))
        .collect();

    // Find root
    let root_idx = options
        .get("root")
        .and_then(|r| id_map.get(r.as_str()).copied())
        .unwrap_or_else(|| {
            // Default: first node with no incoming edges
            let mut has_incoming: HashSet<usize> = HashSet::new();
            for conn in connections {
                if let Some(&to) = id_map.get(conn.to_id.as_str()) {
                    has_incoming.insert(to);
                }
            }
            (0..n).find(|i| !has_incoming.contains(i)).unwrap_or(0)
        });

    // Build adjacency (treat as undirected for tree traversal)
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    for conn in connections {
        if let (Some(&from), Some(&to)) = (
            id_map.get(conn.from_id.as_str()),
            id_map.get(conn.to_id.as_str()),
        ) {
            adj[from].push(to);
            adj[to].push(from);
        }
    }

    // BFS from root to assign ring levels
    let mut ring: Vec<usize> = vec![usize::MAX; n];
    ring[root_idx] = 0;
    let mut queue = VecDeque::new();
    queue.push_back(root_idx);

    while let Some(node) = queue.pop_front() {
        for &neighbor in &adj[node] {
            if ring[neighbor] == usize::MAX {
                ring[neighbor] = ring[node] + 1;
                queue.push_back(neighbor);
            }
        }
    }
    // Unreachable nodes get ring 1
    for r in ring.iter_mut() {
        if *r == usize::MAX {
            *r = 1;
        }
    }

    let max_ring = ring.iter().copied().max().unwrap_or(0);

    // Group by ring
    let mut rings: Vec<Vec<usize>> = vec![vec![]; max_ring + 1];
    for (i, &r) in ring.iter().enumerate() {
        rings[r].push(i);
    }

    // Position: root at center, others on concentric circles
    let cx = parent.x + parent.width / 2.0;
    let cy = parent.y + parent.height / 2.0;
    let max_radius = (parent.width.min(parent.height) / 2.0) - gap;
    let ring_spacing = if max_ring > 0 {
        max_radius / max_ring as f64
    } else {
        max_radius
    };

    for (r, ring_nodes) in rings.iter().enumerate() {
        if r == 0 {
            // Root at center
            for &idx in ring_nodes {
                let w = children[idx].resolved.width;
                let h = children[idx].resolved.height;
                children[idx].resolved.x = cx - w / 2.0;
                children[idx].resolved.y = cy - h / 2.0;
            }
        } else {
            let radius = r as f64 * ring_spacing;
            let count = ring_nodes.len();
            for (order, &idx) in ring_nodes.iter().enumerate() {
                let angle = 2.0 * std::f64::consts::PI * order as f64 / count as f64
                    - std::f64::consts::FRAC_PI_2; // start from top
                let w = children[idx].resolved.width;
                let h = children[idx].resolved.height;
                children[idx].resolved.x = cx + radius * angle.cos() - w / 2.0;
                children[idx].resolved.y = cy + radius * angle.sin() - h / 2.0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Grid — regular grid arrangement
// ---------------------------------------------------------------------------

pub fn layout_grid(
    children: &mut [ShapeNode],
    _connections: &[Connection],
    parent: &Bounds,
    gap: f64,
    options: &IndexMap<String, String>,
) {
    let n = children.len();
    if n == 0 {
        return;
    }

    let columns: usize = options
        .get("columns")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| (n as f64).sqrt().ceil() as usize)
        .max(1);

    // Find max cell size
    let max_w: f64 = children
        .iter()
        .map(|c| c.resolved.width)
        .fold(0.0, f64::max);
    let max_h: f64 = children
        .iter()
        .map(|c| c.resolved.height)
        .fold(0.0, f64::max);

    let cell_w = max_w + gap;
    let cell_h = max_h + gap;

    // Center the grid within parent
    let rows = n.div_ceil(columns) as f64;
    let grid_w = columns as f64 * cell_w - gap;
    let grid_h = rows * cell_h - gap;
    let offset_x = parent.x + (parent.width - grid_w).max(0.0) / 2.0;
    let offset_y = parent.y + (parent.height - grid_h).max(0.0) / 2.0;

    for (i, child) in children.iter_mut().enumerate() {
        let col = i % columns;
        let row = i / columns;
        // Center each node within its cell
        child.resolved.x = offset_x + col as f64 * cell_w + (max_w - child.resolved.width) / 2.0;
        child.resolved.y = offset_y + row as f64 * cell_h + (max_h - child.resolved.height) / 2.0;
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bounding_box(positions: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for &(x, y) in positions {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    (min_x, max_x, min_y, max_y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::*;

    fn make_node(id: &str, w: f64, h: f64) -> ShapeNode {
        ShapeNode {
            kind: ShapeKind::Rect,
            id: Some(id.to_string()),
            x: None,
            y: None,
            width: Some(w),
            height: Some(h),
            top: None,
            bottom: None,
            left: None,
            right: None,
            resolved: Bounds {
                x: 0.0,
                y: 0.0,
                width: w,
                height: h,
            },
            attrs: IndexMap::new(),
            children: vec![],
            align: Alignment::None,
            gap: 0.0,
            padding: 0.0,
        }
    }

    fn make_conn(from: &str, to: &str) -> Connection {
        Connection {
            from_id: from.to_string(),
            to_id: to.to_string(),
            direction: Direction::To,
            from_anchor: AnchorPoint::Auto,
            to_anchor: AnchorPoint::Auto,
            label: None,
            curve: CurveStyle::Straight,
            attrs: IndexMap::new(),
        }
    }

    #[test]
    fn test_layered_linear() {
        let mut nodes = vec![
            make_node("a", 80.0, 40.0),
            make_node("b", 80.0, 40.0),
            make_node("c", 80.0, 40.0),
        ];
        let conns = vec![make_conn("a", "b"), make_conn("b", "c")];
        let parent = Bounds {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 300.0,
        };
        layout_layered(&mut nodes, &conns, &parent, 20.0, &IndexMap::new());

        // a should be in layer 0, b in layer 1, c in layer 2
        assert!(nodes[0].resolved.y < nodes[1].resolved.y);
        assert!(nodes[1].resolved.y < nodes[2].resolved.y);
    }

    #[test]
    fn test_force_separates_nodes() {
        let mut nodes = vec![make_node("a", 40.0, 40.0), make_node("b", 40.0, 40.0)];
        let conns = vec![make_conn("a", "b")];
        let parent = Bounds {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 400.0,
        };
        layout_force(&mut nodes, &conns, &parent, 20.0, &IndexMap::new());

        // Nodes should not overlap
        let dist = ((nodes[0].resolved.x - nodes[1].resolved.x).powi(2)
            + (nodes[0].resolved.y - nodes[1].resolved.y).powi(2))
        .sqrt();
        assert!(dist > 10.0);
    }

    #[test]
    fn test_radial_root_at_center() {
        let mut nodes = vec![
            make_node("root", 40.0, 40.0),
            make_node("a", 30.0, 30.0),
            make_node("b", 30.0, 30.0),
        ];
        let conns = vec![make_conn("root", "a"), make_conn("root", "b")];
        let parent = Bounds {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 400.0,
        };
        let mut opts = IndexMap::new();
        opts.insert("root".to_string(), "root".to_string());
        layout_radial(&mut nodes, &conns, &parent, 20.0, &opts);

        // Root should be near center
        let root_cx = nodes[0].resolved.x + 20.0;
        let root_cy = nodes[0].resolved.y + 20.0;
        assert!((root_cx - 200.0).abs() < 1.0);
        assert!((root_cy - 200.0).abs() < 1.0);
    }

    #[test]
    fn test_grid_layout() {
        let mut nodes = vec![
            make_node("a", 60.0, 40.0),
            make_node("b", 60.0, 40.0),
            make_node("c", 60.0, 40.0),
            make_node("d", 60.0, 40.0),
        ];
        let parent = Bounds {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 300.0,
        };
        let mut opts = IndexMap::new();
        opts.insert("columns".to_string(), "2".to_string());
        layout_grid(&mut nodes, &[], &parent, 20.0, &opts);

        // Should be in 2x2 grid: a,b on row 0; c,d on row 1
        assert!(nodes[0].resolved.x < nodes[1].resolved.x);
        assert!((nodes[0].resolved.y - nodes[1].resolved.y).abs() < 1.0);
        assert!(nodes[0].resolved.y < nodes[2].resolved.y);
    }
}
