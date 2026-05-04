//! Vector shape drawing system — resolves layout and renders shape trees to SVG.
//!
//! The CLI handler builds `ShapeNode` trees from WCL `Value` data, then calls
//! `render_diagram_svg()` to produce inline SVG.

use std::collections::HashMap;
use std::fmt::Write;

use indexmap::IndexMap;

const LAYOUT_DECORATION_ATTR: &str = "_wdoc_layout_decoration";
const ROUTE_MARGIN: f64 = 8.0;
const ROUTE_TERMINAL_MIN: f64 = 16.0;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapeKind {
    Rect,
    Circle,
    Ellipse,
    Line,
    Path,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alignment {
    None,
    Flow,
    Stack,
    Center,
    Layered,
    Force,
    Radial,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    None,
    To,
    From,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnchorPoint {
    Top,
    Bottom,
    Left,
    Right,
    Center,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveStyle {
    Straight,
    Bezier,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    pub fn anchor_pos(&self, anchor: AnchorPoint, other: &Bounds) -> (f64, f64) {
        match anchor {
            AnchorPoint::Top => (self.x + self.width / 2.0, self.y),
            AnchorPoint::Bottom => (self.x + self.width / 2.0, self.y + self.height),
            AnchorPoint::Left => (self.x, self.y + self.height / 2.0),
            AnchorPoint::Right => (self.x + self.width, self.y + self.height / 2.0),
            AnchorPoint::Center => (self.x + self.width / 2.0, self.y + self.height / 2.0),
            AnchorPoint::Auto => {
                let cx = self.x + self.width / 2.0;
                let cy = self.y + self.height / 2.0;
                let ox = other.x + other.width / 2.0;
                let oy = other.y + other.height / 2.0;
                let dx = ox - cx;
                let dy = oy - cy;
                if dx.abs() > dy.abs() {
                    if dx > 0.0 {
                        (self.x + self.width, cy)
                    } else {
                        (self.x, cy)
                    }
                } else if dy > 0.0 {
                    (cx, self.y + self.height)
                } else {
                    (cx, self.y)
                }
            }
        }
    }
}

/// A shape node in the diagram tree.
#[derive(Debug, Clone)]
pub struct ShapeNode {
    pub kind: ShapeKind,
    pub id: Option<String>,
    // Positioning inputs (all optional)
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub top: Option<f64>,
    pub bottom: Option<f64>,
    pub left: Option<f64>,
    pub right: Option<f64>,
    // Resolved position (computed by layout)
    pub resolved: Bounds,
    // Visual attributes (fill, stroke, rx, etc.)
    pub attrs: IndexMap<String, String>,
    // Children
    pub children: Vec<ShapeNode>,
    pub align: Alignment,
    pub gap: f64,
    pub padding: f64,
}

/// A connection between two shapes.
#[derive(Debug, Clone)]
pub struct Connection {
    pub from_id: String,
    pub to_id: String,
    pub direction: Direction,
    pub from_anchor: AnchorPoint,
    pub to_anchor: AnchorPoint,
    pub label: Option<String>,
    pub curve: CurveStyle,
    pub attrs: IndexMap<String, String>,
}

/// A complete diagram ready to render.
pub struct Diagram {
    pub width: f64,
    pub height: f64,
    pub shapes: Vec<ShapeNode>,
    pub connections: Vec<Connection>,
    pub padding: f64,
    pub align: Alignment,
    pub gap: f64,
    pub options: IndexMap<String, String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve layout and render a diagram to an inline SVG string.
pub fn render_diagram_svg(diagram: &mut Diagram) -> String {
    // Phase 2: resolve layout
    let inner = Bounds {
        x: diagram.padding,
        y: diagram.padding,
        width: diagram.width - diagram.padding * 2.0,
        height: diagram.height - diagram.padding * 2.0,
    };
    resolve_children(
        &mut diagram.shapes,
        &diagram.connections,
        &inner,
        "",
        diagram.align,
        diagram.gap,
        &diagram.options,
    );

    // Phase 2b: build shape map for connections
    let shape_map = build_shape_map(&diagram.shapes, 0.0, 0.0);

    // Phase 3: render SVG
    let mut svg = String::new();
    write!(
        svg,
        "<div class=\"wdoc-diagram\">\
         <svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        diagram.width, diagram.height, diagram.width, diagram.height
    )
    .unwrap();

    // Arrow marker defs
    if diagram
        .connections
        .iter()
        .any(|c| c.direction != Direction::None)
    {
        svg.push_str(ARROW_DEFS);
    }

    // Render shapes
    for shape in &diagram.shapes {
        render_shape_svg(shape, &mut svg);
    }

    // Render connections
    for conn in &diagram.connections {
        render_connection_svg(conn, &shape_map, &mut svg);
    }

    svg.push_str("</svg></div>");
    svg
}

pub fn parse_alignment_str(s: &str) -> Alignment {
    match s {
        "flow" => Alignment::Flow,
        "stack" => Alignment::Stack,
        "center" => Alignment::Center,
        "layered" => Alignment::Layered,
        "force" => Alignment::Force,
        "radial" => Alignment::Radial,
        "grid" => Alignment::Grid,
        _ => Alignment::None,
    }
}

pub fn parse_anchor_str(s: &str) -> AnchorPoint {
    match s {
        "top" => AnchorPoint::Top,
        "bottom" => AnchorPoint::Bottom,
        "left" => AnchorPoint::Left,
        "right" => AnchorPoint::Right,
        "center" => AnchorPoint::Center,
        _ => AnchorPoint::Auto,
    }
}

pub fn parse_direction_str(s: &str) -> Direction {
    match s {
        "to" => Direction::To,
        "from" => Direction::From,
        "both" => Direction::Both,
        _ => Direction::None,
    }
}

pub fn parse_curve_str(s: &str) -> CurveStyle {
    match s {
        "bezier" => CurveStyle::Bezier,
        _ => CurveStyle::Straight,
    }
}

pub fn parse_shape_kind(kind: &str) -> Option<ShapeKind> {
    match kind {
        "wdoc::draw::rect" => Some(ShapeKind::Rect),
        "wdoc::draw::circle" => Some(ShapeKind::Circle),
        "wdoc::draw::ellipse" => Some(ShapeKind::Ellipse),
        "wdoc::draw::line" => Some(ShapeKind::Line),
        "wdoc::draw::path" => Some(ShapeKind::Path),
        "wdoc::draw::text" => Some(ShapeKind::Text),
        // Anything else under `wdoc::draw::` (or a user namespace ending in `::draw::`)
        // is treated as a composite shape: a rect-shaped container whose children are
        // produced by a `@template("shape", ...)` function. The connection schema is
        // handled separately by the dispatcher and never reaches this function.
        k if k.starts_with("wdoc::draw::") && k != "wdoc::draw::diagram" => Some(ShapeKind::Rect),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Layout resolution
// ---------------------------------------------------------------------------

fn resolve_children(
    children: &mut [ShapeNode],
    connections: &[Connection],
    parent: &Bounds,
    scope_path: &str,
    align: Alignment,
    gap: f64,
    options: &IndexMap<String, String>,
) {
    // First pass: resolve anchored/absolute children
    for child in children.iter_mut() {
        let bounds_parent = if is_layout_decoration(child) {
            decoration_parent_bounds(parent)
        } else {
            *parent
        };
        resolve_bounds(child, &bounds_parent);
        apply_intrinsic_container_size(child);
    }

    // Second pass: position unpositioned children via alignment engine
    let unpositioned: Vec<usize> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            !is_layout_decoration(c)
                && c.x.is_none()
                && c.y.is_none()
                && c.top.is_none()
                && c.left.is_none()
        })
        .map(|(i, _)| i)
        .collect();

    let layoutable: Vec<usize> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| !is_layout_decoration(c))
        .map(|(i, _)| i)
        .collect();

    match align {
        Alignment::Stack | Alignment::Flow | Alignment::Center if !unpositioned.is_empty() => {
            match align {
                Alignment::Stack => layout_stack(children, &unpositioned, parent, gap),
                Alignment::Flow => layout_flow(children, &unpositioned, parent, gap),
                Alignment::Center => layout_center(children, &unpositioned, parent),
                _ => {}
            }
        }
        Alignment::Layered | Alignment::Force | Alignment::Radial | Alignment::Grid
            if !layoutable.is_empty() =>
        {
            layout_graph_subset(
                children,
                &layoutable,
                connections,
                parent,
                scope_path,
                align,
                gap,
                options,
            );
        }
        _ => {}
    }

    // Recurse into children
    for child in children.iter_mut() {
        let insets = child_content_insets(child);
        let mut inner = Bounds {
            x: insets.left,
            y: insets.top,
            width: (child.resolved.width - insets.left - insets.right).max(0.0),
            height: (child.resolved.height - insets.top - insets.bottom).max(0.0),
        };
        let child_scope_path = match (scope_path.is_empty(), child.id.as_deref()) {
            (_, None) => scope_path.to_string(),
            (true, Some(id)) => id.to_string(),
            (false, Some(id)) => format!("{scope_path}.{id}"),
        };
        resolve_children(
            &mut child.children,
            connections,
            &inner,
            &child_scope_path,
            child.align,
            child.gap,
            &child.attrs,
        );
        apply_post_layout_container_size(child);
        if child.align == Alignment::Layered {
            expand_container_to_fit_layered_children(child);
            let insets = child_content_insets(child);
            inner.x = insets.left;
            inner.y = insets.top;
            inner.width = (child.resolved.width - insets.left - insets.right).max(0.0);
            inner.height = (child.resolved.height - insets.top - insets.bottom).max(0.0);
        }
        if has_explicit_width(child) && has_explicit_height(child) {
            clamp_children_to_parent(&mut child.children, &inner);
        }
    }
}

fn is_layout_decoration(node: &ShapeNode) -> bool {
    node.attrs
        .get(LAYOUT_DECORATION_ATTR)
        .map(|v| v == "true")
        .unwrap_or(false)
}

#[derive(Clone, Copy)]
struct Insets {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

fn child_content_insets(node: &ShapeNode) -> Insets {
    let mut insets = Insets {
        left: node.padding,
        top: node.padding,
        right: node.padding,
        bottom: node.padding,
    };

    if node.padding == 0.0
        && node.align != Alignment::None
        && node.children.iter().any(is_layout_decoration)
    {
        insets.left = 16.0;
        insets.right = 16.0;
        insets.bottom = 16.0;
        insets.top = decoration_header_inset(node).unwrap_or(16.0);
    }

    insets
}

fn decoration_header_inset(node: &ShapeNode) -> Option<f64> {
    let mut inset: Option<f64> = None;
    for child in node.children.iter().filter(|child| {
        is_layout_decoration(child) && !is_full_container_decoration(child, node.resolved)
    }) {
        let bottom = child.resolved.y + child.resolved.height + 6.0;
        inset = Some(inset.map_or(bottom, |current| current.max(bottom)));
    }
    inset
}

fn is_full_container_decoration(child: &ShapeNode, container: Bounds) -> bool {
    nearly_eq(child.resolved.x, 0.0)
        && nearly_eq(child.resolved.y, 0.0)
        && nearly_eq(child.resolved.width, container.width)
        && nearly_eq(child.resolved.height, container.height)
}

fn decoration_parent_bounds(parent: &Bounds) -> Bounds {
    Bounds {
        x: 0.0,
        y: 0.0,
        width: parent.width + parent.x * 2.0,
        height: parent.height + parent.y * 2.0,
    }
}

fn layout_graph_subset(
    children: &mut [ShapeNode],
    indices: &[usize],
    connections: &[Connection],
    parent: &Bounds,
    scope_path: &str,
    align: Alignment,
    gap: f64,
    options: &IndexMap<String, String>,
) {
    let mut layout_children: Vec<ShapeNode> =
        indices.iter().map(|&i| children[i].clone()).collect();
    let local_connections = localize_connections(&layout_children, connections, scope_path);

    match align {
        Alignment::Layered => crate::graph_layout::layout_layered(
            &mut layout_children,
            &local_connections,
            parent,
            gap,
            options,
        ),
        Alignment::Force => crate::graph_layout::layout_force(
            &mut layout_children,
            &local_connections,
            parent,
            gap,
            options,
        ),
        Alignment::Radial => crate::graph_layout::layout_radial(
            &mut layout_children,
            &local_connections,
            parent,
            gap,
            options,
        ),
        Alignment::Grid => crate::graph_layout::layout_grid(
            &mut layout_children,
            &local_connections,
            parent,
            gap,
            options,
        ),
        _ => {}
    }

    for (layout_child, &original_idx) in layout_children.into_iter().zip(indices) {
        children[original_idx].resolved = layout_child.resolved;
    }
}

fn localize_connections(
    children: &[ShapeNode],
    connections: &[Connection],
    scope_path: &str,
) -> Vec<Connection> {
    connections
        .iter()
        .filter_map(|conn| {
            let from_id = localize_endpoint(&conn.from_id, children, scope_path)?;
            let to_id = localize_endpoint(&conn.to_id, children, scope_path)?;
            let mut local = conn.clone();
            local.from_id = from_id;
            local.to_id = to_id;
            Some(local)
        })
        .collect()
}

fn localize_endpoint(endpoint: &str, children: &[ShapeNode], scope_path: &str) -> Option<String> {
    let endpoint = if !scope_path.is_empty() {
        endpoint
            .strip_prefix(scope_path)
            .and_then(|rest| rest.strip_prefix('.'))
            .unwrap_or(endpoint)
    } else {
        endpoint
    };

    if endpoint.contains('.') {
        return None;
    }

    children
        .iter()
        .filter_map(|child| child.id.as_deref())
        .find(|id| *id == endpoint)
        .map(str::to_string)
}

fn apply_intrinsic_container_size(node: &mut ShapeNode) {
    if node.children.is_empty() {
        return;
    }

    let needs_width = !has_explicit_width(node) && node.resolved.width == 0.0;
    let needs_height = !has_explicit_height(node) && node.resolved.height == 0.0;
    if !needs_width && !needs_height {
        return;
    }

    if let Some(bounds) = input_children_bounds(&node.children) {
        if needs_width {
            node.resolved.width = (bounds.x + bounds.width + node.padding * 2.0).max(0.0);
        }
        if needs_height {
            node.resolved.height = (bounds.y + bounds.height + node.padding * 2.0).max(0.0);
        }
    }
}

fn apply_post_layout_container_size(node: &mut ShapeNode) {
    if node.children.is_empty() {
        return;
    }

    let needs_width = !has_explicit_width(node);
    let needs_height = !has_explicit_height(node);
    if !needs_width && !needs_height {
        return;
    }

    if let Some(bounds) = children_bounds(&node.children) {
        let insets = child_content_insets(node);
        if needs_width {
            node.resolved.width = node
                .resolved
                .width
                .max(bounds.x.max(insets.left) + bounds.width + insets.right);
        }
        if needs_height {
            node.resolved.height = node
                .resolved
                .height
                .max(bounds.y.max(insets.top) + bounds.height + insets.bottom);
        }
    }
}

fn expand_container_to_fit_layered_children(node: &mut ShapeNode) {
    let Some(bounds) = children_bounds_without_decoration(&node.children) else {
        return;
    };

    let old_width = node.resolved.width;
    let old_height = node.resolved.height;
    let insets = child_content_insets(node);
    let needed_width = bounds.x.max(insets.left) + bounds.width + insets.right;
    let needed_height = bounds.y.max(insets.top) + bounds.height + insets.bottom;

    node.resolved.width = node.resolved.width.max(needed_width);
    node.resolved.height = node.resolved.height.max(needed_height);

    if node.resolved.width != old_width || node.resolved.height != old_height {
        resize_full_container_decorations(&mut node.children, old_width, old_height, node.resolved);
    }
}

fn input_children_bounds(children: &[ShapeNode]) -> Option<Bounds> {
    let mut resolved = Vec::with_capacity(children.len());
    for child in children {
        let mut child = child.clone();
        let parent = Bounds {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
        resolve_bounds(&mut child, &parent);
        apply_intrinsic_container_size(&mut child);
        resolved.push(child);
    }
    children_bounds(&resolved)
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

fn children_bounds_without_decoration(children: &[ShapeNode]) -> Option<Bounds> {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut found = false;

    for child in children.iter().filter(|c| !is_layout_decoration(c)) {
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

fn resize_full_container_decorations(
    children: &mut [ShapeNode],
    old_width: f64,
    old_height: f64,
    new_bounds: Bounds,
) {
    for child in children.iter_mut().filter(|c| is_layout_decoration(c)) {
        if child.resolved.x == 0.0
            && child.resolved.y == 0.0
            && child.resolved.width == old_width
            && child.resolved.height == old_height
        {
            child.resolved.width = new_bounds.width;
            child.resolved.height = new_bounds.height;
        }
    }
}

fn clamp_children_to_parent(children: &mut [ShapeNode], parent: &Bounds) {
    for child in children.iter_mut().filter(|c| !is_layout_decoration(c)) {
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
}

fn clamp_origin(origin: f64, size: f64, parent_origin: f64, parent_size: f64) -> f64 {
    if size >= parent_size {
        parent_origin
    } else {
        origin.clamp(parent_origin, parent_origin + parent_size - size)
    }
}

fn has_explicit_width(node: &ShapeNode) -> bool {
    node.width.is_some() || (node.left.is_some() && node.right.is_some())
}

fn has_explicit_height(node: &ShapeNode) -> bool {
    node.height.is_some() || (node.top.is_some() && node.bottom.is_some())
}

fn resolve_bounds(node: &mut ShapeNode, parent: &Bounds) {
    let (mut rx, mut rw) = resolve_axis(
        node.x,
        node.width,
        node.left,
        node.right,
        parent.x,
        parent.width,
    );
    let (mut ry, mut rh) = resolve_axis(
        node.y,
        node.height,
        node.top,
        node.bottom,
        parent.y,
        parent.height,
    );

    // Text shapes with no explicit position/size fill their parent's inner space
    if node.kind == ShapeKind::Text && rw == 0.0 && rh == 0.0 {
        rx = parent.x;
        ry = parent.y;
        rw = parent.width;
        rh = parent.height;
    }

    // Circle/ellipse: derive size from r/rx/ry attributes
    let (rw, rh) = match node.kind {
        ShapeKind::Circle => {
            let r = node
                .attrs
                .get("r")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(rw.max(rh) / 2.0);
            (r * 2.0, r * 2.0)
        }
        ShapeKind::Ellipse => {
            let erx = node
                .attrs
                .get("rx")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(rw / 2.0);
            let ery = node
                .attrs
                .get("ry")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(rh / 2.0);
            (erx * 2.0, ery * 2.0)
        }
        _ => (rw, rh),
    };

    node.resolved = Bounds {
        x: rx,
        y: ry,
        width: rw,
        height: rh,
    };
}

fn resolve_axis(
    pos: Option<f64>,
    size: Option<f64>,
    near: Option<f64>,
    far: Option<f64>,
    parent_origin: f64,
    parent_size: f64,
) -> (f64, f64) {
    match (pos, size, near, far) {
        (Some(p), Some(s), _, _) => (parent_origin + p, s),
        (Some(p), None, _, _) => (parent_origin + p, 0.0),
        (_, _, Some(n), Some(f)) => (
            parent_origin + n,
            size.unwrap_or((parent_size - n - f).max(0.0)),
        ),
        (_, Some(s), Some(n), None) => (parent_origin + n, s),
        (_, Some(s), None, Some(f)) => (parent_origin + (parent_size - f - s).max(0.0), s),
        (_, Some(s), None, None) => (parent_origin, s),
        _ => (parent_origin, 0.0),
    }
}

// ---------------------------------------------------------------------------
// Layout engines
// ---------------------------------------------------------------------------

fn layout_stack(children: &mut [ShapeNode], indices: &[usize], parent: &Bounds, gap: f64) {
    let mut y = parent.y;
    for &i in indices {
        children[i].resolved.x = parent.x;
        children[i].resolved.y = y;
        if children[i].resolved.width == 0.0 {
            children[i].resolved.width = parent.width;
        }
        y += children[i].resolved.height + gap;
    }
}

fn layout_flow(children: &mut [ShapeNode], indices: &[usize], parent: &Bounds, gap: f64) {
    let mut x = parent.x;
    let mut y = parent.y;
    let mut row_height: f64 = 0.0;

    for &i in indices {
        let w = children[i].resolved.width;
        let h = children[i].resolved.height;

        if x + w > parent.x + parent.width && x > parent.x {
            x = parent.x;
            y += row_height + gap;
            row_height = 0.0;
        }

        children[i].resolved.x = x;
        children[i].resolved.y = y;
        x += w + gap;
        row_height = row_height.max(h);
    }
}

fn layout_center(children: &mut [ShapeNode], indices: &[usize], parent: &Bounds) {
    for &i in indices {
        let w = children[i].resolved.width;
        let h = children[i].resolved.height;
        children[i].resolved.x = parent.x + (parent.width - w) / 2.0;
        children[i].resolved.y = parent.y + (parent.height - h) / 2.0;
    }
}

// ---------------------------------------------------------------------------
// Connection resolution
// ---------------------------------------------------------------------------

fn build_shape_map(shapes: &[ShapeNode], ox: f64, oy: f64) -> HashMap<String, Bounds> {
    let mut map = HashMap::new();
    for shape in shapes {
        if let Some(id) = &shape.id {
            let abs = Bounds {
                x: shape.resolved.x + ox,
                y: shape.resolved.y + oy,
                width: shape.resolved.width,
                height: shape.resolved.height,
            };
            map.insert(id.clone(), abs);
            let child_map = build_shape_map(
                &shape.children,
                ox + shape.resolved.x,
                oy + shape.resolved.y,
            );
            for (cid, bounds) in child_map {
                map.insert(format!("{id}.{cid}"), bounds);
            }
        }
    }
    map
}

// ---------------------------------------------------------------------------
// SVG rendering
// ---------------------------------------------------------------------------

const ARROW_DEFS: &str = r#"<defs>
<marker id="wdoc-arrow" viewBox="0 0 10 10" refX="10" refY="5"
  markerWidth="8" markerHeight="8" orient="auto-start-reverse">
  <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor"/>
</marker>
</defs>"#;

fn render_shape_svg(node: &ShapeNode, svg: &mut String) {
    let b = &node.resolved;
    let style = svg_style_attrs(&node.attrs);

    // Wrap in <a> if shape has an href attribute (clickable)
    let href = node.attrs.get("href");
    if let Some(url) = href {
        write!(svg, "<a href=\"{url}\" target=\"_top\">").unwrap();
    }

    match node.kind {
        ShapeKind::Rect => {
            let rx = node.attrs.get("rx").map(|s| s.as_str()).unwrap_or("0");
            let ry = node.attrs.get("ry").map(|s| s.as_str()).unwrap_or(rx);
            write!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{rx}\" ry=\"{ry}\"{style}/>",
                b.x, b.y, b.width, b.height
            )
            .unwrap();
        }
        ShapeKind::Circle => {
            let r = b.width / 2.0;
            write!(
                svg,
                "<circle cx=\"{}\" cy=\"{}\" r=\"{r}\"{style}/>",
                b.x + r,
                b.y + r
            )
            .unwrap();
        }
        ShapeKind::Ellipse => {
            let erx = b.width / 2.0;
            let ery = b.height / 2.0;
            write!(
                svg,
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{erx}\" ry=\"{ery}\"{style}/>",
                b.x + erx,
                b.y + ery
            )
            .unwrap();
        }
        ShapeKind::Line => {
            let x1 = attr_f64(&node.attrs, "x1").unwrap_or(b.x);
            let y1 = attr_f64(&node.attrs, "y1").unwrap_or(b.y);
            let x2 = attr_f64(&node.attrs, "x2").unwrap_or(b.x + b.width);
            let y2 = attr_f64(&node.attrs, "y2").unwrap_or(b.y + b.height);
            write!(
                svg,
                "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"{style}/>"
            )
            .unwrap();
        }
        ShapeKind::Path => {
            let d = node.attrs.get("d").map(|s| s.as_str()).unwrap_or("");
            write!(svg, "<path d=\"{d}\"{style}/>").unwrap();
        }
        ShapeKind::Text => {
            let content = node.attrs.get("content").map(|s| s.as_str()).unwrap_or("");
            let font_size = node
                .attrs
                .get("font_size")
                .map(|s| s.as_str())
                .unwrap_or("14");
            let anchor = node
                .attrs
                .get("anchor")
                .map(|s| s.as_str())
                .unwrap_or("middle");
            // Position text based on anchor
            let tx = match anchor {
                "start" => b.x,
                "end" => b.x + b.width,
                _ => b.x + b.width / 2.0, // "middle"
            };
            let ty = b.y + b.height / 2.0;
            // Default fill to currentColor so text is visible in dark mode
            let fill_default = if node.attrs.contains_key("fill") {
                ""
            } else {
                " fill=\"currentColor\""
            };
            write!(
                svg,
                "<text x=\"{tx}\" y=\"{ty}\" font-size=\"{font_size}\" \
                 text-anchor=\"{anchor}\" dominant-baseline=\"central\"\
                 {fill_default}{style}>{content}</text>"
            )
            .unwrap();
        }
    }

    // Render children in a translated group
    if !node.children.is_empty() {
        let gx = b.x;
        let gy = b.y;
        write!(svg, "<g transform=\"translate({gx},{gy})\">").unwrap();
        for child in &node.children {
            render_shape_svg(child, svg);
        }
        svg.push_str("</g>");
    }

    // Close <a> wrapper if shape was clickable
    if href.is_some() {
        svg.push_str("</a>");
    }
}

fn render_connection_svg(conn: &Connection, shape_map: &HashMap<String, Bounds>, svg: &mut String) {
    let from_bounds = match shape_map.get(&conn.from_id) {
        Some(b) => b,
        None => return,
    };
    let to_bounds = match shape_map.get(&conn.to_id) {
        Some(b) => b,
        None => return,
    };

    let ms = match conn.direction {
        Direction::From | Direction::Both => " marker-start=\"url(#wdoc-arrow)\"",
        _ => "",
    };
    let me = match conn.direction {
        Direction::To | Direction::Both => " marker-end=\"url(#wdoc-arrow)\"",
        _ => "",
    };

    let style = svg_style_attrs(&conn.attrs);
    let stroke_default = if conn.attrs.contains_key("stroke") {
        ""
    } else {
        " stroke=\"currentColor\""
    };

    match conn.curve {
        CurveStyle::Straight => {
            if conn.from_anchor == AnchorPoint::Auto && conn.to_anchor == AnchorPoint::Auto {
                let (x1, y1) = from_bounds.anchor_pos(AnchorPoint::Auto, to_bounds);
                let (x2, y2) = to_bounds.anchor_pos(AnchorPoint::Auto, from_bounds);
                let obstacles: Vec<Bounds> = shape_map
                    .iter()
                    .filter(|(id, b)| {
                        id.as_str() != conn.from_id
                            && id.as_str() != conn.to_id
                            && !bounds_contains_point(b, x1, y1)
                            && !bounds_contains_point(b, x2, y2)
                    })
                    .map(|(_, b)| *b)
                    .collect();
                if let Some(points) = route_orthogonal(from_bounds, to_bounds, &obstacles) {
                    let d = path_data(&points);
                    write!(
                        svg,
                        "<path d=\"{d}\" fill=\"none\"{stroke_default}{style}{ms}{me}/>"
                    )
                    .unwrap();
                } else {
                    write!(
                        svg,
                        "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"\
                         {stroke_default}{style}{ms}{me}/>"
                    )
                    .unwrap();
                }
            } else {
                let (x1, y1) = from_bounds.anchor_pos(conn.from_anchor, to_bounds);
                let (x2, y2) = to_bounds.anchor_pos(conn.to_anchor, from_bounds);
                write!(
                    svg,
                    "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"\
                     {stroke_default}{style}{ms}{me}/>"
                )
                .unwrap();
            }
        }
        CurveStyle::Bezier => {
            let (x1, y1) = from_bounds.anchor_pos(conn.from_anchor, to_bounds);
            let (x2, y2) = to_bounds.anchor_pos(conn.to_anchor, from_bounds);
            let dx = (x2 - x1).abs() / 2.0;
            let dy = (y2 - y1).abs() / 2.0;
            let (c1x, c1y) = ctrl_point(x1, y1, conn.from_anchor, dx, dy);
            let (c2x, c2y) = ctrl_point(x2, y2, conn.to_anchor, dx, dy);
            write!(
                svg,
                "<path d=\"M {x1} {y1} C {c1x} {c1y}, {c2x} {c2y}, {x2} {y2}\" \
                 fill=\"none\"{stroke_default}{style}{ms}{me}/>"
            )
            .unwrap();
        }
    }

    if let Some(label) = &conn.label {
        let (x1, y1) = from_bounds.anchor_pos(conn.from_anchor, to_bounds);
        let (x2, y2) = to_bounds.anchor_pos(conn.to_anchor, from_bounds);
        let mx = (x1 + x2) / 2.0;
        let my = (y1 + y2) / 2.0 - 10.0;
        write!(
            svg,
            "<text x=\"{mx}\" y=\"{my}\" text-anchor=\"middle\" \
             dominant-baseline=\"auto\" font-size=\"12\" fill=\"currentColor\">{label}</text>"
        )
        .unwrap();
    }
}

fn route_orthogonal(from: &Bounds, to: &Bounds, obstacles: &[Bounds]) -> Option<Vec<(f64, f64)>> {
    let start = from.anchor_pos(AnchorPoint::Auto, to);
    let end = to.anchor_pos(AnchorPoint::Auto, from);
    if direct_auto_line_is_clean(start, end, obstacles) {
        return None;
    }

    let mut candidates = Vec::new();

    let mut x_lanes = route_x_lanes(from, to, ROUTE_MARGIN);
    let mut y_lanes = route_y_lanes(from, to, ROUTE_MARGIN);
    for b in obstacles {
        x_lanes.push(b.x - ROUTE_MARGIN);
        x_lanes.push(b.x + b.width + ROUTE_MARGIN);
        y_lanes.push(b.y - ROUTE_MARGIN);
        y_lanes.push(b.y + b.height + ROUTE_MARGIN);
    }

    for mid_x in x_lanes {
        candidates.push(build_hv_route(from, to, mid_x));
    }
    for mid_y in y_lanes {
        candidates.push(build_vh_route(from, to, mid_y));
    }

    candidates
        .into_iter()
        .filter(|points| {
            route_exits_endpoint_bounds(points, from, to)
                && route_has_visible_terminal_segments(points, ROUTE_TERMINAL_MIN)
                && !path_intersects_obstacle(points, obstacles)
        })
        .min_by(|a, b| {
            route_score(a)
                .partial_cmp(&route_score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn route_x_lanes(from: &Bounds, to: &Bounds, margin: f64) -> Vec<f64> {
    let from_left = from.x;
    let from_right = from.x + from.width;
    let to_left = to.x;
    let to_right = to.x + to.width;
    let mut lanes = Vec::new();

    if from_right + margin <= to_left - margin {
        lanes.push((from_right + to_left) / 2.0);
    } else if to_right + margin <= from_left - margin {
        lanes.push((to_right + from_left) / 2.0);
    }

    lanes.push(from_left.min(to_left) - ROUTE_TERMINAL_MIN);
    lanes.push(from_right.max(to_right) + ROUTE_TERMINAL_MIN);
    lanes
}

fn route_y_lanes(from: &Bounds, to: &Bounds, margin: f64) -> Vec<f64> {
    let from_top = from.y;
    let from_bottom = from.y + from.height;
    let to_top = to.y;
    let to_bottom = to.y + to.height;
    let mut lanes = Vec::new();

    if from_bottom + margin <= to_top - margin {
        lanes.push((from_bottom + to_top) / 2.0);
    } else if to_bottom + margin <= from_top - margin {
        lanes.push((to_bottom + from_top) / 2.0);
    }

    lanes.push(from_top.min(to_top) - ROUTE_TERMINAL_MIN);
    lanes.push(from_bottom.max(to_bottom) + ROUTE_TERMINAL_MIN);
    lanes
}

fn direct_auto_line_is_clean(start: (f64, f64), end: (f64, f64), obstacles: &[Bounds]) -> bool {
    let aligned = (start.0 - end.0).abs() < 0.001 || (start.1 - end.1).abs() < 0.001;
    aligned && !path_intersects_obstacle(&[start, end], obstacles)
}

fn build_hv_route(from: &Bounds, to: &Bounds, mid_x: f64) -> Vec<(f64, f64)> {
    let fc = bounds_center(from);
    let tc = bounds_center(to);
    let from_anchor = horizontal_anchor(nonzero_delta(mid_x - fc.0, tc.0 - fc.0));
    let to_anchor = if nonzero_delta(tc.0 - mid_x, tc.0 - fc.0) >= 0.0 {
        AnchorPoint::Left
    } else {
        AnchorPoint::Right
    };
    let start = from.anchor_pos(from_anchor, to);
    let end = to.anchor_pos(to_anchor, from);
    simplify_points(vec![start, (mid_x, start.1), (mid_x, end.1), end])
}

fn build_vh_route(from: &Bounds, to: &Bounds, mid_y: f64) -> Vec<(f64, f64)> {
    let fc = bounds_center(from);
    let tc = bounds_center(to);
    let from_anchor = vertical_anchor(nonzero_delta(mid_y - fc.1, tc.1 - fc.1));
    let to_anchor = if nonzero_delta(tc.1 - mid_y, tc.1 - fc.1) >= 0.0 {
        AnchorPoint::Top
    } else {
        AnchorPoint::Bottom
    };
    let start = from.anchor_pos(from_anchor, to);
    let end = to.anchor_pos(to_anchor, from);
    simplify_points(vec![start, (start.0, mid_y), (end.0, mid_y), end])
}

fn bounds_center(bounds: &Bounds) -> (f64, f64) {
    (
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    )
}

fn horizontal_anchor(delta: f64) -> AnchorPoint {
    if delta >= 0.0 {
        AnchorPoint::Right
    } else {
        AnchorPoint::Left
    }
}

fn vertical_anchor(delta: f64) -> AnchorPoint {
    if delta >= 0.0 {
        AnchorPoint::Bottom
    } else {
        AnchorPoint::Top
    }
}

fn nonzero_delta(delta: f64, fallback: f64) -> f64 {
    if delta.abs() < f64::EPSILON {
        fallback
    } else {
        delta
    }
}

fn simplify_points(points: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut simplified = Vec::new();
    for point in points {
        if simplified
            .last()
            .map(|last: &(f64, f64)| {
                (last.0 - point.0).abs() < 0.001 && (last.1 - point.1).abs() < 0.001
            })
            .unwrap_or(false)
        {
            continue;
        }
        simplified.push(point);
    }
    simplified
}

fn route_score(points: &[(f64, f64)]) -> f64 {
    let length: f64 = points
        .windows(2)
        .map(|segment| segment_length(segment[0], segment[1]))
        .sum();
    let bends = points.len().saturating_sub(2) as f64;
    length + bends * 20.0
}

fn segment_length(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.0 - a.0).abs() + (b.1 - a.1).abs()
}

fn path_data(points: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (idx, (x, y)) in points.iter().enumerate() {
        if idx == 0 {
            write!(d, "M {x} {y}").unwrap();
        } else {
            write!(d, " L {x} {y}").unwrap();
        }
    }
    d
}

fn path_intersects_obstacle(points: &[(f64, f64)], obstacles: &[Bounds]) -> bool {
    points.windows(2).any(|segment| {
        obstacles
            .iter()
            .any(|b| segment_intersects_bounds(segment[0], segment[1], b))
    })
}

fn route_exits_endpoint_bounds(points: &[(f64, f64)], from: &Bounds, to: &Bounds) -> bool {
    if points.len() < 2 {
        return false;
    }

    let first = points[0];
    let second = points[1];
    let penultimate = points[points.len() - 2];
    let last = points[points.len() - 1];

    leg_exits_bounds(from, first, second) && leg_enters_bounds(to, penultimate, last)
}

fn route_has_visible_terminal_segments(points: &[(f64, f64)], min_len: f64) -> bool {
    if points.len() < 4 {
        return true;
    }

    segment_length(points[0], points[1]) >= min_len
        && segment_length(points[points.len() - 2], points[points.len() - 1]) >= min_len
}

fn leg_exits_bounds(bounds: &Bounds, edge: (f64, f64), next: (f64, f64)) -> bool {
    if nearly_eq(edge.0, next.0) {
        if nearly_eq(edge.1, bounds.y) {
            return next.1 <= bounds.y;
        }
        if nearly_eq(edge.1, bounds.y + bounds.height) {
            return next.1 >= bounds.y + bounds.height;
        }
    }

    if nearly_eq(edge.1, next.1) {
        if nearly_eq(edge.0, bounds.x) {
            return next.0 <= bounds.x;
        }
        if nearly_eq(edge.0, bounds.x + bounds.width) {
            return next.0 >= bounds.x + bounds.width;
        }
    }

    false
}

fn leg_enters_bounds(bounds: &Bounds, prev: (f64, f64), edge: (f64, f64)) -> bool {
    leg_exits_bounds(bounds, edge, prev)
}

fn segment_intersects_bounds(a: (f64, f64), b: (f64, f64), bounds: &Bounds) -> bool {
    let (mut t0, mut t1) = (0.0, 1.0);
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    for (p, q) in [
        (-dx, a.0 - bounds.x),
        (dx, bounds.x + bounds.width - a.0),
        (-dy, a.1 - bounds.y),
        (dy, bounds.y + bounds.height - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return false;
                }
                t0 = f64::max(t0, r);
            } else {
                if r < t0 {
                    return false;
                }
                t1 = f64::min(t1, r);
            }
        }
    }
    true
}

fn bounds_contains_point(bounds: &Bounds, x: f64, y: f64) -> bool {
    x >= bounds.x && x <= bounds.x + bounds.width && y >= bounds.y && y <= bounds.y + bounds.height
}

fn nearly_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.001
}

fn ctrl_point(x: f64, y: f64, anchor: AnchorPoint, dx: f64, dy: f64) -> (f64, f64) {
    match anchor {
        AnchorPoint::Right => (x + dx, y),
        AnchorPoint::Left => (x - dx, y),
        AnchorPoint::Bottom => (x, y + dy),
        AnchorPoint::Top => (x, y - dy),
        _ => (x + dx, y),
    }
}

fn svg_style_attrs(attrs: &IndexMap<String, String>) -> String {
    let mut s = String::new();
    for name in &[
        "fill",
        "stroke",
        "stroke_width",
        "stroke_dasharray",
        "opacity",
    ] {
        if let Some(val) = attrs.get(*name) {
            let svg_name = name.replace('_', "-");
            write!(s, " {svg_name}=\"{val}\"").unwrap();
        }
    }
    s
}

fn attr_f64(attrs: &IndexMap<String, String>, key: &str) -> Option<f64> {
    attrs.get(key).and_then(|s| s.parse().ok())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(id: &str, width: f64, height: f64) -> ShapeNode {
        ShapeNode {
            kind: ShapeKind::Rect,
            id: Some(id.to_string()),
            x: None,
            y: None,
            width: Some(width),
            height: Some(height),
            top: None,
            bottom: None,
            left: None,
            right: None,
            resolved: Bounds::default(),
            attrs: IndexMap::new(),
            children: vec![],
            align: Alignment::None,
            gap: 0.0,
            padding: 0.0,
        }
    }

    fn connection(from: &str, to: &str) -> Connection {
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
    fn test_resolve_axis_absolute() {
        assert_eq!(
            resolve_axis(Some(10.0), Some(100.0), None, None, 0.0, 500.0),
            (10.0, 100.0)
        );
    }

    #[test]
    fn test_resolve_axis_anchored_both() {
        assert_eq!(
            resolve_axis(None, None, Some(20.0), Some(30.0), 0.0, 500.0),
            (20.0, 450.0)
        );
    }

    #[test]
    fn test_resolve_axis_anchored_near_with_size() {
        assert_eq!(
            resolve_axis(None, Some(100.0), Some(20.0), None, 0.0, 500.0),
            (20.0, 100.0)
        );
    }

    #[test]
    fn test_resolve_axis_anchored_far_with_size() {
        assert_eq!(
            resolve_axis(None, Some(100.0), None, Some(30.0), 0.0, 500.0),
            (370.0, 100.0)
        );
    }

    #[test]
    fn test_anchor_points() {
        let b = Bounds {
            x: 100.0,
            y: 50.0,
            width: 200.0,
            height: 100.0,
        };
        let other = Bounds::default();
        assert_eq!(b.anchor_pos(AnchorPoint::Top, &other), (200.0, 50.0));
        assert_eq!(b.anchor_pos(AnchorPoint::Bottom, &other), (200.0, 150.0));
        assert_eq!(b.anchor_pos(AnchorPoint::Left, &other), (100.0, 100.0));
        assert_eq!(b.anchor_pos(AnchorPoint::Right, &other), (300.0, 100.0));
    }

    #[test]
    fn test_simple_diagram() {
        let mut diagram = Diagram {
            width: 400.0,
            height: 200.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes: vec![ShapeNode {
                kind: ShapeKind::Rect,
                id: Some("box1".into()),
                x: Some(10.0),
                y: Some(10.0),
                width: Some(100.0),
                height: Some(50.0),
                top: None,
                bottom: None,
                left: None,
                right: None,
                resolved: Bounds::default(),
                attrs: [("fill".into(), "#ccc".into())].into_iter().collect(),
                children: vec![],
                align: Alignment::None,
                gap: 0.0,
                padding: 0.0,
            }],
            connections: vec![],
        };
        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<rect"));
        assert!(svg.contains("x=\"10\""));
        assert!(svg.contains("fill=\"#ccc\""));
        assert!(svg.contains("wdoc-diagram"));
    }

    #[test]
    fn nested_grid_layout_ignores_template_decoration() {
        let mut frame = shape("frame", 200.0, 120.0);
        frame.x = Some(0.0);
        frame.y = Some(0.0);
        frame
            .attrs
            .insert(LAYOUT_DECORATION_ATTR.to_string(), "true".to_string());

        let mut label = shape("label", 180.0, 20.0);
        label.x = Some(10.0);
        label.y = Some(4.0);
        label
            .attrs
            .insert(LAYOUT_DECORATION_ATTR.to_string(), "true".to_string());

        let mut boundary = shape("boundary", 200.0, 120.0);
        boundary.x = Some(10.0);
        boundary.y = Some(20.0);
        boundary.align = Alignment::Grid;
        boundary.gap = 10.0;
        boundary
            .attrs
            .insert("columns".to_string(), "2".to_string());
        boundary.children = vec![frame, label, shape("a", 30.0, 20.0), shape("b", 30.0, 20.0)];

        let mut diagram = Diagram {
            width: 240.0,
            height: 180.0,
            shapes: vec![boundary],
            connections: vec![],
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let children = &diagram.shapes[0].children;
        assert_eq!(children[0].resolved.x, 0.0);
        assert_eq!(children[0].resolved.y, 0.0);
        assert_eq!(children[1].resolved.x, 10.0);
        assert_eq!(children[1].resolved.y, 4.0);
        assert!(children[2].resolved.x > 0.0);
        assert!(children[3].resolved.x > children[2].resolved.x);
        assert_eq!(children[2].resolved.y, children[3].resolved.y);
    }

    #[test]
    fn nested_stack_layout_positions_unanchored_children_in_container() {
        let mut container = shape("container", 100.0, 100.0);
        container.x = Some(20.0);
        container.y = Some(30.0);
        container.align = Alignment::Stack;
        container.gap = 10.0;
        container.padding = 5.0;
        container.children = vec![shape("a", 0.0, 20.0), shape("b", 0.0, 20.0)];

        let mut diagram = Diagram {
            width: 200.0,
            height: 200.0,
            shapes: vec![container],
            connections: vec![],
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let children = &diagram.shapes[0].children;
        assert_eq!(children[0].resolved.x, 5.0);
        assert_eq!(children[0].resolved.y, 5.0);
        assert_eq!(children[0].resolved.width, 90.0);
        assert_eq!(children[1].resolved.x, 5.0);
        assert_eq!(children[1].resolved.y, 35.0);
        assert_eq!(children[1].resolved.width, 90.0);
    }

    #[test]
    fn nested_graph_layout_uses_dotted_connection_ids() {
        let mut boundary = shape("boundary", 240.0, 180.0);
        boundary.x = Some(10.0);
        boundary.y = Some(10.0);
        boundary.align = Alignment::Layered;
        boundary.gap = 30.0;
        boundary.children = vec![shape("a", 40.0, 20.0), shape("b", 40.0, 20.0)];

        let mut diagram = Diagram {
            width: 300.0,
            height: 240.0,
            shapes: vec![boundary],
            connections: vec![connection("boundary.a", "boundary.b")],
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let children = &diagram.shapes[0].children;
        assert!(children[0].resolved.y < children[1].resolved.y);
    }

    #[test]
    fn top_level_layered_layout_respects_diagram_padding() {
        let mut diagram = Diagram {
            width: 240.0,
            height: 180.0,
            shapes: vec![shape("a", 100.0, 60.0), shape("b", 100.0, 60.0)],
            connections: vec![connection("a", "b")],
            padding: 30.0,
            align: Alignment::Layered,
            gap: 80.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        for child in &diagram.shapes {
            assert!(child.resolved.x >= 30.0);
            assert!(child.resolved.y >= 30.0);
            assert!(child.resolved.x + child.resolved.width <= 210.0);
            assert!(child.resolved.y + child.resolved.height <= 150.0);
        }
    }

    #[test]
    fn nested_layered_layout_keeps_children_inside_fixed_boundary() {
        let mut boundary = shape("boundary", 120.0, 120.0);
        boundary.x = Some(20.0);
        boundary.y = Some(20.0);
        boundary.align = Alignment::Layered;
        boundary.gap = 80.0;
        boundary.children = vec![
            shape("a", 80.0, 40.0),
            shape("b", 80.0, 40.0),
            shape("c", 80.0, 40.0),
        ];

        let mut diagram = Diagram {
            width: 200.0,
            height: 200.0,
            shapes: vec![boundary],
            connections: vec![
                connection("boundary.a", "boundary.b"),
                connection("boundary.b", "boundary.c"),
            ],
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let boundary = &diagram.shapes[0];
        for child in &boundary.children {
            assert!(child.resolved.x >= 0.0);
            assert!(child.resolved.y >= 0.0);
            assert!(child.resolved.x + child.resolved.width <= boundary.resolved.width);
            assert!(child.resolved.y + child.resolved.height <= boundary.resolved.height);
        }
    }

    #[test]
    fn nested_layered_layout_respects_boundary_padding_on_all_sides() {
        let mut frame = shape("frame", 140.0, 140.0);
        frame.x = Some(0.0);
        frame.y = Some(0.0);
        frame
            .attrs
            .insert(LAYOUT_DECORATION_ATTR.to_string(), "true".to_string());

        let mut boundary = shape("boundary", 140.0, 140.0);
        boundary.x = Some(20.0);
        boundary.y = Some(20.0);
        boundary.align = Alignment::Layered;
        boundary.gap = 80.0;
        boundary.padding = 16.0;
        boundary.children = vec![frame, shape("a", 70.0, 30.0), shape("b", 70.0, 30.0)];

        let mut diagram = Diagram {
            width: 220.0,
            height: 220.0,
            shapes: vec![boundary],
            connections: vec![connection("boundary.a", "boundary.b")],
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let boundary = &diagram.shapes[0];
        let frame = &boundary.children[0];
        assert_eq!(frame.resolved.x, 0.0);
        assert_eq!(frame.resolved.y, 0.0);

        for child in boundary
            .children
            .iter()
            .filter(|c| !is_layout_decoration(c))
        {
            assert!(child.resolved.x >= boundary.padding);
            assert!(child.resolved.y >= boundary.padding);
            assert!(
                child.resolved.x + child.resolved.width
                    <= boundary.resolved.width - boundary.padding
            );
            assert!(
                child.resolved.y + child.resolved.height
                    <= boundary.resolved.height - boundary.padding
            );
        }
    }

    #[test]
    fn decorated_layered_boundary_reserves_header_without_explicit_padding() {
        let mut frame = shape("frame", 180.0, 150.0);
        frame.x = Some(0.0);
        frame.y = Some(0.0);
        frame
            .attrs
            .insert(LAYOUT_DECORATION_ATTR.to_string(), "true".to_string());

        let mut label = shape("label", 160.0, 18.0);
        label.kind = ShapeKind::Text;
        label.x = Some(8.0);
        label.y = Some(4.0);
        label
            .attrs
            .insert(LAYOUT_DECORATION_ATTR.to_string(), "true".to_string());

        let mut boundary = shape("boundary", 180.0, 150.0);
        boundary.x = Some(20.0);
        boundary.y = Some(20.0);
        boundary.align = Alignment::Layered;
        boundary.gap = 40.0;
        boundary.children = vec![frame, label, shape("a", 80.0, 30.0), shape("b", 80.0, 30.0)];

        let mut diagram = Diagram {
            width: 240.0,
            height: 220.0,
            shapes: vec![boundary],
            connections: vec![connection("boundary.a", "boundary.b")],
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let boundary = &diagram.shapes[0];
        let label_bottom = boundary.children[1].resolved.y + boundary.children[1].resolved.height;
        for child in boundary
            .children
            .iter()
            .filter(|c| !is_layout_decoration(c))
        {
            assert!(child.resolved.x >= 16.0);
            assert!(child.resolved.y >= label_bottom + 6.0);
            assert!(child.resolved.x + child.resolved.width <= boundary.resolved.width - 16.0);
            assert!(child.resolved.y + child.resolved.height <= boundary.resolved.height - 16.0);
        }
    }

    #[test]
    fn nested_layered_layout_wraps_wide_rank_and_expands_boundary_height() {
        let mut boundary = shape("boundary", 120.0, 80.0);
        boundary.x = Some(20.0);
        boundary.y = Some(20.0);
        boundary.align = Alignment::Layered;
        boundary.gap = 20.0;
        boundary.children = vec![
            shape("a", 80.0, 40.0),
            shape("b", 80.0, 40.0),
            shape("c", 80.0, 40.0),
            shape("d", 80.0, 40.0),
        ];

        let mut diagram = Diagram {
            width: 240.0,
            height: 300.0,
            shapes: vec![boundary],
            connections: vec![
                connection("boundary.a", "boundary.d"),
                connection("boundary.b", "boundary.d"),
                connection("boundary.c", "boundary.d"),
            ],
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let boundary = &diagram.shapes[0];
        let children = &boundary.children;
        assert!(boundary.resolved.height > 80.0);
        assert_eq!(children[0].resolved.width, 80.0);
        assert_eq!(children[1].resolved.width, 80.0);
        assert!(!overlaps(&children[0], &children[1]));
        assert!(!overlaps(&children[1], &children[2]));
        for child in children {
            assert!(child.resolved.x + child.resolved.width <= boundary.resolved.width);
            assert!(child.resolved.y + child.resolved.height <= boundary.resolved.height);
        }
    }

    #[test]
    fn fixed_container_clamps_oversized_child_origin() {
        let mut child = shape("child", 80.0, 80.0);
        child.x = Some(100.0);
        child.y = Some(100.0);

        let mut container = shape("container", 50.0, 50.0);
        container.x = Some(10.0);
        container.y = Some(10.0);
        container.children = vec![child];

        let mut diagram = Diagram {
            width: 120.0,
            height: 120.0,
            shapes: vec![container],
            connections: vec![],
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let child = &diagram.shapes[0].children[0];
        assert_eq!(child.resolved.x, 0.0);
        assert_eq!(child.resolved.y, 0.0);
    }

    #[test]
    fn unsized_container_derives_size_from_children() {
        let mut child = shape("child", 90.0, 50.0);
        child.x = Some(20.0);
        child.y = Some(30.0);

        let mut container = shape("container", 0.0, 0.0);
        container.width = None;
        container.height = None;
        container.padding = 5.0;
        container.children = vec![child];

        let mut diagram = Diagram {
            width: 200.0,
            height: 200.0,
            shapes: vec![container],
            connections: vec![],
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let container = &diagram.shapes[0];
        assert!(container.resolved.width >= 120.0);
        assert!(container.resolved.height >= 90.0);
    }

    #[test]
    fn straight_auto_connection_routes_around_obstacle() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "a".to_string(),
            Bounds {
                x: 0.0,
                y: 40.0,
                width: 40.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "b".to_string(),
            Bounds {
                x: 160.0,
                y: 40.0,
                width: 40.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "mid".to_string(),
            Bounds {
                x: 80.0,
                y: 40.0,
                width: 40.0,
                height: 40.0,
            },
        );
        let mut svg = String::new();
        render_connection_svg(&connection("a", "b"), &shape_map, &mut svg);

        assert!(svg.contains("<path"));
        assert!(!svg.contains("<line"));
    }

    #[test]
    fn routed_auto_connection_uses_target_side_matching_final_segment() {
        let from = Bounds {
            x: 0.0,
            y: 80.0,
            width: 40.0,
            height: 40.0,
        };
        let to = Bounds {
            x: 160.0,
            y: 80.0,
            width: 40.0,
            height: 40.0,
        };
        let obstacle = Bounds {
            x: 80.0,
            y: 80.0,
            width: 40.0,
            height: 40.0,
        };

        let points = route_orthogonal(&from, &to, &[obstacle]).expect("expected routed path");
        assert_eq!(points.last().copied(), Some((180.0, 80.0)));
    }

    #[test]
    fn routed_auto_connection_keeps_visible_segment_before_target_marker() {
        let from = Bounds {
            x: 636.0,
            y: 37.0,
            width: 190.0,
            height: 55.0,
        };
        let to = Bounds {
            x: 636.0,
            y: 610.0,
            width: 190.0,
            height: 55.0,
        };
        let obstacle = Bounds {
            x: 636.0,
            y: 135.0,
            width: 190.0,
            height: 55.0,
        };

        let points = route_orthogonal(&from, &to, &[obstacle]).expect("expected routed path");
        let approach = segment_length(points[points.len() - 2], points[points.len() - 1]);

        assert!(route_has_visible_terminal_segments(
            &points,
            ROUTE_TERMINAL_MIN
        ));
        assert!(approach >= ROUTE_TERMINAL_MIN);
    }

    #[test]
    fn routed_auto_connection_keeps_visible_segment_after_source_marker() {
        let from = Bounds {
            x: 636.0,
            y: 610.0,
            width: 190.0,
            height: 55.0,
        };
        let to = Bounds {
            x: 636.0,
            y: 37.0,
            width: 190.0,
            height: 55.0,
        };
        let obstacle = Bounds {
            x: 636.0,
            y: 500.0,
            width: 190.0,
            height: 55.0,
        };

        let points = route_orthogonal(&from, &to, &[obstacle]).expect("expected routed path");
        let departure = segment_length(points[0], points[1]);

        assert!(route_has_visible_terminal_segments(
            &points,
            ROUTE_TERMINAL_MIN
        ));
        assert!(departure >= ROUTE_TERMINAL_MIN);
    }

    #[test]
    fn straight_auto_connection_without_obstacle_keeps_line_rendering() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "a".to_string(),
            Bounds {
                x: 0.0,
                y: 40.0,
                width: 40.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "b".to_string(),
            Bounds {
                x: 160.0,
                y: 40.0,
                width: 40.0,
                height: 40.0,
            },
        );
        let mut svg = String::new();
        render_connection_svg(&connection("a", "b"), &shape_map, &mut svg);

        assert!(svg.contains("<line"));
        assert!(!svg.contains("<path"));
    }

    #[test]
    fn diagonal_auto_connection_without_obstacle_uses_elbow() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "a".to_string(),
            Bounds {
                x: 100.0,
                y: 40.0,
                width: 80.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "b".to_string(),
            Bounds {
                x: 20.0,
                y: 160.0,
                width: 80.0,
                height: 40.0,
            },
        );
        let mut svg = String::new();
        render_connection_svg(&connection("a", "b"), &shape_map, &mut svg);

        assert!(svg.contains("<path"));
        assert!(!svg.contains("<line"));
    }

    #[test]
    fn diagonal_auto_connection_exits_endpoint_bounds_before_turning() {
        let from = Bounds {
            x: 510.0,
            y: 485.0,
            width: 190.0,
            height: 55.0,
        };
        let to = Bounds {
            x: 385.0,
            y: 603.0,
            width: 190.0,
            height: 55.0,
        };

        let points = route_orthogonal(&from, &to, &[]).expect("expected elbow path");

        assert!(route_exits_endpoint_bounds(&points, &from, &to));
        assert_eq!(points.first().copied(), Some((605.0, 540.0)));
        assert_eq!(points.last().copied(), Some((480.0, 603.0)));
        assert!(points[1].1 >= from.y + from.height);
        assert!(points[points.len() - 2].1 <= to.y);
    }

    #[test]
    fn explicit_anchor_connection_preserves_line_rendering() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "a".to_string(),
            Bounds {
                x: 0.0,
                y: 40.0,
                width: 40.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "b".to_string(),
            Bounds {
                x: 160.0,
                y: 40.0,
                width: 40.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "mid".to_string(),
            Bounds {
                x: 80.0,
                y: 40.0,
                width: 40.0,
                height: 40.0,
            },
        );
        let mut conn = connection("a", "b");
        conn.from_anchor = AnchorPoint::Right;
        conn.to_anchor = AnchorPoint::Left;
        let mut svg = String::new();
        render_connection_svg(&conn, &shape_map, &mut svg);

        assert!(svg.contains("<line"));
    }
}
