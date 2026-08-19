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
    /// The top edge.
    North,
    /// The right edge.
    East,
    /// The bottom edge.
    South,
    /// The left edge.
    West,
}

impl Side {
    /// Parse an author-written anchor symbol (`:north`, …).
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
    /// Left edge in SVG coordinates.
    pub x: f64,
    /// Top edge in SVG coordinates.
    pub y: f64,
    /// Width in SVG units.
    pub w: f64,
    /// Height in SVG units.
    pub h: f64,
}

/// Routing grid pitch in SVG units. Coarser means faster search
/// and blockier routes.
const CELL: f64 = 10.0;
/// Clearance kept around each obstacle, in SVG units.
const PAD: f64 = 4.0;
/// Extra cost charged when a route changes direction. Multiplied by
/// ten against the straight-step cost, which is what makes the search
/// prefer long straight runs over shorter zigzags.
const TURN_PENALTY: i32 = 5;
/// Extra per-cell cost for routing a cell that sits on a *visible*
/// container border line. A run *along* a border pays this every cell
/// (so it's strongly avoided), while a perpendicular *crossing* pays it
/// once (so it's still allowed). Never blocks — only costs.
const BORDER_PENALTY: i32 = 8;
/// How close (SVG units) a cell must be to a border line to be penalised.
/// One `CELL`: penalises exactly the lane coincident with the border and
/// leaves the `±CELL` lanes on either side clean.
const BORDER_CLEARANCE: f64 = CELL;

/// Route an orthogonal polyline from `src` to `dst`.
///
/// `src_side` / `dst_side` constrain the first / last leg direction
/// so the path leaves perpendicular to each shape. `obstacles` are
/// other shapes to route around (the source and destination's own
/// bboxes should be excluded by the caller). `borders` are visible
/// container boxes the route is *penalised* (not forbidden) from
/// running flush along, so an edge doesn't merge into a boundary line.
/// `viewport` is the diagram size, used to bound the search grid.
///
/// Returns a polyline including both endpoints, or `None` when no
/// obstacle-free orthogonal path exists. The first attempt keeps the
/// normal `PAD` breathing room; on failure the obstacle padding is
/// relaxed (`PAD → 1 → 0`) so a corridor that's genuinely routable but
/// merely quantized away by the coarse grid can still be threaded. We
/// deliberately do *not* fall back to an obstacle-ignoring elbow — a
/// caller that gets `None` should surface a diagnostic rather than draw
/// a line straight through a shape.
pub(crate) fn route_elbow(
    src: (f64, f64),
    src_side: Side,
    dst: (f64, f64),
    dst_side: Side,
    obstacles: &[Obstacle],
    borders: &[(f64, f64, f64, f64)],
    viewport: (f64, f64),
) -> Option<Vec<(f64, f64)>> {
    // Pre-flight the grid size once so an over-cap diagram gets a
    // size diagnostic instead of the misleading "too tightly packed"
    // message the caller records for an exhausted search.
    if grid_dims(src, dst, obstacles, viewport, PAD).is_none() {
        crate::render::record_route_error(format!(
            "diagram is too large to route edges: content extends to roughly \
             ({:.0}, {:.0}) SVG units, beyond the routing grid's capacity. \
             Reduce the diagram's coordinates/size, or set routing: \"straight\".",
            viewport.0.max(src.0).max(dst.0),
            viewport.1.max(src.1).max(dst.1),
        ));
        return None;
    }
    for pad in [PAD, 1.0, 0.0] {
        // `borders` is the same at every pad: the penalty is a soft cost,
        // independent of the obstacle-padding relaxation.
        if let Some(path) = astar_route(
            src, src_side, dst, dst_side, obstacles, borders, viewport, pad,
        ) {
            return Some(snap_endpoints(path, src, src_side, dst, dst_side));
        }
    }
    None
}

/// Hard ceiling on the routing grid's total cell count. Beyond this
/// the search is hopeless anyway, and the cell math / allocation would
/// otherwise overflow i32 or OOM on absurd user coordinates.
const MAX_GRID_CELLS: i64 = 4_000_000;

/// Size the grid to cover the declared viewport *and* all content
/// (obstacles + both endpoints), then add a routing margin so an edge
/// has room to detour below / right of a tightly-packed row instead of
/// being boxed in by the grid edge. Sizing from content also keeps
/// routing working when a diagram omits `width`/`height` (viewport 0).
///
/// Sized in i64 and capped at [`MAX_GRID_CELLS`]; `None` means the
/// coordinates are too large to route.
fn grid_dims(
    src: (f64, f64),
    dst: (f64, f64),
    obstacles: &[Obstacle],
    viewport: (f64, f64),
    pad: f64,
) -> Option<(i32, i32)> {
    const MARGIN_CELLS: i64 = 4;
    let mut max_x = viewport.0.max(src.0).max(dst.0);
    let mut max_y = viewport.1.max(src.1).max(dst.1);
    for o in obstacles {
        max_x = max_x.max(o.x + o.w + pad);
        max_y = max_y.max(o.y + o.h + pad);
    }
    let gw = ((max_x / CELL).ceil() as i64).saturating_add(2 + MARGIN_CELLS);
    let gh = ((max_y / CELL).ceil() as i64).saturating_add(2 + MARGIN_CELLS);
    if gw <= 0 || gh <= 0 || gw.saturating_mul(gh) > MAX_GRID_CELLS {
        return None;
    }
    Some((gw as i32, gh as i32))
}

// ── A* search ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
/// One cell of the routing grid, in grid coordinates.
struct Cell {
    /// Column index.
    x: i32,
    /// Row index.
    y: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
/// A search node: a cell plus the direction it was entered from, so
/// the turn penalty can be charged when the direction changes.
struct Node {
    /// The grid cell.
    cell: Cell,
    /// Unit direction of travel into this cell.
    dir: (i32, i32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
/// An entry in the A* open set. `Ord` is reversed so `BinaryHeap`
/// yields the lowest `f` first.
struct OpenEntry {
    /// Estimated total cost: `g` plus the heuristic.
    f: i32,
    /// Cost accumulated so far.
    g: i32,
    /// The node this entry stands for.
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

#[allow(clippy::too_many_arguments)] // internal pathfinder; bundling these obscures more than it helps
/// Find a low-cost orthogonal route between two points, avoiding the
/// blocked cells and preferring straight runs.
fn astar_route(
    src: (f64, f64),
    src_side: Side,
    dst: (f64, f64),
    dst_side: Side,
    obstacles: &[Obstacle],
    borders: &[(f64, f64, f64, f64)],
    viewport: (f64, f64),
    pad: f64,
) -> Option<Vec<(f64, f64)>> {
    let (gw, gh) = grid_dims(src, dst, obstacles, viewport, pad)?;
    let blocked = build_blocked_grid(obstacles, gw, gh, pad);
    let start_cell = snap(src);
    let goal_cell = snap(dst);
    if start_cell == goal_cell {
        return Some(vec![src, dst]);
    }
    // Unblock the start and goal cells so we can enter / leave them
    // even if they sit inside an obstacle's inflated bbox. Also
    // unblock the cell *adjacent* to each — for start, the cell in
    // the egress direction (the first move target); for goal, the
    // cell in the ingress direction (the cell the arriving move
    // comes from). Without this, the source/destination shape's
    // PAD inflation can block the only valid first / last move.
    let mut blocked = blocked;
    let goal_dir = dst_side.outward();
    let start_dir = src_side.outward();
    set_blocked(&mut blocked, gw, start_cell, false);
    set_blocked(&mut blocked, gw, goal_cell, false);
    set_blocked(
        &mut blocked,
        gw,
        Cell {
            x: start_cell.x + start_dir.0,
            y: start_cell.y + start_dir.1,
        },
        false,
    );
    set_blocked(
        &mut blocked,
        gw,
        Cell {
            x: goal_cell.x + goal_dir.0,
            y: goal_cell.y + goal_dir.1,
        },
        false,
    );

    // Cells exempt from the border penalty: the endpoint cells and their
    // egress / ingress neighbours (the same four cells force-unblocked
    // above). An edge whose anchor sits on a bordered container must be
    // able to cross that border to leave / enter without being taxed.
    let exempt = [
        start_cell,
        goal_cell,
        Cell {
            x: start_cell.x + start_dir.0,
            y: start_cell.y + start_dir.1,
        },
        Cell {
            x: goal_cell.x + goal_dir.0,
            y: goal_cell.y + goal_dir.1,
        },
    ];

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
                return Some(reconstruct(&came_from, node, src, src_side, dst, dst_side));
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
            // Soft border avoidance: pay extra to enter a cell sitting on a
            // visible container border line, unless it's an exempt endpoint
            // cell (which must be free to cross the border).
            let pen = if exempt.contains(&next) {
                0
            } else {
                border_penalty(next, borders)
            };
            let step_cost = 10 + if straight { 0 } else { TURN_PENALTY } + pen;
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

/// Rasterise the obstacles into a blocked-cell bitmap, dilated by
/// `pad` so routes keep their distance.
fn build_blocked_grid(obstacles: &[Obstacle], gw: i32, gh: i32, pad: f64) -> Vec<bool> {
    let mut grid = vec![false; (gw * gh) as usize];
    for o in obstacles {
        let (x0, y0, x1, y1) = (
            ((o.x - pad) / CELL).floor() as i32,
            ((o.y - pad) / CELL).floor() as i32,
            ((o.x + o.w + pad) / CELL).ceil() as i32,
            ((o.y + o.h + pad) / CELL).ceil() as i32,
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

/// Mark one cell blocked or clear, ignoring out-of-range cells.
fn set_blocked(grid: &mut [bool], gw: i32, c: Cell, v: bool) {
    if c.x < 0 || c.y < 0 {
        return;
    }
    let idx = (c.y * gw + c.x) as usize;
    if idx < grid.len() {
        grid[idx] = v;
    }
}

/// Whether a cell is blocked. Out-of-range cells read as blocked, so
/// the search cannot leave the grid.
fn is_blocked(grid: &[bool], gw: i32, c: Cell) -> bool {
    grid.get((c.y * gw + c.x) as usize).copied().unwrap_or(true)
}

/// `BORDER_PENALTY` if `cell` sits within `BORDER_CLEARANCE` of any
/// container border *line*, else `0`. A flat constant (not summed across
/// borders) so overlapping / nested chrome can't pile up an outsized cost.
fn border_penalty(cell: Cell, borders: &[(f64, f64, f64, f64)]) -> i32 {
    let (px, py) = (cell.x as f64 * CELL, cell.y as f64 * CELL);
    if borders.iter().any(|&r| on_border(px, py, r)) {
        BORDER_PENALTY
    } else {
        0
    }
}

/// Whether `(px, py)` lies on the outer rectangle outline of `(x, y, w, h)`
/// within `BORDER_CLEARANCE`: near a left/right edge while inside the
/// vertical extent, or near a top/bottom edge while inside the horizontal
/// extent (each extent grown by the clearance so frame corners are caught).
fn on_border(px: f64, py: f64, (x, y, w, h): (f64, f64, f64, f64)) -> bool {
    let c = BORDER_CLEARANCE;
    let on_vert =
        ((px - x).abs() <= c || (px - (x + w)).abs() <= c) && py >= y - c && py <= y + h + c;
    let on_horiz =
        ((py - y).abs() <= c || (py - (y + h)).abs() <= c) && px >= x - c && px <= x + w + c;
    on_vert || on_horiz
}

/// Round an SVG point to the nearest grid cell.
fn snap(p: (f64, f64)) -> Cell {
    Cell {
        x: (p.0 / CELL).round() as i32,
        y: (p.1 / CELL).round() as i32,
    }
}

/// The A* heuristic: grid distance, which never overestimates for
/// orthogonal movement.
fn manhattan(a: Cell, b: Cell) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

/// Walk the came-from map back to the start, yielding the path.
fn reconstruct(
    came_from: &HashMap<Node, Node>,
    end: Node,
    src: (f64, f64),
    src_side: Side,
    dst: (f64, f64),
    dst_side: Side,
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
    snap_endpoints(pts, src, src_side, dst, dst_side)
}

/// Convert a cell path to SVG points, collapsing collinear runs so
/// the polyline carries only its corners.
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
/// (the grid-snapped versions may be off by up to half a cell), then
/// patch up the adjoining segments so every leg of the polyline
/// stays axis-aligned. Without the patch-up, the snap can leave the
/// first or last segment diagonal whenever the anchor doesn't sit on
/// a cell boundary — the bend gets inserted on the axis perpendicular
/// to the egress / ingress direction so the leg into / out of the
/// shape still matches its declared side.
fn snap_endpoints(
    mut pts: Vec<(f64, f64)>,
    src: (f64, f64),
    src_side: Side,
    dst: (f64, f64),
    dst_side: Side,
) -> Vec<(f64, f64)> {
    if pts.is_empty() {
        return vec![src, dst];
    }
    pts[0] = src;
    *pts.last_mut().unwrap() = dst;

    // Fix the START segment: pts[0] (src) -> pts[1]. The egress
    // direction comes from src_side (horizontal for East/West,
    // vertical for North/South); the first leg must travel along
    // that axis. If pts[1] differs on both axes from src, insert a
    // corner so the first leg stays axis-aligned.
    if pts.len() >= 2 && diagonal(pts[0], pts[1]) {
        let corner = corner_after(src, src_side, pts[1]);
        pts.insert(1, corner);
    }

    // Fix the END segment: pts[len-2] -> pts[len-1] (dst). The
    // final leg must arrive along the ingress axis of dst_side.
    let n = pts.len();
    if n >= 2 && diagonal(pts[n - 2], pts[n - 1]) {
        let corner = corner_before(pts[n - 2], dst, dst_side);
        pts.insert(n - 1, corner);
    }

    // The inserted corner can produce a short U-turn (collinear
    // segments doubling back). Collapse any run of three colinear
    // points so the polyline reads as a single straight leg.
    simplify_collinear(&mut pts);

    pts
}

/// Walk `pts` and drop any middle point that's collinear with both
/// neighbours on the same axis. Handles two cases produced by the
/// snap fix-up: continuation (A=B=C on one axis, same direction —
/// drop B) and U-turn (A→B then B→C reverse on the same axis — drop
/// B and shorten the run).
fn simplify_collinear(pts: &mut Vec<(f64, f64)>) {
    let mut i = 1;
    while i + 1 < pts.len() {
        let a = pts[i - 1];
        let b = pts[i];
        let c = pts[i + 1];
        let same_x = (a.0 - b.0).abs() < 1e-6 && (b.0 - c.0).abs() < 1e-6;
        let same_y = (a.1 - b.1).abs() < 1e-6 && (b.1 - c.1).abs() < 1e-6;
        if same_x || same_y {
            pts.remove(i);
            if i > 1 {
                i -= 1;
            }
        } else {
            i += 1;
        }
    }
}

/// Whether a segment is neither horizontal nor vertical.
fn diagonal(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() > 1e-6 && (a.1 - b.1).abs() > 1e-6
}

/// Insert a corner just after `src` so the first leg travels along
/// `src_side`'s outward axis before turning toward `next`.
fn corner_after(src: (f64, f64), src_side: Side, next: (f64, f64)) -> (f64, f64) {
    match src_side {
        // Horizontal egress: keep y = src.y for the first leg, turn
        // at next.x.
        Side::East | Side::West => (next.0, src.1),
        // Vertical egress: keep x = src.x, turn at next.y.
        Side::North | Side::South => (src.0, next.1),
    }
}

/// Insert a corner just before `dst` so the final leg arrives along
/// `dst_side`'s ingress axis.
fn corner_before(prev: (f64, f64), dst: (f64, f64), dst_side: Side) -> (f64, f64) {
    match dst_side {
        // Horizontal ingress: final leg lies on y = dst.y, so the
        // corner sits at (prev.x, dst.y).
        Side::East | Side::West => (prev.0, dst.1),
        // Vertical ingress: final leg lies on x = dst.x.
        Side::North | Side::South => (dst.0, prev.1),
    }
}

// ── Edge separation ────────────────────────────────────────────────

/// Each edge passes through as a polyline plus its declared source
/// and destination anchor coords (used to identify shared-anchor
/// edges that should be left aligned).
#[derive(Clone)]
pub(crate) struct EdgePath {
    /// The routed polyline, corner to corner.
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
    // Per-path endpoint signatures used to detect bundled groups —
    // edges that all leave the same anchor (or all arrive at the
    // same anchor) should overlap in the shared corridor rather
    // than fan out into parallel lines.
    let key = |p: (f64, f64)| ((p.0 * 1e3).round() as i64, (p.1 * 1e3).round() as i64);
    let starts: Vec<_> = paths
        .iter()
        .map(|p| p.points.first().copied().map(key))
        .collect();
    let ends: Vec<_> = paths
        .iter()
        .map(|p| p.points.last().copied().map(key))
        .collect();
    for (_, members) in groups {
        if members.len() < 2 {
            continue;
        }
        let mut members = members;
        members.sort_by_key(|&i| (segs[i].path_idx, segs[i].seg_idx));

        // If every path contributing to this corridor shares the same
        // source anchor, or every path shares the same destination,
        // leave the segments aligned. Visually the bundle reads as a
        // single trunk that branches at the divergent end — and the
        // endpoint-segment exclusion already protects the divergent
        // tail from being nudged.
        let path_ids: Vec<usize> = members.iter().map(|&i| segs[i].path_idx).collect();
        let shares_start = path_ids
            .iter()
            .all(|&pi| starts[pi].is_some() && starts[pi] == starts[path_ids[0]]);
        let shares_end = path_ids
            .iter()
            .all(|&pi| ends[pi].is_some() && ends[pi] == ends[path_ids[0]]);
        if shares_start || shares_end {
            continue;
        }

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
    fn absurd_coordinates_bail_instead_of_allocating() {
        // A viewport in the 1e30 range used to overflow the i32 cell
        // math and attempt a colossal grid allocation. It must fail
        // fast with `None` (and record a route error) instead.
        let got = route_elbow(
            (50.0, 100.0),
            Side::East,
            (1.0e30, 100.0),
            Side::West,
            &[],
            &[],
            (1.0e30, 1.0e30),
        );
        assert!(got.is_none());
        let err = crate::render::take_route_error().expect("size diagnostic recorded");
        assert!(err.contains("too large"), "unexpected message: {err}");
    }

    #[test]
    fn nan_coordinates_do_not_panic() {
        // `f64::max` ignores NaN, so sizing falls back to the finite
        // endpoint and snap() maps NaN to cell 0. The router may or may
        // not find a path — the guarantee is no panic, no huge alloc.
        let _ = route_elbow(
            (f64::NAN, f64::NAN),
            Side::East,
            (250.0, 100.0),
            Side::West,
            &[],
            &[],
            (f64::NAN, 200.0),
        );
        crate::render::take_route_error();
    }

    #[test]
    fn straight_horizontal_path_has_no_bends() {
        let pts = route_elbow(
            (50.0, 100.0),
            Side::East,
            (250.0, 100.0),
            Side::West,
            &[],
            &[],
            (320.0, 200.0),
        )
        .expect("unobstructed route");
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
            &[],
            (320.0, 200.0),
        )
        .expect("route around a single obstacle");
        // At least 3 points (one bend), and at least one y differs
        // from the straight-line y.
        assert!(pts.len() >= 3, "expected bends, got {:?}", pts);
        assert!(
            pts.iter().any(|(_, y)| (y - 100.0).abs() > 1.0),
            "expected the polyline to deviate from y=100 to clear the obstacle, got {:?}",
            pts
        );
    }

    /// `true` if the axis-aligned segment `a`→`b` passes through the
    /// strict interior of obstacle `o` (a leg merely grazing a boundary
    /// edge doesn't count).
    fn seg_hits_box(a: (f64, f64), b: (f64, f64), o: &Obstacle) -> bool {
        let (x0, y0, x1, y1) = (o.x, o.y, o.x + o.w, o.y + o.h);
        let (sx0, sx1) = (a.0.min(b.0), a.0.max(b.0));
        let (sy0, sy1) = (a.1.min(b.1), a.1.max(b.1));
        let eps = 1e-6;
        sx1 > x0 + eps && sx0 < x1 - eps && sy1 > y0 + eps && sy0 < y1 - eps
    }

    #[test]
    fn route_around_tightly_packed_row() {
        // Three 60x40 boxes in a horizontal row, 24px gaps. The edge
        // runs from box A's east anchor to box C's WEST anchor, which is
        // wedged in the 24px B->C gap. At PAD=4/CELL=10 that gap quantizes
        // to zero free columns on the vertical travel axis, so A* fails at
        // the normal padding; the relaxation retry (PAD->0) opens the gap
        // and routes *around* box B instead of the old blind elbow that
        // cut straight through it.
        let box_a = Obstacle {
            x: 0.0,
            y: 100.0,
            w: 60.0,
            h: 40.0,
        };
        let box_b = Obstacle {
            x: 84.0,
            y: 100.0,
            w: 60.0,
            h: 40.0,
        };
        let box_c = Obstacle {
            x: 168.0,
            y: 100.0,
            w: 60.0,
            h: 40.0,
        };
        let src = (60.0, 120.0); // east anchor of A
        let dst = (168.0, 120.0); // west anchor of C (in the B->C gap)

        let pts = route_elbow(
            src,
            Side::East,
            dst,
            Side::West,
            &[box_a, box_b, box_c],
            &[],
            (320.0, 240.0),
        )
        .expect("a detour around the packed row exists");

        assert_eq!(*pts.first().unwrap(), src);
        assert_eq!(*pts.last().unwrap(), dst);

        // No segment may pass through the interior of box B.
        for w in pts.windows(2) {
            assert!(
                !seg_hits_box(w[0], w[1], &box_b),
                "edge segment {:?} -> {:?} cuts through box B in {:?}",
                w[0],
                w[1],
                pts
            );
        }
        // A real detour leaves the row band (y in [100, 140]) — proving it
        // routed around rather than taking the degenerate straight line.
        let band = (100.0 - 1e-6)..=(140.0 + 1e-6);
        assert!(
            pts.iter().any(|&(_, y)| !band.contains(&y)),
            "expected a vertical detour out of the row band, got {:?}",
            pts
        );
    }

    #[test]
    fn route_elbow_returns_none_when_truly_boxed_in() {
        // The destination's west anchor is flush against box B (zero gap)
        // and box B fully spans above and below the anchor, so there is no
        // obstacle-free approach at any padding. The router must give up
        // (`None`) rather than draw a line through box B.
        let box_b = Obstacle {
            x: 60.0,
            y: 0.0,
            w: 80.0,
            h: 240.0,
        };
        let dst_box = Obstacle {
            x: 140.0,
            y: 100.0,
            w: 60.0,
            h: 40.0,
        };
        let src = (40.0, 120.0); // east anchor of a source left of box B
        let dst = (140.0, 120.0); // west anchor of dst_box, flush on box B

        let routed = route_elbow(
            src,
            Side::East,
            dst,
            Side::West,
            &[box_b, dst_box],
            &[],
            (240.0, 240.0),
        );
        assert!(
            routed.is_none(),
            "expected no route through a flush wall, got {:?}",
            routed
        );
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
    fn route_endpoints_stay_axis_aligned_when_anchor_off_grid() {
        // West-ingress destination at (252, 37) — neither coord lies
        // on a CELL=10 boundary. Without the corner fix-up, the snap
        // would leave the final leg diagonal. The patched route must
        // arrive on a horizontal segment (y=37) coming in from the
        // west and have every other leg axis-aligned too.
        let pts = route_elbow(
            (132.0, 30.0),
            Side::East,
            (252.0, 37.0),
            Side::West,
            &[],
            &[],
            (520.0, 320.0),
        )
        .expect("unobstructed route");
        assert!(pts.len() >= 2, "route should not be empty");
        // Every consecutive pair must share either x or y.
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let aligned = (a.0 - b.0).abs() < 1e-6 || (a.1 - b.1).abs() < 1e-6;
            assert!(aligned, "diagonal segment {a:?} -> {b:?} in {pts:?}");
        }
        // First segment goes horizontal (east egress).
        assert!(
            (pts[0].1 - pts[1].1).abs() < 1e-6,
            "first segment not horizontal: {pts:?}"
        );
        // Final segment arrives horizontal (west ingress at y=37).
        let n = pts.len();
        assert!(
            (pts[n - 2].1 - 37.0).abs() < 1e-6 && (pts[n - 1].1 - 37.0).abs() < 1e-6,
            "final segment should hug y=37: {pts:?}"
        );
    }

    #[test]
    fn separate_edges_keeps_bundled_middle_segments_aligned() {
        // Three edges all leaving the same source (10, 50), bending
        // east through a common horizontal corridor at y=100, then
        // turning down to three distinct destinations. The shared
        // corridor should remain a single line (no nudge), because
        // visually a bundle of edges leaving one anchor reads better
        // as one trunk that splits near the destinations.
        let mut paths = vec![
            EdgePath {
                points: vec![(10.0, 50.0), (10.0, 100.0), (200.0, 100.0), (200.0, 30.0)],
            },
            EdgePath {
                points: vec![(10.0, 50.0), (10.0, 100.0), (210.0, 100.0), (210.0, 30.0)],
            },
            EdgePath {
                points: vec![(10.0, 50.0), (10.0, 100.0), (220.0, 100.0), (220.0, 30.0)],
            },
        ];
        separate_edges(&mut paths, 4.0);
        // All three corridor segments stay at y=100.
        assert!((paths[0].points[1].1 - 100.0).abs() < 1e-6);
        assert!((paths[1].points[1].1 - 100.0).abs() < 1e-6);
        assert!((paths[2].points[1].1 - 100.0).abs() < 1e-6);
        // And so do the corner alignments on the other endpoint of
        // each middle segment.
        assert!((paths[0].points[2].1 - 100.0).abs() < 1e-6);
        assert!((paths[1].points[2].1 - 100.0).abs() < 1e-6);
        assert!((paths[2].points[2].1 - 100.0).abs() < 1e-6);
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

    #[test]
    fn on_border_marks_outline_cells_within_clearance() {
        // A 200x200 box at the origin: its outline is x∈{0,200}, y∈{0,200}.
        let b = (0.0, 0.0, 200.0, 200.0);
        assert!(on_border(0.0, 100.0, b), "left edge");
        assert!(on_border(200.0, 100.0, b), "right edge");
        assert!(on_border(100.0, 0.0, b), "top edge");
        assert!(on_border(100.0, 200.0, b), "bottom edge");
        assert!(on_border(0.0, 0.0, b), "corner");
        // Within one clearance (CELL) of an edge still counts.
        assert!(
            on_border(BORDER_CLEARANCE, 100.0, b),
            "just inside left edge"
        );
        // Interior, far from every edge: not on the outline.
        assert!(!on_border(100.0, 100.0, b), "interior");
        // Outside and far: not on the outline.
        assert!(!on_border(400.0, 100.0, b), "far outside");
        // The penalty is flat (one constant) regardless of how many borders.
        let c = Cell { x: 0, y: 10 };
        assert_eq!(border_penalty(c, &[b]), BORDER_PENALTY);
        assert_eq!(border_penalty(c, &[b, b]), BORDER_PENALTY); // not summed
        assert_eq!(border_penalty(Cell { x: 10, y: 10 }, &[b]), 0); // interior cell
    }

    #[test]
    fn route_avoids_running_along_a_border() {
        // Both anchors sit on a box's top border line (y=0); the natural
        // route is a straight run along y=0. Passing the box as a border
        // must push the run off the line into the clear interior.
        let border = (0.0, 0.0, 200.0, 200.0);
        let src = (10.0, 0.0);
        let dst = (190.0, 0.0);
        let on_top = |pts: &[(f64, f64)]| {
            pts.windows(2).any(|w| {
                let (a, b) = (w[0], w[1]);
                (a.1 - b.1).abs() < 1e-6 // horizontal
                    && a.1.abs() <= BORDER_CLEARANCE // within clearance of y=0
                    && (a.0 - b.0).abs() > 2.0 * CELL // a real run, not a stub
            })
        };
        // Control: with no border, the route runs straight along y=0.
        let bare = route_elbow(src, Side::East, dst, Side::West, &[], &[], (220.0, 220.0))
            .expect("bare route");
        assert!(on_top(&bare), "control should run along y=0: {bare:?}");
        // With the border, the long run must leave the border line.
        let routed = route_elbow(
            src,
            Side::East,
            dst,
            Side::West,
            &[],
            &[border],
            (220.0, 220.0),
        )
        .expect("bordered route");
        assert!(
            !on_top(&routed),
            "route still runs along the y=0 border: {routed:?}"
        );
    }

    #[test]
    fn border_penalty_never_blocks_a_route() {
        // A border the path has no choice but to cross must not make the
        // edge unroutable — the penalty only costs, never blocks.
        let border = (0.0, 0.0, 200.0, 200.0);
        let routed = route_elbow(
            (100.0, 0.0),
            Side::South,
            (100.0, 200.0),
            Side::North,
            &[],
            &[border],
            (220.0, 220.0),
        );
        assert!(
            routed.is_some(),
            "a route that must cross a border should still exist"
        );
    }
}
