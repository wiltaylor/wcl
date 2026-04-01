//! Vector shape drawing system — resolves layout and renders shape trees to SVG.
//!
//! The CLI handler builds `ShapeNode` trees from WCL `Value` data, then calls
//! `render_diagram_svg()` to produce inline SVG.

use std::collections::HashMap;
use std::fmt::Write;

use indexmap::IndexMap;

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
    resolve_children(&mut diagram.shapes, &inner, Alignment::None, 0.0);

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
        "svg_rect" => Some(ShapeKind::Rect),
        "svg_circle" => Some(ShapeKind::Circle),
        "svg_ellipse" => Some(ShapeKind::Ellipse),
        "svg_line" => Some(ShapeKind::Line),
        "svg_path" => Some(ShapeKind::Path),
        "svg_text" => Some(ShapeKind::Text),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Layout resolution
// ---------------------------------------------------------------------------

fn resolve_children(children: &mut [ShapeNode], parent: &Bounds, align: Alignment, gap: f64) {
    // First pass: resolve anchored/absolute children
    for child in children.iter_mut() {
        resolve_bounds(child, parent);
    }

    // Second pass: position unpositioned children via alignment engine
    let unpositioned: Vec<usize> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.x.is_none() && c.y.is_none() && c.top.is_none() && c.left.is_none())
        .map(|(i, _)| i)
        .collect();

    if !unpositioned.is_empty() {
        match align {
            Alignment::Stack => layout_stack(children, &unpositioned, parent, gap),
            Alignment::Flow => layout_flow(children, &unpositioned, parent, gap),
            Alignment::Center => layout_center(children, &unpositioned, parent),
            Alignment::None => {}
        }
    }

    // Recurse into children
    for child in children.iter_mut() {
        let inner = Bounds {
            x: 0.0,
            y: 0.0,
            width: (child.resolved.width - child.padding * 2.0).max(0.0),
            height: (child.resolved.height - child.padding * 2.0).max(0.0),
        };
        resolve_children(&mut child.children, &inner, child.align, child.gap);
    }
}

fn resolve_bounds(node: &mut ShapeNode, parent: &Bounds) {
    let (mut rx, mut rw) = resolve_axis(node.x, node.width, node.left, node.right, parent.width);
    let (mut ry, mut rh) = resolve_axis(node.y, node.height, node.top, node.bottom, parent.height);

    // Text shapes with no explicit position/size fill their parent's inner space
    if node.kind == ShapeKind::Text && rw == 0.0 && rh == 0.0 {
        rx = 0.0;
        ry = 0.0;
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
    parent_size: f64,
) -> (f64, f64) {
    match (pos, size, near, far) {
        (Some(p), Some(s), _, _) => (p, s),
        (Some(p), None, _, _) => (p, 0.0),
        (_, _, Some(n), Some(f)) => (n, size.unwrap_or((parent_size - n - f).max(0.0))),
        (_, Some(s), Some(n), None) => (n, s),
        (_, Some(s), None, Some(f)) => ((parent_size - f - s).max(0.0), s),
        (_, Some(s), None, None) => (0.0, s),
        _ => (0.0, 0.0),
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
                ox + shape.resolved.x + shape.padding,
                oy + shape.resolved.y + shape.padding,
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
            // Center text within resolved bounds
            let tx = b.x + b.width / 2.0;
            let ty = b.y + b.height / 2.0;
            write!(
                svg,
                "<text x=\"{tx}\" y=\"{ty}\" font-size=\"{font_size}\" \
                 text-anchor=\"{anchor}\" dominant-baseline=\"central\"{style}>{content}</text>"
            )
            .unwrap();
        }
    }

    // Render children in a translated group
    if !node.children.is_empty() {
        let gx = b.x + node.padding;
        let gy = b.y + node.padding;
        write!(svg, "<g transform=\"translate({gx},{gy})\">").unwrap();
        for child in &node.children {
            render_shape_svg(child, svg);
        }
        svg.push_str("</g>");
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

    let (x1, y1) = from_bounds.anchor_pos(conn.from_anchor, to_bounds);
    let (x2, y2) = to_bounds.anchor_pos(conn.to_anchor, from_bounds);

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
            write!(
                svg,
                "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"\
                 {stroke_default}{style}{ms}{me}/>"
            )
            .unwrap();
        }
        CurveStyle::Bezier => {
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
        let mx = (x1 + x2) / 2.0;
        let my = (y1 + y2) / 2.0;
        write!(
            svg,
            "<text x=\"{mx}\" y=\"{my}\" text-anchor=\"middle\" \
             dominant-baseline=\"central\" font-size=\"12\" fill=\"currentColor\">{label}</text>"
        )
        .unwrap();
    }
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

    #[test]
    fn test_resolve_axis_absolute() {
        assert_eq!(
            resolve_axis(Some(10.0), Some(100.0), None, None, 500.0),
            (10.0, 100.0)
        );
    }

    #[test]
    fn test_resolve_axis_anchored_both() {
        assert_eq!(
            resolve_axis(None, None, Some(20.0), Some(30.0), 500.0),
            (20.0, 450.0)
        );
    }

    #[test]
    fn test_resolve_axis_anchored_near_with_size() {
        assert_eq!(
            resolve_axis(None, Some(100.0), Some(20.0), None, 500.0),
            (20.0, 100.0)
        );
    }

    #[test]
    fn test_resolve_axis_anchored_far_with_size() {
        assert_eq!(
            resolve_axis(None, Some(100.0), None, Some(30.0), 500.0),
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
}
