//! UI Widget shapes — composite shapes built from primitives.
//!
//! Each widget function returns `Vec<ShapeNode>` containing rects, circles,
//! text, etc. These become the widget's children in the shape tree.
//! The diagram renderer converts them to SVG (or any output format).

use indexmap::IndexMap;

use crate::shapes::{Alignment, Bounds, ShapeKind, ShapeNode};

// ---------------------------------------------------------------------------
// Helpers for concise ShapeNode construction
// ---------------------------------------------------------------------------

fn make_node(kind: ShapeKind, x: f64, y: f64, w: f64, h: f64) -> ShapeNode {
    ShapeNode {
        kind,
        id: None,
        x: Some(x),
        y: Some(y),
        width: Some(w),
        height: Some(h),
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

fn rect(x: f64, y: f64, w: f64, h: f64, attrs: &[(&str, &str)]) -> ShapeNode {
    let mut node = make_node(ShapeKind::Rect, x, y, w, h);
    for &(k, v) in attrs {
        node.attrs.insert(k.to_string(), v.to_string());
    }
    node
}

fn circle(x: f64, y: f64, r: f64, attrs: &[(&str, &str)]) -> ShapeNode {
    let mut node = make_node(ShapeKind::Circle, x, y, r * 2.0, r * 2.0);
    node.attrs.insert("r".to_string(), r.to_string());
    for &(k, v) in attrs {
        node.attrs.insert(k.to_string(), v.to_string());
    }
    node
}

fn text(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    content: &str,
    font_size: f64,
    attrs: &[(&str, &str)],
) -> ShapeNode {
    let mut node = make_node(ShapeKind::Text, x, y, w, h);
    node.attrs
        .insert("content".to_string(), content.to_string());
    node.attrs
        .insert("font_size".to_string(), font_size.to_string());
    for &(k, v) in attrs {
        node.attrs.insert(k.to_string(), v.to_string());
    }
    node
}

fn line(x1: f64, y1: f64, x2: f64, y2: f64, attrs: &[(&str, &str)]) -> ShapeNode {
    let mut node = make_node(ShapeKind::Line, 0.0, 0.0, 0.0, 0.0);
    node.x = None;
    node.y = None;
    node.attrs.insert("x1".to_string(), x1.to_string());
    node.attrs.insert("y1".to_string(), y1.to_string());
    node.attrs.insert("x2".to_string(), x2.to_string());
    node.attrs.insert("y2".to_string(), y2.to_string());
    for &(k, v) in attrs {
        node.attrs.insert(k.to_string(), v.to_string());
    }
    node
}

fn attr<'a>(attrs: &'a IndexMap<String, String>, key: &str) -> Option<&'a str> {
    attrs.get(key).map(|s| s.as_str())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Check if a block kind is a widget.
pub fn is_widget(kind: &str) -> bool {
    kind.starts_with("widget_")
}

/// Build widget child shapes by kind. Returns primitives for the widget's visual structure.
pub fn build_widget(
    kind: &str,
    w: f64,
    h: f64,
    attrs: &IndexMap<String, String>,
) -> Vec<ShapeNode> {
    match kind {
        "widget_phone" => phone_shapes(w, h, attrs),
        "widget_browser" => browser_shapes(w, h, attrs),
        "widget_button" => button_shapes(w, h, attrs),
        "widget_input" => input_shapes(w, h, attrs),
        "widget_card" => card_shapes(w, h, attrs),
        "widget_avatar" => avatar_shapes(w, h, attrs),
        "widget_toggle" => toggle_shapes(w, h, attrs),
        "widget_badge" => badge_shapes(w, h, attrs),
        "widget_navbar" => navbar_shapes(w, h, attrs),
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Widget implementations
// ---------------------------------------------------------------------------

fn phone_shapes(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let title = attr(attrs, "title").unwrap_or("App");
    let status = attr(attrs, "status_text").unwrap_or("9:41");
    let hdr_fill = attr(attrs, "header_fill").unwrap_or("var(--color-link)");

    vec![
        // Outer frame
        rect(
            0.0,
            0.0,
            w,
            h,
            &[
                ("rx", "20"),
                ("fill", "var(--color-bg)"),
                ("stroke", "var(--color-nav-border)"),
                ("stroke_width", "3"),
            ],
        ),
        // Status bar text
        text(20.0, 5.0, 60.0, 20.0, status, 10.0, &[("anchor", "start")]),
        // Header bar
        rect(0.0, 30.0, w, 44.0, &[("fill", hdr_fill)]),
        // Header title
        text(0.0, 30.0, w, 44.0, title, 16.0, &[("fill", "#fff")]),
        // Bottom nav bar
        rect(
            0.0,
            h - 50.0,
            w,
            50.0,
            &[
                ("fill", "var(--color-nav-bg)"),
                ("stroke", "var(--color-nav-border)"),
                ("stroke_width", "1"),
            ],
        ),
        // Home indicator
        rect(
            (w - 80.0) / 2.0,
            h - 12.0,
            80.0,
            4.0,
            &[("rx", "2"), ("fill", "var(--color-nav-border)")],
        ),
    ]
}

fn browser_shapes(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let url = attr(attrs, "url").unwrap_or("https://example.com");
    let title = attr(attrs, "title").unwrap_or("Browser");

    vec![
        // Window frame
        rect(
            0.0,
            0.0,
            w,
            h,
            &[
                ("rx", "8"),
                ("fill", "var(--color-bg)"),
                ("stroke", "var(--color-nav-border)"),
                ("stroke_width", "2"),
            ],
        ),
        // Title bar
        rect(
            0.0,
            0.0,
            w,
            36.0,
            &[("rx", "8"), ("fill", "var(--color-nav-bg)")],
        ),
        // Square off bottom of title bar
        rect(0.0, 28.0, w, 10.0, &[("fill", "var(--color-nav-bg)")]),
        // Traffic lights
        circle(10.0, 8.0, 6.0, &[("fill", "#ff5f56")]),
        circle(28.0, 8.0, 6.0, &[("fill", "#ffbd2e")]),
        circle(46.0, 8.0, 6.0, &[("fill", "#27c93f")]),
        // Tab
        rect(
            66.0,
            6.0,
            140.0,
            24.0,
            &[
                ("rx", "4"),
                ("fill", "var(--color-bg)"),
                ("stroke", "var(--color-nav-border)"),
                ("stroke_width", "1"),
            ],
        ),
        text(66.0, 6.0, 140.0, 24.0, title, 10.0, &[]),
        // URL bar
        rect(
            8.0,
            42.0,
            w - 16.0,
            26.0,
            &[
                ("rx", "4"),
                ("fill", "var(--color-code-bg)"),
                ("stroke", "var(--color-code-border)"),
                ("stroke_width", "1"),
            ],
        ),
        text(
            20.0,
            42.0,
            w - 40.0,
            26.0,
            url,
            11.0,
            &[("anchor", "start"), ("opacity", "0.6")],
        ),
    ]
}

fn button_shapes(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let label = attr(attrs, "label").unwrap_or("Button");
    let variant = attr(attrs, "variant").unwrap_or("primary");

    let (fill, stroke, text_fill) = match variant {
        "secondary" => (
            "var(--color-nav-bg)",
            "var(--color-nav-border)",
            "currentColor",
        ),
        "outline" => ("none", "var(--color-link)", "var(--color-link)"),
        _ => ("var(--color-link)", "var(--color-link)", "#fff"),
    };

    vec![
        rect(
            0.0,
            0.0,
            w,
            h,
            &[
                ("rx", "6"),
                ("fill", fill),
                ("stroke", stroke),
                ("stroke_width", "1.5"),
            ],
        ),
        text(0.0, 0.0, w, h, label, 13.0, &[("fill", text_fill)]),
    ]
}

fn input_shapes(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let placeholder = attr(attrs, "placeholder").unwrap_or("");
    let label = attr(attrs, "label");

    let mut shapes = Vec::new();
    let (field_y, field_h) = if let Some(lbl) = label {
        shapes.push(text(0.0, 0.0, w, 16.0, lbl, 11.0, &[("anchor", "start")]));
        (18.0, h - 18.0)
    } else {
        (0.0, h)
    };

    shapes.push(rect(
        0.0,
        field_y,
        w,
        field_h,
        &[
            ("rx", "6"),
            ("fill", "var(--color-code-bg)"),
            ("stroke", "var(--color-code-border)"),
            ("stroke_width", "1"),
        ],
    ));

    if !placeholder.is_empty() {
        shapes.push(text(
            10.0,
            field_y,
            w - 20.0,
            field_h,
            placeholder,
            12.0,
            &[("anchor", "start"), ("opacity", "0.4")],
        ));
    }

    shapes
}

fn card_shapes(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let title = attr(attrs, "title");
    let mut shapes = vec![rect(
        0.0,
        0.0,
        w,
        h,
        &[
            ("rx", "8"),
            ("fill", "var(--color-bg)"),
            ("stroke", "var(--color-nav-border)"),
            ("stroke_width", "1.5"),
        ],
    )];

    if let Some(t) = title {
        shapes.push(text(
            12.0,
            6.0,
            w - 24.0,
            20.0,
            t,
            14.0,
            &[("anchor", "start")],
        ));
        shapes.push(line(
            0.0,
            30.0,
            w,
            30.0,
            &[("stroke", "var(--color-nav-border)"), ("stroke_width", "1")],
        ));
    }

    shapes
}

fn avatar_shapes(w: f64, _h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let initials = attr(attrs, "initials").unwrap_or("?");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let r = w / 2.0;

    vec![
        circle(0.0, 0.0, r, &[("fill", color)]),
        text(
            0.0,
            0.0,
            w,
            w,
            initials,
            (r * 0.9).round(),
            &[("fill", "#fff")],
        ),
    ]
}

fn toggle_shapes(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let on = attr(attrs, "on").unwrap_or("false") == "true";
    let (bg, knob_x) = if on {
        ("var(--color-link)", w - h + 2.0)
    } else {
        ("var(--color-nav-border)", 2.0)
    };

    vec![
        rect(
            0.0,
            0.0,
            w,
            h,
            &[("rx", &format!("{}", h / 2.0)), ("fill", bg)],
        ),
        circle(knob_x, 2.0, (h - 6.0) / 2.0, &[("fill", "#fff")]),
    ]
}

fn badge_shapes(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let label = attr(attrs, "label").unwrap_or("badge");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");

    vec![
        rect(
            0.0,
            0.0,
            w,
            h,
            &[
                ("rx", &format!("{}", h / 2.0)),
                ("fill", color),
                ("opacity", "0.15"),
            ],
        ),
        text(0.0, 0.0, w, h, label, 11.0, &[("fill", color)]),
    ]
}

fn navbar_shapes(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let items_str = attr(attrs, "items").unwrap_or("Home,Search,Profile");
    let active: usize = attr(attrs, "active_index")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let items: Vec<&str> = items_str.split(',').collect();

    let mut shapes = vec![rect(
        0.0,
        0.0,
        w,
        h,
        &[
            ("fill", "var(--color-nav-bg)"),
            ("stroke", "var(--color-nav-border)"),
            ("stroke_width", "1"),
        ],
    )];

    let item_w = w / items.len() as f64;
    for (i, item) in items.iter().enumerate() {
        let ix = i as f64 * item_w;
        let fill = if i == active {
            "var(--color-link)"
        } else {
            "currentColor"
        };

        if i == active {
            shapes.push(rect(ix, 0.0, item_w, 3.0, &[("fill", "var(--color-link)")]));
        }
        shapes.push(text(
            ix,
            0.0,
            item_w,
            h,
            item.trim(),
            11.0,
            &[("fill", fill)],
        ));
    }

    shapes
}
