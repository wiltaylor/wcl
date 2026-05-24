//! Orthogonal edge routing for diagram connections.
//!
//! `route_elbow` finds an orthogonal (right-angled) polyline path
//! between two connect-point anchors, routing around any other
//! shape bounding boxes via A* on a coarse routing grid. Endpoint
//! direction is constrained by the `Side` the anchor lives on, so
//! the first/last leg always leaves perpendicular to the shape
//! outline.
//!
//! `separate_edges` runs after the per-edge routing to nudge
//! parallel middle segments apart so multiple edges sharing a
//! corridor remain individually visible. Endpoint segments (those
//! touching `connect_points`) are intentionally never moved, so
//! edges leaving the same anchor stay aligned.

use std::collections::{BinaryHeap, HashMap};

/// Side of a shape's bounding box that an anchor sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Side {
    North,
    East,
    South,
    West,
}

impl Side {
    pub(crate) fn from_symbol(s: &str) -> Option<Side> {
        match s {
            "north" => Some(Side::North),
            "east" => Some(Side::East),
            "south" => Some(Side::South),
            "west" => Some(Side::West),
            _ => None,
        }
    }

    /// Unit vector pointing away from the shape on this side.
    fn outward(self) -> (i32, i32) {
        match self {
            Side::North => (0, -1),
            Side::East => (1, 0),
            Side::South => (0, 1),
            Side::West => (-1, 0),
        }
    }
}

/// An obstacle bounding box in SVG coords.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Obstacle {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

const CELL: f64 = 10.0;
const PAD: f64 = 4.0;
const TURN_PENALTY: i32 = 5; // multiplied by 10 with straight-step cost to favor straight runs

/// Route an orthogonal polyline from `src` to `dst`.
///
/// `src_side` / `dst_side` constrain the first / last leg direction
/// so the path leaves perpendicular to each shape. `obstacles` are
/// other shapes to route around (the source and destination's own
/// bboxes should be excluded by the caller). `viewport` is the
/// diagram size, used to bound the search grid.
///
/// Returns a polyline including both endpoints. On A* failure the
/// fallback is a direct two-segment elbow ignoring obstacles, so a
/// path is always returned.
pub(crate) fn route_elbow(
    src: (f64, f64),
    src_side: Side,
    dst: (f64, f64),
    dst_side: Side,
    obstacles: &[Obstacle],
    viewport: (f64, f64),
) -> Vec<(f64, f64)> {
    if let Some(path) = astar_route(src, src_side, dst, dst_side, obstacles, viewport) {
        return snap_endpoints(path, src, dst);
    }
    fallback_elbow(src, src_side, dst)
}

// ── A* search ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Cell {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Node {
    cell: Cell,
    dir: (i32, i32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct OpenEntry {
    f: i32,
    g: i32,
    node: Node,
}

impl Ord for OpenEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Lower f wins; BinaryHeap is max-heap so reverse.
        other.f.cmp(&self.f).then(other.g.cmp(&self.g))
    }
}
impl PartialOrd for OpenEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn astar_route(
    src: (f64, f64),
    src_side: Side,
    dst: (f64, f64),
    dst_side: Side,
    obstacles: &[Obstacle],
    viewport: (f64, f64),
) -> Option<Vec<(f64, f64)>> {
    let (gw, gh) = (
        (viewport.0 / CELL).ceil() as i32 + 2,
        (viewport.1 / CELL).ceil() as i32 + 2,
    );
    let blocked = build_blocked_grid(obstacles, gw, gh);
    let start_cell = snap(src);
    let goal_cell = snap(dst);
    if start_cell == goal_cell {
        return Some(vec![src, dst]);
    }
    // Unblock the start and goal cells so we can enter / leave them
    // even if they sit inside an obstacle's inflated bbox.
    let mut blocked = blocked;
    set_blocked(&mut blocked, gw, start_cell, false);
    set_blocked(&mut blocked, gw, goal_cell, false);

    let goal_dir = dst_side.outward();
    let start_dir = src_side.outward();

    let start = Node {
        cell: start_cell,
        dir: start_dir,
    };
    let mut came_from: HashMap<Node, Node> = HashMap::new();
    let mut g_score: HashMap<Node, i32> = HashMap::new();
    g_score.insert(start, 0);
    let mut open = BinaryHeap::new();
    open.push(OpenEntry {
        f: manhattan(start_cell, goal_cell),
        g: 0,
        node: start,
    });

    while let Some(OpenEntry { node, g, .. }) = open.pop() {
        if node.cell == goal_cell {
            // Reached goal: the final move must arrive *from* the
            // direction `goal_dir` points outward, i.e. the move
            // direction equals `(-goal_dir.0, -goal_dir.1)`.
            let arriving = (-goal_dir.0, -goal_dir.1);
            if node.dir == arriving || node.dir == (0, 0) {
                return Some(reconstruct(&came_from, node, src, dst));
            }
            // Otherwise keep searching; another node-arrival from
            // the right direction may surface later.
            continue;
        }
        if g > *g_score.get(&node).unwrap_or(&i32::MAX) {
            continue;
        }
        for &mv in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            // For the very first move from the start, force the
            // egress direction so the polyline leaves perpendicular.
            if node.cell == start_cell && mv != start_dir {
                continue;
            }
            let next = Cell {
                x: node.cell.x + mv.0,
                y: node.cell.y + mv.1,
            };
            if next.x < 0 || next.y < 0 || next.x >= gw || next.y >= gh {
                continue;
            }
            if is_blocked(&blocked, gw, next) {
                continue;
            }
            let straight = node.dir == (0, 0) || node.dir == mv;
            let step_cost = 10 + if straight { 0 } else { TURN_PENALTY };
            let tentative_g = g + step_cost;
            let next_node = Node {
                cell: next,
                dir: mv,
            };
            if tentative_g < *g_score.get(&next_node).unwrap_or(&i32::MAX) {
                g_score.insert(next_node, tentative_g);
                came_from.insert(next_node, node);
                let f = tentative_g + manhattan(next, goal_cell) * 10;
                open.push(OpenEntry {
                    f,
                    g: tentative_g,
                    node: next_node,
                });
            }
        }
    }
    None
}

fn build_blocked_grid(obstacles: &[Obstacle], gw: i32, gh: i32) -> Vec<bool> {
    let mut grid = vec![false; (gw * gh) as usize];
    for o in obstacles {
        let (x0, y0, x1, y1) = (
            ((o.x - PAD) / CELL).floor() as i32,
            ((o.y - PAD) / CELL).floor() as i32,
            ((o.x + o.w + PAD) / CELL).ceil() as i32,
            ((o.y + o.h + PAD) / CELL).ceil() as i32,
        );
        for cy in y0..y1 {
            for cx in x0..x1 {
                if cx < 0 || cy < 0 || cx >= gw || cy >= gh {
                    continue;
                }
                grid[(cy * gw + cx) as usize] = true;
            }
        }
    }
    grid
}

fn set_blocked(grid: &mut [bool], gw: i32, c: Cell, v: bool) {
    if c.x < 0 || c.y < 0 {
        return;
    }
    let idx = (c.y * gw + c.x) as usize;
    if idx < grid.len() {
        grid[idx] = v;
    }
}

fn is_blocked(grid: &[bool], gw: i32, c: Cell) -> bool {
    grid.get((c.y * gw + c.x) as usize).copied().unwrap_or(true)
}

fn snap(p: (f64, f64)) -> Cell {
    Cell {
        x: (p.0 / CELL).round() as i32,
        y: (p.1 / CELL).round() as i32,
    }
}

fn manhattan(a: Cell, b: Cell) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn reconstruct(
    came_from: &HashMap<Node, Node>,
    end: Node,
    src: (f64, f64),
    dst: (f64, f64),
) -> Vec<(f64, f64)> {
    // Walk back through came_from, collecting cells, then simplify
    // runs in the same direction into single segments.
    let mut cells: Vec<Cell> = Vec::new();
    let mut cur = end;
    cells.push(cur.cell);
    while let Some(&prev) = came_from.get(&cur) {
        cells.push(prev.cell);
        cur = prev;
    }
    cells.reverse();
    let pts = cells_to_polyline(&cells);
    snap_endpoints(pts, src, dst)
}

fn cells_to_polyline(cells: &[Cell]) -> Vec<(f64, f64)> {
    if cells.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(f64, f64)> = Vec::new();
    let first = cells[0];
    out.push((first.x as f64 * CELL, first.y as f64 * CELL));
    if cells.len() == 1 {
        return out;
    }
    let mut prev_dir: Option<(i32, i32)> = None;
    for w in cells.windows(2) {
        let a = w[0];
        let b = w[1];
        let dir = (b.x - a.x, b.y - a.y);
        match prev_dir {
            Some(p) if p == dir => {
                // Same direction — extend last point.
                if let Some(last) = out.last_mut() {
                    *last = (b.x as f64 * CELL, b.y as f64 * CELL);
                }
            }
            _ => {
                out.push((b.x as f64 * CELL, b.y as f64 * CELL));
            }
        }
        prev_dir = Some(dir);
    }
    out
}

/// Replace the first and last points with the exact anchor coords
/// (the grid-snapped versions may be off by up to half a cell).
fn snap_endpoints(mut pts: Vec<(f64, f64)>, src: (f64, f64), dst: (f64, f64)) -> Vec<(f64, f64)> {
    if pts.is_empty() {
        return vec![src, dst];
    }
    pts[0] = src;
    *pts.last_mut().unwrap() = dst;
    pts
}

fn fallback_elbow(src: (f64, f64), src_side: Side, dst: (f64, f64)) -> Vec<(f64, f64)> {
    // Step out the egress side by one cell, then over to dst.
    let step = CELL;
    let (dx, dy) = src_side.outward();
    let mid = (src.0 + dx as f64 * step, src.1 + dy as f64 * step);
    // Depending on egress axis, route the corner via mid then go
    // straight to dst's matching coordinate.
    if dx != 0 {
        // Horizontal egress: corner at (mid.x, dst.y).
        vec![src, mid, (mid.0, dst.1), dst]
    } else {
        // Vertical egress: corner at (dst.x, mid.y).
        vec![src, mid, (dst.0, mid.1), dst]
    }
}

// ── Edge separation ────────────────────────────────────────────────

/// Each edge passes through as a polyline plus its declared source
/// and destination anchor coords (used to identify shared-anchor
/// edges that should be left aligned).
#[derive(Clone)]
pub(crate) struct EdgePath {
    pub points: Vec<(f64, f64)>,
}

/// Nudge parallel middle segments of `paths` so they don't sit on
/// top of each other. Endpoint segments (those touching the source
/// or destination anchor) are never moved — that preserves edges
/// leaving a shared `connect_point`.
pub(crate) fn separate_edges(paths: &mut [EdgePath], step: f64) {
    if step <= 0.0 || paths.len() < 2 {
        return;
    }
    // For each path, list its middle-segment indices (between two
    // bends). A segment with index `i` lies between
    // `points[i]` and `points[i+1]`. An "endpoint" segment is the
    // first (index 0) or last (index points.len() - 2). Only
    // middle segments are eligible.
    #[derive(Clone, Copy)]
    struct Seg {
        path_idx: usize,
        seg_idx: usize,
        axis: Axis,
        fixed: f64,
        lo: f64,
        hi: f64,
    }
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Axis {
        Horizontal,
        Vertical,
    }

    let mut segs: Vec<Seg> = Vec::new();
    for (pi, p) in paths.iter().enumerate() {
        let pts = &p.points;
        if pts.len() < 4 {
            // Need at least 3 segments to have a middle one.
            continue;
        }
        for si in 1..(pts.len() - 2) {
            let (a, b) = (pts[si], pts[si + 1]);
            if (a.1 - b.1).abs() < 1e-6 {
                let (lo, hi) = if a.0 < b.0 { (a.0, b.0) } else { (b.0, a.0) };
                segs.push(Seg {
                    path_idx: pi,
                    seg_idx: si,
                    axis: Axis::Horizontal,
                    fixed: a.1,
                    lo,
                    hi,
                });
            } else if (a.0 - b.0).abs() < 1e-6 {
                let (lo, hi) = if a.1 < b.1 { (a.1, b.1) } else { (b.1, a.1) };
                segs.push(Seg {
                    path_idx: pi,
                    seg_idx: si,
                    axis: Axis::Vertical,
                    fixed: a.0,
                    lo,
                    hi,
                });
            }
        }
    }

    // Group overlapping segments via union-find on the segment list.
    let n = segs.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while parent[r] != r {
            r = parent[r];
        }
        let mut cur = i;
        while parent[cur] != r {
            let nx = parent[cur];
            parent[cur] = r;
            cur = nx;
        }
        r
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if segs[i].axis != segs[j].axis {
                continue;
            }
            if (segs[i].fixed - segs[j].fixed).abs() > 1e-6 {
                continue;
            }
            // Ranges must overlap (open intervals — touching at
            // endpoints doesn't count as a corridor share).
            if segs[i].hi <= segs[j].lo + 1e-6 || segs[j].hi <= segs[i].lo + 1e-6 {
                continue;
            }
            let ri = find(&mut parent, i);
            let rj = find(&mut parent, j);
            if ri != rj {
                parent[ri] = rj;
            }
        }
    }

    // Bucket segments by group root.
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }

    // Compute nudge offset per segment.
    let mut nudges: HashMap<(usize, usize), (f64, f64)> = HashMap::new();
    for (_, members) in groups {
        if members.len() < 2 {
            continue;
        }
        let mut members = members;
        members.sort_by_key(|&i| (segs[i].path_idx, segs[i].seg_idx));
        let count = members.len();
        let center = (count as f64 - 1.0) / 2.0;
        for (rank, &idx) in members.iter().enumerate() {
            let off = (rank as f64 - center) * step;
            let seg = segs[idx];
            let nudge = match seg.axis {
                Axis::Horizontal => (0.0, off),
                Axis::Vertical => (off, 0.0),
            };
            nudges.insert((seg.path_idx, seg.seg_idx), nudge);
        }
    }

    // Apply nudges: shift both endpoints of each nudged middle
    // segment perpendicular to the segment axis. The adjoining
    // segments (which are perpendicular) thus extend/shrink to
    // meet the new corner — they keep their fixed coordinates.
    for ((path_idx, seg_idx), (dx, dy)) in nudges {
        let p = &mut paths[path_idx].points;
        p[seg_idx].0 += dx;
        p[seg_idx].1 += dy;
        p[seg_idx + 1].0 += dx;
        p[seg_idx + 1].1 += dy;
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_horizontal_path_has_no_bends() {
        let pts = route_elbow(
            (50.0, 100.0),
            Side::East,
            (250.0, 100.0),
            Side::West,
            &[],
            (320.0, 200.0),
        );
        // Start, end and no intermediate bends (or only redundant ones at the same y).
        assert!(pts.len() >= 2);
        assert!(pts.iter().all(|(_, y)| (y - 100.0).abs() < 1e-6));
    }

    #[test]
    fn route_takes_a_bend_around_an_obstacle() {
        // Source and destination at same y; an obstacle dead in between.
        let pts = route_elbow(
            (50.0, 100.0),
            Side::East,
            (250.0, 100.0),
            Side::West,
            &[Obstacle {
                x: 130.0,
                y: 80.0,
                w: 40.0,
                h: 40.0,
            }],
            (320.0, 200.0),
        );
        // At least 3 points (one bend), and at least one y differs
        // from the straight-line y.
        assert!(pts.len() >= 3, "expected bends, got {:?}", pts);
        assert!(
            pts.iter().any(|(_, y)| (y - 100.0).abs() > 1.0),
            "expected the polyline to deviate from y=100 to clear the obstacle, got {:?}",
            pts
        );
    }

    #[test]
    fn fallback_elbow_produces_valid_polyline_when_grid_unreachable() {
        // viewport so small that A* will fail; the fallback should
        // still produce a path with both endpoints.
        let pts = route_elbow(
            (50.0, 50.0),
            Side::East,
            (60.0, 60.0),
            Side::West,
            &[],
            (5.0, 5.0),
        );
        assert!(pts.len() >= 2);
        assert_eq!(*pts.first().unwrap(), (50.0, 50.0));
        assert_eq!(*pts.last().unwrap(), (60.0, 60.0));
    }

    #[test]
    fn separate_edges_nudges_two_parallel_middle_segments() {
        // Two paths, each: src -> bend -> bend -> dst, sharing a
        // long horizontal middle segment at y=100. With `step = 4`,
        // adjacent edges sit 4 apart — so the two-edge case is
        // ±2 from the original corridor.
        let mut paths = vec![
            EdgePath {
                points: vec![(10.0, 50.0), (10.0, 100.0), (200.0, 100.0), (200.0, 150.0)],
            },
            EdgePath {
                points: vec![(20.0, 60.0), (20.0, 100.0), (210.0, 100.0), (210.0, 160.0)],
            },
        ];
        separate_edges(&mut paths, 4.0);
        let y1 = paths[0].points[1].1;
        let y2 = paths[1].points[1].1;
        assert!(
            ((y1 - 98.0).abs() < 1e-6 && (y2 - 102.0).abs() < 1e-6)
                || ((y1 - 102.0).abs() < 1e-6 && (y2 - 98.0).abs() < 1e-6),
            "expected y1/y2 near 98/102, got {y1}/{y2}"
        );
        // The trailing vertical segment's top y must follow the
        // horizontal corridor (corner alignment).
        assert!((paths[0].points[2].1 - paths[0].points[1].1).abs() < 1e-6);
        assert!((paths[1].points[2].1 - paths[1].points[1].1).abs() < 1e-6);
    }

    #[test]
    fn separate_edges_leaves_shared_anchor_first_segment_alone() {
        // Both paths start at the SAME anchor (10, 50) and head
        // east, so their first segment overlaps by design. They
        // diverge in their middle segments; only those should be
        // nudged.
        let mut paths = vec![
            EdgePath {
                points: vec![(10.0, 50.0), (100.0, 50.0), (100.0, 30.0), (200.0, 30.0)],
            },
            EdgePath {
                points: vec![(10.0, 50.0), (100.0, 50.0), (100.0, 70.0), (200.0, 70.0)],
            },
        ];
        separate_edges(&mut paths, 4.0);
        // Both first segments still start at (10, 50). The first
        // segment is index 0 — never nudged.
        assert_eq!(paths[0].points[0], (10.0, 50.0));
        assert_eq!(paths[1].points[0], (10.0, 50.0));
        // And the shared horizontal y=50 segment (also index 0) is
        // not nudged on either path.
        assert!((paths[0].points[1].1 - 50.0).abs() < 1e-6);
        assert!((paths[1].points[1].1 - 50.0).abs() < 1e-6);
    }
}
