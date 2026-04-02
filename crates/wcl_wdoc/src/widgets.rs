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

/// Check if a block kind is a widget (composite shape in the wdoc::draw namespace).
pub fn is_widget(kind: &str) -> bool {
    kind.starts_with("wdoc::draw::")
        && !matches!(
            kind,
            "wdoc::draw::rect"
                | "wdoc::draw::circle"
                | "wdoc::draw::ellipse"
                | "wdoc::draw::line"
                | "wdoc::draw::path"
                | "wdoc::draw::text"
                | "wdoc::draw::connection"
                | "wdoc::draw::diagram"
        )
}

/// Build widget child shapes by kind. Returns primitives for the widget's visual structure.
pub fn build_widget(
    kind: &str,
    w: f64,
    h: f64,
    attrs: &IndexMap<String, String>,
) -> Vec<ShapeNode> {
    match kind {
        "wdoc::draw::phone" => phone_shapes(w, h, attrs),
        "wdoc::draw::browser" => browser_shapes(w, h, attrs),
        "wdoc::draw::button" => button_shapes(w, h, attrs),
        "wdoc::draw::input" => input_shapes(w, h, attrs),
        "wdoc::draw::card" => card_shapes(w, h, attrs),
        "wdoc::draw::avatar" => avatar_shapes(w, h, attrs),
        "wdoc::draw::toggle" => toggle_shapes(w, h, attrs),
        "wdoc::draw::badge" => badge_shapes(w, h, attrs),
        "wdoc::draw::navbar" => navbar_shapes(w, h, attrs),
        // Flowchart
        "wdoc::draw::flow_process" => flow_process(w, h, attrs),
        "wdoc::draw::flow_decision" => flow_decision(w, h, attrs),
        "wdoc::draw::flow_terminal" => flow_terminal(w, h, attrs),
        "wdoc::draw::flow_io" => flow_io(w, h, attrs),
        "wdoc::draw::flow_subprocess" => flow_subprocess(w, h, attrs),
        // C4
        "wdoc::draw::c4_person" => c4_person(w, h, attrs),
        "wdoc::draw::c4_system" => c4_system(w, h, attrs),
        "wdoc::draw::c4_container" => c4_container(w, h, attrs),
        "wdoc::draw::c4_component" => c4_component(w, h, attrs),
        "wdoc::draw::c4_boundary" => c4_boundary(w, h, attrs),
        // UML
        "wdoc::draw::uml_class" => uml_class(w, h, attrs),
        "wdoc::draw::uml_actor" => uml_actor(w, h, attrs),
        "wdoc::draw::uml_package" => uml_package(w, h, attrs),
        "wdoc::draw::uml_note" => uml_note(w, h, attrs),
        // Network
        "wdoc::draw::server" => node_server(w, h, attrs),
        "wdoc::draw::database" => node_database(w, h, attrs),
        "wdoc::draw::cloud" => node_cloud(w, h, attrs),
        "wdoc::draw::user" => node_user(w, h, attrs),
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

// ===========================================================================
// Flowchart shapes
// ===========================================================================

fn flow_process(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let label = attr(attrs, "label").unwrap_or("Process");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    vec![
        rect(
            0.0,
            0.0,
            w,
            h,
            &[
                ("rx", "4"),
                ("fill", "var(--color-bg)"),
                ("stroke", color),
                ("stroke_width", "2"),
            ],
        ),
        text(0.0, 0.0, w, h, label, 13.0, &[]),
    ]
}

fn flow_decision(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let label = attr(attrs, "label").unwrap_or("?");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let (cx, cy) = (w / 2.0, h / 2.0);
    let mut p = make_node(ShapeKind::Path, 0.0, 0.0, w, h);
    p.attrs.insert(
        "d".into(),
        format!("M {cx} 0 L {w} {cy} L {cx} {h} L 0 {cy} Z"),
    );
    p.attrs.insert("fill".into(), "var(--color-bg)".into());
    p.attrs.insert("stroke".into(), color.into());
    p.attrs.insert("stroke_width".into(), "2".into());
    vec![p, text(0.0, 0.0, w, h, label, 12.0, &[])]
}

fn flow_terminal(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let label = attr(attrs, "label").unwrap_or("Start");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    vec![
        rect(
            0.0,
            0.0,
            w,
            h,
            &[("rx", &format!("{}", h / 2.0)), ("fill", color)],
        ),
        text(0.0, 0.0, w, h, label, 13.0, &[("fill", "#fff")]),
    ]
}

fn flow_io(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let label = attr(attrs, "label").unwrap_or("I/O");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let s = w * 0.15;
    let mut p = make_node(ShapeKind::Path, 0.0, 0.0, w, h);
    p.attrs.insert(
        "d".into(),
        format!("M {s} 0 L {w} 0 L {} {h} L 0 {h} Z", w - s),
    );
    p.attrs.insert("fill".into(), "var(--color-bg)".into());
    p.attrs.insert("stroke".into(), color.into());
    p.attrs.insert("stroke_width".into(), "2".into());
    vec![p, text(0.0, 0.0, w, h, label, 12.0, &[])]
}

fn flow_subprocess(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let label = attr(attrs, "label").unwrap_or("Subprocess");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    vec![
        rect(
            0.0,
            0.0,
            w,
            h,
            &[
                ("rx", "4"),
                ("fill", "var(--color-bg)"),
                ("stroke", color),
                ("stroke_width", "2"),
            ],
        ),
        line(
            10.0,
            0.0,
            10.0,
            h,
            &[("stroke", color), ("stroke_width", "1")],
        ),
        line(
            w - 10.0,
            0.0,
            w - 10.0,
            h,
            &[("stroke", color), ("stroke_width", "1")],
        ),
        text(0.0, 0.0, w, h, label, 13.0, &[]),
    ]
}

// ===========================================================================
// C4 diagram shapes
// ===========================================================================

fn c4_person(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("Person");
    let desc = attr(attrs, "description").unwrap_or("");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let hr = 18.0;
    let by = hr * 2.0 + 4.0;
    let mut shapes = vec![
        circle(w / 2.0 - hr, 0.0, hr, &[("fill", color)]),
        rect(0.0, by, w, h - by, &[("rx", "4"), ("fill", color)]),
        text(0.0, by, w, 22.0, name, 14.0, &[("fill", "#fff")]),
    ];
    if !desc.is_empty() {
        shapes.push(text(
            4.0,
            by + 22.0,
            w - 8.0,
            16.0,
            desc,
            10.0,
            &[("fill", "#fff"), ("opacity", "0.8")],
        ));
    }
    shapes
}

fn c4_system(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("System");
    let desc = attr(attrs, "description").unwrap_or("");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let ext = attr(attrs, "external").unwrap_or("false") == "true";
    let tag = if ext { "[External System]" } else { "[System]" };
    let dash = if ext { "5,3" } else { "" };
    let mut shapes = vec![
        rect(
            0.0,
            0.0,
            w,
            h,
            &[("rx", "8"), ("fill", color), ("stroke_dasharray", dash)],
        ),
        text(0.0, 8.0, w, 22.0, name, 15.0, &[("fill", "#fff")]),
        text(
            0.0,
            28.0,
            w,
            14.0,
            tag,
            10.0,
            &[("fill", "#fff"), ("opacity", "0.7")],
        ),
    ];
    if !desc.is_empty() {
        shapes.push(text(
            6.0,
            46.0,
            w - 12.0,
            16.0,
            desc,
            10.0,
            &[("fill", "#fff"), ("opacity", "0.85")],
        ));
    }
    shapes
}

fn c4_container(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("Container");
    let tech = attr(attrs, "technology").unwrap_or("");
    let desc = attr(attrs, "description").unwrap_or("");
    let color = attr(attrs, "color").unwrap_or("#438DD5");
    let mut shapes = vec![
        rect(0.0, 0.0, w, h, &[("rx", "6"), ("fill", color)]),
        text(0.0, 8.0, w, 20.0, name, 14.0, &[("fill", "#fff")]),
    ];
    if !tech.is_empty() {
        shapes.push(text(
            0.0,
            26.0,
            w,
            14.0,
            &format!("[{tech}]"),
            10.0,
            &[("fill", "#fff"), ("opacity", "0.7")],
        ));
    }
    if !desc.is_empty() {
        shapes.push(text(
            6.0,
            44.0,
            w - 12.0,
            16.0,
            desc,
            10.0,
            &[("fill", "#fff"), ("opacity", "0.85")],
        ));
    }
    shapes
}

fn c4_component(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("Component");
    let tech = attr(attrs, "technology").unwrap_or("");
    let color = attr(attrs, "color").unwrap_or("#85BBF0");
    let mut shapes = vec![
        rect(0.0, 0.0, w, h, &[("rx", "4"), ("fill", color)]),
        text(0.0, 6.0, w, 18.0, name, 13.0, &[("fill", "#fff")]),
    ];
    if !tech.is_empty() {
        shapes.push(text(
            0.0,
            24.0,
            w,
            14.0,
            &format!("[{tech}]"),
            9.0,
            &[("fill", "#fff"), ("opacity", "0.7")],
        ));
    }
    shapes
}

fn c4_boundary(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("Boundary");
    let color = attr(attrs, "color").unwrap_or("var(--color-nav-border)");
    vec![
        rect(
            0.0,
            0.0,
            w,
            h,
            &[
                ("rx", "4"),
                ("fill", "none"),
                ("stroke", color),
                ("stroke_width", "2"),
                ("stroke_dasharray", "8,4"),
            ],
        ),
        text(8.0, 4.0, w - 16.0, 18.0, name, 12.0, &[("anchor", "start")]),
    ]
}

// ===========================================================================
// UML shapes
// ===========================================================================

fn uml_class(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("ClassName");
    let stereotype = attr(attrs, "stereotype");
    let fields_str = attr(attrs, "fields").unwrap_or("");
    let methods_str = attr(attrs, "methods").unwrap_or("");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let hdr_h = if stereotype.is_some() { 40.0 } else { 28.0 };
    let fields: Vec<&str> = fields_str.split('|').filter(|s| !s.is_empty()).collect();
    let methods: Vec<&str> = methods_str.split('|').filter(|s| !s.is_empty()).collect();
    let field_h = (fields.len() as f64 * 16.0).max(16.0);

    let mut shapes = vec![
        rect(
            0.0,
            0.0,
            w,
            h,
            &[
                ("fill", "var(--color-bg)"),
                ("stroke", color),
                ("stroke_width", "2"),
            ],
        ),
        rect(0.0, 0.0, w, hdr_h, &[("fill", color)]),
    ];
    let mut y = 4.0;
    if let Some(st) = stereotype {
        shapes.push(text(
            0.0,
            y,
            w,
            14.0,
            &format!("<<{st}>>"),
            9.0,
            &[("fill", "#fff"), ("opacity", "0.8")],
        ));
        y += 14.0;
    }
    shapes.push(text(0.0, y, w, 20.0, name, 14.0, &[("fill", "#fff")]));
    shapes.push(line(
        0.0,
        hdr_h,
        w,
        hdr_h,
        &[("stroke", color), ("stroke_width", "1")],
    ));
    let mut fy = hdr_h + 4.0;
    for f in &fields {
        shapes.push(text(
            8.0,
            fy,
            w - 16.0,
            14.0,
            f.trim(),
            11.0,
            &[("anchor", "start")],
        ));
        fy += 16.0;
    }
    let my_start = hdr_h + field_h + 4.0;
    shapes.push(line(
        0.0,
        my_start,
        w,
        my_start,
        &[("stroke", color), ("stroke_width", "1")],
    ));
    let mut my = my_start + 4.0;
    for m in &methods {
        shapes.push(text(
            8.0,
            my,
            w - 16.0,
            14.0,
            m.trim(),
            11.0,
            &[("anchor", "start")],
        ));
        my += 16.0;
    }
    shapes
}

fn uml_actor(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("Actor");
    let color = attr(attrs, "color").unwrap_or("currentColor");
    let cx = w / 2.0;
    let hr = h * 0.12;
    let bt = hr * 2.0 + 2.0;
    let bm = h * 0.5;
    let ay = bt + (bm - bt) * 0.3;
    let ly = h * 0.78;
    vec![
        circle(
            cx - hr,
            0.0,
            hr,
            &[("fill", "none"), ("stroke", color), ("stroke_width", "2")],
        ),
        line(cx, bt, cx, bm, &[("stroke", color), ("stroke_width", "2")]),
        line(
            cx - w * 0.3,
            ay,
            cx + w * 0.3,
            ay,
            &[("stroke", color), ("stroke_width", "2")],
        ),
        line(
            cx,
            bm,
            cx - w * 0.25,
            ly,
            &[("stroke", color), ("stroke_width", "2")],
        ),
        line(
            cx,
            bm,
            cx + w * 0.25,
            ly,
            &[("stroke", color), ("stroke_width", "2")],
        ),
        text(0.0, h * 0.82, w, h * 0.18, name, 11.0, &[]),
    ]
}

fn uml_package(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("Package");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let tw = w * 0.4;
    vec![
        rect(
            0.0,
            0.0,
            tw,
            20.0,
            &[
                ("fill", "var(--color-bg)"),
                ("stroke", color),
                ("stroke_width", "1.5"),
            ],
        ),
        text(0.0, 0.0, tw, 20.0, name, 11.0, &[]),
        rect(
            0.0,
            19.0,
            w,
            h - 19.0,
            &[
                ("fill", "var(--color-bg)"),
                ("stroke", color),
                ("stroke_width", "1.5"),
            ],
        ),
    ]
}

fn uml_note(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let label = attr(attrs, "label").unwrap_or("Note");
    let color = attr(attrs, "color").unwrap_or("var(--color-nav-border)");
    let fold = 15.0;
    let mut body = make_node(ShapeKind::Path, 0.0, 0.0, w, h);
    body.attrs.insert(
        "d".into(),
        format!("M 0 0 L {} 0 L {w} {fold} L {w} {h} L 0 {h} Z", w - fold),
    );
    body.attrs
        .insert("fill".into(), "var(--color-code-bg)".into());
    body.attrs.insert("stroke".into(), color.into());
    body.attrs.insert("stroke_width".into(), "1.5".into());
    let mut corner = make_node(ShapeKind::Path, 0.0, 0.0, w, h);
    corner.attrs.insert(
        "d".into(),
        format!("M {} 0 L {} {fold} L {w} {fold}", w - fold, w - fold),
    );
    corner.attrs.insert("fill".into(), "none".into());
    corner.attrs.insert("stroke".into(), color.into());
    corner.attrs.insert("stroke_width".into(), "1".into());
    vec![
        body,
        corner,
        text(
            8.0,
            8.0,
            w - 16.0,
            h - 16.0,
            label,
            11.0,
            &[("anchor", "start")],
        ),
    ]
}

// ===========================================================================
// Network/infrastructure node shapes
// ===========================================================================

fn node_server(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("Server");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let rh = h * 0.6;
    vec![
        rect(
            0.0,
            0.0,
            w,
            rh,
            &[
                ("rx", "4"),
                ("fill", "var(--color-code-bg)"),
                ("stroke", color),
                ("stroke_width", "2"),
            ],
        ),
        rect(
            6.0,
            6.0,
            w - 12.0,
            rh * 0.25,
            &[
                ("rx", "2"),
                ("fill", "var(--color-nav-bg)"),
                ("stroke", color),
                ("stroke_width", "1"),
            ],
        ),
        rect(
            6.0,
            6.0 + rh * 0.32,
            w - 12.0,
            rh * 0.25,
            &[
                ("rx", "2"),
                ("fill", "var(--color-nav-bg)"),
                ("stroke", color),
                ("stroke_width", "1"),
            ],
        ),
        circle(w - 18.0, 8.0, 3.0, &[("fill", "#28a745")]),
        circle(w - 18.0, 8.0 + rh * 0.32, 3.0, &[("fill", "#28a745")]),
        text(0.0, rh + 4.0, w, h - rh - 4.0, name, 12.0, &[]),
    ]
}

fn node_database(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("Database");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let eh = h * 0.15;
    let bh = h * 0.55;
    let mut body = make_node(ShapeKind::Path, 0.0, 0.0, w, h);
    body.attrs.insert(
        "d".into(),
        format!(
            "M 0 {eh} L 0 {bh} Q 0 {} {} {} Q {w} {} {w} {bh} L {w} {eh}",
            bh + eh,
            w / 2.0,
            bh + eh,
            bh + eh
        ),
    );
    body.attrs
        .insert("fill".into(), "var(--color-code-bg)".into());
    body.attrs.insert("stroke".into(), color.into());
    body.attrs.insert("stroke_width".into(), "2".into());
    let mut top = make_node(ShapeKind::Path, 0.0, 0.0, w, h);
    top.attrs.insert(
        "d".into(),
        format!(
            "M 0 {eh} Q 0 0 {} 0 Q {w} 0 {w} {eh} Q {w} {} {} {} Q 0 {} 0 {eh}",
            w / 2.0,
            eh * 2.0,
            w / 2.0,
            eh * 2.0,
            eh * 2.0
        ),
    );
    top.attrs
        .insert("fill".into(), "var(--color-nav-bg)".into());
    top.attrs.insert("stroke".into(), color.into());
    top.attrs.insert("stroke_width".into(), "2".into());
    vec![
        body,
        top,
        text(0.0, bh + eh + 4.0, w, h - bh - eh - 4.0, name, 12.0, &[]),
    ]
}

fn node_cloud(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("Cloud");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let ch = h * 0.7;
    let mut p = make_node(ShapeKind::Path, 0.0, 0.0, w, h);
    p.attrs.insert(
        "d".into(),
        format!(
            "M {} {} Q 0 {} 0 {} Q 0 {} {} 0 Q {} 0 {} {} Q {} 0 {} {} Q {} {} {} {} Z",
            w * 0.2,
            ch * 0.8,
            ch * 0.8,
            ch * 0.5,
            ch * 0.1,
            w * 0.25,
            w * 0.4,
            w * 0.5,
            ch * 0.05,
            w * 0.6,
            w,
            ch * 0.1,
            w * 1.05,
            ch * 0.6,
            w * 0.8,
            ch * 0.8
        ),
    );
    p.attrs.insert("fill".into(), "var(--color-code-bg)".into());
    p.attrs.insert("stroke".into(), color.into());
    p.attrs.insert("stroke_width".into(), "2".into());
    vec![p, text(0.0, ch * 0.2, w, ch * 0.5, name, 13.0, &[])]
}

fn node_user(w: f64, h: f64, attrs: &IndexMap<String, String>) -> Vec<ShapeNode> {
    let name = attr(attrs, "label").unwrap_or("User");
    let color = attr(attrs, "color").unwrap_or("var(--color-link)");
    let ih = h * 0.65;
    let hr = ih * 0.22;
    let cx = w / 2.0;
    vec![
        circle(cx - hr, 0.0, hr, &[("fill", color)]),
        rect(
            cx - w * 0.35,
            hr * 2.0 + 4.0,
            w * 0.7,
            ih - hr * 2.0 - 4.0,
            &[("rx", &format!("{}", w * 0.15)), ("fill", color)],
        ),
        text(0.0, ih + 4.0, w, h - ih - 4.0, name, 12.0, &[]),
    ]
}
