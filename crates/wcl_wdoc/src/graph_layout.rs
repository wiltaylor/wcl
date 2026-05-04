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

    let components = connected_components(&incoming, &outgoing);
    let component_rank = component_ranks(children, &components, &incoming, &outgoing);
    let isolated: Vec<bool> = (0..n)
        .map(|i| incoming[i].is_empty() && outgoing[i].is_empty())
        .collect();
    let layer = assign_layers(children, &incoming, &outgoing, &component_rank, &isolated);

    let max_layer = layer.iter().copied().max().unwrap_or(0);

    // Group nodes by layer
    let mut layers: Vec<Vec<usize>> = vec![vec![]; max_layer + 1];
    for (i, &l) in layer.iter().enumerate() {
        layers[l].push(i);
    }

    order_layers(
        children,
        &mut layers,
        &layer,
        &incoming,
        &outgoing,
        &component_rank,
        &isolated,
    );

    if horizontal {
        layout_layered_horizontal(children, &layers, parent, gap);
    } else {
        layout_layered_vertical(children, &layers, parent, gap);
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

    fit_children_to_parent(children, parent);
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

    fit_children_to_parent(children, parent);
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

    fit_children_to_parent(children, parent);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn node_sort_key(children: &[ShapeNode], node: usize) -> (String, usize) {
    (children[node].id.clone().unwrap_or_default(), node)
}

fn connected_components(incoming: &[Vec<usize>], outgoing: &[Vec<usize>]) -> Vec<usize> {
    let n = incoming.len();
    let mut component = vec![usize::MAX; n];
    let mut comp_id = 0;

    for start in 0..n {
        if component[start] != usize::MAX {
            continue;
        }
        let mut queue = VecDeque::new();
        queue.push_back(start);
        component[start] = comp_id;

        while let Some(node) = queue.pop_front() {
            for &next in outgoing[node].iter().chain(incoming[node].iter()) {
                if component[next] == usize::MAX {
                    component[next] = comp_id;
                    queue.push_back(next);
                }
            }
        }
        comp_id += 1;
    }

    component
}

fn component_ranks(
    children: &[ShapeNode],
    components: &[usize],
    incoming: &[Vec<usize>],
    outgoing: &[Vec<usize>],
) -> Vec<usize> {
    let comp_count = components.iter().copied().max().map(|c| c + 1).unwrap_or(0);
    let mut comp_nodes: Vec<Vec<usize>> = vec![Vec::new(); comp_count];
    for (node, &comp) in components.iter().enumerate() {
        comp_nodes[comp].push(node);
    }

    let mut ordered: Vec<(usize, bool, String)> = comp_nodes
        .iter()
        .enumerate()
        .map(|(comp, nodes)| {
            let connected = nodes
                .iter()
                .any(|&node| !incoming[node].is_empty() || !outgoing[node].is_empty());
            let min_id = nodes
                .iter()
                .map(|&node| children[node].id.clone().unwrap_or_default())
                .min()
                .unwrap_or_default();
            (comp, connected, min_id)
        })
        .collect();

    ordered.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut rank = vec![0; comp_count];
    for (order, (comp, _, _)) in ordered.into_iter().enumerate() {
        rank[comp] = order;
    }
    components.iter().map(|&comp| rank[comp]).collect()
}

fn assign_layers(
    children: &[ShapeNode],
    incoming: &[Vec<usize>],
    outgoing: &[Vec<usize>],
    component_rank: &[usize],
    isolated: &[bool],
) -> Vec<usize> {
    let n = children.len();
    let mut layer: Vec<usize> = vec![0; n];
    let mut in_degree: Vec<usize> = incoming.iter().map(|v| v.len()).collect();
    let mut ready: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    sort_ready(children, &mut ready, component_rank, isolated);
    let mut visited = vec![false; n];

    while let Some(node) = ready.first().copied() {
        ready.remove(0);
        if visited[node] {
            continue;
        }
        visited[node] = true;
        for &next in &outgoing[node] {
            layer[next] = layer[next].max(layer[node] + 1);
            in_degree[next] = in_degree[next].saturating_sub(1);
            if in_degree[next] == 0 {
                ready.push(next);
                sort_ready(children, &mut ready, component_rank, isolated);
            }
        }
    }

    for node in 0..n {
        if !visited[node] {
            layer[node] = incoming[node]
                .iter()
                .map(|&p| layer[p] + 1)
                .max()
                .unwrap_or(0);
        }
    }

    layer
}

fn sort_ready(
    children: &[ShapeNode],
    ready: &mut [usize],
    component_rank: &[usize],
    isolated: &[bool],
) {
    ready.sort_by(|&a, &b| {
        isolated[a]
            .cmp(&isolated[b])
            .then_with(|| component_rank[a].cmp(&component_rank[b]))
            .then_with(|| node_sort_key(children, a).cmp(&node_sort_key(children, b)))
    });
}

fn order_layers(
    children: &[ShapeNode],
    layers: &mut [Vec<usize>],
    layer: &[usize],
    incoming: &[Vec<usize>],
    outgoing: &[Vec<usize>],
    component_rank: &[usize],
    isolated: &[bool],
) {
    for rank in layers.iter_mut() {
        rank.sort_by(|&a, &b| base_order(children, a, b, component_rank, isolated));
    }

    for _ in 0..4 {
        for l in 1..layers.len() {
            sort_layer_by_neighbors(
                children,
                layers,
                layer,
                l,
                incoming,
                component_rank,
                isolated,
                true,
            );
        }
        for l in (0..layers.len().saturating_sub(1)).rev() {
            sort_layer_by_neighbors(
                children,
                layers,
                layer,
                l,
                outgoing,
                component_rank,
                isolated,
                false,
            );
        }
    }
}

fn base_order(
    children: &[ShapeNode],
    a: usize,
    b: usize,
    component_rank: &[usize],
    isolated: &[bool],
) -> std::cmp::Ordering {
    isolated[a]
        .cmp(&isolated[b])
        .then_with(|| component_rank[a].cmp(&component_rank[b]))
        .then_with(|| node_sort_key(children, a).cmp(&node_sort_key(children, b)))
}

#[allow(clippy::too_many_arguments)]
fn sort_layer_by_neighbors(
    children: &[ShapeNode],
    layers: &mut [Vec<usize>],
    layer: &[usize],
    current_layer: usize,
    neighbors: &[Vec<usize>],
    component_rank: &[usize],
    isolated: &[bool],
    use_previous: bool,
) {
    let positions = layer_positions(layers);
    let mut ranked: Vec<(usize, Option<f64>)> = layers[current_layer]
        .iter()
        .map(|&node| {
            let adjacent: Vec<f64> = neighbors[node]
                .iter()
                .filter(|&&n| {
                    if use_previous {
                        layer[n] + 1 == layer[node]
                    } else {
                        layer[n] == layer[node] + 1
                    }
                })
                .filter_map(|n| positions.get(n).copied().map(|p| p as f64))
                .collect();
            let bary = if adjacent.is_empty() {
                None
            } else {
                Some(adjacent.iter().sum::<f64>() / adjacent.len() as f64)
            };
            (node, bary)
        })
        .collect();

    ranked.sort_by(|a, b| match (a.1, b.1) {
        (Some(ba), Some(bb)) => ba
            .partial_cmp(&bb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| base_order(children, a.0, b.0, component_rank, isolated)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => base_order(children, a.0, b.0, component_rank, isolated),
    });

    layers[current_layer] = ranked.into_iter().map(|(node, _)| node).collect();
}

fn layer_positions(layers: &[Vec<usize>]) -> HashMap<usize, usize> {
    let mut positions = HashMap::new();
    for layer in layers {
        for (pos, &node) in layer.iter().enumerate() {
            positions.insert(node, pos);
        }
    }
    positions
}

#[derive(Debug)]
struct WrappedLine {
    nodes: Vec<usize>,
    main_size: f64,
    cross_size: f64,
}

fn layout_layered_vertical(
    children: &mut [ShapeNode],
    layers: &[Vec<usize>],
    parent: &Bounds,
    gap: f64,
) {
    let layer_rows: Vec<Vec<WrappedLine>> = layers
        .iter()
        .map(|layer_nodes| wrap_layer_rows(children, layer_nodes, parent.width, gap))
        .collect();
    let layer_heights: Vec<f64> = layer_rows
        .iter()
        .map(|rows| layer_group_cross_size(rows, gap))
        .collect();
    let layer_gap = fit_gap(parent.height, &layer_heights, gap);

    let mut y = parent.y;
    for (layer_idx, rows) in layer_rows.into_iter().enumerate() {
        if rows.is_empty() {
            continue;
        }
        for row in rows {
            let mut x = if row.main_size <= parent.width {
                parent.x + (parent.width - row.main_size) / 2.0
            } else {
                parent.x
            };
            for node_idx in row.nodes {
                children[node_idx].resolved.x = x;
                children[node_idx].resolved.y =
                    y + (row.cross_size - children[node_idx].resolved.height) / 2.0;
                x += children[node_idx].resolved.width + gap;
            }
            y += row.cross_size + gap;
        }
        y -= gap;
        if layer_idx + 1 < layer_heights.len() {
            y += layer_gap;
        }
    }
}

fn layout_layered_horizontal(
    children: &mut [ShapeNode],
    layers: &[Vec<usize>],
    parent: &Bounds,
    gap: f64,
) {
    let layer_columns: Vec<Vec<WrappedLine>> = layers
        .iter()
        .map(|layer_nodes| wrap_layer_columns(children, layer_nodes, parent.height, gap))
        .collect();
    let layer_widths: Vec<f64> = layer_columns
        .iter()
        .map(|columns| layer_group_cross_size(columns, gap))
        .collect();
    let layer_gap = fit_gap(parent.width, &layer_widths, gap);

    let mut x = parent.x;
    for (layer_idx, columns) in layer_columns.into_iter().enumerate() {
        if columns.is_empty() {
            continue;
        }
        for column in columns {
            let mut y = if column.main_size <= parent.height {
                parent.y + (parent.height - column.main_size) / 2.0
            } else {
                parent.y
            };
            for node_idx in column.nodes {
                children[node_idx].resolved.x =
                    x + (column.cross_size - children[node_idx].resolved.width) / 2.0;
                children[node_idx].resolved.y = y;
                y += children[node_idx].resolved.height + gap;
            }
            x += column.cross_size + gap;
        }
        x -= gap;
        if layer_idx + 1 < layer_widths.len() {
            x += layer_gap;
        }
    }
}

fn layer_group_cross_size(lines: &[WrappedLine], gap: f64) -> f64 {
    let line_sizes: f64 = lines.iter().map(|line| line.cross_size).sum();
    let gaps = lines.len().saturating_sub(1) as f64 * gap;
    line_sizes + gaps
}

fn fit_gap(available: f64, group_sizes: &[f64], requested_gap: f64) -> f64 {
    if group_sizes.len() <= 1 {
        return 0.0;
    }
    let total_groups: f64 = group_sizes.iter().sum();
    let max_gap =
        ((available - total_groups) / group_sizes.len().saturating_sub(1) as f64).max(0.0);
    requested_gap.min(max_gap)
}

fn wrap_layer_rows(
    children: &[ShapeNode],
    layer_nodes: &[usize],
    available_width: f64,
    gap: f64,
) -> Vec<WrappedLine> {
    wrap_layer(layer_nodes, gap, available_width, |node_idx| {
        (
            children[node_idx].resolved.width,
            children[node_idx].resolved.height,
        )
    })
}

fn wrap_layer_columns(
    children: &[ShapeNode],
    layer_nodes: &[usize],
    available_height: f64,
    gap: f64,
) -> Vec<WrappedLine> {
    wrap_layer(layer_nodes, gap, available_height, |node_idx| {
        (
            children[node_idx].resolved.height,
            children[node_idx].resolved.width,
        )
    })
}

fn wrap_layer<F>(
    layer_nodes: &[usize],
    gap: f64,
    available_main: f64,
    size_of: F,
) -> Vec<WrappedLine>
where
    F: Fn(usize) -> (f64, f64),
{
    let mut lines = Vec::new();
    let mut current = WrappedLine {
        nodes: Vec::new(),
        main_size: 0.0,
        cross_size: 0.0,
    };

    for &node_idx in layer_nodes {
        let (node_main, node_cross) = size_of(node_idx);
        let next_main = if current.nodes.is_empty() {
            node_main
        } else {
            current.main_size + gap + node_main
        };

        if !current.nodes.is_empty() && next_main > available_main {
            lines.push(current);
            current = WrappedLine {
                nodes: Vec::new(),
                main_size: 0.0,
                cross_size: 0.0,
            };
        }

        if !current.nodes.is_empty() {
            current.main_size += gap;
        }
        current.nodes.push(node_idx);
        current.main_size += node_main;
        current.cross_size = current.cross_size.max(node_cross);
    }

    if !current.nodes.is_empty() {
        lines.push(current);
    }

    lines
}

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

fn fit_children_to_parent(children: &mut [ShapeNode], parent: &Bounds) {
    if children.is_empty() {
        return;
    }

    let Some(bounds) = children_bounds(children) else {
        return;
    };

    let dx = if bounds.x < parent.x {
        parent.x - bounds.x
    } else if bounds.x + bounds.width > parent.x + parent.width {
        parent.x + parent.width - bounds.width - bounds.x
    } else {
        0.0
    };
    let dy = if bounds.y < parent.y {
        parent.y - bounds.y
    } else if bounds.y + bounds.height > parent.y + parent.height {
        parent.y + parent.height - bounds.height - bounds.y
    } else {
        0.0
    };

    for child in children.iter_mut() {
        child.resolved.x += dx;
        child.resolved.y += dy;
        clamp_child_to_parent(child, parent);
    }
}

fn children_bounds(children: &[ShapeNode]) -> Option<Bounds> {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut found = false;

    for child in children {
        found = true;
        min_x = min_x.min(child.resolved.x);
        min_y = min_y.min(child.resolved.y);
        max_x = max_x.max(child.resolved.x + child.resolved.width);
        max_y = max_y.max(child.resolved.y + child.resolved.height);
    }

    found.then_some(Bounds {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(0.0),
        height: (max_y - min_y).max(0.0),
    })
}

fn clamp_child_to_parent(child: &mut ShapeNode, parent: &Bounds) {
    child.resolved.x = clamp_origin(
        child.resolved.x,
        child.resolved.width,
        parent.x,
        parent.width,
    );
    child.resolved.y = clamp_origin(
        child.resolved.y,
        child.resolved.height,
        parent.y,
        parent.height,
    );
}

fn clamp_origin(origin: f64, size: f64, parent_origin: f64, parent_size: f64) -> f64 {
    if size >= parent_size {
        parent_origin
    } else {
        origin.clamp(parent_origin, parent_origin + parent_size - size)
    }
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

    fn overlaps(a: &ShapeNode, b: &ShapeNode) -> bool {
        a.resolved.x < b.resolved.x + b.resolved.width
            && a.resolved.x + a.resolved.width > b.resolved.x
            && a.resolved.y < b.resolved.y + b.resolved.height
            && a.resolved.y + a.resolved.height > b.resolved.y
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
    fn test_layered_keeps_node_bounds_inside_parent() {
        let mut nodes = vec![
            make_node("a", 80.0, 40.0),
            make_node("b", 80.0, 40.0),
            make_node("c", 80.0, 40.0),
        ];
        let conns = vec![make_conn("a", "b"), make_conn("b", "c")];
        let parent = Bounds {
            x: 20.0,
            y: 30.0,
            width: 160.0,
            height: 120.0,
        };
        layout_layered(&mut nodes, &conns, &parent, 40.0, &IndexMap::new());

        for node in nodes {
            assert!(node.resolved.x >= parent.x);
            assert!(node.resolved.y >= parent.y);
            assert!(node.resolved.x + node.resolved.width <= parent.x + parent.width);
            assert!(node.resolved.y + node.resolved.height <= parent.y + parent.height);
        }
    }

    #[test]
    fn test_layered_wraps_wide_rank_without_overlap() {
        let mut nodes = vec![
            make_node("a", 80.0, 40.0),
            make_node("b", 80.0, 40.0),
            make_node("c", 80.0, 40.0),
        ];
        let conns = vec![make_conn("a", "c"), make_conn("b", "c")];
        let parent = Bounds {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 240.0,
        };
        layout_layered(&mut nodes, &conns, &parent, 20.0, &IndexMap::new());

        assert_eq!(nodes[0].resolved.width, 80.0);
        assert_eq!(nodes[1].resolved.width, 80.0);
        assert!(!overlaps(&nodes[0], &nodes[1]));
        assert!(nodes[0].resolved.y < nodes[1].resolved.y);
    }

    #[test]
    fn test_layered_orders_rank_by_topology_not_declaration() {
        let mut nodes = vec![
            make_node("s2", 40.0, 30.0),
            make_node("s1", 40.0, 30.0),
            make_node("t2", 40.0, 30.0),
            make_node("t1", 40.0, 30.0),
        ];
        let conns = vec![make_conn("s1", "t1"), make_conn("s2", "t2")];
        let parent = Bounds {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 180.0,
        };
        layout_layered(&mut nodes, &conns, &parent, 20.0, &IndexMap::new());

        assert!(nodes[1].resolved.x < nodes[0].resolved.x);
        assert!(nodes[3].resolved.x < nodes[2].resolved.x);
    }

    #[test]
    fn test_layered_places_isolated_nodes_after_connected_components() {
        let mut nodes = vec![
            make_node("z_iso", 40.0, 30.0),
            make_node("a", 40.0, 30.0),
            make_node("b", 40.0, 30.0),
        ];
        let conns = vec![make_conn("a", "b")];
        let parent = Bounds {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 180.0,
        };
        layout_layered(&mut nodes, &conns, &parent, 20.0, &IndexMap::new());

        assert!(nodes[1].resolved.x < nodes[0].resolved.x);
    }

    #[test]
    fn test_layered_horizontal_wraps_tall_rank_without_overlap() {
        let mut nodes = vec![
            make_node("a", 40.0, 80.0),
            make_node("b", 40.0, 80.0),
            make_node("c", 40.0, 80.0),
        ];
        let conns = vec![make_conn("a", "c"), make_conn("b", "c")];
        let parent = Bounds {
            x: 0.0,
            y: 0.0,
            width: 240.0,
            height: 120.0,
        };
        let opts = [("direction".to_string(), "horizontal".to_string())]
            .into_iter()
            .collect();
        layout_layered(&mut nodes, &conns, &parent, 20.0, &opts);

        assert_eq!(nodes[0].resolved.height, 80.0);
        assert_eq!(nodes[1].resolved.height, 80.0);
        assert!(!overlaps(&nodes[0], &nodes[1]));
        assert!(nodes[0].resolved.x < nodes[1].resolved.x);
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

    #[test]
    fn test_grid_clamps_oversized_grid_inside_parent() {
        let mut nodes = vec![make_node("a", 80.0, 40.0), make_node("b", 80.0, 40.0)];
        let parent = Bounds {
            x: 10.0,
            y: 10.0,
            width: 120.0,
            height: 60.0,
        };
        let mut opts = IndexMap::new();
        opts.insert("columns".to_string(), "2".to_string());
        layout_grid(&mut nodes, &[], &parent, 40.0, &opts);

        for node in nodes {
            assert!(node.resolved.x >= parent.x);
            assert!(node.resolved.y >= parent.y);
            assert!(node.resolved.x + node.resolved.width <= parent.x + parent.width);
            assert!(node.resolved.y + node.resolved.height <= parent.y + parent.height);
        }
    }
}
