//! Vector shape drawing system — resolves layout and renders shape trees to SVG.
//!
//! The CLI handler builds `ShapeNode` trees from WCL `Value` data, then calls
//! `render_diagram_svg()` to produce inline SVG.

use std::collections::HashMap;
use std::fmt::Write;
use std::hash::{DefaultHasher, Hash, Hasher};

use indexmap::IndexMap;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

const LAYOUT_DECORATION_ATTR: &str = "_wdoc_layout_decoration";
const FULL_CONTAINER_DECORATION_ATTR: &str = "_wdoc_full_container";
const CONNECTION_ROUTE_ATTR: &str = "_wdoc_route";
const CONTENT_INSET_LEFT_ATTR: &str = "_wdoc_content_left";
const CONTENT_INSET_TOP_ATTR: &str = "_wdoc_content_top";
const CONTENT_INSET_RIGHT_ATTR: &str = "_wdoc_content_right";
const CONTENT_INSET_BOTTOM_ATTR: &str = "_wdoc_content_bottom";
const CONNECTION_ROUTE_DIRECT: &str = "direct";
const SIZE_LOCKED_ATTR: &str = "_wdoc_size_locked";
const ROUTE_MARGIN: f64 = 18.0;
const ROUTE_TERMINAL_MIN: f64 = 24.0;
const PARALLEL_CORRIDOR_MIN: f64 = 8.0;
const DIRECT_ROUTE_ALIGNMENT_EPSILON: f64 = 6.0;

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
    TextBlock,
    InlineSvg,
    Icon,
    Image,
    Map,
    Sprite,
    DopesheetView,
    Tilemap,
    GameLayer,
    Group,
    Custom,
}

impl ShapeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ShapeKind::Rect => "rect",
            ShapeKind::Circle => "circle",
            ShapeKind::Ellipse => "ellipse",
            ShapeKind::Line => "line",
            ShapeKind::Path => "path",
            ShapeKind::Text => "text",
            ShapeKind::TextBlock => "text_block",
            ShapeKind::InlineSvg => "inline_svg",
            ShapeKind::Icon => "icon",
            ShapeKind::Image => "image",
            ShapeKind::Map => "map",
            ShapeKind::Sprite => "sprite",
            ShapeKind::DopesheetView => "dopesheet_view",
            ShapeKind::Tilemap => "tilemap",
            ShapeKind::GameLayer => "game_layer",
            ShapeKind::Group => "group",
            ShapeKind::Custom => "custom",
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParallelOffsetAxis {
    Auto,
    X,
    Y,
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
    /// Internal render classification. Built-ins use a concrete variant;
    /// user-defined/template-backed shapes use `Custom`.
    pub kind: ShapeKind,
    /// Original shape kind from WCL, preserved so custom schemas are not
    /// collapsed into a built-in Rust shape name.
    pub kind_name: String,
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
    pub events: Vec<DiagramEvent>,
    // Children
    pub children: Vec<ShapeNode>,
    pub text_block_items: Vec<TextBlockItem>,
    pub align: Alignment,
    pub gap: f64,
    pub padding: f64,
    pub z_index: f64,
    pub source_order: usize,
}

#[derive(Debug, Clone)]
pub enum TextBlockItem {
    Paragraph {
        html: String,
    },
    Code {
        content: String,
        language: Option<String>,
    },
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
    pub z_index: f64,
    pub source_order: usize,
}

/// A complete diagram ready to render.
pub struct Diagram {
    pub id: Option<String>,
    pub width: f64,
    pub height: f64,
    pub shapes: Vec<ShapeNode>,
    pub connections: Vec<Connection>,
    pub classes: IndexMap<String, DiagramClass>,
    pub padding: f64,
    pub align: Alignment,
    pub gap: f64,
    pub options: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct DiagramClass {
    pub name: String,
    pub attrs: IndexMap<String, String>,
    pub states: IndexMap<String, DiagramState>,
    pub animations: IndexMap<String, DiagramAnimation>,
}

#[derive(Debug, Clone, Default)]
pub struct DiagramState {
    pub name: String,
    pub attrs: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct DiagramAnimation {
    pub name: String,
    pub duration_ms: i32,
    pub delay_ms: i32,
    pub timing_function: String,
    pub iteration_count: String,
    pub direction: String,
    pub fill_mode: String,
    pub keyframes: Vec<DiagramKeyframe>,
    pub frame_rate: Option<f64>,
    pub frames: Vec<i32>,
}

#[derive(Debug, Clone, Default)]
pub struct DiagramKeyframe {
    pub offset: f64,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub rotate: Option<f64>,
    pub rotate_origin_x: Option<f64>,
    pub rotate_origin_y: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct DiagramEvent {
    pub name: Option<String>,
    pub trigger: String,
    pub state: String,
    pub target: Option<String>,
    pub button: Option<String>,
    pub mode: Option<String>,
    pub duration_ms: Option<i32>,
    pub prevent_default: Option<bool>,
    pub guard_targets: Option<String>,
    pub signal_actions: Vec<SignalAction>,
}

#[derive(Debug, Clone, Default)]
pub struct SignalAction {
    pub signal: String,
    pub value: serde_json::Value,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub width: f64,
    pub height: f64,
    pub baseline: f64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Measure rendered text using WDoc's deterministic fallback metrics.
///
/// This intentionally does not depend on platform fonts or browser APIs. It is
/// stable across machines and approximate rather than exact.
pub fn measure_text_attrs(attrs: &IndexMap<String, String>) -> TextMetrics {
    let content = attrs.get("content").map(|s| s.as_str()).unwrap_or("");
    let font_size = attrs
        .get("font_size")
        .and_then(|s| parse_svg_number(s))
        .unwrap_or(14.0);
    let line_height = attrs
        .get("line_height")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(1.2);
    let letter_spacing = attrs
        .get("letter_spacing")
        .map(|s| parse_css_length(s, font_size))
        .unwrap_or(0.0);
    let wrap_width = attrs
        .get("max_width")
        .or_else(|| attrs.get("width"))
        .and_then(|s| parse_svg_number(s))
        .filter(|w| *w > 0.0);

    let lines = wrapped_text_lines(content, wrap_width, font_size, letter_spacing, attrs);

    let line_step = font_size * line_height;
    let height = line_step * lines.len() as f64;
    let baseline = ((line_step - font_size).max(0.0) / 2.0) + font_size * 0.8;

    TextMetrics {
        width: lines
            .iter()
            .map(|line| measure_line_width(line, font_size, letter_spacing, attrs))
            .fold(0.0, f64::max),
        height,
        baseline,
    }
}

/// Resolve layout and render a diagram to an inline SVG string.
pub fn render_diagram_svg(diagram: &mut Diagram) -> String {
    apply_diagram_classes(diagram);

    // Phase 2: resolve layout
    let inner = Bounds {
        x: diagram.padding,
        y: diagram.padding,
        width: diagram.width - diagram.padding * 2.0,
        height: diagram.height - diagram.padding * 2.0,
    };
    let mut options = diagram.options.clone();
    options.insert("_wdoc_scale_to_fit".to_string(), "true".to_string());
    resolve_children(
        &mut diagram.shapes,
        &mut diagram.connections,
        &inner,
        "",
        diagram.align,
        diagram.gap,
        &options,
    );

    // Phase 2b: build shape map for connections
    let shape_map = build_shape_map(&diagram.shapes, 0.0, 0.0);

    // Phase 3: render SVG
    let mut svg = String::new();
    let class_attr = diagram
        .options
        .get("class")
        .map(|class| class.trim())
        .filter(|class| !class.is_empty())
        .map(|class| format!(" class=\"{}\"", svg_escape_attr(class)))
        .unwrap_or_default();
    let generated_css = diagram_generated_css(diagram);
    let authored_css = diagram
        .options
        .get("css")
        .map(|css| css.trim())
        .unwrap_or("");
    let css_for_scope = [generated_css.as_str(), authored_css]
        .into_iter()
        .filter(|css| !css.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let pan_zoom = diagram_pan_zoom_enabled(diagram);
    let needs_runtime = diagram_has_events(diagram) || pan_zoom;
    if needs_runtime {
        mark_runtime_shapes(&mut diagram.shapes);
    }
    let scoped_css = if css_for_scope.trim().is_empty() && !needs_runtime {
        None
    } else {
        let scope_id = diagram_scope_id(diagram, &css_for_scope);
        let scoped = if css_for_scope.trim().is_empty() {
            String::new()
        } else {
            scope_svg_css(&css_for_scope, &scope_id)
        };
        Some((scope_id, scoped))
    };
    let pan_zoom_attrs = if pan_zoom {
        " data-wdoc-pan-zoom=\"true\" data-wdoc-pan-zoom-min=\"0.25\" data-wdoc-pan-zoom-max=\"8\" style=\"cursor: grab; touch-action: none; user-select: none;\""
    } else {
        ""
    };
    let wrapper_attrs = if pan_zoom {
        " style=\"position: relative; display: inline-block;\""
    } else {
        ""
    };
    let pan_zoom_controls = if pan_zoom {
        "<div class=\"wdoc-diagram-pan-zoom-controls\" data-wdoc-pan-zoom-controls=\"true\" style=\"position: absolute; top: 8px; left: 8px; z-index: 1; display: flex; gap: 4px; background: rgba(13, 17, 23, 0.76); border: 1px solid rgba(148, 163, 184, 0.45); border-radius: 6px; padding: 4px; box-shadow: 0 4px 12px rgba(0, 0, 0, 0.22);\">\
         <button type=\"button\" data-wdoc-pan-zoom-control=\"in\" aria-label=\"Zoom in\" title=\"Zoom in\" style=\"width: 28px; height: 28px; border: 0; border-radius: 4px; background: #f8fafc; color: #0f172a; font: 600 16px/1 system-ui, sans-serif; cursor: pointer;\">+</button>\
         <button type=\"button\" data-wdoc-pan-zoom-control=\"out\" aria-label=\"Zoom out\" title=\"Zoom out\" style=\"width: 28px; height: 28px; border: 0; border-radius: 4px; background: #f8fafc; color: #0f172a; font: 600 16px/1 system-ui, sans-serif; cursor: pointer;\">-</button>\
         <button type=\"button\" data-wdoc-pan-zoom-control=\"reset\" aria-label=\"Reset zoom\" title=\"Reset zoom\" style=\"height: 28px; border: 0; border-radius: 4px; background: #f8fafc; color: #0f172a; font: 600 12px/1 system-ui, sans-serif; cursor: pointer; padding: 0 8px;\">Reset</button>\
         </div>"
    } else {
        ""
    };
    if let Some((scope_id, _)) = &scoped_css {
        let scope_id = svg_escape_attr(scope_id);
        write!(
            svg,
            "<div class=\"wdoc-diagram\"{wrapper_attrs}>{pan_zoom_controls}\
             <svg xmlns=\"http://www.w3.org/2000/svg\" id=\"{scope_id}\"{class_attr} \
             width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"{pan_zoom_attrs}>",
            diagram.width, diagram.height, diagram.width, diagram.height
        )
        .unwrap();
    } else {
        write!(
            svg,
            "<div class=\"wdoc-diagram\"{wrapper_attrs}>{pan_zoom_controls}\
             <svg xmlns=\"http://www.w3.org/2000/svg\"{class_attr} \
             width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"{pan_zoom_attrs}>",
            diagram.width, diagram.height, diagram.width, diagram.height
        )
        .unwrap();
    }

    if let Some((_, css)) = &scoped_css {
        if !css.trim().is_empty() {
            write!(svg, "<style>{}</style>", svg_escape_text(css)).unwrap();
        }
    }

    // Arrow marker defs
    if diagram
        .connections
        .iter()
        .any(|c| c.direction != Direction::None)
    {
        svg.push_str(ARROW_DEFS);
    }

    render_diagram_items_svg(&diagram.shapes, &diagram.connections, &shape_map, &mut svg);

    svg.push_str("</svg>");

    if needs_runtime {
        write!(
            svg,
            "<script>{}</script>",
            html_escape_script(&diagram_runtime_js())
        )
        .unwrap();
    }

    svg.push_str("</div>");
    svg
}

fn apply_diagram_classes(diagram: &mut Diagram) {
    let classes = diagram.classes.clone();
    for shape in &mut diagram.shapes {
        apply_classes_to_shape(shape, &classes);
    }
}

fn apply_classes_to_shape(shape: &mut ShapeNode, classes: &IndexMap<String, DiagramClass>) {
    let class_names: Vec<String> = shape
        .attrs
        .get("class")
        .map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    for class_name in class_names {
        if let Some(class) = classes.get(&class_name) {
            for (key, value) in &class.attrs {
                if key == "z_index" {
                    if shape.z_index == 0.0 {
                        if let Ok(z) = value.parse::<f64>() {
                            shape.z_index = z;
                        }
                    }
                    continue;
                }
                if !is_supported_class_property(key) {
                    continue;
                }
                let (svg_key, svg_value) = class_attr_to_svg_attr_value(key, value);
                shape.attrs.entry(svg_key).or_insert(svg_value);
            }
            let mut state_z_entries = Vec::new();
            if let Some(existing) = shape.attrs.get("_wdoc_state_z") {
                state_z_entries.push(existing.clone());
            }
            for state in class.states.values() {
                if let Some(z) = state
                    .attrs
                    .get("z_index")
                    .and_then(|s| s.parse::<f64>().ok())
                {
                    state_z_entries.push(format!("{}:{}", state.name, z));
                }
            }
            if !state_z_entries.is_empty() {
                shape
                    .attrs
                    .insert("_wdoc_state_z".to_string(), state_z_entries.join(","));
            }
            if !class.states.is_empty() {
                shape
                    .attrs
                    .insert("_wdoc_runtime".to_string(), "true".to_string());
            }
            let state_animation_entries = class
                .states
                .values()
                .filter_map(|state| {
                    let animation = state.attrs.get("animation")?;
                    class
                        .animations
                        .contains_key(animation)
                        .then(|| format!("{}:{}", state.name, animation))
                })
                .collect::<Vec<_>>();
            if !state_animation_entries.is_empty() {
                shape.attrs.insert(
                    "_wdoc_state_animation".to_string(),
                    state_animation_entries.join(","),
                );
            }
            if !class.animations.is_empty() {
                shape.attrs.insert(
                    "_wdoc_animations".to_string(),
                    diagram_animations_data(&class.animations),
                );
            }
        }
    }
    for child in &mut shape.children {
        apply_classes_to_shape(child, classes);
    }
}

fn diagram_generated_css(diagram: &Diagram) -> String {
    let mut css = String::new();
    if diagram_has_class(&diagram.shapes, "wdoc-menu-item") {
        css.push_str(
            ".wdoc-menu-item .wdoc-menu-item-bg { opacity: 0; transition: opacity 120ms ease; }\n",
        );
        css.push_str(".wdoc-menu-item.wdoc-state-hovered .wdoc-menu-item-bg { opacity: 1; }\n");
        css.push_str(".wdoc-menu-item-disabled { opacity: .45; cursor: default; }\n");
        css.push_str(".wdoc-menu-item-disabled .wdoc-menu-item-bg { opacity: 0 !important; }\n");
    }
    for class in diagram.classes.values() {
        if !class.attrs.is_empty() {
            let decls = class_attrs_to_css_decls(&class.attrs, false);
            if !decls.is_empty() {
                let selector = format!(".{}", css_ident_escape(&class.name));
                writeln!(css, "{selector} {{ {decls} }}").unwrap();
            }
        }
        for state in class.states.values() {
            let decls = class_attrs_to_css_decls(&state.attrs, true);
            if !decls.is_empty() {
                let selector = format!(
                    ".{}.{}",
                    css_ident_escape(&class.name),
                    state_class_name(&state.name)
                );
                writeln!(css, "{selector} {{ {decls} }}").unwrap();
            }
        }
    }
    css
}

fn diagram_has_class(shapes: &[ShapeNode], class_name: &str) -> bool {
    shapes.iter().any(|shape| {
        shape
            .attrs
            .get("class")
            .is_some_and(|classes| classes.split_whitespace().any(|class| class == class_name))
            || diagram_has_class(&shape.children, class_name)
    })
}

fn class_attrs_to_css_decls(attrs: &IndexMap<String, String>, include_z: bool) -> String {
    let mut decls = Vec::new();
    for (key, value) in attrs {
        if key == "z_index" && !include_z {
            continue;
        }
        if let Some((css_key, css_value)) = class_attr_to_css_decl(key, value) {
            decls.push(format!("{css_key}: {css_value};"));
        }
    }
    decls.join(" ")
}

fn class_attr_to_css_decl(key: &str, value: &str) -> Option<(String, String)> {
    match key {
        "z_index" => None,
        "visible" => Some((
            "visibility".to_string(),
            if value == "false" {
                "hidden"
            } else {
                "visible"
            }
            .to_string(),
        )),
        "pointer_events" => Some(("pointer-events".to_string(), css_declaration_value(value))),
        "stroke_width" => Some(("stroke-width".to_string(), css_declaration_value(value))),
        "stroke_dasharray" => Some(("stroke-dasharray".to_string(), css_declaration_value(value))),
        "font_family" => Some(("font-family".to_string(), css_declaration_value(value))),
        "font_weight" => Some(("font-weight".to_string(), css_declaration_value(value))),
        "font_style" => Some(("font-style".to_string(), css_declaration_value(value))),
        "text_decoration" => Some(("text-decoration".to_string(), css_declaration_value(value))),
        key if is_supported_class_property(key) => {
            Some((key.replace('_', "-"), css_declaration_value(value)))
        }
        _ => None,
    }
}

fn class_attr_to_svg_attr_value(key: &str, value: &str) -> (String, String) {
    match key {
        "visible" => (
            "visibility".to_string(),
            if value == "false" {
                "hidden"
            } else {
                "visible"
            }
            .to_string(),
        ),
        "pointer_events" => ("pointer_events".to_string(), value.to_string()),
        _ => (key.to_string(), value.to_string()),
    }
}

fn is_supported_class_property(key: &str) -> bool {
    matches!(
        key,
        "fill"
            | "stroke"
            | "stroke_width"
            | "stroke_dasharray"
            | "rx"
            | "ry"
            | "opacity"
            | "visible"
            | "visibility"
            | "display"
            | "pointer_events"
            | "cursor"
            | "transition"
            | "font_family"
            | "font_weight"
            | "font_style"
            | "text_decoration"
            | "letter_spacing"
            | "background_fill"
            | "foreground_fill"
            | "hover_background_fill"
            | "hover_foreground_fill"
            | "chrome_fill"
            | "chrome_foreground_fill"
            | "chrome_border_fill"
            | "chrome_height"
            | "accent_fill"
            | "muted_fill"
            | "label_fill"
            | "placeholder_fill"
            | "selected_background_fill"
            | "selected_foreground_fill"
            | "cell_width"
            | "cell_height"
    )
}

fn css_declaration_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, ';' | '{' | '}' | '<' | '>'))
        .collect()
}

fn css_ident_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    if out.is_empty() {
        "unnamed".to_string()
    } else {
        out
    }
}

fn state_class_name(state: &str) -> String {
    format!("wdoc-state-{}", css_ident_escape(state))
}

fn diagram_has_events(diagram: &Diagram) -> bool {
    diagram.shapes.iter().any(shape_needs_runtime)
}

fn diagram_pan_zoom_enabled(diagram: &Diagram) -> bool {
    diagram
        .options
        .get("mode")
        .map(|mode| matches!(mode.trim(), "pan_zoom" | "pan-zoom"))
        .unwrap_or(false)
}

fn shape_needs_runtime(shape: &ShapeNode) -> bool {
    !shape.events.is_empty()
        || shape.kind == ShapeKind::Map
        || shape.attrs.contains_key("_wdoc_state_z")
        || shape.attrs.contains_key("_wdoc_state_animation")
        || shape.attrs.get("_wdoc_runtime").map(|v| v == "true") == Some(true)
        || shape.attrs.contains_key("data_wdoc_slider")
        || shape.attrs.contains_key("data-wdoc-slider")
        || shape
            .attrs
            .get("draggable")
            .or_else(|| shape.attrs.get("data-wdoc-draggable"))
            .is_some_and(|value| value == "true")
        || shape
            .attrs
            .get("data_wdoc_textbox")
            .or_else(|| shape.attrs.get("data-wdoc-textbox"))
            .is_some_and(|value| value == "true")
        || shape
            .attrs
            .get("data_wdoc_checkbox")
            .or_else(|| shape.attrs.get("data-wdoc-checkbox"))
            .is_some_and(|value| value == "true")
        || shape
            .attrs
            .get("data_wdoc_radio")
            .or_else(|| shape.attrs.get("data-wdoc-radio"))
            .is_some_and(|value| value == "true")
        || shape
            .attrs
            .get("data_wdoc_dropdown")
            .or_else(|| shape.attrs.get("data-wdoc-dropdown"))
            .is_some_and(|value| value == "true")
        || shape.attrs.contains_key("transparent_color")
        || shape.children.iter().any(shape_needs_runtime)
}

fn mark_runtime_shapes(shapes: &mut [ShapeNode]) {
    for shape in shapes {
        shape
            .attrs
            .insert("_wdoc_runtime".to_string(), "true".to_string());
        mark_runtime_shapes(&mut shape.children);
    }
}

#[derive(Clone, Copy)]
enum RenderItemRef<'a> {
    Shape(&'a ShapeNode),
    Connection(&'a Connection),
}

impl RenderItemRef<'_> {
    fn z_index(&self) -> f64 {
        match self {
            RenderItemRef::Shape(shape) => shape.z_index,
            RenderItemRef::Connection(conn) => conn.z_index,
        }
    }

    fn source_order(&self) -> usize {
        match self {
            RenderItemRef::Shape(shape) => shape.source_order,
            RenderItemRef::Connection(conn) => conn.source_order,
        }
    }
}

fn render_diagram_items_svg(
    shapes: &[ShapeNode],
    connections: &[Connection],
    shape_map: &HashMap<String, Bounds>,
    svg: &mut String,
) {
    let routed_connections = routed_connections(connections, shape_map);
    let mut items: Vec<RenderItemRef<'_>> = Vec::with_capacity(shapes.len() + connections.len());
    items.extend(shapes.iter().map(RenderItemRef::Shape));
    items.extend(connections.iter().map(RenderItemRef::Connection));
    sort_render_items(&mut items);
    for item in items {
        match item {
            RenderItemRef::Shape(shape) => render_shape_svg(shape, svg),
            RenderItemRef::Connection(conn) => {
                if let Some(route) = routed_connections.get(&connection_render_key(conn)) {
                    render_routed_connection_svg(route, svg);
                } else {
                    render_connection_svg(conn, shape_map, svg);
                }
            }
        }
    }
}

fn render_child_shapes_svg(children: &[ShapeNode], svg: &mut String) {
    let mut items: Vec<_> = children.iter().enumerate().collect();
    items.sort_by(|(a_idx, a), (b_idx, b)| {
        a.z_index
            .total_cmp(&b.z_index)
            .then_with(|| a.source_order.cmp(&b.source_order))
            .then_with(|| a_idx.cmp(b_idx))
    });
    for (_, child) in items {
        render_shape_svg(child, svg);
    }
}

fn sort_render_items(items: &mut [RenderItemRef<'_>]) {
    items.sort_by(|a, b| {
        a.z_index()
            .total_cmp(&b.z_index())
            .then_with(|| a.source_order().cmp(&b.source_order()))
    });
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
        "wdoc::draw::text_block" => Some(ShapeKind::TextBlock),
        "wdoc::draw::inline_svg" => Some(ShapeKind::InlineSvg),
        "wdoc::draw::icon" => Some(ShapeKind::Icon),
        "wdoc::draw::image" => Some(ShapeKind::Image),
        "wdoc::draw::map" => Some(ShapeKind::Map),
        "wdoc::draw::sprite" => Some(ShapeKind::Sprite),
        "wdoc::draw::dopesheet_view" => Some(ShapeKind::DopesheetView),
        "wdoc::draw::tilemap" => Some(ShapeKind::Tilemap),
        "wdoc::draw::game_layer" => Some(ShapeKind::GameLayer),
        "wdoc::draw::group" => Some(ShapeKind::Group),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Layout resolution
// ---------------------------------------------------------------------------

fn resolve_children(
    children: &mut [ShapeNode],
    connections: &mut [Connection],
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
        apply_intrinsic_container_size(child, connections);
    }

    prelayout_nested_graph_containers(children, connections, parent, scope_path);

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
        Alignment::Grid if !unpositioned.is_empty() => {
            layout_graph_subset(
                children,
                connections,
                GraphSubsetLayout {
                    indices: &unpositioned,
                    parent,
                    scope_path,
                    align,
                    gap,
                    options,
                },
            );
        }
        Alignment::Layered | Alignment::Force | Alignment::Radial if !layoutable.is_empty() => {
            layout_graph_subset(
                children,
                connections,
                GraphSubsetLayout {
                    indices: &layoutable,
                    parent,
                    scope_path,
                    align,
                    gap,
                    options,
                },
            );
        }
        _ => {}
    }

    // Recurse into children
    for child in children.iter_mut() {
        resolve_nested_container(child, connections, scope_path);
    }

    if scope_path.is_empty()
        && align == Alignment::Layered
        && option_enabled(options, "_wdoc_scale_to_fit")
    {
        crate::graph_layout::scale_children_to_parent(children, parent);
        crate::graph_layout::fit_children_to_parent(children, parent);
    }
}

fn prelayout_nested_graph_containers(
    children: &mut [ShapeNode],
    connections: &mut [Connection],
    parent: &Bounds,
    scope_path: &str,
) {
    for child in children.iter_mut() {
        if is_layout_decoration(child)
            || child.children.is_empty()
            || !is_graph_alignment(child.align)
        {
            continue;
        }

        if child.resolved.width == 0.0 || child.resolved.height == 0.0 {
            let bounds_parent = if is_layout_decoration(child) {
                decoration_parent_bounds(parent)
            } else {
                *parent
            };
            resolve_bounds(child, &bounds_parent);
            apply_intrinsic_container_size(child, connections);
        }

        resolve_nested_container(child, connections, scope_path);
        if is_graph_alignment(child.align) {
            child
                .attrs
                .insert(SIZE_LOCKED_ATTR.to_string(), "true".to_string());
        }
    }
}

fn resolve_nested_container(
    child: &mut ShapeNode,
    connections: &mut [Connection],
    scope_path: &str,
) {
    if child
        .attrs
        .get(SIZE_LOCKED_ATTR)
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        return;
    }
    let insets = child_content_insets(child);
    let mut inner = if child.kind == ShapeKind::Map {
        Bounds {
            x: 0.0,
            y: 0.0,
            width: map_content_width(child),
            height: map_content_height(child),
        }
    } else {
        Bounds {
            x: insets.left,
            y: insets.top,
            width: (child.resolved.width - insets.left - insets.right).max(0.0),
            height: (child.resolved.height - insets.top - insets.bottom).max(0.0),
        }
    };
    let child_scope_path = scoped_child_path(scope_path, child.id.as_deref());
    let mut options = child.attrs.clone();
    if has_explicit_width(child) && has_explicit_height(child) {
        options.insert("_wdoc_scale_to_fit".to_string(), "true".to_string());
    }
    resolve_children(
        &mut child.children,
        connections,
        &inner,
        &child_scope_path,
        child.align,
        child.gap,
        &options,
    );
    apply_post_layout_container_size(child);
    if is_graph_alignment(child.align) {
        expand_container_to_fit_layout_children(child);
        let insets = child_content_insets(child);
        if child.kind == ShapeKind::Map {
            inner.x = 0.0;
            inner.y = 0.0;
            inner.width = map_content_width(child);
            inner.height = map_content_height(child);
        } else {
            inner.x = insets.left;
            inner.y = insets.top;
            inner.width = (child.resolved.width - insets.left - insets.right).max(0.0);
            inner.height = (child.resolved.height - insets.top - insets.bottom).max(0.0);
        }
    }
    if has_explicit_width(child) && has_explicit_height(child) {
        clamp_children_to_parent(&mut child.children, &inner);
    }
}

fn scoped_child_path(scope_path: &str, child_id: Option<&str>) -> String {
    match (scope_path.is_empty(), child_id) {
        (_, None) => scope_path.to_string(),
        (true, Some(id)) => id.to_string(),
        (false, Some(id)) => format!("{scope_path}.{id}"),
    }
}

fn is_graph_alignment(align: Alignment) -> bool {
    matches!(
        align,
        Alignment::Grid | Alignment::Layered | Alignment::Force | Alignment::Radial
    )
}

fn option_enabled(options: &IndexMap<String, String>, key: &str) -> bool {
    options.get(key).map(|s| s == "true").unwrap_or(false)
}

fn bool_attr(attrs: &IndexMap<String, String>, key: &str) -> bool {
    attrs
        .get(key)
        .map(|value| matches!(value.as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
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

    insets.left += attr_f64(&node.attrs, CONTENT_INSET_LEFT_ATTR).unwrap_or(0.0);
    insets.top += attr_f64(&node.attrs, CONTENT_INSET_TOP_ATTR).unwrap_or(0.0);
    insets.right += attr_f64(&node.attrs, CONTENT_INSET_RIGHT_ATTR).unwrap_or(0.0);
    insets.bottom += attr_f64(&node.attrs, CONTENT_INSET_BOTTOM_ATTR).unwrap_or(0.0);

    if node.padding == 0.0
        && node.align != Alignment::None
        && !has_explicit_content_insets(node)
        && node.children.iter().any(is_layout_decoration)
    {
        insets.left = 16.0;
        insets.right = 16.0;
        insets.bottom = 16.0;
        insets.top = decoration_header_inset(node).unwrap_or(16.0);
    }

    insets
}

fn has_explicit_content_insets(node: &ShapeNode) -> bool {
    node.attrs.contains_key(CONTENT_INSET_LEFT_ATTR)
        || node.attrs.contains_key(CONTENT_INSET_TOP_ATTR)
        || node.attrs.contains_key(CONTENT_INSET_RIGHT_ATTR)
        || node.attrs.contains_key(CONTENT_INSET_BOTTOM_ATTR)
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
    if child
        .attrs
        .get(FULL_CONTAINER_DECORATION_ATTR)
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        return true;
    }

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

struct GraphSubsetLayout<'a> {
    indices: &'a [usize],
    parent: &'a Bounds,
    scope_path: &'a str,
    align: Alignment,
    gap: f64,
    options: &'a IndexMap<String, String>,
}

fn layout_graph_subset(
    children: &mut [ShapeNode],
    connections: &mut [Connection],
    layout: GraphSubsetLayout<'_>,
) {
    let GraphSubsetLayout {
        indices,
        parent,
        scope_path,
        align,
        gap,
        options,
    } = layout;
    let mut layout_children: Vec<ShapeNode> =
        indices.iter().map(|&i| children[i].clone()).collect();
    let local_connections = localize_connections(&layout_children, connections, scope_path);
    if matches!(align, Alignment::Force | Alignment::Radial) {
        mark_direct_connections_for_scope(&layout_children, connections, scope_path);
    }

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

    // Copy back the laid-out clones in full so any scaling/repositioning of
    // grandchildren (done by layout_layered when `scale_to_fit` is enabled)
    // propagates to the original tree.
    for (layout_child, &original_idx) in layout_children.into_iter().zip(indices) {
        children[original_idx] = layout_child;
    }
}

fn mark_direct_connections_for_scope(
    children: &[ShapeNode],
    connections: &mut [Connection],
    scope_path: &str,
) {
    for conn in connections {
        if localize_endpoint(&conn.from_id, children, scope_path).is_some()
            && localize_endpoint(&conn.to_id, children, scope_path).is_some()
        {
            conn.attrs.insert(
                CONNECTION_ROUTE_ATTR.to_string(),
                CONNECTION_ROUTE_DIRECT.to_string(),
            );
        }
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
            if from_id == to_id {
                return None;
            }
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

    if let Some((owner, _)) = endpoint.split_once('.') {
        return children
            .iter()
            .filter_map(|child| child.id.as_deref())
            .find(|id| *id == owner)
            .map(str::to_string);
    }

    children
        .iter()
        .filter_map(|child| child.id.as_deref())
        .find(|id| *id == endpoint)
        .map(str::to_string)
}

fn apply_intrinsic_container_size(node: &mut ShapeNode, connections: &[Connection]) {
    if node.kind_name == "wdoc::draw::terminal" {
        let (width, height) = crate::terminal::intrinsic_size(&node.attrs);
        if !has_explicit_width(node) && node.resolved.width == 0.0 {
            node.resolved.width = width;
        }
        if !has_explicit_height(node) && node.resolved.height == 0.0 {
            node.resolved.height = height;
        }
    }

    if node.children.is_empty() {
        return;
    }

    let needs_width = !has_explicit_width(node) && node.resolved.width == 0.0;
    let needs_height = !has_explicit_height(node) && node.resolved.height == 0.0;
    if !needs_width && !needs_height {
        return;
    }

    if is_graph_alignment(node.align) {
        if let Some((w, h)) = intrinsic_graph_container_size(node, connections) {
            if needs_width {
                node.resolved.width = w;
            }
            if needs_height {
                node.resolved.height = h;
            }
            return;
        }
    }

    if let Some(bounds) = input_children_bounds(&node.children, connections) {
        if needs_width {
            node.resolved.width = (bounds.x + bounds.width + node.padding * 2.0).max(0.0);
        }
        if needs_height {
            node.resolved.height = (bounds.y + bounds.height + node.padding * 2.0).max(0.0);
        }
    }
}

/// Compute the natural size of a graph-aligned container by simulating its
/// inner graph layout against an unbounded parent and unioning the resulting
/// child bounds. Returns `(width, height)` including the container's content
/// insets, or `None` if children produce no measurable bounds.
fn intrinsic_graph_container_size(
    node: &ShapeNode,
    connections: &[Connection],
) -> Option<(f64, f64)> {
    if node.children.is_empty() || !is_graph_alignment(node.align) {
        return None;
    }

    // Strip this node's id from incoming dotted ids so child-level connections
    // are addressed by their direct child paths (e.g. `outer.inner.a` → `inner.a`).
    let scope = node.id.as_deref().unwrap_or("");
    let stripped: Vec<Connection> = connections
        .iter()
        .filter_map(|conn| {
            let strip = |s: &str| -> Option<String> {
                if scope.is_empty() {
                    Some(s.to_string())
                } else {
                    s.strip_prefix(scope)
                        .and_then(|r| r.strip_prefix('.'))
                        .map(str::to_string)
                }
            };
            let from = strip(&conn.from_id)?;
            let to = strip(&conn.to_id)?;
            let mut c = conn.clone();
            c.from_id = from;
            c.to_id = to;
            Some(c)
        })
        .collect();

    let mut clones: Vec<ShapeNode> = node
        .children
        .iter()
        .filter(|c| !is_layout_decoration(c))
        .cloned()
        .collect();
    if clones.is_empty() {
        return None;
    }
    let zero_parent = Bounds {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };
    for child in clones.iter_mut() {
        resolve_bounds(child, &zero_parent);
        apply_intrinsic_container_size(child, &stripped);
    }

    let unbounded = Bounds {
        x: 0.0,
        y: 0.0,
        width: 1.0e6,
        height: 1.0e6,
    };
    let local_connections = localize_connections(&clones, &stripped, "");
    let mut options = node.attrs.clone();
    options.shift_remove("_wdoc_scale_to_fit");

    match node.align {
        Alignment::Layered => crate::graph_layout::layout_layered(
            &mut clones,
            &local_connections,
            &unbounded,
            node.gap,
            &options,
        ),
        Alignment::Force => crate::graph_layout::layout_force(
            &mut clones,
            &local_connections,
            &unbounded,
            node.gap,
            &options,
        ),
        Alignment::Radial => crate::graph_layout::layout_radial(
            &mut clones,
            &local_connections,
            &unbounded,
            node.gap,
            &options,
        ),
        Alignment::Grid => crate::graph_layout::layout_grid(
            &mut clones,
            &local_connections,
            &unbounded,
            node.gap,
            &options,
        ),
        _ => return None,
    }

    let bounds = children_bounds(&clones)?;
    let insets = child_content_insets(node);
    let width = bounds.width + insets.left + insets.right;
    let height = bounds.height + insets.top + insets.bottom;
    Some((width.max(0.0), height.max(0.0)))
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
        let old_width = node.resolved.width;
        let old_height = node.resolved.height;
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
        resize_full_container_decorations(&mut node.children, old_width, old_height, node.resolved);
    }
}

fn expand_container_to_fit_layout_children(node: &mut ShapeNode) {
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

    resize_full_container_decorations(&mut node.children, old_width, old_height, node.resolved);
}

fn input_children_bounds(children: &[ShapeNode], connections: &[Connection]) -> Option<Bounds> {
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
        apply_intrinsic_container_size(&mut child, connections);
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
        let marked_full_container = child
            .attrs
            .get(FULL_CONTAINER_DECORATION_ATTR)
            .map(|v| v == "true")
            .unwrap_or(false);
        let was_full_container = child.resolved.x == 0.0
            && child.resolved.y == 0.0
            && child.resolved.width == old_width
            && child.resolved.height == old_height;
        if marked_full_container || was_full_container {
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
    let explicit_width = has_explicit_width(node);
    let explicit_height = has_explicit_height(node);
    let has_position = node.x.is_some()
        || node.y.is_some()
        || node.top.is_some()
        || node.bottom.is_some()
        || node.left.is_some()
        || node.right.is_some();

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
    if node.kind == ShapeKind::TextBlock
        && node.y.is_none()
        && node.top.is_some()
        && node.bottom.is_none()
    {
        ry = parent.y + node.top.unwrap_or(0.0);
    }

    // Legacy text labels with no positioning or size fill their parent.
    // Positioned text can omit width/height and use its natural measured size.
    if node.kind == ShapeKind::Text && !has_position && !explicit_width && !explicit_height {
        rx = parent.x;
        ry = parent.y;
        rw = parent.width;
        rh = parent.height;
    } else if node.kind == ShapeKind::Text && (!explicit_width || !explicit_height) {
        let mut measure_attrs = node.attrs.clone();
        if explicit_width && rw > 0.0 {
            measure_attrs.insert("width".to_string(), rw.to_string());
        }
        let metrics = measure_text_attrs(&measure_attrs);
        if !explicit_width {
            rw = metrics.width;
        }
        if !explicit_height {
            rh = metrics.height;
        }
    } else if node.kind == ShapeKind::GameLayer {
        if !explicit_width {
            rw = parent.width;
        }
        if !explicit_height {
            rh = parent.height;
        }
    } else if node.kind == ShapeKind::TextBlock && (!explicit_width || !explicit_height) {
        let width = if explicit_width && rw > 0.0 {
            rw
        } else {
            attr_f64(&node.attrs, "max_width")
                .filter(|w| *w > 0.0)
                .unwrap_or_else(|| {
                    let available = parent.width - (rx - parent.x);
                    if available > 0.0 {
                        available
                    } else {
                        240.0
                    }
                })
        };
        if !explicit_width || rw <= 0.0 {
            rw = width;
        }
        if !explicit_height {
            rh = measure_text_block_height(&node.attrs, &node.text_block_items, rw);
        }
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

#[derive(Debug, Clone, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    code: bool,
    href: Option<String>,
}

#[derive(Debug, Clone)]
struct InlineFragment {
    text: String,
    style: InlineStyle,
}

fn measure_text_block_height(
    attrs: &IndexMap<String, String>,
    items: &[TextBlockItem],
    width: f64,
) -> f64 {
    let padding = attr_f64(attrs, "padding").unwrap_or(0.0);
    let gap = attr_f64(attrs, "gap").unwrap_or(8.0);
    let mut height = padding * 2.0;
    let mut seen = false;
    for item in items {
        if seen {
            height += gap;
        }
        seen = true;
        height += match item {
            TextBlockItem::Paragraph { html } => {
                let font_size = attr_f64(attrs, "font_size").unwrap_or(12.0);
                let line_height = attr_f64(attrs, "line_height").unwrap_or(1.35);
                let lines = wrap_inline_fragments(
                    parse_inline_html_fragments(html),
                    (width - padding * 2.0).max(1.0),
                    font_size,
                    attrs,
                );
                (lines.len().max(1) as f64) * font_size * line_height
            }
            TextBlockItem::Code { content, .. } => {
                let code_padding = attr_f64(attrs, "code_padding").unwrap_or(8.0);
                let font_size = attr_f64(attrs, "code_font_size")
                    .unwrap_or_else(|| attr_f64(attrs, "font_size").unwrap_or(12.0) * 0.92);
                let line_height = attr_f64(attrs, "code_line_height").unwrap_or(1.25);
                let mut measure_attrs = attrs.clone();
                measure_attrs.insert("font_size".to_string(), font_size.to_string());
                measure_attrs.insert(
                    "width".to_string(),
                    (width - padding * 2.0 - code_padding * 2.0)
                        .max(1.0)
                        .to_string(),
                );
                let lines = wrapped_text_lines(
                    content,
                    Some((width - padding * 2.0 - code_padding * 2.0).max(1.0)),
                    font_size,
                    0.0,
                    &measure_attrs,
                );
                (lines.len().max(1) as f64) * font_size * line_height + code_padding * 2.0
            }
        };
    }
    height
}

fn parse_inline_html_fragments(html: &str) -> Vec<InlineFragment> {
    let mut fragments = Vec::new();
    let mut style = InlineStyle::default();
    let mut index = 0;
    while index < html.len() {
        let rest = &html[index..];
        if let Some(tag_start) = rest.find('<') {
            let text = &rest[..tag_start];
            push_inline_text(&mut fragments, text, &style);
            index += tag_start;
            let tag_rest = &html[index..];
            let Some(tag_end) = tag_rest.find('>') else {
                push_inline_text(&mut fragments, tag_rest, &style);
                break;
            };
            apply_inline_tag(&tag_rest[1..tag_end], &mut style);
            index += tag_end + 1;
        } else {
            push_inline_text(&mut fragments, rest, &style);
            break;
        }
    }
    fragments
}

fn push_inline_text(fragments: &mut Vec<InlineFragment>, text: &str, style: &InlineStyle) {
    let decoded = decode_basic_entities(text);
    if !decoded.is_empty() {
        fragments.push(InlineFragment {
            text: decoded,
            style: style.clone(),
        });
    }
}

fn apply_inline_tag(tag: &str, style: &mut InlineStyle) {
    let lower = tag.trim().to_ascii_lowercase();
    if lower.starts_with("strong") || lower == "b" {
        style.bold = true;
    } else if lower.starts_with("/strong") || lower == "/b" {
        style.bold = false;
    } else if lower == "em" || lower == "i" || lower.starts_with("em ") || lower.starts_with("i ") {
        style.italic = true;
    } else if lower == "/em" || lower == "/i" {
        style.italic = false;
    } else if lower.starts_with("code") {
        style.code = true;
    } else if lower.starts_with("/code") {
        style.code = false;
    } else if lower.starts_with("a ") || lower == "a" {
        style.href = extract_href(tag);
    } else if lower.starts_with("/a") {
        style.href = None;
    }
}

fn extract_href(tag: &str) -> Option<String> {
    let href_pos = tag.find("href=")?;
    let value = &tag[href_pos + 5..].trim_start();
    let quote = value.chars().next()?;
    if quote == '"' || quote == '\'' {
        let rest = &value[1..];
        let end = rest.find(quote)?;
        Some(decode_basic_entities(&rest[..end]))
    } else {
        Some(
            value
                .split_whitespace()
                .next()
                .map(decode_basic_entities)
                .unwrap_or_default(),
        )
    }
}

fn decode_basic_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn wrap_inline_fragments(
    fragments: Vec<InlineFragment>,
    max_width: f64,
    font_size: f64,
    attrs: &IndexMap<String, String>,
) -> Vec<Vec<InlineFragment>> {
    let mut lines: Vec<Vec<InlineFragment>> = vec![Vec::new()];
    let mut line_width = 0.0;
    let space_width = measure_inline_text(" ", &InlineStyle::default(), font_size, attrs);

    for fragment in fragments {
        for (segment_index, segment) in fragment.text.split('\n').enumerate() {
            if segment_index > 0 {
                lines.push(Vec::new());
                line_width = 0.0;
            }
            for word in segment.split_whitespace() {
                let word_width = measure_inline_text(word, &fragment.style, font_size, attrs);
                let needs_space = !lines.last().is_none_or(Vec::is_empty);
                let attaches_to_previous = word
                    .chars()
                    .next()
                    .is_some_and(|ch| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?' | ')'));
                let add_space = needs_space && !attaches_to_previous;
                let candidate = line_width + if add_space { space_width } else { 0.0 } + word_width;
                if needs_space && candidate > max_width {
                    lines.push(Vec::new());
                    line_width = 0.0;
                } else if add_space {
                    push_line_fragment(lines.last_mut().unwrap(), " ", &fragment.style);
                    line_width += space_width;
                }
                push_line_fragment(lines.last_mut().unwrap(), word, &fragment.style);
                line_width += word_width;
            }
        }
    }

    if lines.is_empty() {
        vec![Vec::new()]
    } else {
        lines
    }
}

fn push_line_fragment(line: &mut Vec<InlineFragment>, text: &str, style: &InlineStyle) {
    if let Some(last) = line.last_mut() {
        if last.style.bold == style.bold
            && last.style.italic == style.italic
            && last.style.code == style.code
            && last.style.href == style.href
        {
            last.text.push_str(text);
            return;
        }
    }
    line.push(InlineFragment {
        text: text.to_string(),
        style: style.clone(),
    });
}

fn measure_inline_text(
    text: &str,
    style: &InlineStyle,
    font_size: f64,
    attrs: &IndexMap<String, String>,
) -> f64 {
    let letter_spacing = attrs
        .get("letter_spacing")
        .map(|s| parse_css_length(s, font_size))
        .unwrap_or(0.0);
    let mut width = text
        .chars()
        .map(|ch| char_advance_factor(ch) * font_size + letter_spacing)
        .sum::<f64>();
    if style.bold {
        width *= 1.05;
    }
    if style.code {
        width *= 0.96;
    }
    width
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
    let style = svg_node_attrs(node, &node.attrs);

    if node
        .attrs
        .get("_wdoc_composite")
        .is_some_and(|value| value == "true")
        && !node.children.is_empty()
    {
        if let Some(url) = node.attrs.get("href") {
            let url = svg_escape_attr(url);
            write!(svg, "<a href=\"{url}\" target=\"_top\">").unwrap();
        }
        let gx = b.x;
        let gy = b.y;
        write!(svg, "<g transform=\"translate({gx},{gy})\"{style}>").unwrap();
        render_child_shapes_svg(&node.children, svg);
        svg.push_str("</g>");
        if node.attrs.contains_key("href") {
            svg.push_str("</a>");
        }
        return;
    }

    // Wrap in <a> if shape has an href attribute (clickable)
    let href = node.attrs.get("href");
    if let Some(url) = href {
        let url = svg_escape_attr(url);
        write!(svg, "<a href=\"{url}\" target=\"_top\">").unwrap();
    }

    let mut rendered_children = false;
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
            let font_size_for_layout = parse_svg_number(font_size).unwrap_or(14.0);
            let line_height = node
                .attrs
                .get("line_height")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(1.2);
            let letter_spacing = node
                .attrs
                .get("letter_spacing")
                .map(|s| parse_css_length(s, font_size_for_layout))
                .unwrap_or(0.0);
            let wrap_width = node
                .attrs
                .get("max_width")
                .or_else(|| node.attrs.get("width"))
                .and_then(|s| parse_svg_number(s))
                .filter(|w| *w > 0.0);
            let line_step = font_size_for_layout * line_height;
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
            let font_size_attr = svg_escape_attr(font_size);
            let anchor_attr = svg_escape_attr(anchor);
            write!(
                svg,
                "<text x=\"{tx}\" y=\"{ty}\" font-size=\"{font_size_attr}\" \
                 text-anchor=\"{anchor_attr}\" dominant-baseline=\"central\"\
                {fill_default}{style}>"
            )
            .unwrap();
            let lines = wrapped_text_lines(
                content,
                wrap_width,
                font_size_for_layout,
                letter_spacing,
                &node.attrs,
            );
            if lines.len() <= 1 {
                svg.push_str(&svg_escape_text(
                    lines.first().map(String::as_str).unwrap_or(""),
                ));
            } else {
                let first_y = ty - ((lines.len() - 1) as f64 * line_step / 2.0);
                for (idx, line) in lines.iter().enumerate() {
                    let escaped = svg_escape_text(line);
                    if idx == 0 {
                        write!(svg, "<tspan x=\"{tx}\" y=\"{first_y}\">{escaped}</tspan>").unwrap();
                    } else {
                        write!(
                            svg,
                            "<tspan x=\"{tx}\" dy=\"{line_step}\">{escaped}</tspan>"
                        )
                        .unwrap();
                    }
                }
            }
            svg.push_str("</text>");
        }
        ShapeKind::TextBlock => render_text_block_svg(node, svg),
        ShapeKind::InlineSvg => render_inline_svg_shape_svg(node, svg),
        ShapeKind::Icon => render_icon_shape_svg(node, svg),
        ShapeKind::Image => render_image_shape_svg(node, svg),
        ShapeKind::Map => {
            render_map_shape_svg(node, svg);
            rendered_children = true;
        }
        ShapeKind::Sprite => render_sprite_shape_svg(node, svg),
        ShapeKind::DopesheetView => render_dopesheet_view_shape_svg(node, svg),
        ShapeKind::Tilemap => render_tilemap_shape_svg(node, svg),
        ShapeKind::GameLayer => {
            render_game_layer_shape_svg(node, svg);
            rendered_children = true;
        }
        ShapeKind::Group => {
            let gx = b.x;
            let gy = b.y;
            write!(svg, "<g transform=\"translate({gx},{gy})\"{style}>").unwrap();
            render_child_shapes_svg(&node.children, svg);
            svg.push_str("</g>");
            rendered_children = true;
        }
        ShapeKind::Custom => {
            if node.kind_name == "wdoc::draw::terminal" {
                crate::terminal::render_terminal_svg(node, svg);
                rendered_children = true;
            } else if !node.children.is_empty() {
                let gx = b.x;
                let gy = b.y;
                write!(svg, "<g transform=\"translate({gx},{gy})\"{style}>").unwrap();
                render_child_shapes_svg(&node.children, svg);
                svg.push_str("</g>");
                rendered_children = true;
            }
        }
    }

    // Render children in a translated group
    if !rendered_children && !node.children.is_empty() {
        let gx = b.x;
        let gy = b.y;
        write!(svg, "<g transform=\"translate({gx},{gy})\">").unwrap();
        render_child_shapes_svg(&node.children, svg);
        svg.push_str("</g>");
    }

    // Close <a> wrapper if shape was clickable
    if href.is_some() {
        svg.push_str("</a>");
    }
}

fn render_text_block_svg(node: &ShapeNode, svg: &mut String) {
    let b = node.resolved;
    let padding = attr_f64(&node.attrs, "padding").unwrap_or(0.0);
    let gap = attr_f64(&node.attrs, "gap").unwrap_or(8.0);
    let font_size = attr_f64(&node.attrs, "font_size").unwrap_or(12.0);
    let line_height = attr_f64(&node.attrs, "line_height").unwrap_or(1.35);
    let line_step = font_size * line_height;
    let fill = node
        .attrs
        .get("fill")
        .map(String::as_str)
        .unwrap_or("currentColor");
    let mut y = b.y + padding;
    let inner_width = (b.width - padding * 2.0).max(1.0);

    for (index, item) in node.text_block_items.iter().enumerate() {
        if index > 0 {
            y += gap;
        }
        match item {
            TextBlockItem::Paragraph { html } => {
                let lines = wrap_inline_fragments(
                    parse_inline_html_fragments(html),
                    inner_width,
                    font_size,
                    &node.attrs,
                );
                render_inline_lines_svg(
                    svg,
                    &lines,
                    InlineLineLayout {
                        x: b.x + padding,
                        y,
                        font_size,
                        line_step,
                        fill,
                    },
                    node,
                );
                y += (lines.len().max(1) as f64) * line_step;
            }
            TextBlockItem::Code { content, language } => {
                y += render_code_panel_svg(
                    svg,
                    node,
                    content,
                    language.as_deref(),
                    b.x + padding,
                    y,
                    inner_width,
                );
            }
        }
    }
}

fn render_inline_lines_svg(
    svg: &mut String,
    lines: &[Vec<InlineFragment>],
    layout: InlineLineLayout<'_>,
    node: &ShapeNode,
) {
    let InlineLineLayout {
        x,
        y,
        font_size,
        line_step,
        fill,
    } = layout;
    let baseline = font_size * 0.8 + ((line_step - font_size).max(0.0) / 2.0);
    let font_family = node
        .attrs
        .get("font_family")
        .map(String::as_str)
        .unwrap_or("Inter, system-ui, sans-serif");
    write!(
        svg,
        "<text x=\"{x}\" y=\"{}\" font-size=\"{}\" font-family=\"{}\" text-anchor=\"start\" dominant-baseline=\"auto\" fill=\"{}\">",
        y + baseline,
        font_size,
        svg_escape_attr(font_family),
        svg_escape_attr(fill)
    )
    .unwrap();
    for (line_index, line) in lines.iter().enumerate() {
        let line_y = y + baseline + (line_index as f64 * line_step);
        write!(svg, "<tspan x=\"{x}\" y=\"{line_y}\">").unwrap();
        if line.is_empty() {
            svg.push(' ');
        } else {
            for fragment in line {
                render_inline_fragment_svg(svg, fragment, node);
            }
        }
        svg.push_str("</tspan>");
    }
    svg.push_str("</text>");
}

struct InlineLineLayout<'a> {
    x: f64,
    y: f64,
    font_size: f64,
    line_step: f64,
    fill: &'a str,
}

fn render_inline_fragment_svg(svg: &mut String, fragment: &InlineFragment, node: &ShapeNode) {
    let mut attrs = String::new();
    if fragment.style.bold {
        attrs.push_str(" font-weight=\"700\"");
    }
    if fragment.style.italic {
        attrs.push_str(" font-style=\"italic\"");
    }
    if fragment.style.code {
        let family = node
            .attrs
            .get("code_font_family")
            .map(String::as_str)
            .unwrap_or("JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, monospace");
        let fill = node
            .attrs
            .get("inline_code_fill")
            .map(String::as_str)
            .unwrap_or("var(--color-link)");
        write!(
            attrs,
            " font-family=\"{}\" fill=\"{}\"",
            svg_escape_attr(family),
            svg_escape_attr(fill)
        )
        .unwrap();
    }
    if fragment.style.href.is_some() {
        let fill = node
            .attrs
            .get("link_fill")
            .map(String::as_str)
            .unwrap_or("var(--color-link)");
        write!(
            attrs,
            " fill=\"{}\" text-decoration=\"underline\"",
            svg_escape_attr(fill)
        )
        .unwrap();
    }
    let text = svg_escape_text(&fragment.text);
    if let Some(href) = &fragment.style.href {
        write!(
            svg,
            "<a href=\"{}\" target=\"_top\"><tspan{attrs}>{text}</tspan></a>",
            svg_escape_attr(href)
        )
        .unwrap();
    } else {
        write!(svg, "<tspan{attrs}>{text}</tspan>").unwrap();
    }
}

fn render_code_panel_svg(
    svg: &mut String,
    node: &ShapeNode,
    content: &str,
    language: Option<&str>,
    x: f64,
    y: f64,
    width: f64,
) -> f64 {
    let code_padding = attr_f64(&node.attrs, "code_padding").unwrap_or(8.0);
    let font_size = attr_f64(&node.attrs, "code_font_size")
        .unwrap_or_else(|| attr_f64(&node.attrs, "font_size").unwrap_or(12.0) * 0.92);
    let line_height = attr_f64(&node.attrs, "code_line_height").unwrap_or(1.25);
    let line_step = font_size * line_height;
    let wrap_width = (width - code_padding * 2.0).max(1.0);
    let mut measure_attrs = node.attrs.clone();
    measure_attrs.insert("font_size".to_string(), font_size.to_string());
    measure_attrs.insert("width".to_string(), wrap_width.to_string());
    let lines = wrapped_text_lines(content, Some(wrap_width), font_size, 0.0, &measure_attrs);
    let height = (lines.len().max(1) as f64) * line_step + code_padding * 2.0;
    let fill = node
        .attrs
        .get("code_background_fill")
        .map(String::as_str)
        .unwrap_or("var(--color-code-bg)");
    let stroke = node
        .attrs
        .get("code_border_stroke")
        .map(String::as_str)
        .unwrap_or("var(--color-nav-border)");
    let radius = attr_f64(&node.attrs, "code_radius").unwrap_or(6.0);
    write!(
        svg,
        "<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" rx=\"{radius}\" ry=\"{radius}\" fill=\"{}\" stroke=\"{}\"/>",
        svg_escape_attr(fill),
        svg_escape_attr(stroke)
    )
    .unwrap();
    let family = node
        .attrs
        .get("code_font_family")
        .map(String::as_str)
        .unwrap_or("JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, monospace");
    let text_fill = node
        .attrs
        .get("code_fill")
        .map(String::as_str)
        .unwrap_or("currentColor");
    let baseline = font_size * 0.8 + ((line_step - font_size).max(0.0) / 2.0);
    write!(
        svg,
        "<text x=\"{}\" y=\"{}\" font-size=\"{}\" font-family=\"{}\" text-anchor=\"start\" dominant-baseline=\"auto\" fill=\"{}\" xml:space=\"preserve\"",
        x + code_padding,
        y + code_padding + baseline,
        font_size,
        svg_escape_attr(family),
        svg_escape_attr(text_fill)
    )
    .unwrap();
    if let Some(language) = language.filter(|value| !value.is_empty()) {
        write!(svg, " data-language=\"{}\"", svg_escape_attr(language)).unwrap();
    }
    svg.push('>');
    for (line_index, line) in lines.iter().enumerate() {
        let line_y = y + code_padding + baseline + (line_index as f64 * line_step);
        write!(svg, "<tspan x=\"{}\" y=\"{line_y}\">", x + code_padding).unwrap();
        render_code_line_svg(svg, line, language);
        svg.push_str("</tspan>");
    }
    svg.push_str("</text>");
    height
}

fn render_code_line_svg(svg: &mut String, line: &str, language: Option<&str>) {
    if !language
        .map(|lang| lang.eq_ignore_ascii_case("wcl"))
        .unwrap_or(false)
    {
        svg.push_str(&svg_escape_text(line));
        return;
    }

    for (text, class_name) in tokenize_wcl_code_line(line) {
        if let Some(class_name) = class_name {
            write!(
                svg,
                "<tspan class=\"{}\" fill=\"currentColor\">{}</tspan>",
                svg_escape_attr(class_name),
                svg_escape_text(text)
            )
            .unwrap();
        } else {
            svg.push_str(&svg_escape_text(text));
        }
    }
}

fn tokenize_wcl_code_line(line: &str) -> Vec<(&str, Option<&'static str>)> {
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        if rest.starts_with("//") {
            tokens.push((rest, Some("hljs-comment")));
            break;
        }

        let ch = rest.chars().next().unwrap();
        if ch == '"' {
            let end = quoted_token_end(line, index);
            tokens.push((&line[index..end], Some("hljs-string")));
            index = end;
            continue;
        }

        if ch == '@' {
            let end = take_while_from(line, index + ch.len_utf8(), is_ident_char);
            tokens.push((&line[index..end], Some("hljs-meta")));
            index = end;
            continue;
        }

        if ch.is_ascii_digit() {
            let end = take_while_from(line, index + ch.len_utf8(), |c| {
                c.is_ascii_digit() || c == '.'
            });
            tokens.push((&line[index..end], Some("hljs-number")));
            index = end;
            continue;
        }

        if is_ident_start(ch) {
            let end = take_while_from(line, index + ch.len_utf8(), is_ident_char);
            let ident = &line[index..end];
            let class_name = if is_wcl_keyword(ident) {
                Some("hljs-keyword")
            } else if next_non_ws_char(line, end) == Some('=') {
                Some("hljs-attr")
            } else {
                None
            };
            tokens.push((ident, class_name));
            index = end;
            continue;
        }

        if matches!(ch, '{' | '}' | '[' | ']' | '(' | ')' | '=' | ',' | '.') {
            let end = index + ch.len_utf8();
            tokens.push((&line[index..end], None));
            index = end;
            continue;
        }

        let end = index + ch.len_utf8();
        tokens.push((&line[index..end], None));
        index = end;
    }

    tokens
}

fn quoted_token_end(line: &str, start: usize) -> usize {
    let mut escaped = false;
    for (offset, ch) in line[start + 1..].char_indices() {
        let pos = start + 1 + offset + ch.len_utf8();
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return pos;
        }
    }
    line.len()
}

fn take_while_from<F>(line: &str, start: usize, predicate: F) -> usize
where
    F: Fn(char) -> bool,
{
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !predicate(ch) {
            return start + offset;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn next_non_ws_char(line: &str, start: usize) -> Option<char> {
    line[start..].chars().find(|ch| !ch.is_whitespace())
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_char(ch: char) -> bool {
    ch == '_' || ch == '-' || ch.is_ascii_alphanumeric()
}

fn is_wcl_keyword(ident: &str) -> bool {
    matches!(
        ident,
        "as" | "else"
            | "export"
            | "false"
            | "for"
            | "if"
            | "import"
            | "in"
            | "let"
            | "namespace"
            | "null"
            | "schema"
            | "true"
            | "use"
            | "connection"
            | "diagram"
            | "graph_node"
            | "graph_row"
            | "text_block"
            | "paragraph"
            | "code"
            | "rect"
            | "circle"
            | "database"
    )
}

fn render_inline_svg_shape_svg(node: &ShapeNode, svg: &mut String) {
    let b = &node.resolved;
    let style = svg_node_attrs(node, &node.attrs);
    let content = node
        .attrs
        .get("content")
        .or_else(|| node.attrs.get("_wdoc_inline_svg_content"))
        .map(|s| s.as_str())
        .unwrap_or("");
    let sanitized = sanitize_inline_svg(content).unwrap_or_default();
    let view_box = svg_root_view_box(content)
        .unwrap_or_else(|| format!("0 0 {} {}", b.width.max(0.0), b.height.max(0.0)));
    let (min_x, min_y, vb_width, vb_height) =
        parse_view_box(&view_box).unwrap_or((0.0, 0.0, b.width.max(1.0), b.height.max(1.0)));
    let scale_x = if vb_width == 0.0 {
        1.0
    } else {
        b.width / vb_width
    };
    let scale_y = if vb_height == 0.0 {
        1.0
    } else {
        b.height / vb_height
    };
    write!(
        svg,
        "<g transform=\"translate({},{}) scale({},{}) translate({},{})\"{style}>",
        b.x, b.y, scale_x, scale_y, -min_x, -min_y
    )
    .unwrap();
    svg.push_str(&sanitized);
    svg.push_str("</g>");
}

fn render_icon_shape_svg(node: &ShapeNode, svg: &mut String) {
    let b = &node.resolved;
    let style_attrs = svg_node_attrs(node, &node.attrs);
    let content = node
        .attrs
        .get("_wdoc_icon_content")
        .map(|s| s.as_str())
        .unwrap_or("");
    let sanitized = sanitize_inline_svg(content).unwrap_or_default();
    let native_view_box = svg_root_view_box(content)
        .and_then(|view_box| parse_view_box(&view_box))
        .unwrap_or((0.0, 0.0, b.width.max(1.0), b.height.max(1.0)));
    let (min_x, min_y, native_width, native_height) = native_view_box;
    let native_width = native_width.max(1.0);
    let native_height = native_height.max(1.0);
    let mode = node
        .attrs
        .get("_wdoc_icon_normalize_mode")
        .map(|s| s.as_str())
        .unwrap_or("viewbox");
    let requested_width = attr_f64(&node.attrs, "_wdoc_icon_normalize_width");
    let requested_height = attr_f64(&node.attrs, "_wdoc_icon_normalize_height");
    let ratio = native_width / native_height;
    let (view_min_x, view_min_y, view_width, view_height, transform) = if mode == "none" {
        (min_x, min_y, native_width, native_height, String::new())
    } else {
        let norm_width = requested_width
            .or_else(|| requested_height.map(|height| height * ratio))
            .unwrap_or(native_width)
            .max(1.0);
        let norm_height = requested_height
            .or_else(|| requested_width.map(|width| width / ratio))
            .unwrap_or(native_height)
            .max(1.0);
        let scale = (norm_width / native_width).min(norm_height / native_height);
        let tx = (norm_width - native_width * scale) / 2.0 - min_x * scale;
        let ty = (norm_height - native_height * scale) / 2.0 - min_y * scale;
        (
            0.0,
            0.0,
            norm_width,
            norm_height,
            format!(" transform=\"translate({tx},{ty}) scale({scale})\""),
        )
    };

    let css = node
        .attrs
        .get("_wdoc_icon_css")
        .map(|s| s.as_str())
        .unwrap_or("");

    write!(
        svg,
        "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" overflow=\"visible\"{style_attrs}>",
        b.x, b.y, b.width, b.height, view_min_x, view_min_y, view_width, view_height
    )
    .unwrap();
    if !css.trim().is_empty() {
        let css = css.replace("</style", "<\\/style");
        write!(svg, "<style>{css}</style>").unwrap();
    }
    if sanitized.is_empty() {
        let label = node
            .attrs
            .get("_wdoc_icon_missing")
            .map(|s| s.as_str())
            .unwrap_or("icon");
        let label = svg_escape_text(label);
        write!(
            svg,
            "<rect x=\"1\" y=\"1\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\"/>\
             <text x=\"{}\" y=\"{}\" fill=\"currentColor\" font-size=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\">{label}</text>",
            (view_width - 2.0).max(1.0),
            (view_height - 2.0).max(1.0),
            view_width / 2.0,
            view_height / 2.0,
            (view_height / 4.0).clamp(6.0, 14.0),
        )
        .unwrap();
    } else if transform.is_empty() {
        svg.push_str(&sanitized);
    } else {
        write!(svg, "<g{transform}>").unwrap();
        svg.push_str(&sanitized);
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");
}

fn render_image_shape_svg(node: &ShapeNode, svg: &mut String) {
    let b = &node.resolved;
    let style = svg_image_node_attrs(node, &node.attrs);
    let src = node.attrs.get("src").map(|s| s.as_str()).unwrap_or("");
    let fit = node
        .attrs
        .get("fit")
        .map(|s| s.as_str())
        .unwrap_or("contain");
    let clip_id = svg_generated_id("wdoc-image-clip", node);
    let rx = node.attrs.get("rx").map(|s| s.as_str()).unwrap_or("0");
    let ry = node.attrs.get("ry").map(|s| s.as_str()).unwrap_or(rx);
    let src = svg_escape_attr(src);

    write!(
        svg,
        "<defs><clipPath id=\"{clip_id}\">\
         <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{rx}\" ry=\"{ry}\"/>\
         </clipPath></defs>",
        b.x, b.y, b.width, b.height
    )
    .unwrap();

    let aria = node
        .attrs
        .get("alt")
        .map(|alt| {
            let alt = svg_escape_attr(alt);
            format!(" role=\"img\" aria-label=\"{alt}\"")
        })
        .unwrap_or_default();

    if !aria.is_empty() {
        write!(svg, "<g{aria}>").unwrap();
    }

    if fit == "tile" {
        let pattern_id = svg_generated_id("wdoc-image-pattern", node);
        let tile_width = attr_f64(&node.attrs, "tile_width").unwrap_or(b.width);
        let tile_height = attr_f64(&node.attrs, "tile_height").unwrap_or(b.height);
        write!(
            svg,
            "<defs><pattern id=\"{pattern_id}\" patternUnits=\"userSpaceOnUse\" \
             x=\"{}\" y=\"{}\" width=\"{tile_width}\" height=\"{tile_height}\">\
             <image href=\"{src}\" x=\"{}\" y=\"{}\" width=\"{tile_width}\" height=\"{tile_height}\" \
             preserveAspectRatio=\"xMidYMid meet\"/>\
             </pattern></defs>\
             <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
             clip-path=\"url(#{clip_id})\" fill=\"url(#{pattern_id})\"{style}/>",
            b.x, b.y, b.x, b.y, b.x, b.y, b.width, b.height
        )
        .unwrap();
    } else {
        let preserve_aspect_ratio = match fit {
            "cover" => "xMidYMid slice",
            "fill" => "none",
            _ => "xMidYMid meet",
        };
        write!(
            svg,
            "<image href=\"{src}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
             preserveAspectRatio=\"{preserve_aspect_ratio}\" \
             clip-path=\"url(#{clip_id})\"{style}/>",
            b.x, b.y, b.width, b.height
        )
        .unwrap();
    }

    if !aria.is_empty() {
        svg.push_str("</g>");
    }
}

fn render_map_shape_svg(node: &ShapeNode, svg: &mut String) {
    let b = &node.resolved;
    let style = svg_image_node_attrs(node, &node.attrs);
    let src = node.attrs.get("src").map(|s| s.as_str()).unwrap_or("");
    let content_w = map_content_width(node);
    let content_h = map_content_height(node);
    let view_x = attr_f64(&node.attrs, "view_x").unwrap_or(0.0);
    let view_y = attr_f64(&node.attrs, "view_y").unwrap_or(0.0);
    let view_w = attr_f64(&node.attrs, "view_width")
        .unwrap_or(b.width.min(content_w))
        .max(1.0);
    let view_h = attr_f64(&node.attrs, "view_height")
        .unwrap_or(b.height.min(content_h))
        .max(1.0);
    let min_zoom = attr_f64(&node.attrs, "min_zoom").unwrap_or(0.25).max(0.001);
    let max_zoom = attr_f64(&node.attrs, "max_zoom")
        .unwrap_or(8.0)
        .max(min_zoom);
    let background_fill = node
        .attrs
        .get("background_fill")
        .map(|s| s.as_str())
        .unwrap_or("none");
    let src = svg_escape_attr(src);
    let background_fill = svg_escape_attr(background_fill);
    let aria = node
        .attrs
        .get("alt")
        .map(|alt| {
            let alt = svg_escape_attr(alt);
            format!(" role=\"img\" aria-label=\"{alt}\"")
        })
        .unwrap_or_default();

    write!(
        svg,
        "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
         viewBox=\"{view_x} {view_y} {view_w} {view_h}\" overflow=\"hidden\" \
         data-wdoc-map=\"true\" data-wdoc-map-content-width=\"{content_w}\" \
         data-wdoc-map-content-height=\"{content_h}\" data-wdoc-map-home-view-box=\"{view_x} {view_y} {view_w} {view_h}\" \
         data-wdoc-map-min-zoom=\"{min_zoom}\" data-wdoc-map-max-zoom=\"{max_zoom}\" \
         {aria}{style}>\
         <rect x=\"0\" y=\"0\" width=\"{content_w}\" height=\"{content_h}\" fill=\"{background_fill}\" stroke=\"none\"/>\
         <image href=\"{src}\" x=\"0\" y=\"0\" width=\"{content_w}\" height=\"{content_h}\" preserveAspectRatio=\"none\"/>",
        b.x, b.y, b.width, b.height
    )
    .unwrap();
    render_child_shapes_svg(&node.children, svg);
    svg.push_str("</svg>");
}

fn render_sprite_shape_svg(node: &ShapeNode, svg: &mut String) {
    let b = &node.resolved;
    let style = svg_image_node_attrs(node, &node.attrs);
    let src = node
        .attrs
        .get("_wdoc_sheet_src")
        .or_else(|| node.attrs.get("src"))
        .map(|s| s.as_str())
        .unwrap_or("");
    let columns = attr_f64(&node.attrs, "_wdoc_sheet_columns")
        .unwrap_or(1.0)
        .max(1.0);
    let frame_w = attr_f64(&node.attrs, "_wdoc_sheet_frame_width").unwrap_or(b.width);
    let frame_h = attr_f64(&node.attrs, "_wdoc_sheet_frame_height").unwrap_or(b.height);
    let offset_x = attr_f64(&node.attrs, "_wdoc_sheet_offset_x").unwrap_or(0.0);
    let offset_y = attr_f64(&node.attrs, "_wdoc_sheet_offset_y").unwrap_or(0.0);
    let gap_x = attr_f64(&node.attrs, "_wdoc_sheet_gap_x").unwrap_or(0.0);
    let gap_y = attr_f64(&node.attrs, "_wdoc_sheet_gap_y").unwrap_or(0.0);
    let frame_count = attr_f64(&node.attrs, "_wdoc_sheet_frame_count")
        .unwrap_or(columns)
        .max(1.0);
    let frame = attr_f64(&node.attrs, "frame")
        .unwrap_or(0.0)
        .max(0.0)
        .floor();
    let row_count = (frame_count / columns).ceil().max(1.0);
    let sheet_w = attr_f64(&node.attrs, "_wdoc_sheet_width")
        .unwrap_or(offset_x + columns * frame_w + (columns - 1.0) * gap_x);
    let sheet_h = attr_f64(&node.attrs, "_wdoc_sheet_height")
        .unwrap_or(offset_y + row_count * frame_h + (row_count - 1.0) * gap_y);
    let col = frame % columns;
    let row = (frame / columns).floor();
    let view_x = offset_x + col * (frame_w + gap_x);
    let view_y = offset_y + row * (frame_h + gap_y);
    let src = svg_escape_attr(src);
    let preserve_aspect_ratio = match node
        .attrs
        .get("fit")
        .map(|s| s.as_str())
        .unwrap_or("contain")
    {
        "cover" => "xMidYMid slice",
        "fill" => "none",
        _ => "xMidYMid meet",
    };
    let aria = node
        .attrs
        .get("alt")
        .map(|alt| {
            let alt = svg_escape_attr(alt);
            format!(" role=\"img\" aria-label=\"{alt}\"")
        })
        .unwrap_or_default();

    write!(
        svg,
        "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"{view_x} {view_y} {frame_w} {frame_h}\" \
         preserveAspectRatio=\"{preserve_aspect_ratio}\" overflow=\"hidden\" data-wdoc-sprite=\"true\" \
         data-wdoc-sprite-columns=\"{columns}\" data-wdoc-sprite-frame-width=\"{frame_w}\" \
         data-wdoc-sprite-frame-height=\"{frame_h}\" data-wdoc-sprite-offset-x=\"{offset_x}\" \
         data-wdoc-sprite-offset-y=\"{offset_y}\" data-wdoc-sprite-gap-x=\"{gap_x}\" \
         data-wdoc-sprite-gap-y=\"{gap_y}\" data-wdoc-sprite-current-frame=\"{frame}\"{}{}{}{}{}{}{}{}{}{}{}{}{}{}{aria}{style}>\
         <image href=\"{src}\" x=\"0\" y=\"0\" width=\"{sheet_w}\" height=\"{sheet_h}\" \
         preserveAspectRatio=\"none\"/>\
         </svg>",
        b.x,
        b.y,
        b.width,
        b.height,
        sprite_data_attr(node, "transparent_color", "transparent-color"),
        sprite_data_attr(node, "transparent_tolerance", "transparent-tolerance"),
        sprite_data_attr(node, "_wdoc_sheet_src", "sheet-src"),
        sprite_data_attr(node, "_wdoc_sheet_frame_count", "frame-count"),
        sprite_data_attr(node, "_wdoc_sheet_width", "sheet-width"),
        sprite_data_attr(node, "_wdoc_sheet_height", "sheet-height"),
        sprite_data_attr(node, "_wdoc_sheet_id", "sheet-id"),
        sprite_data_attr(node, "frame", "initial-frame"),
        sprite_data_attr(node, "sheet", "sheet"),
        sprite_data_attr(node, "src", "src"),
        sprite_data_attr(node, "fit", "fit"),
        sprite_data_attr(node, "alt", "alt"),
        sprite_data_attr(node, "class", "class"),
        sprite_data_attr(node, "id", "id")
    )
    .unwrap();
}

fn sprite_data_attr(node: &ShapeNode, attr: &str, data_name: &str) -> String {
    node.attrs
        .get(attr)
        .map(|value| {
            format!(
                " data-wdoc-sprite-{}=\"{}\"",
                data_name,
                svg_escape_attr(value)
            )
        })
        .unwrap_or_default()
}

fn render_dopesheet_view_shape_svg(node: &ShapeNode, svg: &mut String) {
    let b = &node.resolved;
    let style = svg_node_attrs(node, &node.attrs);
    let src = node
        .attrs
        .get("_wdoc_sheet_src")
        .or_else(|| node.attrs.get("src"))
        .map(|s| s.as_str())
        .unwrap_or("");
    let columns = attr_f64(&node.attrs, "_wdoc_sheet_columns")
        .unwrap_or(1.0)
        .max(1.0)
        .floor() as usize;
    let frame_w = attr_f64(&node.attrs, "_wdoc_sheet_frame_width").unwrap_or(1.0);
    let frame_h = attr_f64(&node.attrs, "_wdoc_sheet_frame_height").unwrap_or(1.0);
    let offset_x = attr_f64(&node.attrs, "_wdoc_sheet_offset_x").unwrap_or(0.0);
    let offset_y = attr_f64(&node.attrs, "_wdoc_sheet_offset_y").unwrap_or(0.0);
    let gap_x = attr_f64(&node.attrs, "_wdoc_sheet_gap_x").unwrap_or(0.0);
    let gap_y = attr_f64(&node.attrs, "_wdoc_sheet_gap_y").unwrap_or(0.0);
    let frame_count = attr_f64(&node.attrs, "_wdoc_sheet_frame_count")
        .unwrap_or(columns as f64)
        .max(1.0)
        .floor() as usize;
    let rows = frame_count.div_ceil(columns).max(1);
    let sheet_w = attr_f64(&node.attrs, "_wdoc_sheet_width").unwrap_or(
        offset_x + columns as f64 * frame_w + (columns.saturating_sub(1)) as f64 * gap_x,
    );
    let sheet_h = attr_f64(&node.attrs, "_wdoc_sheet_height")
        .unwrap_or(offset_y + rows as f64 * frame_h + (rows.saturating_sub(1)) as f64 * gap_y);
    let cell_w = b.width / columns as f64;
    let cell_h = b.height / rows as f64;
    let grid_stroke = node
        .attrs
        .get("grid_stroke")
        .map(|s| s.as_str())
        .unwrap_or("rgba(148, 163, 184, 0.8)");
    let grid_stroke_width = attr_f64(&node.attrs, "grid_stroke_width").unwrap_or(1.0);
    let background_fill = node
        .attrs
        .get("background_fill")
        .map(|s| s.as_str())
        .unwrap_or("none");
    let src = svg_escape_attr(src);
    let grid_stroke = svg_escape_attr(grid_stroke);
    let background_fill = svg_escape_attr(background_fill);

    write!(
        svg,
        "<g data-wdoc-dopesheet-view=\"true\"{style}>\
         <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{background_fill}\" stroke=\"none\"/>",
        b.x, b.y, b.width, b.height
    )
    .unwrap();

    for frame in 0..frame_count {
        let col = frame % columns;
        let row = frame / columns;
        let x = b.x + col as f64 * cell_w;
        let y = b.y + row as f64 * cell_h;
        let view_x = offset_x + col as f64 * (frame_w + gap_x);
        let view_y = offset_y + row as f64 * (frame_h + gap_y);
        write!(
            svg,
            "<svg x=\"{x}\" y=\"{y}\" width=\"{cell_w}\" height=\"{cell_h}\" \
             viewBox=\"{view_x} {view_y} {frame_w} {frame_h}\" preserveAspectRatio=\"xMidYMid meet\" \
             overflow=\"hidden\">\
             <image href=\"{src}\" x=\"0\" y=\"0\" width=\"{sheet_w}\" height=\"{sheet_h}\" preserveAspectRatio=\"none\"/>\
             </svg>"
        )
        .unwrap();
    }

    for col in 0..=columns {
        let x = b.x + col as f64 * cell_w;
        write!(
            svg,
            "<line x1=\"{x}\" y1=\"{}\" x2=\"{x}\" y2=\"{}\" stroke=\"{grid_stroke}\" stroke-width=\"{grid_stroke_width}\"/>",
            b.y,
            b.y + b.height
        )
        .unwrap();
    }
    for row in 0..=rows {
        let y = b.y + row as f64 * cell_h;
        write!(
            svg,
            "<line x1=\"{}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\" stroke=\"{grid_stroke}\" stroke-width=\"{grid_stroke_width}\"/>",
            b.x,
            b.x + b.width
        )
        .unwrap();
    }
    svg.push_str("</g>");
}

fn render_tilemap_shape_svg(node: &ShapeNode, svg: &mut String) {
    let b = &node.resolved;
    let style = svg_node_attrs(node, &node.attrs);
    let src = node
        .attrs
        .get("_wdoc_sheet_src")
        .or_else(|| node.attrs.get("src"))
        .map(|s| s.as_str())
        .unwrap_or("");
    let columns = attr_f64(&node.attrs, "_wdoc_sheet_columns")
        .unwrap_or(1.0)
        .max(1.0)
        .floor() as usize;
    let frame_w = attr_f64(&node.attrs, "_wdoc_sheet_frame_width").unwrap_or(1.0);
    let frame_h = attr_f64(&node.attrs, "_wdoc_sheet_frame_height").unwrap_or(1.0);
    let offset_x = attr_f64(&node.attrs, "_wdoc_sheet_offset_x").unwrap_or(0.0);
    let offset_y = attr_f64(&node.attrs, "_wdoc_sheet_offset_y").unwrap_or(0.0);
    let gap_x = attr_f64(&node.attrs, "_wdoc_sheet_gap_x").unwrap_or(0.0);
    let gap_y = attr_f64(&node.attrs, "_wdoc_sheet_gap_y").unwrap_or(0.0);
    let sheet_rows = attr_f64(&node.attrs, "_wdoc_sheet_frame_count")
        .map(|count| (count.max(1.0) as usize).div_ceil(columns).max(1))
        .unwrap_or(1);
    let sheet_w = attr_f64(&node.attrs, "_wdoc_sheet_width").unwrap_or(
        offset_x + columns as f64 * frame_w + (columns.saturating_sub(1)) as f64 * gap_x,
    );
    let sheet_h = attr_f64(&node.attrs, "_wdoc_sheet_height").unwrap_or(
        offset_y + sheet_rows as f64 * frame_h + (sheet_rows.saturating_sub(1)) as f64 * gap_y,
    );
    let tile_w = attr_f64(&node.attrs, "tile_width")
        .unwrap_or(frame_w)
        .max(1.0);
    let tile_h = attr_f64(&node.attrs, "tile_height")
        .unwrap_or(frame_h)
        .max(1.0);
    let tile_render_h = attr_f64(&node.attrs, "tile_render_height")
        .unwrap_or(tile_h)
        .max(1.0);
    let map_columns = attr_f64(&node.attrs, "_wdoc_tilemap_columns")
        .unwrap_or_else(|| (b.width / tile_w).ceil())
        .max(0.0)
        .floor() as usize;
    let map_rows = attr_f64(&node.attrs, "_wdoc_tilemap_rows_count")
        .unwrap_or_else(|| (b.height / tile_h).ceil())
        .max(0.0)
        .floor() as usize;
    let orientation = node
        .attrs
        .get("orientation")
        .map(|value| value.trim())
        .unwrap_or("orthogonal");
    let is_isometric = orientation == "isometric";
    let grid_stroke = node.attrs.get("grid_stroke").map(|s| s.as_str());
    let grid_stroke_width = attr_f64(&node.attrs, "grid_stroke_width").unwrap_or(1.0);
    let background_fill = node
        .attrs
        .get("background_fill")
        .map(|s| s.as_str())
        .unwrap_or("none");
    let src = svg_escape_attr(src);
    let background_fill = svg_escape_attr(background_fill);

    write!(
        svg,
        "<g data-wdoc-tilemap=\"true\"{style}>\
         <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{background_fill}\" stroke=\"none\"/>",
        b.x, b.y, b.width, b.height
    )
    .unwrap();

    if let Some(rows) = node.attrs.get("_wdoc_tilemap_rows") {
        let mut tiles = Vec::new();
        for (row_idx, row) in rows.split(';').enumerate() {
            for (col_idx, cell) in row.split(',').enumerate() {
                let cell = cell.trim();
                if cell.is_empty() {
                    continue;
                }
                let Ok(frame) = cell.parse::<usize>() else {
                    continue;
                };
                tiles.push((row_idx, col_idx, frame));
            }
        }
        if is_isometric {
            tiles.sort_by(|(row_a, col_a, _), (row_b, col_b, _)| {
                (row_a + col_a)
                    .cmp(&(row_b + col_b))
                    .then_with(|| row_a.cmp(row_b))
                    .then_with(|| col_a.cmp(col_b))
            });
        }
        for (row_idx, col_idx, frame) in tiles {
            let frame_col = frame % columns;
            let frame_row = frame / columns;
            let (x, y) = if is_isometric {
                (
                    b.x + (col_idx as f64 - row_idx as f64) * tile_w / 2.0
                        + (map_rows.saturating_sub(1)) as f64 * tile_w / 2.0,
                    b.y + (col_idx as f64 + row_idx as f64) * tile_h / 2.0,
                )
            } else {
                (b.x + col_idx as f64 * tile_w, b.y + row_idx as f64 * tile_h)
            };
            let view_x = offset_x + frame_col as f64 * (frame_w + gap_x);
            let view_y = offset_y + frame_row as f64 * (frame_h + gap_y);
            write!(
                svg,
                "<svg x=\"{x}\" y=\"{y}\" width=\"{tile_w}\" height=\"{tile_render_h}\" \
                 viewBox=\"{view_x} {view_y} {frame_w} {frame_h}\" preserveAspectRatio=\"none\" \
                 overflow=\"hidden\" data-wdoc-sprite=\"true\" data-wdoc-sprite-current-frame=\"{frame}\"{}{}>\
                 <image href=\"{src}\" x=\"0\" y=\"0\" width=\"{sheet_w}\" height=\"{sheet_h}\" preserveAspectRatio=\"none\"/>\
                 </svg>",
                sprite_data_attr(node, "transparent_color", "transparent-color"),
                sprite_data_attr(node, "transparent_tolerance", "transparent-tolerance")
            )
            .unwrap();
        }
    }

    if let Some(grid_stroke) = grid_stroke.filter(|_| !is_isometric) {
        let grid_stroke = svg_escape_attr(grid_stroke);
        for col in 0..=map_columns {
            let x = b.x + col as f64 * tile_w;
            write!(
                svg,
                "<line x1=\"{x}\" y1=\"{}\" x2=\"{x}\" y2=\"{}\" stroke=\"{grid_stroke}\" stroke-width=\"{grid_stroke_width}\"/>",
                b.y,
                b.y + map_rows as f64 * tile_h
            )
            .unwrap();
        }
        for row in 0..=map_rows {
            let y = b.y + row as f64 * tile_h;
            write!(
                svg,
                "<line x1=\"{}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\" stroke=\"{grid_stroke}\" stroke-width=\"{grid_stroke_width}\"/>",
                b.x,
                b.x + map_columns as f64 * tile_w
            )
            .unwrap();
        }
    }

    svg.push_str("</g>");
}

fn render_game_layer_shape_svg(node: &ShapeNode, svg: &mut String) {
    let b = &node.resolved;
    let style = svg_node_attrs(node, &node.attrs);
    let parallax = attr_f64(&node.attrs, "parallax").unwrap_or(1.0);
    let locked = bool_attr(&node.attrs, "locked");
    let clip = bool_attr(&node.attrs, "clip");
    let runtime_attrs = format!(
        " data-wdoc-game-layer=\"true\" data-wdoc-game-layer-x=\"{}\" data-wdoc-game-layer-y=\"{}\" data-wdoc-game-layer-parallax=\"{}\"{}",
        b.x,
        b.y,
        parallax,
        if locked {
            " data-wdoc-game-layer-locked=\"true\""
        } else {
            ""
        }
    );

    if clip {
        let clip_id = svg_generated_id("wdoc-game-layer-clip", node);
        write!(
            svg,
            "<defs><clipPath id=\"{clip_id}\">\
             <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"/>\
             </clipPath></defs>",
            b.width, b.height
        )
        .unwrap();
        write!(
            svg,
            "<g transform=\"translate({},{})\"{style}{runtime_attrs}>\
             <g clip-path=\"url(#{clip_id})\">",
            b.x, b.y
        )
        .unwrap();
        render_child_shapes_svg(&node.children, svg);
        svg.push_str("</g></g>");
    } else {
        write!(
            svg,
            "<g transform=\"translate({},{})\"{style}{runtime_attrs}>",
            b.x, b.y
        )
        .unwrap();
        render_child_shapes_svg(&node.children, svg);
        svg.push_str("</g>");
    }
}

#[derive(Clone)]
enum RoutedGeometry {
    Line {
        start: (f64, f64),
        end: (f64, f64),
    },
    Orthogonal {
        points: Vec<(f64, f64)>,
    },
    Bezier {
        start: (f64, f64),
        c1: (f64, f64),
        c2: (f64, f64),
        end: (f64, f64),
    },
}

#[derive(Clone)]
struct RoutedConnection {
    conn: Connection,
    geometry: RoutedGeometry,
}

fn connection_render_key(conn: &Connection) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        conn.source_order, conn.from_id, conn.to_id
    )
}

fn routed_connections(
    connections: &[Connection],
    shape_map: &HashMap<String, Bounds>,
) -> HashMap<String, RoutedConnection> {
    let mut routed: Vec<RoutedConnection> = connections
        .iter()
        .filter_map(|conn| compute_connection_route(conn, shape_map))
        .collect();
    separate_parallel_routes(&mut routed);
    routed
        .into_iter()
        .map(|route| (connection_render_key(&route.conn), route))
        .collect()
}

fn render_connection_svg(conn: &Connection, shape_map: &HashMap<String, Bounds>, svg: &mut String) {
    if let Some(route) = compute_connection_route(conn, shape_map) {
        render_routed_connection_svg(&route, svg);
    }
}

fn compute_connection_route(
    conn: &Connection,
    shape_map: &HashMap<String, Bounds>,
) -> Option<RoutedConnection> {
    let from_bounds = match shape_map.get(&conn.from_id) {
        Some(b) => b,
        None => return None,
    };
    let to_bounds = match shape_map.get(&conn.to_id) {
        Some(b) => b,
        None => return None,
    };

    let geometry = match conn.curve {
        CurveStyle::Straight => {
            let from_anchor =
                effective_endpoint_anchor(&conn.from_id, from_bounds, conn.from_anchor, shape_map);
            let to_anchor =
                effective_endpoint_anchor(&conn.to_id, to_bounds, conn.to_anchor, shape_map);
            if conn.from_anchor == AnchorPoint::Auto && conn.to_anchor == AnchorPoint::Auto {
                let (x1, y1) = from_bounds.anchor_pos(from_anchor, to_bounds);
                let (x2, y2) = to_bounds.anchor_pos(to_anchor, from_bounds);
                if connection_uses_direct_route(conn) {
                    RoutedGeometry::Line {
                        start: (x1, y1),
                        end: (x2, y2),
                    }
                } else {
                    let obstacles: Vec<Bounds> =
                        connection_obstacles(&conn.from_id, &conn.to_id, x1, y1, x2, y2, shape_map);
                    if direct_auto_line_is_clean((x1, y1), (x2, y2), &obstacles) {
                        RoutedGeometry::Line {
                            start: (x1, y1),
                            end: (x2, y2),
                        }
                    } else if from_anchor != AnchorPoint::Auto || to_anchor != AnchorPoint::Auto {
                        let from_route_anchor = if from_anchor == AnchorPoint::Auto {
                            conn.from_anchor
                        } else {
                            from_anchor
                        };
                        let to_route_anchor = if to_anchor == AnchorPoint::Auto {
                            conn.to_anchor
                        } else {
                            to_anchor
                        };
                        if let Some(points) = route_orthogonal_anchored(
                            from_bounds,
                            to_bounds,
                            &obstacles,
                            from_route_anchor,
                            to_route_anchor,
                        ) {
                            RoutedGeometry::Orthogonal { points }
                        } else if let Some(points) = route_orthogonal_anchored_best_effort(
                            from_bounds,
                            to_bounds,
                            &obstacles,
                            from_route_anchor,
                            to_route_anchor,
                        ) {
                            RoutedGeometry::Orthogonal { points }
                        } else if let Some(points) =
                            route_orthogonal(from_bounds, to_bounds, &obstacles)
                        {
                            RoutedGeometry::Orthogonal { points }
                        } else if let Some(points) =
                            route_orthogonal_best_effort(from_bounds, to_bounds, &obstacles)
                        {
                            RoutedGeometry::Orthogonal { points }
                        } else {
                            RoutedGeometry::Line {
                                start: (x1, y1),
                                end: (x2, y2),
                            }
                        }
                    } else if let Some(points) =
                        route_orthogonal(from_bounds, to_bounds, &obstacles)
                    {
                        RoutedGeometry::Orthogonal { points }
                    } else if let Some(points) =
                        route_orthogonal_best_effort(from_bounds, to_bounds, &obstacles)
                    {
                        RoutedGeometry::Orthogonal { points }
                    } else {
                        RoutedGeometry::Line {
                            start: (x1, y1),
                            end: (x2, y2),
                        }
                    }
                }
            } else {
                let (x1, y1) = from_bounds.anchor_pos(from_anchor, to_bounds);
                let (x2, y2) = to_bounds.anchor_pos(to_anchor, from_bounds);
                let obstacles =
                    connection_obstacles(&conn.from_id, &conn.to_id, x1, y1, x2, y2, shape_map);

                if !connection_uses_direct_route(conn)
                    && from_anchor != AnchorPoint::Auto
                    && to_anchor != AnchorPoint::Auto
                {
                    let fc = nearest_container_id(&conn.from_id, shape_map)
                        .and_then(|id| shape_map.get(id).copied());
                    let tc = nearest_container_id(&conn.to_id, shape_map)
                        .and_then(|id| shape_map.get(id).copied());
                    let from_outer = fc.unwrap_or(*from_bounds);
                    let to_outer = tc.unwrap_or(*to_bounds);
                    let cross_container =
                        (fc.is_some() || tc.is_some()) && !bounds_eq(&from_outer, &to_outer);
                    if cross_container {
                        let points = route_cross_container(
                            from_bounds,
                            &from_outer,
                            from_anchor,
                            to_bounds,
                            &to_outer,
                            to_anchor,
                            &obstacles,
                        );
                        if !path_intersects_obstacle(&points, &obstacles) && points.len() >= 2 {
                            return Some(RoutedConnection {
                                conn: conn.clone(),
                                geometry: RoutedGeometry::Orthogonal { points },
                            });
                        }
                    }
                }

                if !connection_uses_direct_route(conn) {
                    if let Some(points) = route_orthogonal_anchored(
                        from_bounds,
                        to_bounds,
                        &obstacles,
                        from_anchor,
                        to_anchor,
                    ) {
                        return Some(RoutedConnection {
                            conn: conn.clone(),
                            geometry: RoutedGeometry::Orthogonal { points },
                        });
                    }
                    if let Some(points) = route_orthogonal_anchored_best_effort(
                        from_bounds,
                        to_bounds,
                        &obstacles,
                        from_anchor,
                        to_anchor,
                    ) {
                        return Some(RoutedConnection {
                            conn: conn.clone(),
                            geometry: RoutedGeometry::Orthogonal { points },
                        });
                    }
                }
                RoutedGeometry::Line {
                    start: (x1, y1),
                    end: (x2, y2),
                }
            }
        }
        CurveStyle::Bezier => {
            let from_anchor =
                effective_endpoint_anchor(&conn.from_id, from_bounds, conn.from_anchor, shape_map);
            let to_anchor =
                effective_endpoint_anchor(&conn.to_id, to_bounds, conn.to_anchor, shape_map);
            let (x1, y1) = from_bounds.anchor_pos(from_anchor, to_bounds);
            let (x2, y2) = to_bounds.anchor_pos(to_anchor, from_bounds);
            let obstacles =
                connection_obstacles(&conn.from_id, &conn.to_id, x1, y1, x2, y2, shape_map);
            let dx = (x2 - x1).abs() / 2.0;
            let dy = (y2 - y1).abs() / 2.0;
            let (c1x, c1y) = ctrl_point(x1, y1, from_anchor, dx, dy);
            let (c2x, c2y) = ctrl_point(x2, y2, to_anchor, dx, dy);
            let bezier = RoutedGeometry::Bezier {
                start: (x1, y1),
                c1: (c1x, c1y),
                c2: (c2x, c2y),
                end: (x2, y2),
            };
            if !bezier_intersects_obstacles(&bezier, &obstacles) {
                bezier
            } else if from_anchor != AnchorPoint::Auto && to_anchor != AnchorPoint::Auto {
                if let Some(points) = route_orthogonal_anchored(
                    from_bounds,
                    to_bounds,
                    &obstacles,
                    from_anchor,
                    to_anchor,
                ) {
                    RoutedGeometry::Orthogonal { points }
                } else if let Some(points) = route_orthogonal_anchored_best_effort(
                    from_bounds,
                    to_bounds,
                    &obstacles,
                    from_anchor,
                    to_anchor,
                ) {
                    RoutedGeometry::Orthogonal { points }
                } else {
                    bezier
                }
            } else if let Some(points) = route_orthogonal(from_bounds, to_bounds, &obstacles) {
                RoutedGeometry::Orthogonal { points }
            } else if let Some(points) =
                route_orthogonal_best_effort(from_bounds, to_bounds, &obstacles)
            {
                RoutedGeometry::Orthogonal { points }
            } else {
                bezier
            }
        }
    };

    Some(RoutedConnection {
        conn: conn.clone(),
        geometry,
    })
}

fn render_routed_connection_svg(route: &RoutedConnection, svg: &mut String) {
    let conn = &route.conn;
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
    let runtime_attrs = connection_runtime_attrs(conn);

    match &route.geometry {
        RoutedGeometry::Line { start, end } => {
            write!(
                svg,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"\
                 {stroke_default}{style}{runtime_attrs}{ms}{me}/>",
                start.0, start.1, end.0, end.1
            )
            .unwrap();
        }
        RoutedGeometry::Orthogonal { points } => {
            let d = path_data(points);
            write!(
                svg,
                "<path d=\"{d}\" fill=\"none\"{stroke_default}{style}{runtime_attrs}{ms}{me}/>"
            )
            .unwrap();
        }
        RoutedGeometry::Bezier { start, c1, c2, end } => {
            write!(
                svg,
                "<path d=\"M {} {} C {} {}, {} {}, {} {}\" \
                 fill=\"none\"{stroke_default}{style}{runtime_attrs}{ms}{me}/>",
                start.0, start.1, c1.0, c1.1, c2.0, c2.1, end.0, end.1
            )
            .unwrap();
        }
    }

    if let Some(label) = &conn.label {
        let (mx, my) = connection_label_point(&route.geometry, &conn.attrs);
        let label = svg_escape_text(label);
        write!(
            svg,
            "<text x=\"{mx}\" y=\"{my}\" text-anchor=\"middle\" \
             dominant-baseline=\"auto\" font-size=\"12\" fill=\"currentColor\">{label}</text>"
        )
        .unwrap();
    }
}

fn connection_label_point(
    geometry: &RoutedGeometry,
    attrs: &IndexMap<String, String>,
) -> (f64, f64) {
    let (x, y) = route_label_point(geometry);
    (
        x + attr_f64(attrs, "label_dx").unwrap_or(0.0),
        y + attr_f64(attrs, "label_dy").unwrap_or(0.0),
    )
}

fn route_label_point(geometry: &RoutedGeometry) -> (f64, f64) {
    let points = route_polyline_points(geometry);
    let (x, y, segment) = polyline_midpoint_segment(&points);
    let (dx, dy) = match segment {
        Some((start, end)) => label_offset_for_segment(start, end),
        None => (0.0, -10.0),
    };
    (x + dx, y + dy)
}

fn route_polyline_points(geometry: &RoutedGeometry) -> Vec<(f64, f64)> {
    match geometry {
        RoutedGeometry::Line { start, end } => vec![*start, *end],
        RoutedGeometry::Orthogonal { points } => points.clone(),
        RoutedGeometry::Bezier { start, c1, c2, end } => {
            let mut points = Vec::with_capacity(17);
            for i in 0..=16 {
                let t = i as f64 / 16.0;
                points.push(bezier_point(*start, *c1, *c2, *end, t));
            }
            points
        }
    }
}

fn polyline_midpoint_segment(
    points: &[(f64, f64)],
) -> (f64, f64, Option<((f64, f64), (f64, f64))>) {
    if points.is_empty() {
        return (0.0, 0.0, None);
    }
    if points.len() == 1 {
        return (points[0].0, points[0].1, None);
    }
    let total: f64 = points
        .windows(2)
        .map(|segment| segment_length(segment[0], segment[1]))
        .sum();
    if total <= 0.001 {
        return (points[0].0, points[0].1, None);
    }
    let target = total / 2.0;
    let mut walked = 0.0;
    for segment in points.windows(2) {
        let len = segment_length(segment[0], segment[1]);
        if walked + len >= target {
            let t = ((target - walked) / len).clamp(0.0, 1.0);
            return (
                segment[0].0 + (segment[1].0 - segment[0].0) * t,
                segment[0].1 + (segment[1].1 - segment[0].1) * t,
                Some((segment[0], segment[1])),
            );
        }
        walked += len;
    }
    let last = *points.last().unwrap();
    (last.0, last.1, None)
}

fn label_offset_for_segment(start: (f64, f64), end: (f64, f64)) -> (f64, f64) {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    if dx.abs() >= dy.abs() {
        (0.0, -10.0)
    } else {
        (10.0, 0.0)
    }
}

fn bezier_point(
    start: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    end: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let mt = 1.0 - t;
    let x = mt.powi(3) * start.0
        + 3.0 * mt.powi(2) * t * c1.0
        + 3.0 * mt * t.powi(2) * c2.0
        + t.powi(3) * end.0;
    let y = mt.powi(3) * start.1
        + 3.0 * mt.powi(2) * t * c1.1
        + 3.0 * mt * t.powi(2) * c2.1
        + t.powi(3) * end.1;
    (x, y)
}

fn bezier_intersects_obstacles(geometry: &RoutedGeometry, obstacles: &[Bounds]) -> bool {
    let points = route_polyline_points(geometry);
    path_intersects_obstacle(&points, obstacles)
}

fn separate_parallel_routes(routes: &mut [RoutedConnection]) {
    let mut clusters: Vec<Vec<usize>> = Vec::new();

    for idx in 0..routes.len() {
        let mut matching_clusters = Vec::new();
        for (cluster_idx, cluster) in clusters.iter().enumerate() {
            if routes_share_corridor(&routes[idx], &routes[cluster[0]]) {
                matching_clusters.push(cluster_idx);
            }
        }

        if matching_clusters.is_empty() {
            clusters.push(vec![idx]);
        } else {
            let first = matching_clusters[0];
            clusters[first].push(idx);
            for &cluster_idx in matching_clusters.iter().skip(1).rev() {
                let merged = clusters.remove(cluster_idx);
                clusters[first].extend(merged);
            }
        }
    }

    for cluster in clusters {
        if cluster.len() < 2 {
            continue;
        }
        let mut sorted = cluster;
        sorted.sort_by_key(|idx| routes[*idx].conn.source_order);
        let preserve_start_trunk = cluster_shares_start_trunk(routes, &sorted);
        let offset_axis = cluster_shared_corridor_axis(routes, &sorted);
        let count = sorted.len() as f64;
        for (rank, idx) in sorted.into_iter().enumerate() {
            if route_polyline_points(&routes[idx].geometry).len() > 6 {
                continue;
            }
            let offset = ((rank as f64 - (count - 1.0) / 2.0) * 7.0).clamp(-28.0, 28.0);
            apply_route_parallel_offset(
                &mut routes[idx].geometry,
                offset,
                preserve_start_trunk,
                offset_axis,
            );
        }
    }
}

fn cluster_shares_start_trunk(routes: &[RoutedConnection], indices: &[usize]) -> bool {
    let mut points_iter = indices
        .iter()
        .map(|idx| route_polyline_points(&routes[*idx].geometry));
    let Some(first) = points_iter.next() else {
        return false;
    };
    if first.len() < 3 {
        return false;
    }
    let first_start = first[0];
    let first_next = first[1];
    let first_horizontal = nearly_eq(first_start.1, first_next.1);
    let first_vertical = nearly_eq(first_start.0, first_next.0);
    if !first_horizontal && !first_vertical {
        return false;
    }

    points_iter.all(|points| {
        points.len() >= 3
            && point_nearly_eq(points[0], first_start)
            && ((first_horizontal && nearly_eq(points[0].1, points[1].1))
                || (first_vertical && nearly_eq(points[0].0, points[1].0)))
            && same_direction(first_start, first_next, points[0], points[1])
    })
}

fn point_nearly_eq(a: (f64, f64), b: (f64, f64)) -> bool {
    nearly_eq(a.0, b.0) && nearly_eq(a.1, b.1)
}

fn same_direction(a1: (f64, f64), a2: (f64, f64), b1: (f64, f64), b2: (f64, f64)) -> bool {
    let adx = a2.0 - a1.0;
    let ady = a2.1 - a1.1;
    let bdx = b2.0 - b1.0;
    let bdy = b2.1 - b1.1;
    (adx.signum() == bdx.signum() || (adx.abs() < 0.001 && bdx.abs() < 0.001))
        && (ady.signum() == bdy.signum() || (ady.abs() < 0.001 && bdy.abs() < 0.001))
}

fn routes_share_corridor(a: &RoutedConnection, b: &RoutedConnection) -> bool {
    routes_shared_corridor_axis(a, b).is_some()
}

fn cluster_shared_corridor_axis(
    routes: &[RoutedConnection],
    indices: &[usize],
) -> ParallelOffsetAxis {
    for (pos, a_idx) in indices.iter().enumerate() {
        for b_idx in indices.iter().skip(pos + 1) {
            if let Some(vertical) = routes_shared_corridor_axis(&routes[*a_idx], &routes[*b_idx]) {
                return if vertical {
                    ParallelOffsetAxis::X
                } else {
                    ParallelOffsetAxis::Y
                };
            }
        }
    }
    ParallelOffsetAxis::Auto
}

fn routes_shared_corridor_axis(a: &RoutedConnection, b: &RoutedConnection) -> Option<bool> {
    let a_points = route_polyline_points(&a.geometry);
    let b_points = route_polyline_points(&b.geometry);
    a_points.windows(2).find_map(|a_seg| {
        b_points
            .windows(2)
            .find_map(|b_seg| shared_corridor_axis(a_seg[0], a_seg[1], b_seg[0], b_seg[1]))
    })
}

fn shared_corridor_axis(
    a1: (f64, f64),
    a2: (f64, f64),
    b1: (f64, f64),
    b2: (f64, f64),
) -> Option<bool> {
    let a_vertical = nearly_eq(a1.0, a2.0);
    let b_vertical = nearly_eq(b1.0, b2.0);
    let a_horizontal = nearly_eq(a1.1, a2.1);
    let b_horizontal = nearly_eq(b1.1, b2.1);

    if a_vertical && b_vertical && (a1.0 - b1.0).abs() <= 2.0 {
        return (ranges_overlap_len(a1.1, a2.1, b1.1, b2.1) >= PARALLEL_CORRIDOR_MIN)
            .then_some(true);
    }
    if a_horizontal && b_horizontal && (a1.1 - b1.1).abs() <= 2.0 {
        return (ranges_overlap_len(a1.0, a2.0, b1.0, b2.0) >= PARALLEL_CORRIDOR_MIN)
            .then_some(false);
    }
    None
}

fn ranges_overlap_len(a1: f64, a2: f64, b1: f64, b2: f64) -> f64 {
    let a_min = a1.min(a2);
    let a_max = a1.max(a2);
    let b_min = b1.min(b2);
    let b_max = b1.max(b2);
    (a_max.min(b_max) - a_min.max(b_min)).max(0.0)
}

fn apply_route_parallel_offset(
    geometry: &mut RoutedGeometry,
    offset: f64,
    preserve_start_trunk: bool,
    offset_axis: ParallelOffsetAxis,
) {
    if offset.abs() < 0.001 {
        return;
    }
    match geometry {
        RoutedGeometry::Line { start, end } => {
            let dx = end.0 - start.0;
            let dy = end.1 - start.1;
            let len = (dx * dx + dy * dy).sqrt();
            if len <= 0.001 {
                return;
            }
            let ox = -dy / len * offset;
            let oy = dx / len * offset;
            start.0 += ox;
            start.1 += oy;
            end.0 += ox;
            end.1 += oy;
        }
        RoutedGeometry::Orthogonal { points } => {
            if points.len() < 2 {
                return;
            }
            if points.len() == 2 {
                let dx = points[1].0 - points[0].0;
                let dy = points[1].1 - points[0].1;
                let len = (dx * dx + dy * dy).sqrt();
                if len <= 0.001 {
                    return;
                }
                let ox = -dy / len * offset;
                let oy = dx / len * offset;
                points[0].0 += ox;
                points[0].1 += oy;
                points[1].0 += ox;
                points[1].1 += oy;
                return;
            }
            let (dx, dy) = match offset_axis {
                ParallelOffsetAxis::X => (offset, 0.0),
                ParallelOffsetAxis::Y => (0.0, offset),
                ParallelOffsetAxis::Auto => {
                    let (vertical, horizontal) = route_orientation_lengths(points);
                    if vertical >= horizontal {
                        (offset, 0.0)
                    } else {
                        (0.0, offset)
                    }
                }
            };
            let original = points.clone();
            let last = points.len() - 1;
            let first_offset_idx = if preserve_start_trunk && points.len() > 3 {
                2
            } else {
                1
            };
            let end_offset_idx = if points.len() > 3 { last - 1 } else { last };
            for point in points
                .iter_mut()
                .take(end_offset_idx)
                .skip(first_offset_idx)
            {
                point.0 += dx;
                point.1 += dy;
            }
            *points = orthogonalize_offset_points(&original, std::mem::take(points));
        }
        RoutedGeometry::Bezier { start, c1, c2, end } => {
            let dx = end.0 - start.0;
            let dy = end.1 - start.1;
            let len = (dx * dx + dy * dy).sqrt();
            if len <= 0.001 {
                return;
            }
            let ox = -dy / len * offset;
            let oy = dx / len * offset;
            start.0 += ox;
            start.1 += oy;
            c1.0 += ox;
            c1.1 += oy;
            c2.0 += ox;
            c2.1 += oy;
            end.0 += ox;
            end.1 += oy;
        }
    }
}

fn orthogonalize_offset_points(
    original: &[(f64, f64)],
    points: Vec<(f64, f64)>,
) -> Vec<(f64, f64)> {
    if points.len() < 2 {
        return points;
    }

    let mut out = Vec::with_capacity(points.len() * 2);
    for (idx, segment) in points.windows(2).enumerate() {
        let start = segment[0];
        let end = segment[1];
        out.push(start);
        if !nearly_eq(start.0, end.0) && !nearly_eq(start.1, end.1) {
            let original_segment = original.get(idx).zip(original.get(idx + 1));
            let horizontal = original_segment
                .map(|(a, b)| nearly_eq(a.1, b.1))
                .unwrap_or((end.0 - start.0).abs() >= (end.1 - start.1).abs());
            let corner = if horizontal {
                (end.0, start.1)
            } else {
                (start.0, end.1)
            };
            out.push(corner);
        }
    }
    out.push(*points.last().unwrap());
    simplify_collinear_points(out)
}

fn route_orientation_lengths(points: &[(f64, f64)]) -> (f64, f64) {
    let mut vertical = 0.0;
    let mut horizontal = 0.0;
    for segment in points.windows(2) {
        let dx = (segment[1].0 - segment[0].0).abs();
        let dy = (segment[1].1 - segment[0].1).abs();
        if dx <= dy {
            vertical += dy;
        } else {
            horizontal += dx;
        }
    }
    (vertical, horizontal)
}

fn connection_runtime_attrs(conn: &Connection) -> String {
    format!(
        " data-wdoc-conn-from=\"{}\" data-wdoc-conn-to=\"{}\" data-wdoc-conn-curve=\"{}\" data-wdoc-conn-from-anchor=\"{}\" data-wdoc-conn-to-anchor=\"{}\"",
        svg_escape_attr(&conn.from_id),
        svg_escape_attr(&conn.to_id),
        match conn.curve {
            CurveStyle::Bezier => "bezier",
            CurveStyle::Straight => "straight",
        },
        anchor_name(conn.from_anchor),
        anchor_name(conn.to_anchor),
    )
}

fn anchor_name(anchor: AnchorPoint) -> &'static str {
    match anchor {
        AnchorPoint::Top => "top",
        AnchorPoint::Bottom => "bottom",
        AnchorPoint::Left => "left",
        AnchorPoint::Right => "right",
        AnchorPoint::Center => "center",
        AnchorPoint::Auto => "auto",
    }
}

fn connection_uses_direct_route(conn: &Connection) -> bool {
    conn.attrs
        .get(CONNECTION_ROUTE_ATTR)
        .map(|route| route == CONNECTION_ROUTE_DIRECT)
        .unwrap_or(false)
}

fn effective_endpoint_anchor(
    id: &str,
    bounds: &Bounds,
    anchor: AnchorPoint,
    shape_map: &HashMap<String, Bounds>,
) -> AnchorPoint {
    if anchor != AnchorPoint::Auto {
        return anchor;
    }

    let Some(container) = nearest_container_id(id, shape_map).and_then(|id| shape_map.get(id))
    else {
        return AnchorPoint::Auto;
    };

    let center_x = bounds.x + bounds.width / 2.0;
    let center_y = bounds.y + bounds.height / 2.0;
    let distances = [
        (AnchorPoint::Left, (center_x - container.x).abs()),
        (
            AnchorPoint::Right,
            (center_x - (container.x + container.width)).abs(),
        ),
        (AnchorPoint::Top, (center_y - container.y).abs()),
        (
            AnchorPoint::Bottom,
            (center_y - (container.y + container.height)).abs(),
        ),
    ];
    distances
        .into_iter()
        .filter(|(_, distance)| *distance <= ROUTE_TERMINAL_MIN)
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(anchor, _)| anchor)
        .unwrap_or(AnchorPoint::Auto)
}

fn route_orthogonal(from: &Bounds, to: &Bounds, obstacles: &[Bounds]) -> Option<Vec<(f64, f64)>> {
    let start = from.anchor_pos(AnchorPoint::Auto, to);
    let end = to.anchor_pos(AnchorPoint::Auto, from);
    if direct_auto_line_is_clean(start, end, obstacles) {
        return None;
    }

    route_orthogonal_candidates(from, to, obstacles)
        .into_iter()
        .filter(|points| {
            !path_intersects_obstacle(points, obstacles)
                && route_exits_endpoint_bounds(points, from, to)
                && route_avoids_endpoint_reentry(points, from, to)
                && route_has_visible_terminal_segments(points, ROUTE_TERMINAL_MIN)
        })
        .min_by(|a, b| {
            route_score(a)
                .partial_cmp(&route_score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn route_orthogonal_best_effort(
    from: &Bounds,
    to: &Bounds,
    obstacles: &[Bounds],
) -> Option<Vec<(f64, f64)>> {
    route_orthogonal_candidates(from, to, obstacles)
        .into_iter()
        .filter(|points| {
            route_exits_endpoint_bounds(points, from, to)
                && route_avoids_endpoint_reentry(points, from, to)
                && route_has_visible_terminal_segments(points, ROUTE_TERMINAL_MIN)
        })
        .min_by(|a, b| {
            route_score_with_obstacles(a, obstacles)
                .partial_cmp(&route_score_with_obstacles(b, obstacles))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn route_orthogonal_candidates(
    from: &Bounds,
    to: &Bounds,
    obstacles: &[Bounds],
) -> Vec<Vec<(f64, f64)>> {
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
        candidates.push(build_hv_route_with_escape(from, to, mid_x));
    }
    for mid_y in y_lanes {
        candidates.push(build_vh_route(from, to, mid_y));
        candidates.push(build_vh_route_with_escape(from, to, mid_y));
    }

    candidates
}

fn route_orthogonal_anchored(
    from: &Bounds,
    to: &Bounds,
    obstacles: &[Bounds],
    from_anchor: AnchorPoint,
    to_anchor: AnchorPoint,
) -> Option<Vec<(f64, f64)>> {
    let start = from.anchor_pos(from_anchor, to);
    let end = to.anchor_pos(to_anchor, from);
    if direct_auto_line_is_clean(start, end, obstacles) {
        return None;
    }

    if let Some(points) =
        route_orthogonal_anchored_grid(from, to, obstacles, from_anchor, to_anchor)
    {
        return Some(points);
    }

    route_orthogonal_anchored_best_effort(from, to, obstacles, from_anchor, to_anchor).filter(
        |points| {
            !path_intersects_obstacle(points, obstacles)
                && route_exits_endpoint_bounds(points, from, to)
                && route_avoids_endpoint_reentry(points, from, to)
                && route_has_visible_terminal_segments(points, ROUTE_TERMINAL_MIN)
        },
    )
}

fn route_orthogonal_anchored_best_effort(
    from: &Bounds,
    to: &Bounds,
    obstacles: &[Bounds],
    from_anchor: AnchorPoint,
    to_anchor: AnchorPoint,
) -> Option<Vec<(f64, f64)>> {
    let start = from.anchor_pos(from_anchor, to);
    let end = to.anchor_pos(to_anchor, from);
    let start_exit = anchor_escape_point(from, from_anchor, start, ROUTE_TERMINAL_MIN);
    let end_exit = anchor_escape_point(to, to_anchor, end, ROUTE_TERMINAL_MIN);

    let mut candidates = vec![
        simplify_points(vec![start, (start.0, end.1), end]),
        simplify_points(vec![start, (end.0, start.1), end]),
        simplify_points(vec![
            start,
            start_exit,
            (start_exit.0, end_exit.1),
            end_exit,
            end,
        ]),
        simplify_points(vec![
            start,
            start_exit,
            (end_exit.0, start_exit.1),
            end_exit,
            end,
        ]),
    ];

    for mid_x in route_x_lanes(from, to, ROUTE_MARGIN) {
        candidates.push(simplify_points(vec![
            start,
            (mid_x, start.1),
            (mid_x, end.1),
            end,
        ]));
        candidates.push(simplify_points(vec![
            start,
            start_exit,
            (mid_x, start_exit.1),
            (mid_x, end_exit.1),
            end_exit,
            end,
        ]));
    }
    for mid_y in route_y_lanes(from, to, ROUTE_MARGIN) {
        candidates.push(simplify_points(vec![
            start,
            (start.0, mid_y),
            (end.0, mid_y),
            end,
        ]));
        candidates.push(simplify_points(vec![
            start,
            start_exit,
            (start_exit.0, mid_y),
            (end_exit.0, mid_y),
            end_exit,
            end,
        ]));
    }
    for b in obstacles {
        candidates.push(simplify_points(vec![
            start,
            (b.x - ROUTE_MARGIN, start.1),
            (b.x - ROUTE_MARGIN, end.1),
            end,
        ]));
        candidates.push(simplify_points(vec![
            start,
            (b.x + b.width + ROUTE_MARGIN, start.1),
            (b.x + b.width + ROUTE_MARGIN, end.1),
            end,
        ]));
        candidates.push(simplify_points(vec![
            start,
            (start.0, b.y - ROUTE_MARGIN),
            (end.0, b.y - ROUTE_MARGIN),
            end,
        ]));
        candidates.push(simplify_points(vec![
            start,
            (start.0, b.y + b.height + ROUTE_MARGIN),
            (end.0, b.y + b.height + ROUTE_MARGIN),
            end,
        ]));
    }

    candidates
        .into_iter()
        .filter(|points| {
            route_exits_endpoint_bounds(points, from, to)
                && route_avoids_endpoint_reentry(points, from, to)
                && route_has_visible_terminal_segments(points, ROUTE_TERMINAL_MIN)
        })
        .min_by(|a, b| {
            route_score_with_obstacles(a, obstacles)
                .partial_cmp(&route_score_with_obstacles(b, obstacles))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn route_orthogonal_anchored_grid(
    from: &Bounds,
    to: &Bounds,
    obstacles: &[Bounds],
    from_anchor: AnchorPoint,
    to_anchor: AnchorPoint,
) -> Option<Vec<(f64, f64)>> {
    let start = from.anchor_pos(from_anchor, to);
    let end = to.anchor_pos(to_anchor, from);
    let start_exit = anchor_escape_point(from, from_anchor, start, ROUTE_TERMINAL_MIN);
    let end_exit = anchor_escape_point(to, to_anchor, end, ROUTE_TERMINAL_MIN);

    if path_intersects_obstacle(&[start, start_exit], obstacles)
        || path_intersects_obstacle(&[end_exit, end], obstacles)
    {
        return None;
    }

    let mut xs = vec![start_exit.0, end_exit.0, start.0, end.0];
    let mut ys = vec![start_exit.1, end_exit.1, start.1, end.1];
    for x in route_x_lanes(from, to, ROUTE_MARGIN) {
        xs.push(x);
    }
    for y in route_y_lanes(from, to, ROUTE_MARGIN) {
        ys.push(y);
    }
    for b in obstacles {
        xs.push(b.x - ROUTE_MARGIN);
        xs.push(b.x + b.width + ROUTE_MARGIN);
        ys.push(b.y - ROUTE_MARGIN);
        ys.push(b.y + b.height + ROUTE_MARGIN);
    }
    sort_dedup_f64(&mut xs);
    sort_dedup_f64(&mut ys);

    let start_idx = grid_point_index(&xs, &ys, start_exit)?;
    let end_idx = grid_point_index(&xs, &ys, end_exit)?;
    let node_count = xs.len() * ys.len();
    let mut dist = vec![f64::INFINITY; node_count];
    let mut prev: Vec<Option<usize>> = vec![None; node_count];
    let mut used = vec![false; node_count];
    dist[start_idx] = 0.0;

    for _ in 0..node_count {
        let Some(current) = (0..node_count)
            .filter(|&idx| !used[idx] && dist[idx].is_finite())
            .min_by(|&a, &b| {
                dist[a]
                    .partial_cmp(&dist[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            break;
        };
        if current == end_idx {
            break;
        }
        used[current] = true;

        let xi = current % xs.len();
        let yi = current / xs.len();
        for (nx, ny) in grid_neighbors(xi, yi, xs.len(), ys.len()) {
            let next = ny * xs.len() + nx;
            if used[next] {
                continue;
            }
            let a = (xs[xi], ys[yi]);
            let b = (xs[nx], ys[ny]);
            if path_intersects_obstacle(&[a, b], obstacles) {
                continue;
            }
            let bend = prev[current]
                .map(|previous| {
                    let py = previous / xs.len();
                    let was_horizontal = py == yi;
                    let is_horizontal = ny == yi;
                    if was_horizontal == is_horizontal {
                        0.0
                    } else {
                        20.0
                    }
                })
                .unwrap_or(0.0);
            let candidate = dist[current] + segment_length(a, b) + bend;
            if candidate < dist[next] {
                dist[next] = candidate;
                prev[next] = Some(current);
            }
        }
    }

    if !dist[end_idx].is_finite() {
        return None;
    }

    let mut grid_path = Vec::new();
    let mut cursor = end_idx;
    grid_path.push((xs[cursor % xs.len()], ys[cursor / xs.len()]));
    while cursor != start_idx {
        cursor = prev[cursor]?;
        grid_path.push((xs[cursor % xs.len()], ys[cursor / xs.len()]));
    }
    grid_path.reverse();

    let mut points = vec![start, start_exit];
    points.extend(grid_path.into_iter().skip(1));
    points.push(end);
    let points = simplify_collinear_points(points);
    if points.len() >= 2
        && !path_intersects_obstacle(&points, obstacles)
        && route_exits_endpoint_bounds(&points, from, to)
        && route_avoids_endpoint_reentry(&points, from, to)
        && route_has_visible_terminal_segments(&points, ROUTE_TERMINAL_MIN)
    {
        Some(points)
    } else {
        None
    }
}

fn sort_dedup_f64(values: &mut Vec<f64>) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.dedup_by(|a, b| (*a - *b).abs() < 0.001);
}

fn grid_point_index(xs: &[f64], ys: &[f64], point: (f64, f64)) -> Option<usize> {
    let xi = xs.iter().position(|x| (*x - point.0).abs() < 0.001)?;
    let yi = ys.iter().position(|y| (*y - point.1).abs() < 0.001)?;
    Some(yi * xs.len() + xi)
}

fn grid_neighbors(x: usize, y: usize, width: usize, height: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(4);
    if x > 0 {
        out.push((x - 1, y));
    }
    if x + 1 < width {
        out.push((x + 1, y));
    }
    if y > 0 {
        out.push((x, y - 1));
    }
    if y + 1 < height {
        out.push((x, y + 1));
    }
    out
}

fn anchor_escape_point(
    bounds: &Bounds,
    anchor: AnchorPoint,
    point: (f64, f64),
    margin: f64,
) -> (f64, f64) {
    match anchor {
        AnchorPoint::Top => (point.0, bounds.y - margin),
        AnchorPoint::Bottom => (point.0, bounds.y + bounds.height + margin),
        AnchorPoint::Left => (bounds.x - margin, point.1),
        AnchorPoint::Right => (bounds.x + bounds.width + margin, point.1),
        AnchorPoint::Center | AnchorPoint::Auto => point,
    }
}

fn connection_obstacles(
    from_id: &str,
    to_id: &str,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    shape_map: &HashMap<String, Bounds>,
) -> Vec<Bounds> {
    let from_ancestors = id_ancestors(from_id);
    let to_ancestors = id_ancestors(to_id);
    shape_map
        .iter()
        .filter(|(id, b)| {
            let id_str = id.as_str();
            if id_str == from_id || id_str == to_id {
                return false;
            }
            if is_strict_ancestor(id_str, &from_ancestors)
                || is_strict_ancestor(id_str, &to_ancestors)
                || is_descendant_of_endpoint(id_str, from_id)
                || is_descendant_of_endpoint(id_str, to_id)
            {
                return false;
            }
            !bounds_contains_point(b, x1, y1) && !bounds_contains_point(b, x2, y2)
        })
        .map(|(_, b)| *b)
        .collect()
}

/// Returns `["a.b.c", "a.b", "a"]` for input `"a.b.c"` — the endpoint id and
/// each ancestor container id derived from the dotted path.
fn id_ancestors(id: &str) -> Vec<&str> {
    let mut out = vec![id];
    let mut cursor = id;
    while let Some(idx) = cursor.rfind('.') {
        cursor = &cursor[..idx];
        out.push(cursor);
    }
    out
}

/// Finds the nearest ancestor container of `id` in `shape_map` — that is, the
/// closest enclosing dotted-path prefix that exists as its own shape. Returns
/// `None` for top-level ids or ids whose ancestors aren't tracked.
fn nearest_container_id<'a>(id: &'a str, shape_map: &HashMap<String, Bounds>) -> Option<&'a str> {
    let mut cursor = id;
    while let Some(idx) = cursor.rfind('.') {
        cursor = &cursor[..idx];
        if shape_map.contains_key(cursor) {
            return Some(cursor);
        }
    }
    None
}

/// Projects an inner endpoint onto its container's anchored edge, so the
/// stub leaving the inner node aligns with the inner node's center axis.
fn project_anchor_through(inner: &Bounds, container: &Bounds, anchor: AnchorPoint) -> (f64, f64) {
    match anchor {
        AnchorPoint::Top => (inner.x + inner.width / 2.0, container.y),
        AnchorPoint::Bottom => (inner.x + inner.width / 2.0, container.y + container.height),
        AnchorPoint::Left => (container.x, inner.y + inner.height / 2.0),
        AnchorPoint::Right => (container.x + container.width, inner.y + inner.height / 2.0),
        AnchorPoint::Center | AnchorPoint::Auto => inner.anchor_pos(anchor, container),
    }
}

fn anchor_axis_is_vertical(anchor: AnchorPoint) -> bool {
    matches!(anchor, AnchorPoint::Top | AnchorPoint::Bottom)
}

fn anchor_axis_is_horizontal(anchor: AnchorPoint) -> bool {
    matches!(anchor, AnchorPoint::Left | AnchorPoint::Right)
}

/// Build an orthogonal route between two cross-container anchored points,
/// connecting through an inter-container channel that avoids `obstacles`.
fn route_cross_container(
    from_inner: &Bounds,
    from_container: &Bounds,
    from_anchor: AnchorPoint,
    to_inner: &Bounds,
    to_container: &Bounds,
    to_anchor: AnchorPoint,
    obstacles: &[Bounds],
) -> Vec<(f64, f64)> {
    let inner_start = from_inner.anchor_pos(from_anchor, to_container);
    let inner_end = to_inner.anchor_pos(to_anchor, from_container);
    let exit = project_anchor_through(from_inner, from_container, from_anchor);
    let entry = project_anchor_through(to_inner, to_container, to_anchor);

    let mut points: Vec<(f64, f64)> = Vec::new();
    points.push(inner_start);
    if exit != inner_start {
        points.push(exit);
    }

    let from_v = anchor_axis_is_vertical(from_anchor);
    let to_v = anchor_axis_is_vertical(to_anchor);
    let from_h = anchor_axis_is_horizontal(from_anchor);
    let to_h = anchor_axis_is_horizontal(to_anchor);

    if from_v && to_v {
        let containers_overlap_y = from_container.y < to_container.y + to_container.height
            && to_container.y < from_container.y + from_container.height;
        if containers_overlap_y {
            // Containers are arranged horizontally. Route through the vertical
            // gap between them: exit → (gap_x, exit.y) → (gap_x, entry.y) → entry.
            let gap_x = inter_container_lane_x(from_container, to_container, obstacles);
            points.push((gap_x, exit.1));
            points.push((gap_x, entry.1));
        } else {
            // Containers stacked vertically — direct mid-y channel works.
            let mid_y = pick_mid_lane(exit.1, entry.1, true, obstacles);
            points.push((exit.0, mid_y));
            points.push((entry.0, mid_y));
        }
    } else if from_h && to_h {
        let containers_overlap_x = from_container.x < to_container.x + to_container.width
            && to_container.x < from_container.x + from_container.width;
        if containers_overlap_x {
            let gap_y = inter_container_lane_y(from_container, to_container, obstacles);
            points.push((exit.0, gap_y));
            points.push((entry.0, gap_y));
        } else {
            let mid_x = pick_mid_lane(exit.0, entry.0, false, obstacles);
            points.push((mid_x, exit.1));
            points.push((mid_x, entry.1));
        }
    } else if from_v {
        points.push((entry.0, exit.1));
    } else if from_h {
        points.push((exit.0, entry.1));
    }

    if entry != inner_end {
        points.push(entry);
    }
    points.push(inner_end);
    simplify_points(points)
}

/// Pick a clean x-coordinate in the inter-container vertical gap. Falls back
/// to a midpoint between the two container x-extents if there's no gap.
fn inter_container_lane_x(a: &Bounds, b: &Bounds, obstacles: &[Bounds]) -> f64 {
    let (left, right) = if a.x + a.width <= b.x {
        (a.x + a.width, b.x)
    } else if b.x + b.width <= a.x {
        (b.x + b.width, a.x)
    } else {
        // Containers overlap on x too — fall back to midpoint of nearest edges.
        let candidates = [(a.x + a.width).min(b.x + b.width), a.x.max(b.x)];
        return (candidates[0] + candidates[1]) / 2.0;
    };
    let mid = (left + right) / 2.0;
    let intersects = |lane: f64| {
        obstacles
            .iter()
            .any(|o| lane > o.x - ROUTE_MARGIN && lane < o.x + o.width + ROUTE_MARGIN)
    };
    if !intersects(mid) {
        return mid;
    }
    let mut best = mid;
    let mut best_dist = f64::MAX;
    let mut step = 1.0;
    while left + step < right - step {
        for cand in [mid - step, mid + step] {
            if cand > left && cand < right && !intersects(cand) {
                let d = (cand - mid).abs();
                if d < best_dist {
                    best = cand;
                    best_dist = d;
                }
            }
        }
        if best_dist < f64::MAX {
            break;
        }
        step *= 2.0;
    }
    best
}

/// Pick a clean y-coordinate in the inter-container horizontal gap.
fn inter_container_lane_y(a: &Bounds, b: &Bounds, obstacles: &[Bounds]) -> f64 {
    let (top, bottom) = if a.y + a.height <= b.y {
        (a.y + a.height, b.y)
    } else if b.y + b.height <= a.y {
        (b.y + b.height, a.y)
    } else {
        let candidates = [(a.y + a.height).min(b.y + b.height), a.y.max(b.y)];
        return (candidates[0] + candidates[1]) / 2.0;
    };
    let mid = (top + bottom) / 2.0;
    let intersects = |lane: f64| {
        obstacles
            .iter()
            .any(|o| lane > o.y - ROUTE_MARGIN && lane < o.y + o.height + ROUTE_MARGIN)
    };
    if !intersects(mid) {
        return mid;
    }
    let mut best = mid;
    let mut best_dist = f64::MAX;
    let mut step = 1.0;
    while top + step < bottom - step {
        for cand in [mid - step, mid + step] {
            if cand > top && cand < bottom && !intersects(cand) {
                let d = (cand - mid).abs();
                if d < best_dist {
                    best = cand;
                    best_dist = d;
                }
            }
        }
        if best_dist < f64::MAX {
            break;
        }
        step *= 2.0;
    }
    best
}

/// Picks the midpoint of an inter-container channel, nudged off obstacle edges
/// when the naive midpoint would clip an obstacle. `vertical` selects which
/// axis to scan along.
fn pick_mid_lane(a: f64, b: f64, vertical: bool, obstacles: &[Bounds]) -> f64 {
    let mid = (a + b) / 2.0;
    let intersects = |lane: f64| {
        obstacles.iter().any(|o| {
            if vertical {
                lane > o.y - ROUTE_MARGIN && lane < o.y + o.height + ROUTE_MARGIN
            } else {
                lane > o.x - ROUTE_MARGIN && lane < o.x + o.width + ROUTE_MARGIN
            }
        })
    };
    if !intersects(mid) {
        return mid;
    }
    let lo = a.min(b);
    let hi = a.max(b);
    let mut candidates: Vec<f64> = vec![mid];
    for o in obstacles {
        if vertical {
            candidates.push(o.y - ROUTE_MARGIN);
            candidates.push(o.y + o.height + ROUTE_MARGIN);
        } else {
            candidates.push(o.x - ROUTE_MARGIN);
            candidates.push(o.x + o.width + ROUTE_MARGIN);
        }
    }
    candidates
        .into_iter()
        .filter(|lane| *lane >= lo && *lane <= hi && !intersects(*lane))
        .min_by(|x, y| {
            ((x - mid).abs())
                .partial_cmp(&(y - mid).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(mid)
}

fn is_strict_ancestor(id: &str, ancestors: &[&str]) -> bool {
    ancestors.iter().skip(1).any(|ancestor| id == *ancestor)
}

fn is_descendant_of_endpoint(id: &str, endpoint: &str) -> bool {
    id.len() > endpoint.len() && id.starts_with(endpoint) && id.as_bytes()[endpoint.len()] == b'.'
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
    let aligned = (start.0 - end.0).abs() <= DIRECT_ROUTE_ALIGNMENT_EPSILON
        || (start.1 - end.1).abs() <= DIRECT_ROUTE_ALIGNMENT_EPSILON;
    aligned && !path_intersects_obstacle(&[start, end], obstacles)
}

fn build_hv_route(from: &Bounds, to: &Bounds, mid_x: f64) -> Vec<(f64, f64)> {
    let from_anchor = horizontal_anchor_for_lane(from, mid_x);
    let to_anchor = horizontal_anchor_for_lane(to, mid_x);
    let start = from.anchor_pos(from_anchor, to);
    let end = to.anchor_pos(to_anchor, from);
    simplify_points(vec![start, (mid_x, start.1), (mid_x, end.1), end])
}

fn build_hv_route_with_escape(from: &Bounds, to: &Bounds, mid_x: f64) -> Vec<(f64, f64)> {
    let from_anchor = horizontal_anchor_for_lane(from, mid_x);
    let to_anchor = horizontal_anchor_for_lane(to, mid_x);
    let start = from.anchor_pos(from_anchor, to);
    let end = to.anchor_pos(to_anchor, from);
    let start_exit = anchor_escape_point(from, from_anchor, start, ROUTE_TERMINAL_MIN);
    let end_exit = anchor_escape_point(to, to_anchor, end, ROUTE_TERMINAL_MIN);
    simplify_points(vec![
        start,
        start_exit,
        (mid_x, start_exit.1),
        (mid_x, end_exit.1),
        end_exit,
        end,
    ])
}

fn build_vh_route(from: &Bounds, to: &Bounds, mid_y: f64) -> Vec<(f64, f64)> {
    let from_anchor = vertical_anchor_for_lane(from, mid_y);
    let to_anchor = vertical_anchor_for_lane(to, mid_y);
    let start = from.anchor_pos(from_anchor, to);
    let end = to.anchor_pos(to_anchor, from);
    simplify_points(vec![start, (start.0, mid_y), (end.0, mid_y), end])
}

fn build_vh_route_with_escape(from: &Bounds, to: &Bounds, mid_y: f64) -> Vec<(f64, f64)> {
    let from_anchor = vertical_anchor_for_lane(from, mid_y);
    let to_anchor = vertical_anchor_for_lane(to, mid_y);
    let start = from.anchor_pos(from_anchor, to);
    let end = to.anchor_pos(to_anchor, from);
    let start_exit = anchor_escape_point(from, from_anchor, start, ROUTE_TERMINAL_MIN);
    let end_exit = anchor_escape_point(to, to_anchor, end, ROUTE_TERMINAL_MIN);
    simplify_points(vec![
        start,
        start_exit,
        (start_exit.0, mid_y),
        (end_exit.0, mid_y),
        end_exit,
        end,
    ])
}

fn horizontal_anchor_for_lane(bounds: &Bounds, x: f64) -> AnchorPoint {
    if x <= bounds.x {
        AnchorPoint::Left
    } else {
        AnchorPoint::Right
    }
}

fn vertical_anchor_for_lane(bounds: &Bounds, y: f64) -> AnchorPoint {
    if y <= bounds.y {
        AnchorPoint::Top
    } else {
        AnchorPoint::Bottom
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

fn simplify_collinear_points(points: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut simplified = Vec::new();
    for point in simplify_points(points) {
        simplified.push(point);
        while simplified.len() >= 3 {
            let len = simplified.len();
            let a = simplified[len - 3];
            let b = simplified[len - 2];
            let c = simplified[len - 1];
            let collinear_x = (a.0 - b.0).abs() < 0.001 && (b.0 - c.0).abs() < 0.001;
            let collinear_y = (a.1 - b.1).abs() < 0.001 && (b.1 - c.1).abs() < 0.001;
            if collinear_x || collinear_y {
                simplified.remove(len - 2);
            } else {
                break;
            }
        }
    }
    simplified
}

fn route_score(points: &[(f64, f64)]) -> f64 {
    let length: f64 = points
        .windows(2)
        .map(|segment| segment_length(segment[0], segment[1]))
        .sum();
    let bends = points.len().saturating_sub(2) as f64;
    let terminal_penalty = endpoint_clearance_penalty(points) * 200.0;
    length + bends * 20.0 + terminal_penalty
}

fn route_score_with_obstacles(points: &[(f64, f64)], obstacles: &[Bounds]) -> f64 {
    let intersections = obstacle_intersection_count(points, obstacles) as f64;
    route_score(points) + intersections * 10_000.0
}

fn obstacle_intersection_count(points: &[(f64, f64)], obstacles: &[Bounds]) -> usize {
    points
        .windows(2)
        .map(|segment| {
            obstacles
                .iter()
                .filter(|b| segment_intersects_bounds(segment[0], segment[1], b))
                .count()
        })
        .sum()
}

fn endpoint_clearance_penalty(points: &[(f64, f64)]) -> f64 {
    if points.len() < 4 {
        return 0.0;
    }

    let first = segment_length(points[0], points[1]);
    let last = segment_length(points[points.len() - 2], points[points.len() - 1]);
    (ROUTE_TERMINAL_MIN - first).max(0.0) + (ROUTE_TERMINAL_MIN - last).max(0.0)
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

fn route_avoids_endpoint_reentry(points: &[(f64, f64)], from: &Bounds, to: &Bounds) -> bool {
    if points.len() < 3 {
        return true;
    }
    let from_inner = inset_bounds(from, 2.0);
    let to_inner = inset_bounds(to, 2.0);
    for (idx, segment) in points.windows(2).enumerate() {
        if idx > 0 && segment_intersects_bounds(segment[0], segment[1], &from_inner) {
            return false;
        }
        if idx + 2 < points.len() && segment_intersects_bounds(segment[0], segment[1], &to_inner) {
            return false;
        }
    }
    true
}

fn inset_bounds(bounds: &Bounds, amount: f64) -> Bounds {
    Bounds {
        x: bounds.x + amount,
        y: bounds.y + amount,
        width: (bounds.width - amount * 2.0).max(0.0),
        height: (bounds.height - amount * 2.0).max(0.0),
    }
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

fn bounds_eq(a: &Bounds, b: &Bounds) -> bool {
    nearly_eq(a.x, b.x)
        && nearly_eq(a.y, b.y)
        && nearly_eq(a.width, b.width)
        && nearly_eq(a.height, b.height)
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
        "vector_effect",
        "stroke_dasharray",
        "opacity",
        "visible",
        "visibility",
        "display",
        "class",
        "style",
        "cursor",
        "pointer_events",
        "font_family",
        "font_weight",
        "font_style",
        "text_decoration",
        "letter_spacing",
    ] {
        if let Some(val) = attrs.get(*name) {
            let (svg_name, attr_value) = svg_render_attr_name_value(name, val);
            let escaped = svg_escape_attr(&attr_value);
            write!(s, " {svg_name}=\"{escaped}\"").unwrap();
        }
    }
    for (name, val) in attrs {
        if name.starts_with("data_wdoc_") || name.starts_with("data-wdoc-") {
            let svg_name = name.replace('_', "-");
            let escaped = svg_escape_attr(val);
            write!(s, " {svg_name}=\"{escaped}\"").unwrap();
        }
    }
    s
}

fn svg_node_attrs(node: &ShapeNode, attrs: &IndexMap<String, String>) -> String {
    let mut s = svg_style_attrs(attrs);
    append_shape_runtime_attrs(node, &mut s);
    s
}

fn svg_image_attrs(attrs: &IndexMap<String, String>) -> String {
    let mut s = String::new();
    for name in &[
        "opacity",
        "visible",
        "visibility",
        "display",
        "class",
        "style",
        "cursor",
        "pointer_events",
    ] {
        if let Some(val) = attrs.get(*name) {
            let (svg_name, attr_value) = svg_render_attr_name_value(name, val);
            let escaped = svg_escape_attr(&attr_value);
            write!(s, " {svg_name}=\"{escaped}\"").unwrap();
        }
    }
    s
}

fn svg_render_attr_name_value(name: &str, value: &str) -> (String, String) {
    match name {
        "visible" => (
            "visibility".to_string(),
            if value == "false" {
                "hidden"
            } else {
                "visible"
            }
            .to_string(),
        ),
        _ => (name.replace('_', "-"), value.to_string()),
    }
}

fn svg_image_node_attrs(node: &ShapeNode, attrs: &IndexMap<String, String>) -> String {
    let mut s = svg_image_attrs(attrs);
    append_shape_runtime_attrs(node, &mut s);
    s
}

fn append_shape_runtime_attrs(node: &ShapeNode, out: &mut String) {
    let Some(id) = node.id.as_deref() else {
        return;
    };
    if node.events.is_empty()
        && !node.attrs.contains_key("_wdoc_state_z")
        && node.attrs.get("_wdoc_runtime").map(|v| v == "true") != Some(true)
        && !node.attrs.contains_key("data_wdoc_slider")
        && !node.attrs.contains_key("data-wdoc-slider")
        && node
            .attrs
            .get("draggable")
            .or_else(|| node.attrs.get("data-wdoc-draggable"))
            .map(|value| value == "true")
            != Some(true)
        && node
            .attrs
            .get("data_wdoc_textbox")
            .or_else(|| node.attrs.get("data-wdoc-textbox"))
            .map(|value| value == "true")
            != Some(true)
        && node
            .attrs
            .get("data_wdoc_checkbox")
            .or_else(|| node.attrs.get("data-wdoc-checkbox"))
            .map(|value| value == "true")
            != Some(true)
        && node
            .attrs
            .get("data_wdoc_radio")
            .or_else(|| node.attrs.get("data-wdoc-radio"))
            .map(|value| value == "true")
            != Some(true)
        && node
            .attrs
            .get("data_wdoc_dropdown")
            .or_else(|| node.attrs.get("data-wdoc-dropdown"))
            .map(|value| value == "true")
            != Some(true)
    {
        return;
    }
    let id = svg_escape_attr(id);
    write!(
        out,
        " data-wdoc-id=\"{id}\" data-wdoc-z-base=\"{}\"",
        node.z_index
    )
    .unwrap();
    if !node.events.is_empty() {
        let events = diagram_events_json(&node.events);
        write!(out, " data-wdoc-events=\"{}\"", svg_escape_attr(&events)).unwrap();
    }
    let state_z = state_z_json_for_shape(node);
    if !state_z.is_empty() {
        write!(out, " data-wdoc-state-z=\"{}\"", svg_escape_attr(&state_z)).unwrap();
    }
    if let Some(state_animation) = node.attrs.get("_wdoc_state_animation") {
        write!(
            out,
            " data-wdoc-state-animation=\"{}\"",
            svg_escape_attr(state_animation)
        )
        .unwrap();
    }
    if let Some(animations) = node.attrs.get("_wdoc_animations") {
        write!(
            out,
            " data-wdoc-animations=\"{}\"",
            svg_escape_attr(animations)
        )
        .unwrap();
    }
    if node
        .attrs
        .get("draggable")
        .or_else(|| node.attrs.get("data-wdoc-draggable"))
        .is_some_and(|value| value == "true")
    {
        out.push_str(" data-wdoc-draggable=\"true\"");
        for (attr, data_attr) in [
            ("drag_signal", "data-wdoc-drag-signal"),
            ("drag_x_path", "data-wdoc-drag-x-path"),
            ("drag_y_path", "data-wdoc-drag-y-path"),
        ] {
            if let Some(value) = node.attrs.get(attr) {
                write!(out, " {data_attr}=\"{}\"", svg_escape_attr(value)).unwrap();
            }
        }
    }
    if bool_attr(&node.attrs, "map_fixed") {
        out.push_str(" data-wdoc-map-fixed=\"true\"");
        let anchor_x = attr_f64(&node.attrs, "map_anchor_x")
            .unwrap_or(node.resolved.x + node.resolved.width / 2.0);
        let anchor_y = attr_f64(&node.attrs, "map_anchor_y")
            .unwrap_or(node.resolved.y + node.resolved.height / 2.0);
        write!(
            out,
            " data-wdoc-map-anchor-x=\"{}\" data-wdoc-map-anchor-y=\"{}\"",
            anchor_x, anchor_y
        )
        .unwrap();
    }
    write!(
        out,
        " data-wdoc-x=\"{}\" data-wdoc-y=\"{}\" data-wdoc-width=\"{}\" data-wdoc-height=\"{}\"",
        node.resolved.x, node.resolved.y, node.resolved.width, node.resolved.height
    )
    .unwrap();
}

fn state_z_json_for_shape(node: &ShapeNode) -> String {
    node.attrs.get("_wdoc_state_z").cloned().unwrap_or_default()
}

fn diagram_events_json(events: &[DiagramEvent]) -> String {
    events
        .iter()
        .map(|event| {
            let duration_ms = event.duration_ms.unwrap_or(0).to_string();
            let mut fields = vec![
                event.trigger.as_str(),
                event.state.as_str(),
                event.target.as_deref().unwrap_or("self"),
                event.mode.as_deref().unwrap_or(""),
                event.button.as_deref().unwrap_or("left"),
                duration_ms.as_str(),
                if event
                    .prevent_default
                    .unwrap_or(event.trigger == "right_click")
                {
                    "true"
                } else {
                    "false"
                },
            ];
            if let Some(guard_targets) = event.guard_targets.as_deref() {
                fields.push(guard_targets);
            } else if !event.signal_actions.is_empty() {
                fields.push("");
            }
            let actions_json;
            if !event.signal_actions.is_empty() {
                actions_json = signal_actions_data(&event.signal_actions);
                fields.push(actions_json.as_str());
            }
            fields
                .into_iter()
                .map(runtime_field_escape)
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn signal_actions_data(actions: &[SignalAction]) -> String {
    let values = actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "signal": action.signal,
                "value": action.value,
                "path": action.path,
            })
        })
        .collect::<Vec<_>>();
    serde_json::Value::Array(values).to_string()
}

fn diagram_animations_data(animations: &IndexMap<String, DiagramAnimation>) -> String {
    animations
        .values()
        .map(|animation| {
            let duration_ms = animation.duration_ms.to_string();
            let delay_ms = animation.delay_ms.to_string();
            let keyframes = animation
                .keyframes
                .iter()
                .map(|frame| {
                    [
                        frame.offset.to_string(),
                        frame.x.map(|v| v.to_string()).unwrap_or_default(),
                        frame.y.map(|v| v.to_string()).unwrap_or_default(),
                        frame.width.map(|v| v.to_string()).unwrap_or_default(),
                        frame.height.map(|v| v.to_string()).unwrap_or_default(),
                        frame.rotate.map(|v| v.to_string()).unwrap_or_default(),
                        frame
                            .rotate_origin_x
                            .map(|v| v.to_string())
                            .unwrap_or_default(),
                        frame
                            .rotate_origin_y
                            .map(|v| v.to_string())
                            .unwrap_or_default(),
                    ]
                    .join(",")
                })
                .collect::<Vec<_>>()
                .join("~");
            let frame_rate = animation
                .frame_rate
                .map(|v| v.to_string())
                .unwrap_or_default();
            let frames = animation
                .frames
                .iter()
                .map(|frame| frame.to_string())
                .collect::<Vec<_>>()
                .join(",");
            [
                animation.name.as_str(),
                duration_ms.as_str(),
                delay_ms.as_str(),
                animation.timing_function.as_str(),
                animation.iteration_count.as_str(),
                animation.direction.as_str(),
                animation.fill_mode.as_str(),
                keyframes.as_str(),
                frame_rate.as_str(),
                frames.as_str(),
            ]
            .into_iter()
            .map(runtime_field_escape)
            .collect::<Vec<_>>()
            .join("|")
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn runtime_field_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\p")
        .replace(';', "\\s")
}

fn diagram_runtime_js() -> String {
    r#"(function(){
if(window.__wdocDiagramRuntimeInit){var s0=document.currentScript;if(s0&&s0.parentNode)window.__wdocDiagramRuntimeInit(s0.parentNode);return;}
function unesc(s){return (s||'').replace(/\\s/g,';').replace(/\\p/g,'|').replace(/\\\\/g,'\\');}
function parseEvents(el){return (el.getAttribute('data-wdoc-events')||'').split(';').filter(Boolean).map(function(row){var p=row.split('|').map(unesc),actions=[];try{actions=p[8]?JSON.parse(p[8]):[];}catch(e){}return{trigger:p[0],state:p[1],target:p[2]||'self',mode:p[3],button:p[4]||'left',duration:parseInt(p[5]||'0',10)||0,prevent:p[6]==='true',guard:p[7]||'',actions:actions};});}
function parseStateAnimations(el){var out={};(el.getAttribute('data-wdoc-state-animation')||'').split(',').filter(Boolean).forEach(function(pair){var i=pair.indexOf(':');if(i>0)out[pair.slice(0,i)]=pair.slice(i+1);});return out;}
function parseAnimations(el){var out={};(el.getAttribute('data-wdoc-animations')||'').split(';').filter(Boolean).forEach(function(row){var p=row.split('|').map(unesc),k=(p[7]||'').split('~').filter(Boolean).map(function(f){var q=f.split(',');return{offset:parseFloat(q[0])||0,x:num(q[1]),y:num(q[2]),width:num(q[3]),height:num(q[4]),rotate:num(q[5]),rotate_origin_x:num(q[6]),rotate_origin_y:num(q[7])};}).sort(function(a,b){return a.offset-b.offset;}),frames=(p[9]||'').split(',').filter(function(v){return v!=='';}).map(function(v){return parseInt(v,10)||0;});out[p[0]]={name:p[0],duration:parseInt(p[1]||'1000',10)||1000,delay:parseInt(p[2]||'0',10)||0,timing:p[3]||'ease',iteration:p[4]||'1',direction:p[5]||'normal',fill:p[6]||'none',keyframes:k,frameRate:num(p[8]),frames:frames};});return out;}
function num(v){return v===''||v==null?null:parseFloat(v);}
function defaultMode(trigger){if(trigger==='hover'||trigger==='mouse_down')return'while';if(trigger==='click')return'toggle';return'pulse';}
function eventName(trigger,leaving){if(trigger==='hover')return leaving?'mouseleave':'mouseenter';if(trigger==='double_click')return'dblclick';if(trigger==='mouse_down')return leaving?'mouseup':'mousedown';if(trigger==='mouse_leave')return'mouseleave';if(trigger==='right_click')return'contextmenu';return trigger;}
function buttonOk(e,want){var b={left:0,middle:1,right:2}[want||'left'];return e.button===b;}
function stateClass(state){return'wdoc-state-'+String(state||'').replace(/[^A-Za-z0-9_-]/g,'-');}
function attrEscape(s){return String(s).replace(/\\/g,'\\\\').replace(/"/g,'\\"');}
function target(svg,source,name){if(!name||name==='self')return source;return svg.querySelector('[data-wdoc-id="'+attrEscape(name)+'"]');}
function targets(svg,names){return (names||'').split(',').map(function(s){return s.trim();}).filter(Boolean).map(function(name){return target(svg,null,name);}).filter(Boolean);}
function guardedTo(svg,related,names){if(!related)return false;return targets(svg,names).some(function(el){return el===related||(el.contains&&el.contains(related));});}
function zMap(el){var out={};(el.getAttribute('data-wdoc-state-z')||'').split(',').forEach(function(pair){var i=pair.indexOf(':');if(i>0)out[pair.slice(0,i)]=parseFloat(pair.slice(i+1));});return out;}
function reorder(parent){Array.prototype.slice.call(parent.children).filter(function(el){return el.hasAttribute('data-wdoc-id');}).sort(function(a,b){return (parseFloat(a.getAttribute('data-wdoc-z-current')||a.getAttribute('data-wdoc-z-base')||'0')-parseFloat(b.getAttribute('data-wdoc-z-current')||b.getAttribute('data-wdoc-z-base')||'0'))||((parseInt(a.getAttribute('data-wdoc-order')||'0',10))-(parseInt(b.getAttribute('data-wdoc-order')||'0',10)));}).forEach(function(el){parent.appendChild(el);});}
function updateZ(el){var map=zMap(el),z=parseFloat(el.getAttribute('data-wdoc-z-base')||'0');Object.keys(map).forEach(function(state){if(el.classList.contains(stateClass(state)))z=map[state];});el.setAttribute('data-wdoc-z-current',String(z));if(el.parentNode)reorder(el.parentNode);}
function add(el,state){el.classList.add(stateClass(state));updateZ(el);startStateAnimation(el,state);}
function remove(el,state){el.classList.remove(stateClass(state));updateZ(el);stopStateAnimation(el,state);}
function baseBox(el){return{x:parseFloat(el.getAttribute('data-wdoc-x')||'0'),y:parseFloat(el.getAttribute('data-wdoc-y')||'0'),width:parseFloat(el.getAttribute('data-wdoc-width')||'0'),height:parseFloat(el.getAttribute('data-wdoc-height')||'0'),rotate:0,rotate_origin_x:null,rotate_origin_y:null};}
function rotation(b){var a=b.rotate||0;if(!a)return'';var ox=b.rotate_origin_x==null?b.x+b.width/2:b.rotate_origin_x,oy=b.rotate_origin_y==null?b.y+b.height/2:b.rotate_origin_y;return' rotate('+a+','+ox+','+oy+')';}
function localRotation(b,tx,ty){var a=b.rotate||0;if(!a)return'';var ox=b.rotate_origin_x==null?b.x+b.width/2:b.rotate_origin_x,oy=b.rotate_origin_y==null?b.y+b.height/2:b.rotate_origin_y;return' rotate('+a+','+(ox-tx)+','+(oy-ty)+')';}
function setPrimitiveRotation(el,b){var r=rotation(b);if(r)el.setAttribute('transform',r.trim());else el.removeAttribute('transform');}
function setBox(el,b){var tag=el.tagName.toLowerCase(),base=baseBox(el);el.__wdocBox=b;if(tag==='g'){if(el.getAttribute('data-wdoc-terminal-grid-group')==='true'){var dx=b.x-base.x,dy=b.y-base.y;el.setAttribute('transform','translate('+dx+','+dy+')'+localRotation(b,base.x,base.y));}else el.setAttribute('transform','translate('+b.x+','+b.y+')'+localRotation(b,b.x,b.y));}else if(tag==='circle'){el.setAttribute('cx',b.x+b.width/2);el.setAttribute('cy',b.y+b.height/2);el.setAttribute('r',Math.max(b.width,b.height)/2);setPrimitiveRotation(el,b);}else if(tag==='ellipse'){el.setAttribute('cx',b.x+b.width/2);el.setAttribute('cy',b.y+b.height/2);el.setAttribute('rx',b.width/2);el.setAttribute('ry',b.height/2);setPrimitiveRotation(el,b);}else if(tag==='text'){el.setAttribute('x',b.x+b.width/2);el.setAttribute('y',b.y+b.height/2);setPrimitiveRotation(el,b);}else{if(el.hasAttribute('x'))el.setAttribute('x',b.x);if(el.hasAttribute('y'))el.setAttribute('y',b.y);if(el.hasAttribute('width'))el.setAttribute('width',b.width);if(el.hasAttribute('height'))el.setAttribute('height',b.height);setPrimitiveRotation(el,b);}updateConnections(el.closest('svg'));}
function spriteFrameBox(el,frame){var cols=parseFloat(el.getAttribute('data-wdoc-sprite-columns')||'1')||1,fw=parseFloat(el.getAttribute('data-wdoc-sprite-frame-width')||'0')||0,fh=parseFloat(el.getAttribute('data-wdoc-sprite-frame-height')||'0')||0,ox=parseFloat(el.getAttribute('data-wdoc-sprite-offset-x')||'0')||0,oy=parseFloat(el.getAttribute('data-wdoc-sprite-offset-y')||'0')||0,gx=parseFloat(el.getAttribute('data-wdoc-sprite-gap-x')||'0')||0,gy=parseFloat(el.getAttribute('data-wdoc-sprite-gap-y')||'0')||0,f=Math.max(0,parseInt(frame,10)||0),col=f%cols,row=Math.floor(f/cols);return{x:ox+col*(fw+gx),y:oy+row*(fh+gy),width:fw,height:fh};}
function setSpriteFrame(el,frame){if(!el||el.getAttribute('data-wdoc-sprite')!=='true')return;var b=spriteFrameBox(el,frame);el.setAttribute('viewBox',[b.x,b.y,b.width,b.height].join(' '));el.setAttribute('data-wdoc-sprite-current-frame',String(frame));}
function parseHexColor(s){s=String(s||'').trim();if(s[0]==='#')s=s.slice(1);if(s.length===3)s=s.split('').map(function(c){return c+c;}).join('');if(s.length!==6)return null;var n=parseInt(s,16);return isNaN(n)?null:{r:(n>>16)&255,g:(n>>8)&255,b:n&255};}
function applySpriteTransparency(el){var key=parseHexColor(el.getAttribute('data-wdoc-sprite-transparent-color'));if(!key||el.__wdocTransparencyApplied)return;var img=el.querySelector('image');if(!img)return;var href=img.getAttribute('href')||img.getAttributeNS('http://www.w3.org/1999/xlink','href');if(!href)return;el.__wdocTransparencyApplied=true;var source=new Image();source.crossOrigin='anonymous';source.onload=function(){try{var w=source.naturalWidth||source.width,h=source.naturalHeight||source.height,canvas=document.createElement('canvas'),ctx=canvas.getContext('2d',{willReadFrequently:true});canvas.width=w;canvas.height=h;ctx.drawImage(source,0,0);var data=ctx.getImageData(0,0,w,h),px=data.data,tol=parseInt(el.getAttribute('data-wdoc-sprite-transparent-tolerance')||'0',10)||0;for(var i=0;i<px.length;i+=4){if(Math.abs(px[i]-key.r)<=tol&&Math.abs(px[i+1]-key.g)<=tol&&Math.abs(px[i+2]-key.b)<=tol)px[i+3]=0;}ctx.putImageData(data,0,0);img.setAttribute('href',canvas.toDataURL('image/png'));}catch(e){}};source.src=href;}
function boxFor(svg,id){var el=svg.querySelector('[data-wdoc-id="'+attrEscape(id)+'"]');if(!el)return null;return el.__wdocBox||baseBox(el);}
function anchor(b,a,o){var cx=b.x+b.width/2,cy=b.y+b.height/2,ox=o?o.x+o.width/2:cx,oy=o?o.y+o.height/2:cy,dx=ox-cx,dy=oy-cy;if(a==='top')return{x:cx,y:b.y};if(a==='bottom')return{x:cx,y:b.y+b.height};if(a==='left')return{x:b.x,y:cy};if(a==='right')return{x:b.x+b.width,y:cy};if(a==='center')return{x:cx,y:cy};return Math.abs(dx)>Math.abs(dy)?(dx>0?{x:b.x+b.width,y:cy}:{x:b.x,y:cy}):(dy>0?{x:cx,y:b.y+b.height}:{x:cx,y:b.y});}
function updateConnections(svg){if(!svg)return;Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-conn-from]'),function(c){var fb=boxFor(svg,c.getAttribute('data-wdoc-conn-from')),tb=boxFor(svg,c.getAttribute('data-wdoc-conn-to'));if(!fb||!tb)return;var a=anchor(fb,c.getAttribute('data-wdoc-conn-from-anchor'),tb),b=anchor(tb,c.getAttribute('data-wdoc-conn-to-anchor'),fb);if(c.tagName.toLowerCase()==='line'){c.setAttribute('x1',a.x);c.setAttribute('y1',a.y);c.setAttribute('x2',b.x);c.setAttribute('y2',b.y);}else if(c.getAttribute('data-wdoc-conn-curve')==='bezier'){var dx=Math.abs(b.x-a.x)/2;c.setAttribute('d','M '+a.x+' '+a.y+' C '+(a.x+dx)+' '+a.y+', '+(b.x-dx)+' '+b.y+', '+b.x+' '+b.y);}else{c.setAttribute('d','M '+a.x+' '+a.y+' L '+b.x+' '+b.y);}});}
function parseViewBox(svg){var v=(svg.getAttribute('viewBox')||'0 0 0 0').trim().split(/[\s,]+/).map(function(n){return parseFloat(n)||0;});return{x:v[0]||0,y:v[1]||0,width:v[2]||0,height:v[3]||0};}
function updateGameLayers(svg,b){if(!svg)return;var home=svg.__wdocPanZoomHome||parseViewBox(svg),v=b||parseViewBox(svg);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-game-layer="true"]'),function(el){var bx=parseFloat(el.getAttribute('data-wdoc-game-layer-x')||'0')||0,by=parseFloat(el.getAttribute('data-wdoc-game-layer-y')||'0')||0;if(el.getAttribute('data-wdoc-game-layer-locked')==='true'){var sx=v.width/(home.width||1),sy=v.height/(home.height||1),tx=v.x+(bx-home.x)*sx,ty=v.y+(by-home.y)*sy;el.setAttribute('transform','translate('+tx+','+ty+') scale('+sx+','+sy+')');}else{var p=parseFloat(el.getAttribute('data-wdoc-game-layer-parallax')||'1');if(isNaN(p))p=1;el.setAttribute('transform','translate('+(bx+(v.x-home.x)*(1-p))+','+(by+(v.y-home.y)*(1-p))+')');}});}
function setViewBox(svg,b){svg.setAttribute('viewBox',[b.x,b.y,b.width,b.height].join(' '));updateGameLayers(svg,b);}
function svgPoint(svg,clientX,clientY){var r=svg.getBoundingClientRect(),v=parseViewBox(svg),w=r.width||1,h=r.height||1;return{x:v.x+((clientX-r.left)/w)*v.width,y:v.y+((clientY-r.top)/h)*v.height};}
function clampMapView(m,b){var cw=parseFloat(m.getAttribute('data-wdoc-map-content-width')||'0')||b.width,ch=parseFloat(m.getAttribute('data-wdoc-map-content-height')||'0')||b.height;if(b.width>cw){b.x=(cw-b.width)/2;}else{b.x=Math.max(0,Math.min(cw-b.width,b.x));}if(b.height>ch){b.y=(ch-b.height)/2;}else{b.y=Math.max(0,Math.min(ch-b.height,b.y));}return b;}
function setFixedMapShape(el,b){var tag=el.tagName.toLowerCase();if(tag==='circle'){el.setAttribute('cx',b.x+b.width/2);el.setAttribute('cy',b.y+b.height/2);el.setAttribute('r',Math.max(b.width,b.height)/2);}else if(tag==='ellipse'){el.setAttribute('cx',b.x+b.width/2);el.setAttribute('cy',b.y+b.height/2);el.setAttribute('rx',b.width/2);el.setAttribute('ry',b.height/2);}else if(tag==='text'){el.setAttribute('x',b.x+b.width/2);el.setAttribute('y',b.y+b.height/2);}else{if(el.hasAttribute('x'))el.setAttribute('x',b.x);if(el.hasAttribute('y'))el.setAttribute('y',b.y);if(el.hasAttribute('width'))el.setAttribute('width',b.width);if(el.hasAttribute('height'))el.setAttribute('height',b.height);}}
function updateMapFixedShapes(m,b){var home=m.__wdocMapHome||parseViewBox(m),v=b||parseViewBox(m),sx=v.width/(home.width||1),sy=v.height/(home.height||1);Array.prototype.forEach.call(m.querySelectorAll('[data-wdoc-map-fixed="true"]'),function(el){var base=baseBox(el),ax=parseFloat(el.getAttribute('data-wdoc-map-anchor-x')||String(base.x+base.width/2)),ay=parseFloat(el.getAttribute('data-wdoc-map-anchor-y')||String(base.y+base.height/2)),nw=base.width*sx,nh=base.height*sy;setFixedMapShape(el,{x:ax-nw/2,y:ay-nh/2,width:nw,height:nh});});}
function initMap(m){if(!m||m.getAttribute('data-wdoc-map')!=='true'||m.__wdocMapBound)return;m.__wdocMapBound=true;m.style.cursor=m.style.cursor||'grab';m.style.touchAction='none';m.style.userSelect='none';var home=parseViewBox(m);m.__wdocMapHome=home;updateMapFixedShapes(m,home);var min=parseFloat(m.getAttribute('data-wdoc-map-min-zoom')||'0.25')||0.25,max=parseFloat(m.getAttribute('data-wdoc-map-max-zoom')||'8')||8,drag=null;function setMapView(b){m.setAttribute('viewBox',[b.x,b.y,b.width,b.height].join(' '));updateMapFixedShapes(m,b);}function zoomTo(next,p){var v=parseViewBox(m),current=home.width/v.width;next=Math.max(min,Math.min(max,next));var scale=current/next,nw=v.width*scale,nh=v.height*scale,nx=p.x-(p.x-v.x)*scale,ny=p.y-(p.y-v.y)*scale;setMapView(clampMapView(m,{x:nx,y:ny,width:nw,height:nh}));}m.addEventListener('wheel',function(e){e.preventDefault();var v=parseViewBox(m),current=home.width/v.width;zoomTo(current*Math.exp(-e.deltaY*0.001),svgPoint(m,e.clientX,e.clientY));},{passive:false});m.addEventListener('pointerdown',function(e){if(e.button!==0)return;e.stopPropagation();drag={x:e.clientX,y:e.clientY,view:parseViewBox(m)};m.setPointerCapture&&m.setPointerCapture(e.pointerId);m.style.cursor='grabbing';});m.addEventListener('pointermove',function(e){if(!drag)return;e.preventDefault();e.stopPropagation();var r=m.getBoundingClientRect(),dx=(e.clientX-drag.x)/(r.width||1)*drag.view.width,dy=(e.clientY-drag.y)/(r.height||1)*drag.view.height;setMapView(clampMapView(m,{x:drag.view.x-dx,y:drag.view.y-dy,width:drag.view.width,height:drag.view.height}));});function endDrag(e){if(!drag)return;drag=null;m.releasePointerCapture&&m.releasePointerCapture(e.pointerId);m.style.cursor='grab';}m.addEventListener('pointerup',endDrag);m.addEventListener('pointercancel',endDrag);m.addEventListener('dblclick',function(e){e.preventDefault();e.stopPropagation();setMapView(home);});}
function initPanZoom(svg){if(!svg||svg.getAttribute('data-wdoc-pan-zoom')!=='true'||svg.__wdocPanZoomBound)return;svg.__wdocPanZoomBound=true;var home=parseViewBox(svg);svg.__wdocPanZoomHome=home;updateGameLayers(svg,home);var min=parseFloat(svg.getAttribute('data-wdoc-pan-zoom-min')||'0.25')||0.25,max=parseFloat(svg.getAttribute('data-wdoc-pan-zoom-max')||'8')||8,drag=null;function zoomTo(next,p){var v=parseViewBox(svg),current=home.width/v.width;next=Math.max(min,Math.min(max,next));var scale=current/next,nw=v.width*scale,nh=v.height*scale,nx=p.x-(p.x-v.x)*scale,ny=p.y-(p.y-v.y)*scale;setViewBox(svg,{x:nx,y:ny,width:nw,height:nh});}function zoomAt(e){e.preventDefault();var v=parseViewBox(svg),current=home.width/v.width;zoomTo(current*Math.exp(-e.deltaY*0.001),svgPoint(svg,e.clientX,e.clientY));}function zoomBy(factor){var v=parseViewBox(svg);zoomTo((home.width/v.width)*factor,{x:v.x+v.width/2,y:v.y+v.height/2});}svg.addEventListener('wheel',zoomAt,{passive:false});svg.addEventListener('pointerdown',function(e){if(e.button!==0)return;drag={x:e.clientX,y:e.clientY,view:parseViewBox(svg)};svg.setPointerCapture&&svg.setPointerCapture(e.pointerId);svg.style.cursor='grabbing';});svg.addEventListener('pointermove',function(e){if(!drag)return;e.preventDefault();var r=svg.getBoundingClientRect(),dx=(e.clientX-drag.x)/(r.width||1)*drag.view.width,dy=(e.clientY-drag.y)/(r.height||1)*drag.view.height;setViewBox(svg,{x:drag.view.x-dx,y:drag.view.y-dy,width:drag.view.width,height:drag.view.height});});function endDrag(e){if(!drag)return;drag=null;svg.releasePointerCapture&&svg.releasePointerCapture(e.pointerId);svg.style.cursor='grab';}svg.addEventListener('pointerup',endDrag);svg.addEventListener('pointercancel',endDrag);svg.addEventListener('dblclick',function(e){e.preventDefault();setViewBox(svg,home);});var wrap=svg.parentNode;if(wrap)Array.prototype.forEach.call(wrap.querySelectorAll('[data-wdoc-pan-zoom-control]'),function(btn){btn.addEventListener('click',function(e){e.preventDefault();e.stopPropagation();var action=btn.getAttribute('data-wdoc-pan-zoom-control');if(action==='in')zoomBy(1.25);else if(action==='out')zoomBy(0.8);else setViewBox(svg,home);});});}
function ease(t,fn){if(fn==='linear')return t;if(fn==='ease-in')return t*t;if(fn==='ease-out')return 1-Math.pow(1-t,2);if(fn==='ease-in-out')return t<.5?2*t*t:1-Math.pow(-2*t+2,2)/2;return t<.5?4*t*t*t:1-Math.pow(-2*t+2,3)/2;}
function frameValue(frames,p,prop,base){var prev=frames[0],next=frames[frames.length-1];frames.forEach(function(f){if(f.offset<=p)prev=f;if(f.offset>=p&&next.offset<p)next=f;});for(var i=0;i<frames.length;i++){if(frames[i].offset>=p){next=frames[i];break;}}var a=prev[prop];if(a==null)a=base[prop];var b=next[prop];if(b==null)b=a;if(a==null&&b==null)return null;if(next.offset===prev.offset)return b;var t=(p-prev.offset)/(next.offset-prev.offset);return a+(b-a)*t;}
function startStateAnimation(el,state){var map=parseStateAnimations(el),name=map[state];if(!name)return;var anim=parseAnimations(el)[name];if(!anim||(!anim.keyframes.length&&!anim.frames.length))return;if(el.__wdocAnim)cancelAnimationFrame(el.__wdocAnim.raf);var base=baseBox(el),start=performance.now()+anim.delay,loops=anim.iteration==='infinite'?Infinity:Math.max(1,parseFloat(anim.iteration)||1),initialFrame=el.getAttribute('data-wdoc-sprite-current-frame');function tick(now){if(now<start){el.__wdocAnim={raf:requestAnimationFrame(tick)};return;}var elapsed=now-start,idx=Math.floor(elapsed/anim.duration),done=idx>=loops,raw=done?1:(elapsed%anim.duration)/anim.duration,dirReverse=anim.direction==='reverse'||(anim.direction==='alternate'&&idx%2===1),frameRaw=dirReverse?1-raw:raw;if(anim.keyframes.length){var pct=ease(raw,anim.timing)*100;var x=frameValue(anim.keyframes,pct,'x',base),y=frameValue(anim.keyframes,pct,'y',base),width=frameValue(anim.keyframes,pct,'width',base),height=frameValue(anim.keyframes,pct,'height',base),rotBase={rotate:0,rotate_origin_x:x+width/2,rotate_origin_y:y+height/2};var b={x:x,y:y,width:width,height:height,rotate:frameValue(anim.keyframes,pct,'rotate',rotBase),rotate_origin_x:frameValue(anim.keyframes,pct,'rotate_origin_x',rotBase),rotate_origin_y:frameValue(anim.keyframes,pct,'rotate_origin_y',rotBase)};setBox(el,b);}if(anim.frames.length){var fi=done?anim.frames.length-1:Math.min(anim.frames.length-1,Math.floor(frameRaw*anim.frames.length));setSpriteFrame(el,anim.frames[fi]);}if(done){if(anim.fill!=='forwards'&&anim.fill!=='both'){setBox(el,base);if(initialFrame!=null)setSpriteFrame(el,initialFrame);}return;}el.__wdocAnim={raf:requestAnimationFrame(tick)};}el.__wdocAnim={raf:requestAnimationFrame(tick)};}
function stopStateAnimation(el,state){var map=parseStateAnimations(el);if(!map[state]||!el.__wdocAnim)return;cancelAnimationFrame(el.__wdocAnim.raf);el.__wdocAnim=null;setBox(el,baseBox(el));var initial=el.getAttribute('data-wdoc-sprite-initial-frame');if(initial!=null)setSpriteFrame(el,initial);}
function initSlider(el){if(!el||el.__wdocSliderBound)return;el.__wdocSliderBound=true;var signal=el.getAttribute('data-wdoc-slider-signal')||'',min=parseFloat(el.getAttribute('data-wdoc-slider-min')||'0')||0,max=parseFloat(el.getAttribute('data-wdoc-slider-max')||'100')||100,step=parseFloat(el.getAttribute('data-wdoc-slider-step')||'1')||1,w=parseFloat(el.getAttribute('data-wdoc-width')||el.getAttribute('width')||'100')||100,h=parseFloat(el.getAttribute('data-wdoc-height')||'24')||24,drag=false;function clamp(v){v=Math.max(min,Math.min(max,parseFloat(v)||0));return step>0?Math.round(v/step)*step:v;}function pct(v){return max===min?0:(clamp(v)-min)/(max-min);}function draw(v){var p=pct(v),fill=el.querySelector('[data-wdoc-slider-fill]'),thumb=el.querySelector('[data-wdoc-slider-thumb]');if(fill)fill.setAttribute('width',String(w*p));if(thumb){var r=parseFloat(thumb.getAttribute('r')||String(h/2));thumb.setAttribute('cx',String(w*p));thumb.setAttribute('cy',String(h/2));thumb.setAttribute('r',String(r));}}function setFromEvent(e){var r=el.getBoundingClientRect(),p=Math.max(0,Math.min(1,(e.clientX-r.left)/(r.width||1))),v=clamp(min+p*(max-min));draw(v);if(window.__wdocSetSignal&&signal)window.__wdocSetSignal(signal,v,'');}draw(el.getAttribute('data-wdoc-slider-value')||min);el.addEventListener('pointerdown',function(e){if(e.button!==0)return;drag=true;el.setPointerCapture&&el.setPointerCapture(e.pointerId);setFromEvent(e);});el.addEventListener('pointermove',function(e){if(drag)setFromEvent(e);});function end(e){if(!drag)return;drag=false;el.releasePointerCapture&&el.releasePointerCapture(e.pointerId);}el.addEventListener('pointerup',end);el.addEventListener('pointercancel',end);document.addEventListener('wdoc:signal-change',function(e){if(e.detail&&e.detail.name===signal)draw(e.detail.value);});}
function initTextbox(el){if(!el||el.__wdocTextboxBound||el.getAttribute('data-wdoc-textbox')!=='true')return;el.__wdocTextboxBound=true;var signal=el.getAttribute('data-wdoc-textbox-signal')||'',placeholder=el.getAttribute('data-wdoc-textbox-placeholder')||'',value=el.getAttribute('data-wdoc-textbox-value')||'',delay=parseInt(el.getAttribute('data-wdoc-textbox-debounce-ms')||'300',10)||300,multiline=el.getAttribute('data-wdoc-textbox-multiline')==='true',timer=null,editor=null;function textNode(){return el.querySelector('[data-wdoc-textbox-value-node]');}function setDisplay(v){value=v==null?'':String(v);var n=textNode(),shown=value||placeholder;if(n){n.textContent=shown;n.setAttribute('opacity',value?1:.4);var fill=value?(el.getAttribute('data-wdoc-textbox-text-fill')||'currentColor'):(el.getAttribute('data-wdoc-textbox-placeholder-fill')||'currentColor');n.setAttribute('fill',fill);}el.setAttribute('data-wdoc-textbox-value',value);}function flush(){if(timer){clearTimeout(timer);timer=null;}if(window.__wdocSetSignal&&signal)window.__wdocSetSignal(signal,value,'');}function schedule(){if(timer)clearTimeout(timer);timer=setTimeout(flush,delay);}function close(){if(!editor)return;flush();editor.remove();editor=null;}function open(){if(editor)return;var r=el.getBoundingClientRect(),base=baseBox(el),fy=parseFloat(el.getAttribute('data-wdoc-textbox-field-y')||'0')||0,fh=parseFloat(el.getAttribute('data-wdoc-textbox-field-h')||String(base.height))||base.height,px=parseFloat(el.getAttribute('data-wdoc-textbox-padding-x')||'10')||10,py=parseFloat(el.getAttribute('data-wdoc-textbox-padding-y')||'8')||8,sx=r.width/(base.width||r.width||1),sy=r.height/(base.height||r.height||1);editor=document.createElement(multiline?'textarea':'input');if(!multiline)editor.type='text';editor.value=value;editor.placeholder=placeholder;editor.setAttribute('aria-label',signal||'textbox');editor.style.position='fixed';editor.style.left=(r.left+px*sx)+'px';editor.style.top=(r.top+(fy+py)*sy)+'px';editor.style.width=Math.max(20,r.width-(px*2*sx))+'px';editor.style.height=Math.max(20,fh*sy-(py*2*sy))+'px';editor.style.zIndex='2147483647';editor.style.resize='none';editor.style.boxSizing='border-box';editor.style.border='1px solid '+(el.getAttribute('data-wdoc-textbox-border')||'transparent');editor.style.borderRadius='4px';editor.style.padding='2px 4px';editor.style.margin='0';editor.style.outline='2px solid rgba(88,166,255,.55)';editor.style.background=el.getAttribute('data-wdoc-textbox-fill')||'var(--color-code-bg)';editor.style.color=el.getAttribute('data-wdoc-textbox-text-fill')||'currentColor';editor.style.font=(parseFloat(el.getAttribute('data-wdoc-textbox-font-size')||'12')*sy)+'px system-ui, sans-serif';editor.style.lineHeight='1.3';document.body.appendChild(editor);editor.focus();editor.select();editor.addEventListener('input',function(){setDisplay(editor.value);schedule();});editor.addEventListener('blur',close);editor.addEventListener('keydown',function(e){if(e.key==='Escape'||(!multiline&&e.key==='Enter')){e.preventDefault();close();}});}setDisplay(value);el.addEventListener('click',function(e){e.preventDefault();e.stopPropagation();open();});document.addEventListener('wdoc:signal-change',function(e){if(e.detail&&e.detail.name===signal&&!editor)setDisplay(e.detail.value);});}
function initCheckbox(el){if(!el||el.__wdocCheckboxBound||el.getAttribute('data-wdoc-checkbox')!=='true')return;el.__wdocCheckboxBound=true;var signal=el.getAttribute('data-wdoc-checkbox-signal')||'',disabled=el.getAttribute('data-wdoc-checkbox-disabled')==='true';function bool(v){return v===true||v==='true'||v===1||v==='1';}function draw(v){var checked=bool(v),box=el.querySelector('[data-wdoc-checkbox-box]'),marks=el.querySelectorAll('[data-wdoc-checkbox-mark]'),active=el.getAttribute('data-wdoc-checkbox-active-fill')||'currentColor',field=el.getAttribute('data-wdoc-checkbox-field-fill')||'transparent',border=el.getAttribute('data-wdoc-checkbox-border-stroke')||'currentColor';el.setAttribute('data-wdoc-checkbox-checked',checked?'true':'false');if(box){box.setAttribute('fill',checked?active:field);box.setAttribute('stroke',checked?active:border);}Array.prototype.forEach.call(marks,function(m){m.setAttribute('opacity',checked?'1':'0');});}draw(el.getAttribute('data-wdoc-checkbox-checked'));el.addEventListener('click',function(e){if(disabled)return;e.preventDefault();e.stopPropagation();var next=el.getAttribute('data-wdoc-checkbox-checked')!=='true';draw(next);if(window.__wdocSetSignal&&signal)window.__wdocSetSignal(signal,next,'');});document.addEventListener('wdoc:signal-change',function(e){if(e.detail&&e.detail.name===signal)draw(e.detail.value);});}
function initRadio(el){if(!el||el.__wdocRadioBound||el.getAttribute('data-wdoc-radio')!=='true')return;el.__wdocRadioBound=true;var signal=el.getAttribute('data-wdoc-radio-signal')||'',value=el.getAttribute('data-wdoc-radio-value')||'',disabled=el.getAttribute('data-wdoc-radio-disabled')==='true';function draw(selected){var outer=el.querySelector('[data-wdoc-radio-outer]'),dot=el.querySelector('[data-wdoc-radio-dot]'),active=el.getAttribute('data-wdoc-radio-active-fill')||'currentColor',border=el.getAttribute('data-wdoc-radio-border-stroke')||'currentColor';el.setAttribute('data-wdoc-radio-selected',selected?'true':'false');if(outer)outer.setAttribute('stroke',selected?active:border);if(dot)dot.setAttribute('opacity',selected?'1':'0');}function apply(v){draw(String(v)==String(value));}apply(el.getAttribute('data-wdoc-radio-selected')==='true'?value:null);el.addEventListener('click',function(e){if(disabled)return;e.preventDefault();e.stopPropagation();apply(value);if(window.__wdocSetSignal&&signal)window.__wdocSetSignal(signal,value,'');});document.addEventListener('wdoc:signal-change',function(e){if(e.detail&&e.detail.name===signal)apply(e.detail.value);});}
function initDropdown(el){if(!el||el.__wdocDropdownBound||el.getAttribute('data-wdoc-dropdown')!=='true')return;el.__wdocDropdownBound=true;var signal=el.getAttribute('data-wdoc-dropdown-signal')||'',placeholder=el.getAttribute('data-wdoc-dropdown-placeholder')||'Select',value=el.getAttribute('data-wdoc-dropdown-value')||'',menu=null;function items(){return (el.getAttribute('data-wdoc-dropdown-items')||'').split(',').map(function(s){return s.trim();}).filter(Boolean);}function valueNode(){return el.querySelector('[data-wdoc-dropdown-value-node]');}function setDisplay(v){value=v==null?'':String(v);var n=valueNode(),shown=value||placeholder;if(n){n.textContent=shown;n.setAttribute('opacity',value?1:.4);n.setAttribute('fill',value?(el.getAttribute('data-wdoc-dropdown-value-fill')||'currentColor'):(el.getAttribute('data-wdoc-dropdown-placeholder-fill')||'currentColor'));}el.setAttribute('data-wdoc-dropdown-value',value);}function close(){if(menu){menu.remove();menu=null;}}function choose(v){setDisplay(v);if(window.__wdocSetSignal&&signal)window.__wdocSetSignal(signal,v,'');close();}function open(){if(menu){close();return;}var r=el.getBoundingClientRect(),base=baseBox(el),fy=parseFloat(el.getAttribute('data-wdoc-dropdown-field-y')||'0')||0,fh=parseFloat(el.getAttribute('data-wdoc-dropdown-field-h')||String(base.height))||base.height,sy=r.height/(base.height||r.height||1),opts=items();menu=document.createElement('div');menu.setAttribute('role','listbox');menu.style.position='fixed';menu.style.left=r.left+'px';menu.style.top=(r.top+(fy+fh)*sy+4)+'px';menu.style.width=r.width+'px';menu.style.zIndex='2147483647';menu.style.boxSizing='border-box';menu.style.border='1px solid '+(el.getAttribute('data-wdoc-dropdown-border-stroke')||'currentColor');menu.style.borderRadius='4px';menu.style.background=el.getAttribute('data-wdoc-dropdown-menu-fill')||'Canvas';menu.style.color=el.getAttribute('data-wdoc-dropdown-label-fill')||'CanvasText';menu.style.font='12px system-ui, sans-serif';menu.style.boxShadow='0 8px 20px rgba(0,0,0,.18)';opts.forEach(function(opt){var row=document.createElement('button');row.type='button';row.textContent=opt;row.style.display='block';row.style.width='100%';row.style.padding='6px 10px';row.style.border='0';row.style.background=opt===value?'rgba(88,166,255,.18)':'transparent';row.style.color='inherit';row.style.textAlign='left';row.style.cursor='pointer';row.addEventListener('click',function(e){e.preventDefault();e.stopPropagation();choose(opt);});menu.appendChild(row);});document.body.appendChild(menu);setTimeout(function(){document.addEventListener('click',outside,true);},0);}function outside(e){if(menu&&e.target!==menu&&!menu.contains(e.target)){document.removeEventListener('click',outside,true);close();}}setDisplay(value);el.addEventListener('click',function(e){e.preventDefault();e.stopPropagation();open();});document.addEventListener('wdoc:signal-change',function(e){if(e.detail&&e.detail.name===signal)setDisplay(e.detail.value);});}
function initDraggable(el){if(!el||el.__wdocDragBound||el.getAttribute('data-wdoc-draggable')!=='true')return;el.__wdocDragBound=true;var signal=el.getAttribute('data-wdoc-drag-signal')||'',xPath=el.getAttribute('data-wdoc-drag-x-path')||'x',yPath=el.getAttribute('data-wdoc-drag-y-path')||'y',drag=null;el.style.cursor=el.style.cursor||'grab';function point(e){var m=el.closest('[data-wdoc-map="true"]');if(m)return{point:svgPoint(m,e.clientX,e.clientY),map:m};var svg=el.closest('svg');return{point:svgPoint(svg,e.clientX,e.clientY),map:null};}function move(e){if(!drag)return;e.preventDefault();e.stopPropagation();var p=point(e),x=p.point.x-drag.dx,y=p.point.y-drag.dy;if(p.map&&el.getAttribute('data-wdoc-map-fixed')==='true'){el.setAttribute('data-wdoc-map-anchor-x',String(x));el.setAttribute('data-wdoc-map-anchor-y',String(y));updateMapFixedShapes(p.map);}else{var b=el.__wdocBox||baseBox(el);setBox(el,{x:x-drag.localX,y:y-drag.localY,width:b.width,height:b.height,rotate:b.rotate||0,rotate_origin_x:b.rotate_origin_x,rotate_origin_y:b.rotate_origin_y});}if(window.__wdocSetSignal&&signal){window.__wdocSetSignal(signal,x,xPath);window.__wdocSetSignal(signal,y,yPath);}}function end(e){if(!drag)return;drag=null;el.style.cursor='grab';el.releasePointerCapture&&el.releasePointerCapture(e.pointerId);}el.addEventListener('pointerdown',function(e){if(e.button!==0)return;var p=point(e),b=baseBox(el),anchorX=parseFloat(el.getAttribute('data-wdoc-map-anchor-x')||String(b.x+b.width/2)),anchorY=parseFloat(el.getAttribute('data-wdoc-map-anchor-y')||String(b.y+b.height/2));e.preventDefault();e.stopPropagation();drag={dx:p.point.x-anchorX,dy:p.point.y-anchorY,localX:anchorX-b.x,localY:anchorY-b.y};el.style.cursor='grabbing';el.setPointerCapture&&el.setPointerCapture(e.pointerId);move(e);});el.addEventListener('pointermove',move);el.addEventListener('pointerup',end);el.addEventListener('pointercancel',end);}
function eventValue(v,e){return v==='$event.value'&&e&&e.__wdocValue!==undefined?e.__wdocValue:v;}
function runActions(cfg,e){if(!window.__wdocSetSignal||!cfg.actions)return;cfg.actions.forEach(function(a){if(a&&a.signal)window.__wdocSetSignal(a.signal,eventValue(a.value,e),a.path||'');});}
function applyProperty(el,prop,value){var s=value==null?'':String(value),b;if(['x','y','width','height'].indexOf(prop)>=0){b=el.__wdocBox||baseBox(el);b={x:b.x,y:b.y,width:b.width,height:b.height};b[prop]=parseFloat(value)||0;setBox(el,b);return true;}if(prop==='map_anchor_x'||prop==='map_anchor_y'){el.setAttribute(prop==='map_anchor_x'?'data-wdoc-map-anchor-x':'data-wdoc-map-anchor-y',String(parseFloat(value)||0));var m=el.closest('[data-wdoc-map="true"]');if(m)updateMapFixedShapes(m);return true;}if(prop==='content'||prop==='text'){el.textContent=s;return true;}if(['fill','stroke','opacity','visibility','display','class','transform'].indexOf(prop)>=0){el.setAttribute(prop,s);return true;}return false;}
window.__wdocDiagramApplyProperty=applyProperty;
function init(root){(root||document).querySelectorAll('svg').forEach(function(svg){initPanZoom(svg);initMap(svg);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-map]'),initMap);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-sprite]'),applySpriteTransparency);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-slider]'),initSlider);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-textbox="true"]'),initTextbox);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-checkbox="true"]'),initCheckbox);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-radio="true"]'),initRadio);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-dropdown="true"]'),initDropdown);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-draggable="true"]'),initDraggable);Array.prototype.forEach.call(svg.querySelectorAll('[data-wdoc-id]'),function(el,i){if(el.__wdocBound)return;el.__wdocBound=true;if(!el.hasAttribute('data-wdoc-order'))el.setAttribute('data-wdoc-order',String(i));parseEvents(el).forEach(function(cfg){var mode=cfg.mode||defaultMode(cfg.trigger),down=eventName(cfg.trigger,false),up=eventName(cfg.trigger,true);el.addEventListener(down,function(e){if(cfg.trigger==='mouse_down'&&!buttonOk(e,cfg.button))return;if(cfg.prevent)e.preventDefault();if(cfg.trigger==='mouse_leave'&&mode==='remove'&&guardedTo(svg,e.relatedTarget,cfg.guard))return;var t=target(svg,el,cfg.target);if(t){if(mode==='toggle')t.classList.contains(stateClass(cfg.state))?remove(t,cfg.state):add(t,cfg.state);else if(mode==='remove')remove(t,cfg.state);else if(mode==='pulse'){add(t,cfg.state);setTimeout(function(){remove(t,cfg.state);},cfg.duration||180);}else add(t,cfg.state);}runActions(cfg,e);});if(mode==='while'&&(cfg.trigger==='hover'||cfg.trigger==='mouse_down'))el.addEventListener(up,function(){var t=target(svg,el,cfg.target);if(t)remove(t,cfg.state);});});});});}
window.__wdocDiagramRuntimeInit=init;var s=document.currentScript;if(s&&s.parentNode)init(s.parentNode);if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',function(){init(document);});else init(document);
})();"#.to_string()
}

pub fn sanitize_inline_svg(source: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut out = String::new();
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut skip_depth = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = svg_event_name(event.name().as_ref());
                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }
                if !is_allowed_svg_element(&name) {
                    skip_depth = 1;
                    continue;
                }
                if name == "svg" {
                    stack.push(None);
                    continue;
                }
                write_sanitized_svg_start(&mut out, &name, &event, &reader, false);
                stack.push(Some(name));
            }
            Ok(Event::Empty(event)) => {
                let name = svg_event_name(event.name().as_ref());
                if skip_depth > 0 || !is_allowed_svg_element(&name) || name == "svg" {
                    continue;
                }
                write_sanitized_svg_start(&mut out, &name, &event, &reader, true);
            }
            Ok(Event::End(_)) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                if let Some(Some(name)) = stack.pop() {
                    write!(out, "</{name}>").unwrap();
                } else {
                    stack.pop();
                }
            }
            Ok(Event::Text(text)) => {
                if skip_depth == 0 {
                    let text = text.unescape().map_err(|e| e.to_string())?;
                    out.push_str(&svg_escape_text(&text));
                }
            }
            Ok(Event::CData(text)) => {
                if skip_depth == 0 {
                    let text = String::from_utf8_lossy(text.as_ref());
                    out.push_str(&svg_escape_text(&text));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(format!("invalid SVG: {err}")),
        }
    }

    Ok(out)
}

fn write_sanitized_svg_start(
    out: &mut String,
    name: &str,
    event: &BytesStart<'_>,
    reader: &Reader<&[u8]>,
    empty: bool,
) {
    write!(out, "<{name}").unwrap();
    for attr in event.attributes().with_checks(false).flatten() {
        let attr_name = svg_event_name(attr.key.as_ref()).replace('_', "-");
        if !is_allowed_svg_attr(&attr_name) {
            continue;
        }
        let Ok(value) = attr.decode_and_unescape_value(reader.decoder()) else {
            continue;
        };
        if !is_safe_svg_attr_value(&attr_name, &value) {
            continue;
        }
        let value = svg_escape_attr(&value);
        write!(out, " {attr_name}=\"{value}\"").unwrap();
    }
    if empty {
        out.push_str("/>");
    } else {
        out.push('>');
    }
}

fn svg_root_view_box(source: &str) -> Option<String> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event().ok()? {
            Event::Start(event) | Event::Empty(event) => {
                if svg_event_name(event.name().as_ref()) != "svg" {
                    continue;
                }
                let mut width = None;
                let mut height = None;
                for attr in event.attributes().with_checks(false).flatten() {
                    let name = svg_event_name(attr.key.as_ref());
                    let Ok(value) = attr.decode_and_unescape_value(reader.decoder()) else {
                        continue;
                    };
                    match name.as_str() {
                        "viewBox" | "viewbox" => return Some(value.to_string()),
                        "width" => width = parse_svg_number(value.trim_end_matches("px")),
                        "height" => height = parse_svg_number(value.trim_end_matches("px")),
                        _ => {}
                    }
                }
                if let (Some(width), Some(height)) = (width, height) {
                    return Some(format!("0 0 {width} {height}"));
                }
                return None;
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

pub fn svg_source_view_box(source: &str) -> Option<String> {
    svg_root_view_box(source)
}

fn svg_event_name(name: &[u8]) -> String {
    let full = String::from_utf8_lossy(name);
    full.rsplit(':').next().unwrap_or(&full).to_string()
}

fn is_allowed_svg_element(name: &str) -> bool {
    matches!(
        name,
        "svg"
            | "g"
            | "path"
            | "rect"
            | "circle"
            | "ellipse"
            | "line"
            | "polyline"
            | "polygon"
            | "text"
            | "tspan"
            | "defs"
            | "clipPath"
            | "mask"
            | "linearGradient"
            | "radialGradient"
            | "stop"
            | "title"
            | "desc"
            | "use"
    )
}

fn is_allowed_svg_attr(name: &str) -> bool {
    !name.starts_with("on")
        && matches!(
            name,
            "id" | "class"
                | "d"
                | "x"
                | "y"
                | "x1"
                | "y1"
                | "x2"
                | "y2"
                | "cx"
                | "cy"
                | "r"
                | "rx"
                | "ry"
                | "width"
                | "height"
                | "viewBox"
                | "viewbox"
                | "fill"
                | "stroke"
                | "stroke-width"
                | "stroke-linecap"
                | "stroke-linejoin"
                | "stroke-miterlimit"
                | "stroke-dasharray"
                | "stroke-dashoffset"
                | "opacity"
                | "fill-opacity"
                | "stroke-opacity"
                | "fill-rule"
                | "clip-rule"
                | "transform"
                | "clip-path"
                | "mask"
                | "points"
                | "offset"
                | "stop-color"
                | "stop-opacity"
                | "gradientUnits"
                | "gradientTransform"
                | "spreadMethod"
                | "href"
                | "xlink:href"
                | "font-family"
                | "font-size"
                | "font-weight"
                | "font-style"
                | "text-anchor"
                | "dominant-baseline"
                | "letter-spacing"
        )
}

fn is_safe_svg_attr_value(name: &str, value: &str) -> bool {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    if lower.contains("javascript:") || lower.contains("data:") || lower.contains("<script") {
        return false;
    }
    if matches!(name, "href" | "xlink:href") {
        return value.starts_with('#');
    }
    if lower.contains("url(") {
        let Some(start) = lower.find("url(") else {
            return true;
        };
        let rest = lower[start + 4..].trim_start();
        return rest.starts_with('#') || rest.starts_with("'#") || rest.starts_with("\"#");
    }
    true
}

fn svg_generated_id(prefix: &str, node: &ShapeNode) -> String {
    let mut hasher = DefaultHasher::new();
    format!("{:?}", node.kind).hash(&mut hasher);
    node.id.hash(&mut hasher);
    node.resolved.x.to_bits().hash(&mut hasher);
    node.resolved.y.to_bits().hash(&mut hasher);
    node.resolved.width.to_bits().hash(&mut hasher);
    node.resolved.height.to_bits().hash(&mut hasher);
    for (key, value) in &node.attrs {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    format!("{prefix}-{:x}", hasher.finish())
}

fn diagram_scope_id(diagram: &Diagram, css: &str) -> String {
    if let Some(id) = diagram.id.as_deref().filter(|id| !id.trim().is_empty()) {
        return format!("wdoc-diagram-{}", sanitize_svg_id_fragment(id));
    }

    let mut hasher = DefaultHasher::new();
    diagram.width.to_bits().hash(&mut hasher);
    diagram.height.to_bits().hash(&mut hasher);
    css.hash(&mut hasher);
    format!("wdoc-diagram-{:x}", hasher.finish())
}

fn sanitize_svg_id_fragment(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            last_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if ch == '-' || ch == '_' || ch.is_whitespace() {
            if last_dash {
                None
            } else {
                last_dash = true;
                Some('-')
            }
        } else {
            None
        };
        if let Some(ch) = next {
            out.push(ch);
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

fn scope_svg_css(css: &str, scope_id: &str) -> String {
    scope_css_to_selector(css, &format!("#{scope_id}"))
}

pub fn scope_css_to_selector(css: &str, root_selector: &str) -> String {
    scope_css_block(css, root_selector)
}

fn scope_css_block(css: &str, root_selector: &str) -> String {
    let mut out = String::new();
    let mut pos = 0;

    while let Some(open_rel) = css[pos..].find('{') {
        let open = pos + open_rel;
        let selector = css[pos..open].trim();
        let Some(close) = find_matching_brace(css, open) else {
            break;
        };
        let body = &css[open + 1..close];

        if selector.starts_with("@media") || selector.starts_with("@supports") {
            let scoped = scope_css_block(body, root_selector);
            if !scoped.trim().is_empty() {
                write!(out, "{selector}{{{scoped}}}").unwrap();
            }
        } else if !selector.starts_with('@') {
            let scoped_selector = scope_selector_list(selector, root_selector);
            if !scoped_selector.is_empty() {
                write!(out, "{scoped_selector}{{{body}}}").unwrap();
            }
        }

        pos = close + 1;
    }

    out
}

fn find_matching_brace(css: &str, open: usize) -> Option<usize> {
    let bytes = css.as_bytes();
    let mut depth = 0usize;
    let mut idx = open;
    while idx < bytes.len() {
        match bytes[idx] {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
        idx += 1;
    }
    None
}

fn scope_selector_list(selector: &str, root_selector: &str) -> String {
    selector
        .split(',')
        .map(str::trim)
        .filter(|sel| !sel.is_empty())
        .map(|sel| {
            if sel == ":root" {
                root_selector.to_string()
            } else if sel.starts_with(root_selector)
                || root_selector
                    .strip_prefix('#')
                    .is_some_and(|id| sel.starts_with(&format!("svg#{id}")))
            {
                sel.to_string()
            } else {
                format!("{root_selector} {sel}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn wrapped_text_lines(
    content: &str,
    wrap_width: Option<f64>,
    font_size: f64,
    letter_spacing: f64,
    attrs: &IndexMap<String, String>,
) -> Vec<String> {
    let mut lines = Vec::new();
    for segment in content.split('\n') {
        if let Some(max_width) = wrap_width {
            wrap_text_segment(
                segment,
                max_width,
                font_size,
                letter_spacing,
                attrs,
                &mut lines,
            );
        } else {
            lines.push(segment.to_string());
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn wrap_text_segment(
    segment: &str,
    max_width: f64,
    font_size: f64,
    letter_spacing: f64,
    attrs: &IndexMap<String, String>,
    lines: &mut Vec<String>,
) {
    if segment.is_empty() {
        lines.push(String::new());
        return;
    }

    let mut current = String::new();
    for word in segment.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if measure_line_width(&candidate, font_size, letter_spacing, attrs) <= max_width {
            current = candidate;
            continue;
        }

        if !current.is_empty() {
            lines.push(current.clone());
            current.clear();
        }

        if measure_line_width(word, font_size, letter_spacing, attrs) <= max_width {
            current.push_str(word);
        } else {
            current = push_wrapped_word(word, max_width, font_size, letter_spacing, attrs, lines);
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
}

fn push_wrapped_word(
    word: &str,
    max_width: f64,
    font_size: f64,
    letter_spacing: f64,
    attrs: &IndexMap<String, String>,
    lines: &mut Vec<String>,
) -> String {
    let mut current = String::new();
    for ch in word.chars() {
        let candidate = format!("{current}{ch}");
        if current.is_empty()
            || measure_line_width(&candidate, font_size, letter_spacing, attrs) <= max_width
        {
            current = candidate;
        } else {
            lines.push(current.clone());
            current.clear();
            current.push(ch);
        }
    }
    current
}

fn measure_line_width(
    line: &str,
    font_size: f64,
    letter_spacing: f64,
    attrs: &IndexMap<String, String>,
) -> f64 {
    let mut width = 0.0;
    let mut count = 0usize;
    for ch in line.chars() {
        width += char_advance_factor(ch) * font_size;
        count += 1;
    }
    if count > 1 {
        width += letter_spacing * (count - 1) as f64;
    }
    width * font_style_width_factor(attrs)
}

fn char_advance_factor(ch: char) -> f64 {
    if ch.is_whitespace() {
        0.33
    } else if matches!(
        ch,
        'i' | 'j' | 'l' | 'I' | '!' | '|' | '.' | ',' | ':' | ';' | '\''
    ) {
        0.28
    } else if matches!(ch, 'f' | 'r' | 't' | '(' | ')' | '[' | ']' | '{' | '}') {
        0.38
    } else if matches!(ch, 'm' | 'w' | 'M' | 'W' | '@' | '#' | '%' | '&') {
        0.88
    } else if ch.is_ascii_digit() {
        0.56
    } else if ch.is_ascii() {
        0.54
    } else {
        1.0
    }
}

fn font_style_width_factor(attrs: &IndexMap<String, String>) -> f64 {
    let mut factor = 1.0;
    if attrs
        .get("font_style")
        .map(|s| s.eq_ignore_ascii_case("italic") || s.eq_ignore_ascii_case("oblique"))
        .unwrap_or(false)
    {
        factor += 0.02;
    }
    if attrs
        .get("font_weight")
        .and_then(|s| s.parse::<u16>().ok())
        .map(|w| w >= 600)
        .unwrap_or_else(|| {
            attrs
                .get("font_weight")
                .map(|s| s.eq_ignore_ascii_case("bold"))
                .unwrap_or(false)
        })
    {
        factor += 0.03;
    }
    factor
}

fn parse_css_length(value: &str, font_size: f64) -> f64 {
    let trimmed = value.trim();
    if let Some(em) = trimmed.strip_suffix("em") {
        em.trim().parse::<f64>().unwrap_or(0.0) * font_size
    } else if let Some(px) = trimmed.strip_suffix("px") {
        px.trim().parse::<f64>().unwrap_or(0.0)
    } else {
        trimmed.parse::<f64>().unwrap_or(0.0)
    }
}

fn attr_f64(attrs: &IndexMap<String, String>, key: &str) -> Option<f64> {
    attrs.get(key).and_then(|s| s.parse().ok())
}

fn map_content_width(node: &ShapeNode) -> f64 {
    attr_f64(&node.attrs, "content_width")
        .unwrap_or(node.resolved.width)
        .max(1.0)
}

fn map_content_height(node: &ShapeNode) -> f64 {
    attr_f64(&node.attrs, "content_height")
        .unwrap_or(node.resolved.height)
        .max(1.0)
}

fn parse_svg_number(value: &str) -> Option<f64> {
    value.trim().parse().ok()
}

fn parse_view_box(value: &str) -> Option<(f64, f64, f64, f64)> {
    let nums: Vec<f64> = value
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<f64>().ok())
        .collect();
    if nums.len() == 4 {
        Some((nums[0], nums[1], nums[2], nums[3]))
    } else {
        None
    }
}

pub fn parse_svg_view_box(value: &str) -> Option<(f64, f64, f64, f64)> {
    parse_view_box(value)
}

fn svg_escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn svg_escape_attr(s: &str) -> String {
    svg_escape_text(s)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn html_escape_script(s: &str) -> String {
    s.replace("</script", "<\\/script")
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
            kind_name: ShapeKind::Rect.as_str().to_string(),
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
            events: Vec::new(),
            children: vec![],
            text_block_items: Vec::new(),
            align: Alignment::None,
            gap: 0.0,
            padding: 0.0,
            z_index: 0.0,
            source_order: 0,
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
            z_index: 0.0,
            source_order: 0,
        }
    }

    fn text_shape(content: &str, width: f64, height: f64) -> ShapeNode {
        let mut node = shape("label", width, height);
        node.kind = ShapeKind::Text;
        node.x = Some(0.0);
        node.y = Some(0.0);
        node.attrs
            .insert("content".to_string(), content.to_string());
        node
    }

    fn image_shape(src: &str, width: f64, height: f64) -> ShapeNode {
        let mut node = shape("hero", width, height);
        node.kind = ShapeKind::Image;
        node.attrs.insert("src".to_string(), src.to_string());
        node
    }

    fn map_shape(src: &str, width: f64, height: f64) -> ShapeNode {
        let mut node = shape("world", width, height);
        node.kind = ShapeKind::Map;
        node.attrs.insert("src".to_string(), src.to_string());
        node
    }

    fn inline_svg_shape(content: &str, width: f64, height: f64) -> ShapeNode {
        let mut node = shape("icon", width, height);
        node.kind = ShapeKind::InlineSvg;
        node.attrs
            .insert("content".to_string(), content.to_string());
        node
    }

    fn index_of(haystack: &str, needle: &str) -> usize {
        haystack
            .find(needle)
            .unwrap_or_else(|| panic!("expected to find {needle:?} in {haystack}"))
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
            id: None,
            width: 400.0,
            height: 200.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![ShapeNode {
                kind: ShapeKind::Rect,
                kind_name: ShapeKind::Rect.as_str().to_string(),
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
                events: Vec::new(),
                children: vec![],
                text_block_items: Vec::new(),
                align: Alignment::None,
                gap: 0.0,
                padding: 0.0,
                z_index: 0.0,
                source_order: 0,
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
    fn text_shape_emits_rich_typography_attributes() {
        let mut text = text_shape("Quoted text", 200.0, 40.0);
        text.attrs.insert("font_size".to_string(), "14".to_string());
        text.attrs.insert(
            "font_family".to_string(),
            "Inter, system-ui, sans-serif".to_string(),
        );
        text.attrs
            .insert("font_weight".to_string(), "600".to_string());
        text.attrs
            .insert("font_style".to_string(), "italic".to_string());
        text.attrs
            .insert("text_decoration".to_string(), "underline".to_string());
        text.attrs
            .insert("letter_spacing".to_string(), "0.02em".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 200.0,
            height: 40.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![text],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("font-size=\"14\""));
        assert!(svg.contains("font-family=\"Inter, system-ui, sans-serif\""));
        assert!(svg.contains("font-weight=\"600\""));
        assert!(svg.contains("font-style=\"italic\""));
        assert!(svg.contains("text-decoration=\"underline\""));
        assert!(svg.contains("letter-spacing=\"0.02em\""));
    }

    #[test]
    fn measure_text_attrs_returns_deterministic_metrics() {
        let attrs: IndexMap<String, String> = [
            ("content".to_string(), "Inline text".to_string()),
            ("font_size".to_string(), "14".to_string()),
            ("font_weight".to_string(), "400".to_string()),
            ("font_style".to_string(), "normal".to_string()),
            ("letter_spacing".to_string(), "0".to_string()),
        ]
        .into_iter()
        .collect();

        let metrics = measure_text_attrs(&attrs);
        assert!(metrics.width > 50.0);
        assert!(metrics.width < 90.0);
        assert_eq!(metrics.height, 16.8);
        assert_eq!(metrics.baseline, 12.600000000000001);
    }

    #[test]
    fn measure_text_attrs_wraps_to_width_constraint() {
        let attrs: IndexMap<String, String> = [
            ("content".to_string(), "Inline text wraps".to_string()),
            ("font_size".to_string(), "14".to_string()),
            ("width".to_string(), "58".to_string()),
        ]
        .into_iter()
        .collect();

        let metrics = measure_text_attrs(&attrs);
        assert!(metrics.width <= 58.0);
        assert!(metrics.height > 16.8);
    }

    #[test]
    fn text_render_wraps_to_width_constraint() {
        let mut text = text_shape("Inline text wraps", 58.0, 80.0);
        text.attrs.insert("font_size".to_string(), "14".to_string());
        text.attrs.insert("max_width".to_string(), "58".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 120.0,
            height: 90.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![text],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<tspan"));
        assert!(svg.contains(">Inline</tspan>"));
        assert!(svg.contains(">text</tspan>"));
        assert!(svg.contains(">wraps</tspan>"));
    }

    #[test]
    fn positioned_text_without_size_uses_natural_bounds() {
        let mut text = text_shape("Inline", 0.0, 0.0);
        text.width = None;
        text.height = None;
        text.attrs.insert("font_size".to_string(), "14".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 200.0,
            height: 80.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![text],
            connections: vec![],
        };

        render_diagram_svg(&mut diagram);
        assert_eq!(diagram.shapes[0].resolved.x, 0.0);
        assert_eq!(diagram.shapes[0].resolved.y, 0.0);
        assert!(diagram.shapes[0].resolved.width > 0.0);
        assert_eq!(diagram.shapes[0].resolved.height, 16.8);
    }

    #[test]
    fn unpositioned_unsized_text_preserves_parent_fill_behavior() {
        let mut text = text_shape("Centered", 0.0, 0.0);
        text.x = None;
        text.y = None;
        text.width = None;
        text.height = None;

        let mut diagram = Diagram {
            id: None,
            width: 220.0,
            height: 90.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![text],
            connections: vec![],
        };

        render_diagram_svg(&mut diagram);
        assert_eq!(diagram.shapes[0].resolved.width, 220.0);
        assert_eq!(diagram.shapes[0].resolved.height, 90.0);
    }

    #[test]
    fn multiline_text_uses_tspans_and_line_height() {
        let mut text = text_shape("One\nTwo\nThree", 160.0, 80.0);
        text.attrs.insert("font_size".to_string(), "10".to_string());
        text.attrs
            .insert("line_height".to_string(), "1.5".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 160.0,
            height: 80.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![text],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<tspan x=\"80\" y=\"25\">One</tspan>"));
        assert!(svg.contains("<tspan x=\"80\" dy=\"15\">Two</tspan>"));
        assert!(svg.contains("<tspan x=\"80\" dy=\"15\">Three</tspan>"));
    }

    #[test]
    fn single_line_text_remains_centered_without_tspans() {
        let mut diagram = Diagram {
            id: None,
            width: 120.0,
            height: 30.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![text_shape("Centered", 120.0, 30.0)],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<text x=\"60\" y=\"15\""));
        assert!(svg.contains("text-anchor=\"middle\""));
        assert!(svg.contains(">Centered</text>"));
        assert!(!svg.contains("<tspan"));
    }

    #[test]
    fn text_content_and_attributes_are_svg_escaped() {
        let mut text = text_shape("A < B & \"C\"", 120.0, 30.0);
        text.attrs
            .insert("font_family".to_string(), "\"Inter\" & sans".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 120.0,
            height: 30.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![text],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("font-family=\"&quot;Inter&quot; &amp; sans\""));
        assert!(svg.contains(">A &lt; B &amp; \"C\"</text>"));
    }

    #[test]
    fn image_fit_modes_emit_svg_image_aspect_ratio() {
        for (fit, expected) in [
            ("contain", "xMidYMid meet"),
            ("cover", "xMidYMid slice"),
            ("fill", "none"),
            ("unknown", "xMidYMid meet"),
        ] {
            let mut image = image_shape("images/hero.png", 160.0, 90.0);
            image.attrs.insert("fit".to_string(), fit.to_string());
            let mut diagram = Diagram {
                id: None,
                width: 160.0,
                height: 90.0,
                padding: 0.0,
                align: Alignment::None,
                gap: 0.0,
                options: IndexMap::new(),
                classes: IndexMap::new(),
                shapes: vec![image],
                connections: vec![],
            };

            let svg = render_diagram_svg(&mut diagram);
            assert!(svg.contains("<image href=\"images/hero.png\""));
            assert!(svg.contains(&format!("preserveAspectRatio=\"{expected}\"")));
            assert!(svg.contains("<clipPath id=\"wdoc-image-clip-"));
            assert!(svg.contains("clip-path=\"url(#wdoc-image-clip-"));
        }
    }

    #[test]
    fn inline_svg_embeds_sanitized_content() {
        let mut icon = inline_svg_shape(
            r##"<svg viewBox="0 0 24 24" onload="bad()">
                <script>alert(1)</script>
                <foreignObject><div>bad</div></foreignObject>
                <path class="mark" onclick="bad()" d="M1 2L3 4" fill="url(#grad)" href="https://example.com/x"/>
                <a href="javascript:alert(1)"><rect width="10" height="10"/></a>
              </svg>"##,
            48.0,
            48.0,
        );
        icon.x = Some(10.0);
        icon.y = Some(12.0);
        icon.attrs.insert("x".to_string(), "10".to_string());
        icon.attrs.insert("y".to_string(), "12".to_string());
        icon.attrs
            .insert("class".to_string(), "brand-icon".to_string());
        icon.attrs
            .insert("fill".to_string(), "currentColor".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 80.0,
            height: 80.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![icon],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("class=\"brand-icon\""));
        assert!(svg.contains("fill=\"currentColor\""));
        assert!(svg.contains("transform=\"translate(10,12) scale(2,2) translate(-0,-0)\""));
        assert!(svg.contains("<path class=\"mark\" d=\"M1 2L3 4\" fill=\"url(#grad)\""));
        assert!(!svg.contains("<script"));
        assert!(!svg.contains("foreignObject"));
        assert!(!svg.contains("onclick"));
        assert!(!svg.contains("onload"));
        assert!(!svg.contains("https://example.com"));
        assert!(!svg.contains("javascript:"));
        assert!(!svg.contains("<a "));
    }

    #[test]
    fn image_rounded_clip_and_alt_are_emitted() {
        let mut image = image_shape("images/hero.png", 160.0, 90.0);
        image
            .attrs
            .insert("alt".to_string(), "Hero image".to_string());
        image.attrs.insert("rx".to_string(), "6".to_string());
        image.attrs.insert("ry".to_string(), "4".to_string());
        image
            .attrs
            .insert("opacity".to_string(), "0.75".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 160.0,
            height: 90.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![image],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<g role=\"img\" aria-label=\"Hero image\">"));
        assert!(svg.contains("rx=\"6\" ry=\"4\""));
        assert!(svg.contains("opacity=\"0.75\""));
    }

    #[test]
    fn tiled_image_uses_pattern_and_tile_size() {
        let mut image = image_shape("images/tile.svg", 120.0, 80.0);
        image.attrs.insert("fit".to_string(), "tile".to_string());
        image
            .attrs
            .insert("tile_width".to_string(), "24".to_string());
        image
            .attrs
            .insert("tile_height".to_string(), "18".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 120.0,
            height: 80.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![image],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<pattern id=\"wdoc-image-pattern-"));
        assert!(svg.contains("patternUnits=\"userSpaceOnUse\""));
        assert!(svg.contains("width=\"24\" height=\"18\""));
        assert!(svg.contains("<image href=\"images/tile.svg\""));
        assert!(svg.contains("fill=\"url(#wdoc-image-pattern-"));
    }

    #[test]
    fn group_renders_as_translated_svg_group_with_children() {
        let mut group = shape("control", 120.0, 36.0);
        group.kind = ShapeKind::Group;
        group.x = Some(10.0);
        group.y = Some(20.0);

        let mut bg = shape("bg", 120.0, 36.0);
        bg.attrs.insert("fill".to_string(), "#88c0d0".to_string());
        let label = text_shape("Save", 120.0, 36.0);
        group.children = vec![bg, label];

        let mut diagram = Diagram {
            id: None,
            width: 160.0,
            height: 80.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![group],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<g transform=\"translate(10,20)\">"));
        assert!(svg.contains("<rect x=\"0\" y=\"0\" width=\"120\" height=\"36\""));
        assert!(svg.contains(">Save</text>"));
        assert!(svg.contains("</g>"));
    }

    #[test]
    fn image_nested_in_group_uses_group_transform() {
        let mut group = shape("media", 160.0, 90.0);
        group.kind = ShapeKind::Group;
        group.x = Some(20.0);
        group.y = Some(10.0);
        group.children = vec![image_shape("images/hero.webp", 160.0, 90.0)];

        let mut diagram = Diagram {
            id: None,
            width: 200.0,
            height: 120.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![group],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<g transform=\"translate(20,10)\">"));
        assert!(svg.contains("<image href=\"images/hero.webp\""));
    }

    #[test]
    fn map_shape_renders_image_viewport_and_children() {
        let mut map = map_shape("images/world.png", 300.0, 180.0);
        map.x = Some(10.0);
        map.y = Some(20.0);
        map.attrs
            .insert("content_width".to_string(), "1200".to_string());
        map.attrs
            .insert("content_height".to_string(), "800".to_string());
        map.attrs.insert("view_x".to_string(), "300".to_string());
        map.attrs.insert("view_y".to_string(), "200".to_string());
        map.attrs
            .insert("view_width".to_string(), "300".to_string());
        map.attrs
            .insert("view_height".to_string(), "180".to_string());
        map.attrs
            .insert("background_fill".to_string(), "#0f172a".to_string());
        let mut marker = shape("pin", 12.0, 12.0);
        marker.kind = ShapeKind::Circle;
        marker.x = Some(740.0);
        marker.y = Some(410.0);
        marker
            .attrs
            .insert("fill".to_string(), "#ef4444".to_string());
        marker
            .attrs
            .insert("map_fixed".to_string(), "true".to_string());
        marker
            .attrs
            .insert("map_anchor_x".to_string(), "746".to_string());
        marker
            .attrs
            .insert("map_anchor_y".to_string(), "428".to_string());
        map.children.push(marker);

        let mut diagram = Diagram {
            id: Some("map_demo".to_string()),
            width: 340.0,
            height: 220.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![map],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("data-wdoc-map=\"true\""));
        assert!(svg.contains("x=\"10\" y=\"20\" width=\"300\" height=\"180\""));
        assert!(svg.contains("viewBox=\"300 200 300 180\""));
        assert!(svg.contains("data-wdoc-map-content-width=\"1200\""));
        assert!(svg.contains("data-wdoc-map-content-height=\"800\""));
        assert!(svg.contains(
            "<image href=\"images/world.png\" x=\"0\" y=\"0\" width=\"1200\" height=\"800\""
        ));
        assert!(svg.contains("fill=\"#0f172a\""));
        assert!(svg.contains("<circle cx=\"746\" cy=\"416\" r=\"6\" fill=\"#ef4444\""));
        assert!(svg.contains("data-wdoc-map-fixed=\"true\""));
        assert!(svg.contains("data-wdoc-map-anchor-x=\"746\""));
        assert!(svg.contains("data-wdoc-map-anchor-y=\"428\""));
        assert!(svg.contains("<script>"));
        assert!(svg.contains("initMap"));
        assert!(svg.contains("updateMapFixedShapes"));
    }

    #[test]
    fn draggable_map_marker_emits_signal_drag_runtime() {
        let mut map = map_shape("images/world.png", 300.0, 180.0);
        map.attrs
            .insert("content_width".to_string(), "1200".to_string());
        map.attrs
            .insert("content_height".to_string(), "800".to_string());
        let mut marker = shape("camp_marker", 24.0, 24.0);
        marker.kind = ShapeKind::Icon;
        marker.x = Some(471.0);
        marker.y = Some(329.0);
        marker
            .attrs
            .insert("map_fixed".to_string(), "true".to_string());
        marker
            .attrs
            .insert("map_anchor_x".to_string(), "483".to_string());
        marker
            .attrs
            .insert("map_anchor_y".to_string(), "341".to_string());
        marker
            .attrs
            .insert("draggable".to_string(), "true".to_string());
        marker
            .attrs
            .insert("drag_signal".to_string(), "camp_marker".to_string());
        marker
            .attrs
            .insert("drag_x_path".to_string(), "x".to_string());
        marker
            .attrs
            .insert("drag_y_path".to_string(), "y".to_string());
        map.children.push(marker);

        let mut diagram = Diagram {
            id: Some("drag_map".to_string()),
            width: 340.0,
            height: 220.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![map],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("data-wdoc-draggable=\"true\""));
        assert!(svg.contains("data-wdoc-drag-signal=\"camp_marker\""));
        assert!(svg.contains("data-wdoc-drag-x-path=\"x\""));
        assert!(svg.contains("data-wdoc-drag-y-path=\"y\""));
        assert!(svg.contains("initDraggable"));
        assert!(svg.contains("data-wdoc-map-anchor-x"));
        assert!(svg.contains("map_anchor_x"));
        assert!(svg.contains("__wdocSetSignal"));
    }

    #[test]
    fn map_children_resolve_against_content_size() {
        let mut map = map_shape("images/world.png", 300.0, 180.0);
        map.attrs
            .insert("content_width".to_string(), "1200".to_string());
        map.attrs
            .insert("content_height".to_string(), "800".to_string());
        let mut anchored = shape("corner", 20.0, 20.0);
        anchored.left = Some(1180.0);
        anchored.top = Some(780.0);
        anchored
            .attrs
            .insert("fill".to_string(), "#22c55e".to_string());
        map.children.push(anchored);

        let mut diagram = Diagram {
            id: Some("map_demo".to_string()),
            width: 300.0,
            height: 180.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![map],
            connections: vec![],
        };

        render_diagram_svg(&mut diagram);
        let child = &diagram.shapes[0].children[0];
        assert_eq!(child.resolved.x, 1180.0);
        assert_eq!(child.resolved.y, 780.0);
    }

    #[test]
    fn top_level_shapes_render_by_z_index_then_source_order() {
        let mut low = shape("low", 20.0, 20.0);
        low.attrs.insert("fill".to_string(), "blue".to_string());
        low.z_index = -1.0;
        let mut same_a = shape("same-a", 20.0, 20.0);
        same_a.attrs.insert("fill".to_string(), "green".to_string());
        let mut same_b = shape("same-b", 20.0, 20.0);
        same_b
            .attrs
            .insert("fill".to_string(), "yellow".to_string());
        let mut high = shape("high", 20.0, 20.0);
        high.attrs.insert("fill".to_string(), "red".to_string());
        high.z_index = 10.0;

        let mut diagram = Diagram {
            id: None,
            width: 100.0,
            height: 100.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![high, same_a, low, same_b],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        let blue = index_of(&svg, "fill=\"blue\"");
        let green = index_of(&svg, "fill=\"green\"");
        let yellow = index_of(&svg, "fill=\"yellow\"");
        let red = index_of(&svg, "fill=\"red\"");
        assert!(blue < green);
        assert!(green < yellow);
        assert!(yellow < red);
    }

    #[test]
    fn group_z_index_moves_entire_subtree_and_children_sort_locally() {
        let mut background = shape("background", 20.0, 20.0);
        background
            .attrs
            .insert("fill".to_string(), "background".to_string());
        background.z_index = 1.0;

        let mut group = shape("group", 20.0, 20.0);
        group.kind = ShapeKind::Group;
        group.z_index = 5.0;
        let mut top_child = shape("top-child", 20.0, 20.0);
        top_child
            .attrs
            .insert("fill".to_string(), "top-child".to_string());
        top_child.z_index = 2.0;
        let mut bottom_child = shape("bottom-child", 20.0, 20.0);
        bottom_child
            .attrs
            .insert("fill".to_string(), "bottom-child".to_string());
        bottom_child.z_index = -2.0;
        group.children = vec![top_child, bottom_child];

        let mut diagram = Diagram {
            id: None,
            width: 100.0,
            height: 100.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![group, background],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        let background = index_of(&svg, "fill=\"background\"");
        let group_start = index_of(&svg, "<g transform=");
        let bottom_child = index_of(&svg, "fill=\"bottom-child\"");
        let top_child = index_of(&svg, "fill=\"top-child\"");
        assert!(background < group_start);
        assert!(group_start < bottom_child);
        assert!(bottom_child < top_child);
    }

    #[test]
    fn game_layer_defaults_to_parent_size_and_renders_children_only() {
        let mut layer = shape("terrain", 0.0, 0.0);
        layer.kind = ShapeKind::GameLayer;
        layer.width = None;
        layer.height = None;
        let mut tile = shape("tile", 40.0, 30.0);
        tile.attrs.insert("fill".to_string(), "terrain".to_string());
        layer.children = vec![tile];

        let mut diagram = Diagram {
            id: None,
            width: 320.0,
            height: 180.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![layer],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert_eq!(diagram.shapes[0].resolved.width, 320.0);
        assert_eq!(diagram.shapes[0].resolved.height, 180.0);
        assert!(svg.contains("data-wdoc-game-layer=\"true\""));
        assert!(svg.contains("<rect x=\"0\" y=\"0\" width=\"40\" height=\"30\""));
        assert!(svg.contains("fill=\"terrain\""));
        assert!(!svg.contains("width=\"320\" height=\"180\" fill"));
    }

    #[test]
    fn game_layer_z_index_and_child_z_index_are_local() {
        let mut background = shape("background", 20.0, 20.0);
        background
            .attrs
            .insert("fill".to_string(), "background".to_string());
        background.z_index = 1.0;

        let mut layer = shape("actors", 100.0, 80.0);
        layer.kind = ShapeKind::GameLayer;
        layer.z_index = 5.0;
        let mut front = shape("front", 20.0, 20.0);
        front.attrs.insert("fill".to_string(), "front".to_string());
        front.z_index = 2.0;
        let mut back = shape("back", 20.0, 20.0);
        back.attrs.insert("fill".to_string(), "back".to_string());
        back.z_index = -2.0;
        layer.children = vec![front, back];

        let mut diagram = Diagram {
            id: None,
            width: 160.0,
            height: 100.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![layer, background],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        let background = index_of(&svg, "fill=\"background\"");
        let layer_start = index_of(&svg, "data-wdoc-game-layer=\"true\"");
        let back = index_of(&svg, "fill=\"back\"");
        let front = index_of(&svg, "fill=\"front\"");
        assert!(background < layer_start);
        assert!(layer_start < back);
        assert!(back < front);
    }

    #[test]
    fn game_layer_clip_and_pan_zoom_runtime_attrs_render() {
        let mut layer = shape("hud", 200.0, 120.0);
        layer.kind = ShapeKind::GameLayer;
        layer.x = Some(12.0);
        layer.y = Some(16.0);
        layer.attrs.insert("clip".to_string(), "true".to_string());
        layer.attrs.insert("locked".to_string(), "true".to_string());
        layer
            .attrs
            .insert("parallax".to_string(), "0.4".to_string());
        layer.children = vec![shape("panel", 60.0, 30.0)];

        let mut options = IndexMap::new();
        options.insert("mode".to_string(), "pan_zoom".to_string());
        let mut diagram = Diagram {
            id: Some("layered_scene".to_string()),
            width: 320.0,
            height: 180.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options,
            classes: IndexMap::new(),
            shapes: vec![layer],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<clipPath id=\"wdoc-game-layer-clip-"));
        assert!(svg.contains("clip-path=\"url(#wdoc-game-layer-clip-"));
        assert!(svg.contains("data-wdoc-game-layer-parallax=\"0.4\""));
        assert!(svg.contains("data-wdoc-game-layer-locked=\"true\""));
        assert!(svg.contains("updateGameLayers"));
        assert!(svg.contains("data-wdoc-pan-zoom=\"true\""));
    }

    #[test]
    fn diagram_connections_interleave_with_shapes_by_z_index() {
        let mut a = shape("a", 20.0, 20.0);
        a.x = Some(0.0);
        a.y = Some(0.0);
        a.attrs.insert("fill".to_string(), "blue".to_string());
        let mut b = shape("b", 20.0, 20.0);
        b.x = Some(60.0);
        b.y = Some(0.0);
        b.attrs.insert("fill".to_string(), "red".to_string());
        b.z_index = 10.0;

        let mut conn = connection("a", "b");
        conn.direction = Direction::None;
        conn.z_index = 5.0;
        conn.attrs
            .insert("stroke".to_string(), "purple".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 100.0,
            height: 40.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![b, a],
            connections: vec![conn],
        };

        let svg = render_diagram_svg(&mut diagram);
        let blue = index_of(&svg, "fill=\"blue\"");
        let purple = index_of(&svg, "stroke=\"purple\"");
        let red = index_of(&svg, "fill=\"red\"");
        assert!(blue < purple);
        assert!(purple < red);
    }

    #[test]
    fn diagram_connections_preserve_source_order_ties_with_shapes() {
        let mut a = shape("a", 20.0, 20.0);
        a.x = Some(0.0);
        a.y = Some(0.0);
        a.attrs.insert("fill".to_string(), "blue".to_string());
        a.source_order = 0;
        let mut b = shape("b", 20.0, 20.0);
        b.x = Some(60.0);
        b.y = Some(0.0);
        b.attrs.insert("fill".to_string(), "red".to_string());
        b.source_order = 2;

        let mut conn = connection("a", "b");
        conn.direction = Direction::None;
        conn.source_order = 1;
        conn.attrs
            .insert("stroke".to_string(), "purple".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 100.0,
            height: 40.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![b, a],
            connections: vec![conn],
        };

        let svg = render_diagram_svg(&mut diagram);
        let blue = index_of(&svg, "fill=\"blue\"");
        let purple = index_of(&svg, "stroke=\"purple\"");
        let red = index_of(&svg, "fill=\"red\"");
        assert!(blue < purple);
        assert!(purple < red);
    }

    #[test]
    fn group_can_wrap_background_and_label_as_interactive_control() {
        let mut group = shape("button", 120.0, 36.0);
        group.kind = ShapeKind::Group;
        group.x = Some(10.0);
        group.y = Some(20.0);
        group
            .attrs
            .insert("href".to_string(), "#clicked".to_string());
        group
            .attrs
            .insert("class".to_string(), "ui-button".to_string());
        group
            .attrs
            .insert("cursor".to_string(), "pointer".to_string());
        group
            .attrs
            .insert("pointer_events".to_string(), "all".to_string());

        let mut bg = shape("bg", 120.0, 36.0);
        bg.attrs
            .insert("class".to_string(), "ui-button-bg".to_string());
        bg.attrs.insert("fill".to_string(), "#5e81ac".to_string());
        group.children = vec![bg, text_shape("Save", 120.0, 36.0)];

        let mut diagram = Diagram {
            id: None,
            width: 160.0,
            height: 80.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![group],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("<a href=\"#clicked\" target=\"_top\"><g"));
        assert!(svg.contains("class=\"ui-button\""));
        assert!(svg.contains("cursor=\"pointer\""));
        assert!(svg.contains("pointer-events=\"all\""));
        assert!(svg.contains("class=\"ui-button-bg\""));
        assert!(svg.contains("</g></a>"));
    }

    #[test]
    fn safe_svg_attrs_render_on_existing_primitives() {
        let mut rect = shape("box", 100.0, 40.0);
        rect.attrs.insert("fill".to_string(), "#ccc".to_string());
        rect.attrs.insert("class".to_string(), "card".to_string());
        rect.attrs
            .insert("style".to_string(), "filter:url(#shadow)".to_string());
        rect.attrs
            .insert("cursor".to_string(), "pointer".to_string());
        rect.attrs
            .insert("pointer_events".to_string(), "bounding-box".to_string());

        let mut diagram = Diagram {
            id: None,
            width: 120.0,
            height: 60.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![rect],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("fill=\"#ccc\""));
        assert!(svg.contains("class=\"card\""));
        assert!(svg.contains("style=\"filter:url(#shadow)\""));
        assert!(svg.contains("cursor=\"pointer\""));
        assert!(svg.contains("pointer-events=\"bounding-box\""));
    }

    #[test]
    fn diagram_classes_apply_base_properties_and_emit_state_css() {
        let mut class = DiagramClass {
            name: "card".to_string(),
            attrs: IndexMap::new(),
            states: IndexMap::new(),
            animations: IndexMap::new(),
        };
        class.attrs.insert("fill".to_string(), "#fff".to_string());
        class
            .attrs
            .insert("stroke_width".to_string(), "1".to_string());
        class.attrs.insert("z_index".to_string(), "10".to_string());
        let mut hovered = DiagramState {
            name: "hovered".to_string(),
            attrs: IndexMap::new(),
        };
        hovered
            .attrs
            .insert("stroke".to_string(), "#3b82f6".to_string());
        hovered
            .attrs
            .insert("z_index".to_string(), "20".to_string());
        class.states.insert("hovered".to_string(), hovered);

        let mut rect = shape("task", 100.0, 40.0);
        rect.attrs.insert("class".to_string(), "card".to_string());

        let mut diagram = Diagram {
            id: Some("classes".to_string()),
            width: 120.0,
            height: 60.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::from([("card".to_string(), class)]),
            shapes: vec![rect],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("fill=\"#fff\""));
        assert!(svg.contains("stroke-width=\"1\""));
        assert!(svg.contains(".card.wdoc-state-hovered"));
        assert!(svg.contains("stroke: #3b82f6;"));
        assert!(svg.contains("data-wdoc-state-z=\"hovered:20\""));
        assert!(!svg.contains("z-index"));
    }

    #[test]
    fn diagram_events_emit_runtime_metadata_and_script() {
        let mut rect = shape("task", 100.0, 40.0);
        rect.events.push(DiagramEvent {
            name: Some("select".to_string()),
            trigger: "click".to_string(),
            state: "selected".to_string(),
            mode: Some("toggle".to_string()),
            ..Default::default()
        });

        let mut diagram = Diagram {
            id: Some("events".to_string()),
            width: 120.0,
            height: 60.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![rect],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("data-wdoc-events=\"click|selected|self|toggle|left|0|false\""));
        assert!(svg.contains("<script>"));
        assert!(svg.contains("wdoc-state-"));
    }

    #[test]
    fn diagram_events_emit_signal_actions() {
        let mut rect = shape("slider_thumb", 20.0, 20.0);
        rect.events.push(DiagramEvent {
            trigger: "click".to_string(),
            signal_actions: vec![SignalAction {
                signal: "progress".to_string(),
                value: serde_json::json!("$event.value"),
                path: Some("value".to_string()),
            }],
            ..Default::default()
        });

        let mut diagram = Diagram {
            id: Some("signals".to_string()),
            width: 80.0,
            height: 40.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![rect],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("data-wdoc-events=\"click||self||left|0|false||"));
        assert!(svg.contains("&quot;signal&quot;:&quot;progress&quot;"));
        assert!(svg.contains("runActions"));
        assert!(svg.contains("__wdocSetSignal"));
    }

    #[test]
    fn textbox_signal_widget_emits_runtime_metadata_and_script() {
        let mut textbox = shape("notes_editor", 240.0, 90.0);
        textbox.kind = ShapeKind::Group;
        textbox
            .attrs
            .insert("data_wdoc_textbox".to_string(), "true".to_string());
        textbox
            .attrs
            .insert("data_wdoc_textbox_signal".to_string(), "notes".to_string());
        textbox.attrs.insert(
            "data_wdoc_textbox_debounce_ms".to_string(),
            "300".to_string(),
        );
        let mut value = text_shape("Edit this note", 220.0, 40.0);
        value.id = Some("notes_editor_value".to_string());
        value.attrs.insert(
            "data_wdoc_textbox_value_node".to_string(),
            "true".to_string(),
        );
        textbox.children.push(value);

        let mut diagram = Diagram {
            id: Some("textbox".to_string()),
            width: 260.0,
            height: 120.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![textbox],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("data-wdoc-textbox=\"true\""));
        assert!(svg.contains("data-wdoc-textbox-signal=\"notes\""));
        assert!(svg.contains("data-wdoc-textbox-debounce-ms=\"300\""));
        assert!(svg.contains("data-wdoc-textbox-value-node=\"true\""));
        assert!(svg.contains("initTextbox"));
        assert!(svg.contains("data-wdoc-textbox-multiline"));
        assert!(svg.contains("createElement(multiline?'textarea':'input')"));
        assert!(svg.contains("!multiline&&e.key==='Enter'"));
        assert!(svg.contains("__wdocSetSignal"));
    }

    #[test]
    fn signal_form_widgets_emit_runtime_metadata_and_script() {
        let mut checkbox = shape("enabled_toggle", 160.0, 28.0);
        checkbox.kind = ShapeKind::Group;
        checkbox
            .attrs
            .insert("data_wdoc_checkbox".to_string(), "true".to_string());
        checkbox.attrs.insert(
            "data_wdoc_checkbox_signal".to_string(),
            "enabled".to_string(),
        );
        checkbox
            .attrs
            .insert("data_wdoc_checkbox_checked".to_string(), "true".to_string());
        let mut checkbox_box = shape("enabled_box", 16.0, 16.0);
        checkbox_box
            .attrs
            .insert("data_wdoc_checkbox_box".to_string(), "true".to_string());
        checkbox.children.push(checkbox_box);

        let mut radio = shape("plan_team", 160.0, 28.0);
        radio.kind = ShapeKind::Group;
        radio
            .attrs
            .insert("data_wdoc_radio".to_string(), "true".to_string());
        radio
            .attrs
            .insert("data_wdoc_radio_signal".to_string(), "plan".to_string());
        radio
            .attrs
            .insert("data_wdoc_radio_value".to_string(), "Team".to_string());
        let mut radio_dot = shape("plan_dot", 8.0, 8.0);
        radio_dot.kind = ShapeKind::Circle;
        radio_dot
            .attrs
            .insert("data_wdoc_radio_dot".to_string(), "true".to_string());
        radio.children.push(radio_dot);

        let mut dropdown = shape("environment_picker", 200.0, 56.0);
        dropdown.kind = ShapeKind::Group;
        dropdown
            .attrs
            .insert("data_wdoc_dropdown".to_string(), "true".to_string());
        dropdown.attrs.insert(
            "data_wdoc_dropdown_signal".to_string(),
            "environment".to_string(),
        );
        dropdown.attrs.insert(
            "data_wdoc_dropdown_items".to_string(),
            "Dev,Stage,Prod".to_string(),
        );
        let mut value = text_shape("Stage", 180.0, 38.0);
        value.id = Some("environment_value".to_string());
        value.attrs.insert(
            "data_wdoc_dropdown_value_node".to_string(),
            "true".to_string(),
        );
        dropdown.children.push(value);

        let mut diagram = Diagram {
            id: Some("signal_widgets".to_string()),
            width: 560.0,
            height: 180.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![checkbox, radio, dropdown],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("data-wdoc-checkbox=\"true\""));
        assert!(svg.contains("data-wdoc-checkbox-signal=\"enabled\""));
        assert!(svg.contains("data-wdoc-radio=\"true\""));
        assert!(svg.contains("data-wdoc-radio-value=\"Team\""));
        assert!(svg.contains("data-wdoc-dropdown=\"true\""));
        assert!(svg.contains("data-wdoc-dropdown-items=\"Dev,Stage,Prod\""));
        assert!(svg.contains("initCheckbox"));
        assert!(svg.contains("initRadio"));
        assert!(svg.contains("initDropdown"));
        assert!(svg.contains("__wdocSetSignal"));
    }

    #[test]
    fn right_click_events_prevent_default_and_target_stateful_shapes() {
        let mut menu_class = DiagramClass {
            name: "popup_menu".to_string(),
            attrs: IndexMap::new(),
            states: IndexMap::new(),
            animations: IndexMap::new(),
        };
        menu_class
            .attrs
            .insert("visible".to_string(), "false".to_string());
        let mut shown = DiagramState {
            name: "shown".to_string(),
            attrs: IndexMap::new(),
        };
        shown
            .attrs
            .insert("visible".to_string(), "true".to_string());
        menu_class.states.insert("shown".to_string(), shown);

        let mut source = shape("button", 100.0, 40.0);
        source.events.push(DiagramEvent {
            trigger: "right_click".to_string(),
            target: Some("menu".to_string()),
            state: "shown".to_string(),
            mode: Some("toggle".to_string()),
            ..Default::default()
        });

        let mut menu = shape("menu", 120.0, 80.0);
        menu.x = Some(120.0);
        menu.attrs
            .insert("class".to_string(), "popup_menu".to_string());

        let mut diagram = Diagram {
            id: Some("menu".to_string()),
            width: 280.0,
            height: 120.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::from([("popup_menu".to_string(), menu_class)]),
            shapes: vec![source, menu],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("data-wdoc-events=\"right_click|shown|menu|toggle|left|0|true\""));
        assert!(svg.contains("data-wdoc-id=\"menu\""));
        assert!(svg.contains("contextmenu"));
    }

    #[test]
    fn state_animation_emits_runtime_metadata_and_script() {
        let mut animation = DiagramAnimation {
            name: "slide".to_string(),
            duration_ms: 800,
            delay_ms: 25,
            timing_function: "ease-in-out".to_string(),
            iteration_count: "infinite".to_string(),
            direction: "alternate".to_string(),
            fill_mode: "both".to_string(),
            keyframes: Vec::new(),
            frame_rate: None,
            frames: Vec::new(),
        };
        animation.keyframes.push(DiagramKeyframe {
            offset: 0.0,
            x: Some(10.0),
            y: Some(10.0),
            width: Some(100.0),
            height: Some(40.0),
            rotate: Some(0.0),
            rotate_origin_x: None,
            rotate_origin_y: None,
        });
        animation.keyframes.push(DiagramKeyframe {
            offset: 100.0,
            x: Some(80.0),
            y: Some(20.0),
            width: Some(120.0),
            height: Some(50.0),
            rotate: Some(90.0),
            rotate_origin_x: Some(100.0),
            rotate_origin_y: Some(45.0),
        });

        let mut state = DiagramState {
            name: "active".to_string(),
            attrs: IndexMap::new(),
        };
        state
            .attrs
            .insert("animation".to_string(), "slide".to_string());

        let class = DiagramClass {
            name: "card".to_string(),
            attrs: IndexMap::new(),
            states: IndexMap::from([("active".to_string(), state)]),
            animations: IndexMap::from([("slide".to_string(), animation)]),
        };

        let mut rect = shape("task", 100.0, 40.0);
        rect.attrs.insert("class".to_string(), "card".to_string());

        let mut diagram = Diagram {
            id: Some("animation".to_string()),
            width: 180.0,
            height: 90.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::from([("card".to_string(), class)]),
            shapes: vec![rect],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("data-wdoc-state-animation=\"active:slide\""));
        assert!(svg
            .contains("data-wdoc-animations=\"slide|800|25|ease-in-out|infinite|alternate|both|"));
        assert!(svg.contains("100,80,20,120,50,90,100,45"));
        assert!(svg.contains("data-wdoc-x=\"0\""));
        assert!(svg.contains("<script>"));
        assert!(svg.contains("startStateAnimation"));
        assert!(svg.contains("rotate_origin_x"));
        assert!(svg.contains("localRotation"));
    }

    #[test]
    fn diagram_css_is_scoped_inside_svg() {
        let mut options = IndexMap::new();
        options.insert(
            "css".to_string(),
            ".ui-button:hover .ui-button-bg { fill: #81A1C1; stroke: #81A1C1; }\n\
             .ui-button:active .ui-button-bg { fill: #4C566A; stroke: #4C566A; }"
                .to_string(),
        );

        let mut diagram = Diagram {
            id: Some("button_preview".to_string()),
            width: 160.0,
            height: 80.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options,
            classes: IndexMap::new(),
            shapes: vec![shape("box", 100.0, 40.0)],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("id=\"wdoc-diagram-button-preview\""));
        assert!(svg.contains("<style>"));
        assert!(svg.contains("#wdoc-diagram-button-preview .ui-button:hover .ui-button-bg"));
        assert!(svg.contains("#wdoc-diagram-button-preview .ui-button:active .ui-button-bg"));
    }

    #[test]
    fn diagram_without_css_does_not_emit_svg_scope_or_style() {
        let mut diagram = Diagram {
            id: Some("plain".to_string()),
            width: 120.0,
            height: 60.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![shape("box", 100.0, 40.0)],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(!svg.contains("id=\"wdoc-diagram-plain\""));
        assert!(!svg.contains("<style>"));
    }

    #[test]
    fn pan_zoom_diagram_emits_runtime_metadata_and_script() {
        let mut options = IndexMap::new();
        options.insert("mode".to_string(), "pan_zoom".to_string());
        let mut diagram = Diagram {
            id: Some("huge_graph".to_string()),
            width: 120.0,
            height: 60.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options,
            classes: IndexMap::new(),
            shapes: vec![shape("box", 100.0, 40.0)],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(svg.contains("id=\"wdoc-diagram-huge-graph\""));
        assert!(svg.contains("data-wdoc-pan-zoom=\"true\""));
        assert!(svg.contains("data-wdoc-pan-zoom-min=\"0.25\""));
        assert!(svg.contains("data-wdoc-pan-zoom-max=\"8\""));
        assert!(svg.contains("data-wdoc-pan-zoom-controls=\"true\""));
        assert!(svg.contains("data-wdoc-pan-zoom-control=\"in\""));
        assert!(svg.contains("data-wdoc-pan-zoom-control=\"out\""));
        assert!(svg.contains("data-wdoc-pan-zoom-control=\"reset\""));
        assert!(svg.contains("<script>"));
        assert!(svg.contains("initPanZoom"));
        assert!(svg.contains("zoomBy"));
        assert!(svg.contains("wheel"));
    }

    #[test]
    fn static_diagram_does_not_emit_pan_zoom_runtime() {
        let mut diagram = Diagram {
            id: Some("plain".to_string()),
            width: 120.0,
            height: 60.0,
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            classes: IndexMap::new(),
            shapes: vec![shape("box", 100.0, 40.0)],
            connections: vec![],
        };

        let svg = render_diagram_svg(&mut diagram);
        assert!(!svg.contains("data-wdoc-pan-zoom"));
        assert!(!svg.contains("data-wdoc-pan-zoom-control"));
        assert!(!svg.contains("<script>"));
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
            id: None,
            width: 240.0,
            height: 180.0,
            shapes: vec![boundary],
            connections: vec![],
            classes: IndexMap::new(),
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
    fn nested_grid_layout_preserves_explicitly_positioned_children() {
        let mut pinned = shape("pinned", 30.0, 20.0);
        pinned.x = Some(150.0);
        pinned.y = Some(90.0);

        let mut container = shape("container", 220.0, 140.0);
        container.align = Alignment::Grid;
        container.gap = 10.0;
        container
            .attrs
            .insert("columns".to_string(), "2".to_string());
        container.children = vec![shape("a", 30.0, 20.0), pinned, shape("b", 30.0, 20.0)];

        let mut diagram = Diagram {
            id: None,
            width: 240.0,
            height: 160.0,
            shapes: vec![container],
            connections: vec![],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let children = &diagram.shapes[0].children;
        assert_eq!(children[1].resolved.x, 150.0);
        assert_eq!(children[1].resolved.y, 90.0);
        assert_ne!(children[0].resolved.x, 0.0);
        assert!(children[2].resolved.x > children[0].resolved.x);
    }

    #[test]
    fn nested_stack_layout_respects_widget_content_insets_and_padding() {
        let mut container = shape("container", 200.0, 180.0);
        container.align = Alignment::Stack;
        container.gap = 8.0;
        container.padding = 12.0;
        container
            .attrs
            .insert(CONTENT_INSET_TOP_ATTR.to_string(), "36".to_string());
        container
            .attrs
            .insert(CONTENT_INSET_BOTTOM_ATTR.to_string(), "20".to_string());
        container.children = vec![shape("a", 0.0, 24.0), shape("b", 0.0, 24.0)];

        let mut diagram = Diagram {
            id: None,
            width: 240.0,
            height: 220.0,
            shapes: vec![container],
            connections: vec![],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let children = &diagram.shapes[0].children;
        assert_eq!(children[0].resolved.x, 12.0);
        assert_eq!(children[0].resolved.y, 48.0);
        assert_eq!(children[0].resolved.width, 176.0);
        assert_eq!(children[1].resolved.y, 80.0);
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
            id: None,
            width: 200.0,
            height: 200.0,
            shapes: vec![container],
            connections: vec![],
            classes: IndexMap::new(),
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
            id: None,
            width: 300.0,
            height: 240.0,
            shapes: vec![boundary],
            connections: vec![connection("boundary.a", "boundary.b")],
            classes: IndexMap::new(),
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
    fn parent_graph_layout_uses_dotted_connection_owners() {
        let mut group = shape("group", 140.0, 100.0);
        group.children = vec![shape("inner", 60.0, 40.0)];
        let leaf = shape("leaf", 60.0, 40.0);

        let mut diagram = Diagram {
            id: None,
            width: 260.0,
            height: 220.0,
            shapes: vec![group, leaf],
            connections: vec![connection("group.inner", "leaf")],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::Layered,
            gap: 24.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let group = &diagram.shapes[0];
        let leaf = &diagram.shapes[1];
        assert!(group.resolved.y < leaf.resolved.y);
    }

    #[test]
    fn parent_graph_layout_ignores_same_owner_dotted_connections() {
        let children = vec![shape("group", 140.0, 100.0), shape("peer", 60.0, 40.0)];
        let connections = vec![connection("group.a", "group.b")];

        assert!(localize_connections(&children, &connections, "").is_empty());
    }

    #[test]
    fn force_layout_marks_connections_for_direct_routing() {
        let mut diagram = Diagram {
            id: None,
            width: 300.0,
            height: 220.0,
            shapes: vec![
                shape("a", 60.0, 40.0),
                shape("b", 60.0, 40.0),
                shape("c", 60.0, 40.0),
            ],
            connections: vec![connection("a", "b"), connection("b", "c")],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::Force,
            gap: 40.0,
            options: IndexMap::new(),
        };

        let svg = render_diagram_svg(&mut diagram);

        assert!(diagram.connections.iter().all(|conn| {
            conn.attrs.get(CONNECTION_ROUTE_ATTR).map(String::as_str)
                == Some(CONNECTION_ROUTE_DIRECT)
        }));
        assert!(svg.contains("<line "));
        assert!(!svg.contains(CONNECTION_ROUTE_ATTR));
    }

    #[test]
    fn radial_layout_marks_connections_for_direct_routing() {
        let mut options = IndexMap::new();
        options.insert("root".to_string(), "root".to_string());
        let mut diagram = Diagram {
            id: None,
            width: 300.0,
            height: 260.0,
            shapes: vec![
                shape("root", 60.0, 40.0),
                shape("left", 60.0, 40.0),
                shape("right", 60.0, 40.0),
            ],
            connections: vec![connection("root", "left"), connection("root", "right")],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::Radial,
            gap: 40.0,
            options,
        };

        let svg = render_diagram_svg(&mut diagram);

        assert!(diagram.connections.iter().all(|conn| {
            conn.attrs.get(CONNECTION_ROUTE_ATTR).map(String::as_str)
                == Some(CONNECTION_ROUTE_DIRECT)
        }));
        assert!(svg.contains("<line "));
        assert!(!svg.contains(CONNECTION_ROUTE_ATTR));
    }

    #[test]
    fn nested_force_layout_marks_only_local_connections_direct() {
        let mut boundary = shape("boundary", 220.0, 160.0);
        boundary.x = Some(10.0);
        boundary.y = Some(10.0);
        boundary.align = Alignment::Force;
        boundary.gap = 30.0;
        boundary.children = vec![shape("a", 50.0, 30.0), shape("b", 50.0, 30.0)];

        let mut external_a = shape("external_a", 50.0, 30.0);
        external_a.x = Some(260.0);
        external_a.y = Some(30.0);
        let mut external_b = shape("external_b", 50.0, 30.0);
        external_b.x = Some(260.0);
        external_b.y = Some(110.0);

        let mut diagram = Diagram {
            id: None,
            width: 340.0,
            height: 220.0,
            shapes: vec![boundary, external_a, external_b],
            connections: vec![
                connection("boundary.a", "boundary.b"),
                connection("external_a", "external_b"),
            ],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        assert_eq!(
            diagram.connections[0]
                .attrs
                .get(CONNECTION_ROUTE_ATTR)
                .map(String::as_str),
            Some(CONNECTION_ROUTE_DIRECT)
        );
        assert_eq!(
            diagram.connections[1]
                .attrs
                .get(CONNECTION_ROUTE_ATTR)
                .map(String::as_str),
            None
        );
    }

    #[test]
    fn top_level_layered_layout_respects_diagram_padding() {
        let mut diagram = Diagram {
            id: None,
            width: 240.0,
            height: 180.0,
            shapes: vec![shape("a", 100.0, 60.0), shape("b", 100.0, 60.0)],
            connections: vec![connection("a", "b")],
            classes: IndexMap::new(),
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
            id: None,
            width: 200.0,
            height: 200.0,
            shapes: vec![boundary],
            connections: vec![
                connection("boundary.a", "boundary.b"),
                connection("boundary.b", "boundary.c"),
            ],
            classes: IndexMap::new(),
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
            id: None,
            width: 220.0,
            height: 220.0,
            shapes: vec![boundary],
            connections: vec![connection("boundary.a", "boundary.b")],
            classes: IndexMap::new(),
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
            id: None,
            width: 240.0,
            height: 220.0,
            shapes: vec![boundary],
            connections: vec![connection("boundary.a", "boundary.b")],
            classes: IndexMap::new(),
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
    fn fixed_nested_layered_layout_scales_wide_rank_to_boundary() {
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
            id: None,
            width: 240.0,
            height: 300.0,
            shapes: vec![boundary],
            connections: vec![
                connection("boundary.a", "boundary.d"),
                connection("boundary.b", "boundary.d"),
                connection("boundary.c", "boundary.d"),
            ],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let boundary = &diagram.shapes[0];
        let children = &boundary.children;
        assert_eq!(boundary.resolved.height, 80.0);
        assert!(children[0].resolved.width < 80.0);
        assert!(children[1].resolved.width < 80.0);
        assert!(!overlaps(&children[0], &children[1]));
        assert!(!overlaps(&children[1], &children[2]));
        for child in children {
            assert!(child.resolved.x + child.resolved.width <= boundary.resolved.width);
            assert!(child.resolved.y + child.resolved.height <= boundary.resolved.height);
        }
    }

    #[test]
    fn autosized_nested_graph_is_sized_before_parent_layout() {
        let left = shape("left", 80.0, 40.0);
        let right = shape("right", 80.0, 40.0);

        let mut boundary = shape("boundary", 0.0, 0.0);
        boundary.width = None;
        boundary.height = None;
        boundary.align = Alignment::Layered;
        boundary.gap = 20.0;
        boundary
            .attrs
            .insert("direction".to_string(), "horizontal".to_string());
        boundary.children = vec![
            shape("a", 80.0, 40.0),
            shape("b", 80.0, 40.0),
            shape("c", 80.0, 40.0),
        ];

        let mut diagram = Diagram {
            id: None,
            width: 400.0,
            height: 240.0,
            shapes: vec![left, boundary, right],
            connections: vec![
                connection("boundary.a", "boundary.b"),
                connection("boundary.b", "boundary.c"),
            ],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::Layered,
            gap: 20.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let left = &diagram.shapes[0];
        let boundary = &diagram.shapes[1];
        let right = &diagram.shapes[2];
        assert!(boundary.resolved.width >= 240.0);
        assert!(!overlaps(left, boundary));
        assert!(!overlaps(boundary, right));
        assert!(!overlaps(left, right));
        for child in &boundary.children {
            assert!(child.resolved.x + child.resolved.width <= boundary.resolved.width);
        }
    }

    #[test]
    fn marked_full_container_decoration_tracks_autosized_graph() {
        let mut frame = shape("frame", 120.0, 80.0);
        frame.x = Some(0.0);
        frame.y = Some(0.0);
        frame
            .attrs
            .insert(LAYOUT_DECORATION_ATTR.to_string(), "true".to_string());
        frame.attrs.insert(
            FULL_CONTAINER_DECORATION_ATTR.to_string(),
            "true".to_string(),
        );

        let mut boundary = shape("boundary", 0.0, 0.0);
        boundary.width = None;
        boundary.height = None;
        boundary.align = Alignment::Layered;
        boundary.gap = 20.0;
        boundary.children = vec![
            frame,
            shape("a", 80.0, 40.0),
            shape("b", 80.0, 40.0),
            shape("c", 80.0, 40.0),
        ];

        let mut diagram = Diagram {
            id: None,
            width: 240.0,
            height: 320.0,
            shapes: vec![boundary],
            connections: vec![connection("boundary.a", "boundary.c")],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let boundary = &diagram.shapes[0];
        let frame = &boundary.children[0];
        assert_eq!(frame.resolved.width, boundary.resolved.width);
        assert_eq!(frame.resolved.height, boundary.resolved.height);
        for child in boundary
            .children
            .iter()
            .filter(|c| !is_layout_decoration(c))
        {
            assert!(child.resolved.y + child.resolved.height <= frame.resolved.height);
        }
    }

    #[test]
    fn autosized_nested_graphs_size_bottom_up() {
        let mut inner = shape("inner", 0.0, 0.0);
        inner.width = None;
        inner.height = None;
        inner.align = Alignment::Layered;
        inner.gap = 20.0;
        inner
            .attrs
            .insert("direction".to_string(), "horizontal".to_string());
        inner.children = vec![
            shape("a", 80.0, 40.0),
            shape("b", 80.0, 40.0),
            shape("c", 80.0, 40.0),
        ];

        let mut outer = shape("outer", 0.0, 0.0);
        outer.width = None;
        outer.height = None;
        outer.align = Alignment::Layered;
        outer.gap = 20.0;
        outer.children = vec![inner, shape("tail", 80.0, 40.0)];

        let mut diagram = Diagram {
            id: None,
            width: 520.0,
            height: 320.0,
            shapes: vec![outer],
            connections: vec![
                connection("outer.inner.a", "outer.inner.b"),
                connection("outer.inner.b", "outer.inner.c"),
                connection("outer.inner.c", "outer.tail"),
            ],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let outer = &diagram.shapes[0];
        let inner = &outer.children[0];
        let tail = &outer.children[1];
        assert!(inner.resolved.width >= 240.0);
        assert!(outer.resolved.width >= inner.resolved.width);
        assert!(!overlaps(inner, tail));
        assert!(inner.resolved.x + inner.resolved.width <= outer.resolved.width);
        assert!(tail.resolved.x + tail.resolved.width <= outer.resolved.width);
    }

    #[test]
    fn top_level_scale_to_fit_runs_after_nested_autosize() {
        let mut boundary = shape("boundary", 0.0, 0.0);
        boundary.width = None;
        boundary.height = None;
        boundary.align = Alignment::Layered;
        boundary.gap = 20.0;
        boundary
            .attrs
            .insert("direction".to_string(), "horizontal".to_string());
        boundary.children = vec![
            shape("a", 90.0, 40.0),
            shape("b", 90.0, 40.0),
            shape("c", 90.0, 40.0),
        ];

        let mut diagram = Diagram {
            id: None,
            width: 180.0,
            height: 140.0,
            shapes: vec![boundary],
            connections: vec![
                connection("boundary.a", "boundary.b"),
                connection("boundary.b", "boundary.c"),
            ],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::Layered,
            gap: 20.0,
            options: IndexMap::new(),
        };

        render_diagram_svg(&mut diagram);

        let boundary = &diagram.shapes[0];
        assert!(boundary.resolved.x >= 0.0);
        assert!(boundary.resolved.y >= 0.0);
        assert!(boundary.resolved.x + boundary.resolved.width <= diagram.width);
        assert!(boundary.resolved.y + boundary.resolved.height <= diagram.height);
        assert!(boundary.resolved.width < 270.0);
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
            id: None,
            width: 120.0,
            height: 120.0,
            shapes: vec![container],
            connections: vec![],
            classes: IndexMap::new(),
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
            id: None,
            width: 200.0,
            height: 200.0,
            shapes: vec![container],
            connections: vec![],
            classes: IndexMap::new(),
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
        assert!(points[points.len() - 2].1 < to.y);
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
    fn nearly_aligned_auto_connection_without_obstacle_keeps_line_rendering() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "a".to_string(),
            Bounds {
                x: 100.0,
                y: 40.0,
                width: 120.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "b".to_string(),
            Bounds {
                x: 102.0,
                y: 140.0,
                width: 120.0,
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
        assert!(bounds_contains_point(
            &to,
            points.last().unwrap().0,
            points.last().unwrap().1
        ));
        assert!(points[1].1 >= from.y + from.height);
        assert!(points[points.len() - 2].1 <= to.y);
    }

    #[test]
    fn explicit_anchor_connection_can_route_around_obstacle() {
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

        assert!(svg.contains("<path"));
        assert!(!svg.contains("<line"));
    }

    #[test]
    fn explicit_bottom_top_anchors_can_use_elbow_route() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "a".to_string(),
            Bounds {
                x: 20.0,
                y: 20.0,
                width: 80.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "b".to_string(),
            Bounds {
                x: 140.0,
                y: 140.0,
                width: 80.0,
                height: 40.0,
            },
        );
        let mut conn = connection("a", "b");
        conn.from_anchor = AnchorPoint::Bottom;
        conn.to_anchor = AnchorPoint::Top;
        let mut svg = String::new();
        render_connection_svg(&conn, &shape_map, &mut svg);

        assert!(svg.contains("<path"));
        assert!(!svg.contains("<line"));
    }

    #[test]
    fn graph_node_edge_ports_auto_anchor_away_from_parent_table() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "orders".to_string(),
            Bounds {
                x: 370.0,
                y: 28.0,
                width: 240.0,
                height: 142.0,
            },
        );
        shape_map.insert(
            "orders.items".to_string(),
            Bounds {
                x: 605.0,
                y: 66.0,
                width: 10.0,
                height: 10.0,
            },
        );
        shape_map.insert(
            "order_items".to_string(),
            Bounds {
                x: 370.0,
                y: 315.0,
                width: 240.0,
                height: 114.0,
            },
        );
        shape_map.insert(
            "order_items.order_id".to_string(),
            Bounds {
                x: 365.0,
                y: 386.0,
                width: 10.0,
                height: 10.0,
            },
        );

        let conn = connection("orders.items", "order_items.order_id");
        let route = compute_connection_route(&conn, &shape_map).expect("route");
        let points = route_polyline_points(&route.geometry);

        assert!(points.len() >= 4);
        assert_eq!(points[0], (615.0, 71.0));
        assert!(points[1].0 > points[0].0);
        assert_eq!(points.last().copied(), Some((365.0, 391.0)));
        assert!(points[points.len() - 2].0 < points[points.len() - 1].0);
    }

    #[test]
    fn parallel_route_offsets_keep_orthogonal_segments() {
        let mut geometry = RoutedGeometry::Orthogonal {
            points: vec![
                (615.0, 71.0),
                (639.0, 71.0),
                (639.0, 290.0),
                (341.0, 290.0),
                (341.0, 391.0),
                (365.0, 391.0),
            ],
        };

        apply_route_parallel_offset(&mut geometry, -7.0, false, ParallelOffsetAxis::Auto);
        let points = route_polyline_points(&geometry);

        assert!(points.windows(2).all(|segment| {
            nearly_eq(segment[0].0, segment[1].0) || nearly_eq(segment[0].1, segment[1].1)
        }));
    }

    #[test]
    fn short_shared_corridors_are_separated_on_the_shared_axis() {
        let mut first = connection("a", "b");
        first.source_order = 0;
        first.from_id = "users.orders".to_string();
        first.to_id = "orders.user_id".to_string();
        let mut second = connection("c", "d");
        second.source_order = 1;
        second.from_id = "users.addresses".to_string();
        second.to_id = "addresses.user_id".to_string();

        let mut routes = vec![
            RoutedConnection {
                conn: first.clone(),
                geometry: RoutedGeometry::Orthogonal {
                    points: vec![(275.0, 79.0), (299.0, 79.0), (299.0, 104.0), (365.0, 104.0)],
                },
            },
            RoutedConnection {
                conn: second.clone(),
                geometry: RoutedGeometry::Orthogonal {
                    points: vec![
                        (275.0, 88.0),
                        (299.0, 88.0),
                        (299.0, 322.0),
                        (11.0, 322.0),
                        (11.0, 416.0),
                        (35.0, 416.0),
                    ],
                },
            },
        ];

        separate_parallel_routes(&mut routes);
        let first_points = route_polyline_points(&routes[0].geometry);
        let second_points = route_polyline_points(&routes[1].geometry);

        assert_eq!(first_points[1].0, 295.5);
        assert_eq!(second_points[1].0, 302.5);
        assert_ne!(first_points[1].0, second_points[1].0);
        assert!(first_points.windows(2).all(|segment| {
            nearly_eq(segment[0].0, segment[1].0) || nearly_eq(segment[0].1, segment[1].1)
        }));
        assert!(second_points.windows(2).all(|segment| {
            nearly_eq(segment[0].0, segment[1].0) || nearly_eq(segment[0].1, segment[1].1)
        }));
    }

    #[test]
    fn route_label_uses_midpoint_along_rendered_path() {
        let route = RoutedGeometry::Orthogonal {
            points: vec![
                (100.0, 60.0),
                (100.0, 120.0),
                (220.0, 120.0),
                (220.0, 200.0),
            ],
        };

        let label = route_label_point(&route);

        assert_eq!(label, (170.0, 110.0));
    }

    #[test]
    fn vertical_route_label_offsets_to_side_of_path() {
        let route = RoutedGeometry::Orthogonal {
            points: vec![(100.0, 60.0), (100.0, 160.0)],
        };

        let label = route_label_point(&route);

        assert_eq!(label, (110.0, 110.0));
    }

    #[test]
    fn connection_label_point_applies_manual_offsets_after_auto_offset() {
        let route = RoutedGeometry::Line {
            start: (20.0, 40.0),
            end: (120.0, 40.0),
        };
        let attrs = IndexMap::from([
            ("label_dx".to_string(), "5".to_string()),
            ("label_dy".to_string(), "-3".to_string()),
        ]);

        let label = connection_label_point(&route, &attrs);

        assert_eq!(label, (75.0, 27.0));
    }

    #[test]
    fn same_corridor_connections_are_rendered_as_parallel_lines() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "a".to_string(),
            Bounds {
                x: 0.0,
                y: 80.0,
                width: 40.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "b".to_string(),
            Bounds {
                x: 160.0,
                y: 80.0,
                width: 40.0,
                height: 40.0,
            },
        );

        let mut c1 = connection("a", "b");
        c1.from_anchor = AnchorPoint::Right;
        c1.to_anchor = AnchorPoint::Left;
        c1.source_order = 1;
        let mut c2 = c1.clone();
        c2.source_order = 2;
        let mut c3 = c1.clone();
        c3.source_order = 3;

        let routes = routed_connections(&[c1.clone(), c2.clone(), c3.clone()], &shape_map);
        let ys: Vec<f64> = [c1, c2, c3]
            .iter()
            .map(
                |conn| match &routes[&connection_render_key(conn)].geometry {
                    RoutedGeometry::Line { start, .. } => start.1,
                    other => route_polyline_points(other)[1].1,
                },
            )
            .collect();

        assert_eq!(ys, vec![93.0, 100.0, 107.0]);
    }

    #[test]
    fn same_source_branch_routes_keep_shared_initial_trunk() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "main".to_string(),
            Bounds {
                x: 270.0,
                y: 80.0,
                width: 160.0,
                height: 90.0,
            },
        );
        shape_map.insert(
            "email".to_string(),
            Bounds {
                x: 530.0,
                y: 30.0,
                width: 150.0,
                height: 80.0,
            },
        );
        shape_map.insert(
            "payment".to_string(),
            Bounds {
                x: 530.0,
                y: 160.0,
                width: 150.0,
                height: 80.0,
            },
        );

        let mut email = connection("main", "email");
        email.source_order = 1;
        let mut payment = connection("main", "payment");
        payment.source_order = 2;

        let routes = routed_connections(&[email.clone(), payment.clone()], &shape_map);
        let email_points = route_polyline_points(&routes[&connection_render_key(&email)].geometry);
        let payment_points =
            route_polyline_points(&routes[&connection_render_key(&payment)].geometry);

        assert_eq!(email_points[0], (430.0, 125.0));
        assert_eq!(payment_points[0], (430.0, 125.0));
        assert_eq!(email_points[1], (480.0, 125.0));
        assert_eq!(payment_points[1], (480.0, 125.0));
        assert_ne!(email_points[2], payment_points[2]);
    }

    #[test]
    fn bezier_that_crosses_obstacle_falls_back_to_orthogonal_route() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "a".to_string(),
            Bounds {
                x: 0.0,
                y: 80.0,
                width: 40.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "b".to_string(),
            Bounds {
                x: 200.0,
                y: 80.0,
                width: 40.0,
                height: 40.0,
            },
        );
        shape_map.insert(
            "mid".to_string(),
            Bounds {
                x: 90.0,
                y: 80.0,
                width: 60.0,
                height: 40.0,
            },
        );

        let mut conn = connection("a", "b");
        conn.curve = CurveStyle::Bezier;
        conn.from_anchor = AnchorPoint::Right;
        conn.to_anchor = AnchorPoint::Left;

        let route = compute_connection_route(&conn, &shape_map).expect("route");

        assert!(matches!(route.geometry, RoutedGeometry::Orthogonal { .. }));
    }

    #[test]
    fn explicit_anchor_grid_route_avoids_stacked_obstacles() {
        let from = Bounds {
            x: 444.0,
            y: 842.0,
            width: 92.0,
            height: 38.0,
        };
        let to = Bounds {
            x: 430.0,
            y: 204.0,
            width: 120.0,
            height: 61.0,
        };
        let obstacles = vec![
            Bounds {
                x: 566.0,
                y: 771.0,
                width: 93.0,
                height: 38.0,
            },
            Bounds {
                x: 492.0,
                y: 297.0,
                width: 119.0,
                height: 38.0,
            },
            Bounds {
                x: 423.0,
                y: 460.0,
                width: 134.0,
                height: 38.0,
            },
        ];

        let route = route_orthogonal_anchored_grid(
            &from,
            &to,
            &obstacles,
            AnchorPoint::Top,
            AnchorPoint::Right,
        )
        .expect("expected clean grid route");

        assert!(!path_intersects_obstacle(&route, &obstacles));
        assert!(route_exits_endpoint_bounds(&route, &from, &to));
        assert!(route_avoids_endpoint_reentry(&route, &from, &to));
    }

    #[test]
    fn endpoint_reentry_is_rejected_after_initial_exit() {
        let from = Bounds {
            x: 430.0,
            y: 204.0,
            width: 120.0,
            height: 61.0,
        };
        let to = Bounds {
            x: 492.0,
            y: 297.0,
            width: 119.0,
            height: 38.0,
        };
        let route = vec![
            (430.0, 234.0),
            (406.0, 234.0),
            (636.0, 234.0),
            (636.0, 316.0),
            (611.0, 316.0),
        ];

        assert!(!route_avoids_endpoint_reentry(&route, &from, &to));
    }

    #[test]
    fn dotted_cross_graph_connection_uses_elbow_route() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "frontend".to_string(),
            Bounds {
                x: 40.0,
                y: 40.0,
                width: 180.0,
                height: 220.0,
            },
        );
        shape_map.insert(
            "frontend.api".to_string(),
            Bounds {
                x: 90.0,
                y: 200.0,
                width: 80.0,
                height: 34.0,
            },
        );
        shape_map.insert(
            "backend".to_string(),
            Bounds {
                x: 260.0,
                y: 40.0,
                width: 180.0,
                height: 220.0,
            },
        );
        shape_map.insert(
            "backend.gateway".to_string(),
            Bounds {
                x: 310.0,
                y: 70.0,
                width: 80.0,
                height: 34.0,
            },
        );
        shape_map.insert(
            "backend.allowed".to_string(),
            Bounds {
                x: 305.0,
                y: 125.0,
                width: 90.0,
                height: 60.0,
            },
        );
        let conn = connection("frontend.api", "backend.gateway");
        let mut svg = String::new();
        render_connection_svg(&conn, &shape_map, &mut svg);

        assert!(svg.contains("<path"));
        assert!(!svg.contains("<line"));
        assert!(svg.matches(" L ").count() >= 2);
    }

    #[test]
    fn orthogonal_route_starts_on_side_facing_first_lane() {
        let from = Bounds {
            x: 100.0,
            y: 100.0,
            width: 80.0,
            height: 40.0,
        };
        let to = Bounds {
            x: 260.0,
            y: 80.0,
            width: 80.0,
            height: 40.0,
        };
        let route = build_hv_route(&from, &to, 220.0);

        assert_eq!(route[0], (180.0, 120.0));
        assert!(route[1].0 > from.x + from.width);
    }

    #[test]
    fn orthogonal_hv_route_enters_target_side_facing_lane() {
        let left = Bounds {
            x: 0.0,
            y: 80.0,
            width: 40.0,
            height: 40.0,
        };
        let right = Bounds {
            x: 160.0,
            y: 80.0,
            width: 40.0,
            height: 40.0,
        };

        let left_to_right = build_hv_route(&left, &right, 100.0);
        assert_eq!(left_to_right.last().copied(), Some((160.0, 100.0)));
        assert_eq!(left_to_right[left_to_right.len() - 2], (100.0, 100.0));

        let right_to_left = build_hv_route(&right, &left, 100.0);
        assert_eq!(right_to_left.last().copied(), Some((40.0, 100.0)));
        assert_eq!(right_to_left[right_to_left.len() - 2], (100.0, 100.0));
    }

    #[test]
    fn orthogonal_vh_route_enters_target_side_facing_lane() {
        let top = Bounds {
            x: 80.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
        };
        let bottom = Bounds {
            x: 80.0,
            y: 160.0,
            width: 40.0,
            height: 40.0,
        };

        let top_to_bottom = build_vh_route(&top, &bottom, 100.0);
        assert_eq!(top_to_bottom.last().copied(), Some((100.0, 160.0)));
        assert_eq!(top_to_bottom[top_to_bottom.len() - 2], (100.0, 100.0));

        let bottom_to_top = build_vh_route(&bottom, &top, 100.0);
        assert_eq!(bottom_to_top.last().copied(), Some((100.0, 40.0)));
        assert_eq!(bottom_to_top[bottom_to_top.len() - 2], (100.0, 100.0));
    }

    #[test]
    fn intrinsic_size_of_layered_container_accounts_for_stacked_children() {
        let mut container = shape("container", 0.0, 0.0);
        container.width = None;
        container.height = None;
        container.align = Alignment::Layered;
        container.gap = 20.0;
        container.children = vec![
            shape("a", 100.0, 30.0),
            shape("b", 100.0, 30.0),
            shape("c", 100.0, 30.0),
            shape("d", 100.0, 30.0),
            shape("e", 100.0, 30.0),
        ];
        let connections = vec![
            connection("container.a", "container.b"),
            connection("container.b", "container.c"),
            connection("container.c", "container.d"),
            connection("container.d", "container.e"),
        ];

        let (w, h) = intrinsic_graph_container_size(&container, &connections)
            .expect("container should yield intrinsic size");
        assert!(
            h >= 5.0 * 30.0 + 4.0 * 20.0,
            "expected intrinsic height ≥ 230, got {h}"
        );
        assert!(w >= 100.0, "expected intrinsic width ≥ 100, got {w}");
    }

    #[test]
    fn nested_horizontal_layered_containers_do_not_overlap() {
        let mut frontend = shape("frontend", 0.0, 0.0);
        frontend.width = None;
        frontend.height = None;
        frontend.align = Alignment::Layered;
        frontend.gap = 20.0;
        frontend.children = vec![
            shape("fe_a", 100.0, 40.0),
            shape("fe_b", 100.0, 40.0),
            shape("fe_c", 100.0, 40.0),
        ];

        let mut backend = shape("backend", 0.0, 0.0);
        backend.width = None;
        backend.height = None;
        backend.align = Alignment::Layered;
        backend.gap = 20.0;
        backend.children = vec![
            shape("be_a", 100.0, 40.0),
            shape("be_b", 100.0, 40.0),
            shape("be_c", 100.0, 40.0),
            shape("be_d", 100.0, 40.0),
        ];

        let mut data = shape("data", 0.0, 0.0);
        data.width = None;
        data.height = None;
        data.align = Alignment::Layered;
        data.attrs
            .insert("direction".to_string(), "horizontal".to_string());
        data.gap = 20.0;
        data.children = vec![
            shape("d_a", 100.0, 40.0),
            shape("d_b", 100.0, 40.0),
            shape("d_c", 100.0, 40.0),
        ];

        let mut diagram = Diagram {
            id: None,
            width: 1200.0,
            height: 600.0,
            shapes: vec![frontend, backend, data],
            connections: vec![
                connection("frontend.fe_a", "frontend.fe_b"),
                connection("frontend.fe_b", "frontend.fe_c"),
                connection("backend.be_a", "backend.be_b"),
                connection("backend.be_b", "backend.be_c"),
                connection("backend.be_c", "backend.be_d"),
                connection("data.d_a", "data.d_b"),
                connection("data.d_b", "data.d_c"),
            ],
            classes: IndexMap::new(),
            padding: 0.0,
            align: Alignment::Layered,
            gap: 24.0,
            options: {
                let mut opts = IndexMap::new();
                opts.insert("direction".to_string(), "horizontal".to_string());
                opts
            },
        };

        render_diagram_svg(&mut diagram);

        let frontend = &diagram.shapes[0];
        let backend = &diagram.shapes[1];
        let data = &diagram.shapes[2];
        assert!(
            !overlaps(frontend, backend),
            "frontend overlaps backend: f={:?} b={:?}",
            frontend.resolved,
            backend.resolved
        );
        assert!(
            !overlaps(backend, data),
            "backend overlaps data: b={:?} d={:?}",
            backend.resolved,
            data.resolved
        );
        assert!(
            !overlaps(frontend, data),
            "frontend overlaps data: f={:?} d={:?}",
            frontend.resolved,
            data.resolved
        );

        for container in [frontend, backend, data] {
            for child in container
                .children
                .iter()
                .filter(|c| !is_layout_decoration(c))
            {
                assert!(
                    child.resolved.x + child.resolved.width <= container.resolved.width + 0.01,
                    "child {:?} overflows container width {}",
                    child.id,
                    container.resolved.width
                );
                assert!(
                    child.resolved.y + child.resolved.height <= container.resolved.height + 0.01,
                    "child {:?} overflows container height {}",
                    child.id,
                    container.resolved.height
                );
            }
        }
    }

    #[test]
    fn cross_container_edge_routes_around_sibling_container() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "a".to_string(),
            Bounds {
                x: 0.0,
                y: 100.0,
                width: 120.0,
                height: 200.0,
            },
        );
        shape_map.insert(
            "a.x".to_string(),
            Bounds {
                x: 20.0,
                y: 200.0,
                width: 60.0,
                height: 30.0,
            },
        );
        shape_map.insert(
            "b".to_string(),
            Bounds {
                x: 200.0,
                y: 100.0,
                width: 120.0,
                height: 200.0,
            },
        );
        shape_map.insert(
            "b.y".to_string(),
            Bounds {
                x: 220.0,
                y: 200.0,
                width: 60.0,
                height: 30.0,
            },
        );
        shape_map.insert(
            "c".to_string(),
            Bounds {
                x: 400.0,
                y: 100.0,
                width: 120.0,
                height: 200.0,
            },
        );
        shape_map.insert(
            "c.z".to_string(),
            Bounds {
                x: 420.0,
                y: 200.0,
                width: 60.0,
                height: 30.0,
            },
        );

        let conn = connection("a.x", "c.z");
        let mut svg = String::new();
        render_connection_svg(&conn, &shape_map, &mut svg);

        assert!(svg.contains("<path"), "expected elbow path, got {svg}");
        let b_bounds = shape_map["b"];
        let path_d = svg
            .split("d=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("path d attribute");
        let coords: Vec<(f64, f64)> = path_d
            .split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<f64>().ok())
            .collect::<Vec<_>>()
            .chunks(2)
            .filter(|w| w.len() == 2)
            .map(|w| (w[0], w[1]))
            .collect();
        assert!(coords.len() >= 3);
        for window in coords.windows(2) {
            let (sx, sy) = window[0];
            let (ex, ey) = window[1];
            // No segment may pass through `b`'s interior.
            assert!(
                !segment_intersects_obstacle((sx, sy), (ex, ey), &b_bounds),
                "segment ({sx},{sy})->({ex},{ey}) crosses obstacle b={:?}",
                b_bounds
            );
        }
    }

    #[test]
    fn cross_container_obstacles_include_destination_sibling_children() {
        let backend = Bounds {
            x: 440.0,
            y: 100.0,
            width: 260.0,
            height: 475.0,
        };
        let data_svc = Bounds {
            x: 530.0,
            y: 405.0,
            width: 105.0,
            height: 45.0,
        };
        let data_layer = Bounds {
            x: 745.0,
            y: 198.0,
            width: 295.0,
            height: 206.0,
        };
        let postgres = Bounds {
            x: 905.0,
            y: 230.0,
            width: 107.0,
            height: 44.0,
        };
        let redis = Bounds {
            x: 760.0,
            y: 230.0,
            width: 120.0,
            height: 50.0,
        };
        let kafka = Bounds {
            x: 760.0,
            y: 313.0,
            width: 120.0,
            height: 50.0,
        };

        let mut shape_map = HashMap::new();
        shape_map.insert("backend".to_string(), backend);
        shape_map.insert("backend.be_data_svc".to_string(), data_svc);
        shape_map.insert("data_layer".to_string(), data_layer);
        shape_map.insert("data_layer.dl_postgres".to_string(), postgres);
        shape_map.insert("data_layer.dl_cache".to_string(), redis);
        shape_map.insert("data_layer.dl_stream".to_string(), kafka);

        let obstacles = connection_obstacles(
            "backend.be_data_svc",
            "data_layer.dl_postgres",
            data_svc.x + data_svc.width,
            data_svc.y + data_svc.height / 2.0,
            postgres.x,
            postgres.y + postgres.height / 2.0,
            &shape_map,
        );

        assert!(contains_bounds(&obstacles, &redis));
        assert!(contains_bounds(&obstacles, &kafka));
        assert!(!contains_bounds(&obstacles, &backend));
        assert!(!contains_bounds(&obstacles, &data_layer));
        assert!(!contains_bounds(&obstacles, &data_svc));
        assert!(!contains_bounds(&obstacles, &postgres));
    }

    #[test]
    fn cross_container_edge_routes_around_destination_sibling_children() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "backend".to_string(),
            Bounds {
                x: 440.0,
                y: 100.0,
                width: 260.0,
                height: 475.0,
            },
        );
        shape_map.insert(
            "backend.be_data_svc".to_string(),
            Bounds {
                x: 530.0,
                y: 405.0,
                width: 105.0,
                height: 45.0,
            },
        );
        shape_map.insert(
            "data_layer".to_string(),
            Bounds {
                x: 745.0,
                y: 198.0,
                width: 295.0,
                height: 206.0,
            },
        );
        shape_map.insert(
            "data_layer.dl_cache".to_string(),
            Bounds {
                x: 760.0,
                y: 230.0,
                width: 120.0,
                height: 50.0,
            },
        );
        shape_map.insert(
            "data_layer.dl_postgres".to_string(),
            Bounds {
                x: 905.0,
                y: 230.0,
                width: 107.0,
                height: 44.0,
            },
        );
        shape_map.insert(
            "data_layer.dl_stream".to_string(),
            Bounds {
                x: 760.0,
                y: 313.0,
                width: 120.0,
                height: 50.0,
            },
        );

        let conn = connection("backend.be_data_svc", "data_layer.dl_postgres");
        let mut svg = String::new();
        render_connection_svg(&conn, &shape_map, &mut svg);

        assert!(svg.contains("<path"), "expected elbow path, got {svg}");
        let coords = path_coords(&svg);
        assert!(coords.len() >= 3);
        for obstacle in [
            &shape_map["data_layer.dl_cache"],
            &shape_map["data_layer.dl_stream"],
        ] {
            for window in coords.windows(2) {
                assert!(
                    !segment_intersects_obstacle(window[0], window[1], obstacle),
                    "segment {:?}->{:?} crosses obstacle {:?}; svg={svg}",
                    window[0],
                    window[1],
                    obstacle
                );
            }
        }
    }

    fn segment_intersects_obstacle(s: (f64, f64), e: (f64, f64), b: &Bounds) -> bool {
        // Trivial axis-aligned segment / rect overlap test for orthogonal routes.
        let xmin = s.0.min(e.0);
        let xmax = s.0.max(e.0);
        let ymin = s.1.min(e.1);
        let ymax = s.1.max(e.1);
        let bxmin = b.x;
        let bxmax = b.x + b.width;
        let bymin = b.y;
        let bymax = b.y + b.height;
        let overlap_x = xmax > bxmin && xmin < bxmax;
        let overlap_y = ymax > bymin && ymin < bymax;
        overlap_x && overlap_y
    }

    fn contains_bounds(bounds: &[Bounds], needle: &Bounds) -> bool {
        bounds.iter().any(|bounds| bounds_eq(bounds, needle))
    }

    fn path_coords(svg: &str) -> Vec<(f64, f64)> {
        let path_d = svg
            .split("d=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("path d attribute");
        path_d
            .split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<f64>().ok())
            .collect::<Vec<_>>()
            .chunks(2)
            .filter(|w| w.len() == 2)
            .map(|w| (w[0], w[1]))
            .collect()
    }

    #[test]
    fn cross_container_edge_with_anchors_exits_via_container_boundary() {
        let mut shape_map = HashMap::new();
        shape_map.insert(
            "frontend".to_string(),
            Bounds {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 200.0,
            },
        );
        shape_map.insert(
            "frontend.api".to_string(),
            Bounds {
                x: 30.0,
                y: 150.0,
                width: 80.0,
                height: 30.0,
            },
        );
        shape_map.insert(
            "backend".to_string(),
            Bounds {
                x: 360.0,
                y: 0.0,
                width: 200.0,
                height: 200.0,
            },
        );
        shape_map.insert(
            "backend.gateway".to_string(),
            Bounds {
                x: 380.0,
                y: 30.0,
                width: 80.0,
                height: 30.0,
            },
        );

        let mut conn = connection("frontend.api", "backend.gateway");
        conn.from_anchor = AnchorPoint::Right;
        conn.to_anchor = AnchorPoint::Left;
        let mut svg = String::new();
        render_connection_svg(&conn, &shape_map, &mut svg);

        assert!(svg.contains("<path"), "expected path output, got {svg}");
        // The path should include a vertex on frontend's right edge (x=200) and on
        // backend's left edge (x=360).
        let path_d = svg
            .split("d=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("path d attribute");
        let xs: Vec<f64> = path_d
            .split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<f64>().ok())
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .map(|(_, v)| v)
            .collect();
        assert!(
            xs.iter().any(|x| (*x - 200.0).abs() < 0.01),
            "no vertex on frontend right edge: xs={xs:?}"
        );
        assert!(
            xs.iter().any(|x| (*x - 360.0).abs() < 0.01),
            "no vertex on backend left edge: xs={xs:?}"
        );
    }
}
