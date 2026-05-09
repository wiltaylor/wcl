use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::cli::vars::parse_var_args;
use crate::cli::LibraryArgs;
use crate::lang::ast;
use crate::{BlockRef, BuiltinFn, FunctionRegistry, FunctionSignature, FunctionValue, Value};
use wcl_wdoc::model::*;

// ---------------------------------------------------------------------------
// Template function dispatch
// ---------------------------------------------------------------------------

/// A callable template: either a WCL lambda or a Rust builtin.
enum TemplateFn {
    Lambda(FunctionValue),
    Builtin(BuiltinFn),
}

/// Map from (format, schema_name) → function_name, built from AST @template decorators.
fn collect_template_map(doc: &crate::Document) -> HashMap<(String, String), String> {
    let mut map = HashMap::new();
    for item in &doc.ast.items {
        if let ast::DocItem::Body(ast::BodyItem::Schema(schema)) = item {
            let schema_name = schema
                .name
                .parts
                .iter()
                .filter_map(|p| {
                    if let ast::StringPart::Literal(s) = p {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .collect::<String>();

            for dec in &schema.decorators {
                if dec.name.name == "template" && dec.args.len() >= 2 {
                    let format = extract_string_arg(&dec.args[0]);
                    let fn_name = extract_string_arg(&dec.args[1]);
                    if let (Some(fmt), Some(name)) = (format, fn_name) {
                        map.insert((fmt, schema_name.clone()), name);
                    }
                }
            }
        }
    }
    map
}

fn extract_string_arg(arg: &ast::DecoratorArg) -> Option<String> {
    match arg {
        ast::DecoratorArg::Positional(expr) => extract_string_expr(expr),
        ast::DecoratorArg::Named(_, expr) => extract_string_expr(expr),
    }
}

fn extract_string_expr(expr: &ast::Expr) -> Option<String> {
    if let ast::Expr::StringLit(lit) = expr {
        Some(
            lit.parts
                .iter()
                .filter_map(|p| {
                    if let ast::StringPart::Literal(s) = p {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .collect(),
        )
    } else {
        None
    }
}

/// Collect callable template functions from doc values (Value::Function) and builtins.
fn collect_template_fns(
    doc: &crate::Document,
    builtins: &HashMap<String, BuiltinFn>,
) -> HashMap<String, TemplateFn> {
    let mut fns = HashMap::new();

    // User-defined functions from evaluated values take priority
    for (name, value) in &doc.values {
        if let Value::Function(func) = value {
            fns.insert(name.clone(), TemplateFn::Lambda(func.clone()));
        }
    }

    // Builtins as fallback
    for (name, f) in builtins {
        fns.entry(name.clone())
            .or_insert_with(|| TemplateFn::Builtin(f.clone()));
    }

    fns
}

/// Call a template function with block attributes as a Value::Map.
fn call_template(
    func: &TemplateFn,
    block: &BlockRef,
    builtins: &HashMap<String, BuiltinFn>,
) -> Result<String, String> {
    // Pass the full BlockRef so template functions can access children
    let arg = Value::BlockRef(block.clone());
    let result = match func {
        TemplateFn::Lambda(fv) => crate::call_lambda(fv, &[arg], builtins)?,
        TemplateFn::Builtin(f) => f(&[arg])?,
    };
    match result {
        Value::String(s) => Ok(s),
        other => Ok(format!("{other}")),
    }
}

// ---------------------------------------------------------------------------
// WCL custom functions (inline formatting + template rendering)
// ---------------------------------------------------------------------------

fn wdoc_functions() -> FunctionRegistry {
    let mut reg = FunctionRegistry::new();
    let mk = |name: &str, params: Vec<&str>, doc: &str| FunctionSignature {
        name: name.into(),
        params: params.into_iter().map(|s| s.to_string()).collect(),
        return_type: "string".into(),
        doc: doc.into(),
    };

    // Inline formatting (qualified under wdoc:: namespace)
    reg.register(
        "wdoc::bold",
        std::sync::Arc::new(|args: &[Value]| {
            let t = args
                .first()
                .and_then(|v| v.as_string())
                .ok_or("bold() expects a string argument")?;
            Ok(Value::String(format!("<strong>{t}</strong>")))
        }) as BuiltinFn,
        mk(
            "wdoc::bold",
            vec!["text: string"],
            "Wrap text in <strong> tags",
        ),
    );

    reg.register(
        "wdoc::italic",
        std::sync::Arc::new(|args: &[Value]| {
            let t = args
                .first()
                .and_then(|v| v.as_string())
                .ok_or("italic() expects a string argument")?;
            Ok(Value::String(format!("<em>{t}</em>")))
        }) as BuiltinFn,
        mk(
            "wdoc::italic",
            vec!["text: string"],
            "Wrap text in <em> tags",
        ),
    );

    reg.register(
        "wdoc::link",
        std::sync::Arc::new(|args: &[Value]| {
            if args.len() != 2 {
                return Err("link() expects 2 arguments (text, url)".into());
            }
            let text = args[0]
                .as_string()
                .ok_or("link() first argument must be a string")?;
            let url = args[1]
                .as_string()
                .ok_or("link() second argument must be a string")?;
            Ok(Value::String(format!("<a href=\"{url}\">{text}</a>")))
        }) as BuiltinFn,
        mk(
            "wdoc::link",
            vec!["text: string", "url: string"],
            "Create an <a> link",
        ),
    );

    reg.register(
        "wdoc::icon",
        std::sync::Arc::new(|args: &[Value]| {
            let name = args
                .first()
                .and_then(|v| v.as_string())
                .ok_or("icon() expects a string argument (icon name)")?;
            // Optional second arg: size (e.g. "1.5em", "24px")
            let size = args.get(1).and_then(|v| v.as_string());
            // Optional third arg: color (e.g. "red", "#ff0000", "var(--color-link)")
            let color = args.get(2).and_then(|v| v.as_string());

            let mut style = String::new();
            if let Some(s) = size {
                style.push_str(&format!("font-size:{s};"));
            }
            if let Some(c) = color {
                style.push_str(&format!("color:{c};"));
            }

            let style_attr = if style.is_empty() {
                String::new()
            } else {
                format!(" style=\"{style}\"")
            };

            Ok(Value::String(format!(
                "<i class=\"bi bi-{name}\"{style_attr}></i>"
            )))
        }) as BuiltinFn,
        mk(
            "wdoc::icon",
            vec!["name: string", "size: string", "color: string"],
            "Insert a Bootstrap Icon (optional size and color)",
        ),
    );

    let measure_text = std::sync::Arc::new(|args: &[Value]| {
        if args.len() != 1 {
            return Err("measure_text() expects 1 argument (text attributes or text block)".into());
        }
        let attrs = value_map_to_string_map(args.first())?;
        let metrics = wcl_wdoc::shapes::measure_text_attrs(&attrs);
        let mut map = IndexMap::new();
        map.insert("width".to_string(), Value::Float(metrics.width));
        map.insert("height".to_string(), Value::Float(metrics.height));
        map.insert("baseline".to_string(), Value::Float(metrics.baseline));
        Ok(Value::Map(map))
    }) as BuiltinFn;
    let measure_sig = |name: &str| FunctionSignature {
        name: name.into(),
        params: vec!["text: any".into()],
        return_type: "map".into(),
        doc: "Measure text using WDoc's deterministic fallback metrics".into(),
    };
    reg.register(
        "measure_text",
        measure_text.clone(),
        measure_sig("measure_text"),
    );
    reg.register(
        "wdoc::measure_text",
        measure_text,
        measure_sig("wdoc::measure_text"),
    );

    // Template rendering functions — receive Value::Map, return HTML string
    register_template_builtins(&mut reg);

    reg
}

fn register_template_builtins(reg: &mut FunctionRegistry) {
    let mk = |name: &str, doc: &str| FunctionSignature {
        name: name.into(),
        params: vec!["block: map".into()],
        return_type: "string".into(),
        doc: doc.into(),
    };

    reg.register(
        "wdoc::render_heading",
        std::sync::Arc::new(|args: &[Value]| {
            let attrs = value_map_to_string_map(args.first())?;
            Ok(Value::String(wcl_wdoc::templates::render_heading(&attrs)))
        }) as BuiltinFn,
        mk("wdoc::render_heading", "Render a heading element"),
    );

    // Per-level heading shorthands (h1..h6 schemas).
    macro_rules! register_h {
        ($name:literal, $level:literal) => {
            reg.register(
                concat!("wdoc::render_h", stringify!($level)),
                std::sync::Arc::new(|args: &[Value]| {
                    let attrs = value_map_to_string_map(args.first())?;
                    Ok(Value::String(wcl_wdoc::templates::render_heading_at(
                        $level, &attrs,
                    )))
                }) as BuiltinFn,
                mk($name, "Render a heading at a fixed level"),
            );
        };
    }
    register_h!("wdoc::render_h1", 1);
    register_h!("wdoc::render_h2", 2);
    register_h!("wdoc::render_h3", 3);
    register_h!("wdoc::render_h4", 4);
    register_h!("wdoc::render_h5", 5);
    register_h!("wdoc::render_h6", 6);

    reg.register(
        "wdoc::render_paragraph",
        std::sync::Arc::new(|args: &[Value]| {
            let attrs = value_map_to_string_map(args.first())?;
            Ok(Value::String(wcl_wdoc::templates::render_paragraph(&attrs)))
        }) as BuiltinFn,
        mk("wdoc::render_paragraph", "Render a paragraph element"),
    );

    reg.register(
        "wdoc::render_image",
        std::sync::Arc::new(|args: &[Value]| {
            let attrs = value_map_to_string_map(args.first())?;
            Ok(Value::String(wcl_wdoc::templates::render_image(&attrs)))
        }) as BuiltinFn,
        mk("wdoc::render_image", "Render an image element"),
    );

    reg.register(
        "wdoc::render_code",
        std::sync::Arc::new(|args: &[Value]| {
            let attrs = value_map_to_string_map(args.first())?;
            Ok(Value::String(wcl_wdoc::templates::render_code(&attrs)))
        }) as BuiltinFn,
        mk("wdoc::render_code", "Render a code block"),
    );

    reg.register(
        "wdoc::render_table",
        std::sync::Arc::new(|args: &[Value]| {
            let attrs = match args.first() {
                Some(Value::Map(m)) => m,
                Some(Value::BlockRef(br)) => &br.attributes,
                _ => return Err("wdoc_render_table expects a map argument".into()),
            };
            Ok(Value::String(render_table_html(attrs)))
        }) as BuiltinFn,
        mk("wdoc::render_table", "Render a table element"),
    );

    // Note: there is no `wdoc::render_diagram` builtin. Diagram rendering needs
    // access to the template dispatch context (so that shape templates can be
    // resolved), so it is special-cased in `extract_layout_children` and never
    // goes through the html template path.

    // attr_or(block, "key", default) — read an attribute from a BlockRef or Map
    // with a fallback. Used by shape template functions to handle optional widget
    // attributes without erroring on missing keys.
    reg.register(
        "attr_or",
        std::sync::Arc::new(|args: &[Value]| {
            if args.len() != 3 {
                return Err("attr_or() expects 3 arguments (block, key, default)".into());
            }
            let key = args[1]
                .as_string()
                .ok_or("attr_or() second argument must be a string")?;
            let val = match &args[0] {
                Value::BlockRef(br) => br.attributes.get(key).cloned(),
                Value::Map(m) => m.get(key).cloned(),
                _ => None,
            };
            Ok(val
                .filter(|v| !matches!(v, Value::Null))
                .unwrap_or_else(|| args[2].clone()))
        }) as BuiltinFn,
        FunctionSignature {
            name: "attr_or".into(),
            params: vec![
                "block: any".into(),
                "key: string".into(),
                "default: any".into(),
            ],
            return_type: "any".into(),
            doc: "Read an attribute from a block or map, returning a default if missing".into(),
        },
    );
}

/// Convert a Value::Map to IndexMap<String, String> for template functions.
fn value_map_to_string_map(val: Option<&Value>) -> Result<IndexMap<String, String>, String> {
    let map = match val {
        Some(Value::Map(m)) => m,
        Some(Value::BlockRef(br)) => &br.attributes,
        _ => return Err("template function expects a map argument".into()),
    };
    let mut result = IndexMap::new();
    for (k, v) in map {
        let s = match v {
            Value::String(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            _ => format!("{v}"),
        };
        result.insert(k.clone(), s);
    }
    Ok(result)
}

/// Render a `wdoc_table` block to an HTML `<table>`.
/// Finds the first `Value::List` attribute (the table data) and builds HTML rows.
fn render_table_html(attrs: &IndexMap<String, Value>) -> String {
    use std::fmt::Write;

    let caption = attrs.get("caption").and_then(|v| v.as_string());

    // Find the first List attribute — that's the table data
    let rows: Option<&Vec<Value>> = attrs.values().find_map(|v| match v {
        Value::List(list) => Some(list),
        _ => None,
    });

    let rows = match rows {
        Some(r) if !r.is_empty() => r,
        _ => return "<p class=\"wdoc-paragraph\"><em>(empty table)</em></p>".to_string(),
    };

    let mut html = String::from("<table class=\"wdoc-table\">\n");

    if let Some(cap) = caption {
        writeln!(html, "<caption>{cap}</caption>").unwrap();
    }

    // Extract headers from the first row's keys
    if let Value::Map(first_row) = &rows[0] {
        html.push_str("<thead><tr>");
        for key in first_row.keys() {
            write!(html, "<th>{key}</th>").unwrap();
        }
        html.push_str("</tr></thead>\n");
    }

    // Render body rows
    html.push_str("<tbody>\n");
    for row in rows {
        if let Value::Map(map) = row {
            html.push_str("<tr>");
            for val in map.values() {
                let cell = match val {
                    Value::String(s) => s.clone(),
                    Value::Int(i) => i.to_string(),
                    Value::Float(f) => f.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => String::new(),
                    other => format!("{other}"),
                };
                write!(html, "<td>{cell}</td>").unwrap();
            }
            html.push_str("</tr>\n");
        }
    }
    html.push_str("</tbody>\n</table>");
    html
}

/// Render a `callout` block — colored container with icon, header, and nested content.
fn render_callout_html(block: &BlockRef, ctx: &ExtractCtx) -> String {
    use std::fmt::Write;

    let color = block
        .attributes
        .get("color")
        .and_then(|v| v.as_string())
        .unwrap_or("var(--color-nav-border)");
    let header = block.attributes.get("header").and_then(|v| v.as_string());
    let icon = block.attributes.get("icon").and_then(|v| v.as_string());

    let mut html = String::new();
    write!(
        html,
        "<div class=\"wdoc-callout\" style=\"border-left-color:{color};\">"
    )
    .unwrap();

    // Header with optional icon
    if header.is_some() || icon.is_some() {
        write!(
            html,
            "<div class=\"wdoc-callout-header\" style=\"color:{color};\">"
        )
        .unwrap();
        if let Some(ic) = icon {
            write!(html, "<i class=\"bi bi-{ic}\"></i> ").unwrap();
        }
        if let Some(hdr) = header {
            html.push_str(hdr);
        }
        html.push_str("</div>");
    }

    // Body: render child content blocks
    html.push_str("<div class=\"wdoc-callout-body\">");
    for child_block in all_child_blocks(block) {
        match child_block.kind.as_str() {
            // Skip known non-content attributes
            "wdoc::layout" | "wdoc::section" | "wdoc::page" | "wdoc::doc" | "wdoc::style" => {}
            "wdoc::draw::diagram" => {
                html.push_str(&render_diagram_with_ctx(child_block, ctx));
                html.push('\n');
            }
            _kind => {
                if let Ok(child_html) = ctx.render_block(child_block) {
                    html.push_str(&child_html);
                    html.push('\n');
                }
            }
        }
    }
    html.push_str("</div></div>");

    html
}

/// Render a `wdoc::draw::diagram` block to inline SVG. Walks the diagram's child
/// blocks, dispatching shape templates via `ctx`, and feeds the resulting
/// `ShapeNode` tree to `shapes::render_diagram_svg`.
fn render_diagram_with_ctx(br: &BlockRef, ctx: &ExtractCtx) -> String {
    use wcl_wdoc::shapes::*;

    let mut str_attrs = value_map_to_string_map_lossy(&br.attributes);
    if let Some(scope) = str_attrs
        .get("design_system")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    {
        let scope_class = design_system_class(&scope);
        if let Some(css) = str_attrs.get("css") {
            ctx.css_registry.borrow_mut().register(&scope_class, css);
        }
        append_class_attr(&mut str_attrs, &scope_class);
        str_attrs.shift_remove("css");
    }

    let diagram_w = val_f64(br.attributes.get("width")).unwrap_or(600.0);
    let diagram_h = val_f64(br.attributes.get("height")).unwrap_or(400.0);
    let padding = val_f64(br.attributes.get("padding")).unwrap_or(0.0);
    let gap = val_f64(br.attributes.get("gap")).unwrap_or(40.0);
    let align = parse_alignment_str(str_attrs.get("align").map(|s| s.as_str()).unwrap_or("none"));

    let mut shapes = Vec::new();
    let mut connections = Vec::new();

    let mut source_order = 0;
    for val in br.attributes.values() {
        if let Value::BlockRef(child) = val {
            collect_shape_or_connection(child, &mut shapes, &mut connections, ctx, source_order);
            source_order += 1;
        }
    }
    for child in &br.children {
        collect_shape_or_connection(child, &mut shapes, &mut connections, ctx, source_order);
        source_order += 1;
    }

    let mut diagram = Diagram {
        id: br.id.clone(),
        width: diagram_w,
        height: diagram_h,
        shapes,
        connections,
        classes: ctx.diagram_classes.borrow().clone(),
        padding,
        align,
        gap,
        options: str_attrs,
    };

    render_diagram_svg(&mut diagram)
}

fn design_system_class(scope: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in scope.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            last_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if ch == '_' {
            last_dash = false;
            Some('_')
        } else if ch == '-' || ch.is_whitespace() {
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
    let suffix = out.trim_matches('-');
    let suffix = if suffix.is_empty() { "unnamed" } else { suffix };
    if suffix.starts_with("wad-ds-") {
        suffix.to_string()
    } else {
        format!("wad-ds-{suffix}")
    }
}

fn append_class_attr(attrs: &mut IndexMap<String, String>, class_name: &str) {
    match attrs.get_mut("class") {
        Some(existing) => {
            if !existing.split_whitespace().any(|class| class == class_name) {
                if !existing.trim().is_empty() {
                    existing.push(' ');
                }
                existing.push_str(class_name);
            }
        }
        None => {
            attrs.insert("class".to_string(), class_name.to_string());
        }
    }
}

fn collect_shape_or_connection(
    br: &BlockRef,
    shapes: &mut Vec<wcl_wdoc::shapes::ShapeNode>,
    connections: &mut Vec<wcl_wdoc::shapes::Connection>,
    ctx: &ExtractCtx,
    source_order: usize,
) {
    use wcl_wdoc::shapes::*;

    if br.kind == "wdoc::draw::connection" {
        let a = value_map_to_string_map_lossy(&br.attributes);
        connections.push(Connection {
            from_id: a.get("from").cloned().unwrap_or_default(),
            to_id: a.get("to").cloned().unwrap_or_default(),
            direction: parse_direction_str(a.get("direction").map(|s| s.as_str()).unwrap_or("")),
            from_anchor: parse_anchor_str(a.get("from_anchor").map(|s| s.as_str()).unwrap_or("")),
            to_anchor: parse_anchor_str(a.get("to_anchor").map(|s| s.as_str()).unwrap_or("")),
            label: a.get("label").cloned(),
            curve: parse_curve_str(a.get("curve").map(|s| s.as_str()).unwrap_or("")),
            z_index: a
                .get("z_index")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0),
            source_order,
            attrs: a,
        });
        return;
    }

    if is_draw_event_block(br)
        || is_draw_class_block(br)
        || is_draw_state_block(br)
        || is_draw_animation_block(br)
        || is_draw_keyframe_block(br)
    {
        return;
    }

    let is_composite = ctx
        .template_map
        .contains_key(&("shape".to_string(), br.kind.clone()));

    let kind = parse_shape_kind(&br.kind).or_else(|| {
        // User-defined @template("shape", ...) schemas are composite shape
        // containers even when they do not live under wdoc::draw::*.
        is_composite.then_some(ShapeKind::Rect)
    });

    if let Some(kind) = kind {
        let mut a = value_map_to_string_map_lossy(&br.attributes);

        // Composite shape: any block whose schema declares @template("shape", "fn").
        // Call the function and convert its returned shape descriptors into the
        // widget container's children.
        let mut children: Vec<ShapeNode> = if is_composite {
            match dispatch_shape_template(br, ctx) {
                Ok(mut result) => {
                    for child in &mut result.shapes {
                        mark_template_layout_decoration(child);
                    }
                    connections.extend(scope_connections(result.connections, br.id.as_deref()));
                    result.shapes
                }
                Err(e) => {
                    eprintln!(
                        "wdoc: warning: shape template for '{}' failed: {e}",
                        br.kind
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Also collect any user-defined child shapes from the block. Composite
        // shapes can still nest hand-written primitives alongside their
        // template-generated children.
        let mut child_connections = Vec::new();
        let mut child_source_order = children.len();
        for val in br.attributes.values() {
            if let Value::BlockRef(child_br) = val {
                if is_draw_event_block(child_br)
                    || is_draw_animation_block(child_br)
                    || is_draw_keyframe_block(child_br)
                {
                    continue;
                }
                collect_shape_or_connection(
                    child_br,
                    &mut children,
                    &mut child_connections,
                    ctx,
                    child_source_order,
                );
                child_source_order += 1;
            }
        }
        for child_br in &br.children {
            if is_draw_event_block(child_br)
                || is_draw_animation_block(child_br)
                || is_draw_keyframe_block(child_br)
            {
                continue;
            }
            collect_shape_or_connection(
                child_br,
                &mut children,
                &mut child_connections,
                ctx,
                child_source_order,
            );
            child_source_order += 1;
        }
        connections.extend(scope_connections(child_connections, br.id.as_deref()));

        let pf =
            |m: &IndexMap<String, String>, k: &str| m.get(k).and_then(|s| s.parse::<f64>().ok());
        let align = parse_alignment_str(a.get("align").map(|s| s.as_str()).unwrap_or("none"));
        let gap = pf(&a, "gap").unwrap_or(0.0);
        let pad = pf(&a, "padding").unwrap_or(0.0);
        let nx = pf(&a, "x");
        let ny = pf(&a, "y");
        let nw = pf(&a, "width");
        let nh = pf(&a, "height");
        let ntop = pf(&a, "top");
        let nbot = pf(&a, "bottom");
        let nleft = pf(&a, "left");
        let nright = pf(&a, "right");
        let z_index = pf(&a, "z_index").unwrap_or(0.0);

        if kind == ShapeKind::InlineSvg {
            hydrate_inline_svg_attrs(&mut a, ctx);
        }

        // Composite shape containers are invisible — the template provides all
        // visuals. Default fill/stroke to none so the wrapping rect doesn't
        // double up on the template's drawing.
        if is_composite {
            a.entry("fill".to_string())
                .or_insert_with(|| "none".to_string());
            a.entry("stroke".to_string())
                .or_insert_with(|| "none".to_string());
        }

        shapes.push(ShapeNode {
            kind,
            id: br.id.clone(),
            x: nx,
            y: ny,
            width: nw,
            height: nh,
            top: ntop,
            bottom: nbot,
            left: nleft,
            right: nright,
            resolved: Bounds::default(),
            attrs: a,
            events: collect_diagram_events(br),
            children,
            align,
            gap,
            padding: pad,
            z_index,
            source_order,
        });
    }
}

fn hydrate_inline_svg_attrs(attrs: &mut IndexMap<String, String>, ctx: &ExtractCtx) {
    if attrs.contains_key("content") || attrs.contains_key("_wdoc_inline_svg_content") {
        return;
    }
    let Some(src) = attrs
        .get("src")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    match read_local_inline_svg(&src, &ctx.svg_search_dirs) {
        Ok(content) => {
            attrs.insert("_wdoc_inline_svg_content".to_string(), content);
        }
        Err(err) => {
            eprintln!("wdoc: warning: inline_svg src '{src}' could not be loaded: {err}");
        }
    }
}

fn read_local_inline_svg(src: &str, search_dirs: &[PathBuf]) -> Result<String, String> {
    let lower = src.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("data:") {
        return Err("remote and data URLs are not supported".to_string());
    }
    if Path::new(src)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("svg"))
    {
        return Err("only .svg files can be embedded".to_string());
    }

    let canonical_dirs: Vec<PathBuf> = search_dirs
        .iter()
        .filter_map(|dir| dir.canonicalize().ok())
        .collect();
    if canonical_dirs.is_empty() {
        return Err("no source directories are available for SVG lookup".to_string());
    }

    let src_path = Path::new(src);
    if src_path.is_absolute() {
        let canonical = src_path
            .canonicalize()
            .map_err(|_| "file was not found in WDoc source directories".to_string())?;
        if !canonical_dirs.iter().any(|dir| canonical.starts_with(dir)) {
            return Err("path escapes the WDoc source directory".to_string());
        }
        return std::fs::read_to_string(&canonical)
            .map_err(|e| format!("failed to read {}: {e}", canonical.display()));
    }

    for dir in &canonical_dirs {
        let candidate = dir.join(src_path);
        let Ok(canonical) = candidate.canonicalize() else {
            continue;
        };
        if !canonical.starts_with(dir) {
            return Err("path escapes the WDoc source directory".to_string());
        }
        return std::fs::read_to_string(&canonical)
            .map_err(|e| format!("failed to read {}: {e}", canonical.display()));
    }

    Err("file was not found in WDoc source directories".to_string())
}

struct ShapeTemplateResult {
    shapes: Vec<wcl_wdoc::shapes::ShapeNode>,
    connections: Vec<wcl_wdoc::shapes::Connection>,
}

/// Look up a `@template("shape", "fn")` function for `br.kind` and call it.
/// The function receives the BlockRef as its single argument and must return a
/// list of "shape descriptor" values (maps describing primitive shapes).
fn dispatch_shape_template(br: &BlockRef, ctx: &ExtractCtx) -> Result<ShapeTemplateResult, String> {
    let fn_name = ctx
        .template_map
        .get(&("shape".to_string(), br.kind.clone()))
        .ok_or_else(|| format!("no @template(\"shape\", ...) on schema '{}'", br.kind))?;

    let func = ctx
        .template_fns
        .get(fn_name)
        .ok_or_else(|| format!("shape template function '{fn_name}' not registered"))?;

    let arg = Value::BlockRef(br.clone());
    let result = match func {
        TemplateFn::Lambda(fv) => crate::call_lambda(fv, &[arg], &ctx.builtins)?,
        TemplateFn::Builtin(f) => f(&[arg])?,
    };

    let descriptors = match result {
        Value::List(items) => items,
        Value::Map(_) => vec![result],
        Value::Null => vec![],
        other => {
            return Err(format!(
                "shape template '{fn_name}' must return a list of shape maps, got {}",
                other.type_name()
            ))
        }
    };

    let mut result = ShapeTemplateResult {
        shapes: Vec::new(),
        connections: Vec::new(),
    };
    for (idx, desc) in descriptors.iter().enumerate() {
        if let Some(conn) = descriptor_to_connection_with_order(desc, idx) {
            result.connections.push(conn);
        } else if let Some((node, connections)) =
            descriptor_to_shape_node_and_connections(desc, idx)
        {
            result.shapes.push(node);
            result.connections.extend(connections);
        }
    }
    Ok(result)
}

/// Convert a shape descriptor `Value::Map` (returned from a WCL shape template)
/// into a `ShapeNode`. Recognized fields: kind, x, y, width, height, top, bottom,
/// left, right, align, gap, padding, children (list of more descriptors), and
/// arbitrary visual attributes (fill, stroke, rx, content, font_size, ...).
#[cfg(test)]
fn descriptor_to_shape_node_with_order(
    val: &Value,
    source_order: usize,
) -> Option<wcl_wdoc::shapes::ShapeNode> {
    descriptor_to_shape_node_and_connections(val, source_order).map(|(node, _)| node)
}

fn descriptor_to_shape_node_and_connections(
    val: &Value,
    source_order: usize,
) -> Option<(
    wcl_wdoc::shapes::ShapeNode,
    Vec<wcl_wdoc::shapes::Connection>,
)> {
    use wcl_wdoc::shapes::*;

    let map = match val {
        Value::Map(m) => m,
        _ => return None,
    };

    let kind_str = map.get("kind").and_then(|v| v.as_string())?;
    // Accept short names ("rect") or fully-qualified ("wdoc::draw::rect").
    let qualified = if kind_str.contains("::") {
        kind_str.to_string()
    } else {
        format!("wdoc::draw::{kind_str}")
    };
    let kind = parse_shape_kind(&qualified)?;

    let pf = |k: &str| map.get(k).and_then(value_as_f64);
    let nx = pf("x");
    let ny = pf("y");
    let nw = pf("width");
    let nh = pf("height");
    let ntop = pf("top");
    let nbot = pf("bottom");
    let nleft = pf("left");
    let nright = pf("right");
    let gap = pf("gap").unwrap_or(0.0);
    let padding = pf("padding").unwrap_or(0.0);
    let z_index = pf("z_index").unwrap_or(0.0);

    let align_str = map
        .get("align")
        .and_then(|v| v.as_string())
        .unwrap_or("none");
    let align = parse_alignment_str(align_str);

    let mut attrs = IndexMap::new();
    for (k, v) in map {
        // Skip structural fields — they become ShapeNode fields, not SVG attrs.
        if matches!(
            k.as_str(),
            "kind"
                | "x"
                | "y"
                | "width"
                | "height"
                | "top"
                | "bottom"
                | "left"
                | "right"
                | "gap"
                | "padding"
                | "z_index"
                | "align"
                | "children"
                | "id"
                | "layout_role"
        ) {
            continue;
        }
        let s = match v {
            Value::String(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => continue,
            _ => continue,
        };
        attrs.insert(k.clone(), s);
    }
    if map
        .get("layout_role")
        .and_then(|v| v.as_string())
        .is_some_and(|role| role == "node")
    {
        attrs.insert("_wdoc_layout_role".to_string(), "node".to_string());
    }

    let mut children = Vec::new();
    let mut connections = Vec::new();
    if let Some(Value::List(items)) = map.get("children") {
        for (idx, item) in items.iter().enumerate() {
            if let Some(conn) = descriptor_to_connection_with_order(item, idx) {
                connections.push(conn);
            } else if let Some((child, child_connections)) =
                descriptor_to_shape_node_and_connections(item, idx)
            {
                children.push(child);
                connections.extend(child_connections);
            }
        }
    }

    let id = map.get("id").and_then(|v| v.as_string()).map(String::from);

    let node = ShapeNode {
        kind,
        id: id.clone(),
        x: nx,
        y: ny,
        width: nw,
        height: nh,
        top: ntop,
        bottom: nbot,
        left: nleft,
        right: nright,
        resolved: Bounds::default(),
        attrs,
        events: Vec::new(),
        children,
        align,
        gap,
        padding,
        z_index,
        source_order,
    };

    Some((node, scope_connections(connections, id.as_deref())))
}

fn descriptor_to_connection_with_order(
    val: &Value,
    source_order: usize,
) -> Option<wcl_wdoc::shapes::Connection> {
    use wcl_wdoc::shapes::*;

    let map = match val {
        Value::Map(m) => m,
        _ => return None,
    };
    let kind = map.get("kind").and_then(|v| v.as_string())?;
    if kind != "connection" && kind != "wdoc::draw::connection" {
        return None;
    }

    let mut attrs = IndexMap::new();
    for (k, v) in map {
        if k == "kind" {
            continue;
        }
        let s = match v {
            Value::String(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => continue,
            _ => continue,
        };
        attrs.insert(k.clone(), s);
    }

    Some(Connection {
        from_id: attrs.get("from").cloned().unwrap_or_default(),
        to_id: attrs.get("to").cloned().unwrap_or_default(),
        direction: parse_direction_str(attrs.get("direction").map(|s| s.as_str()).unwrap_or("")),
        from_anchor: parse_anchor_str(attrs.get("from_anchor").map(|s| s.as_str()).unwrap_or("")),
        to_anchor: parse_anchor_str(attrs.get("to_anchor").map(|s| s.as_str()).unwrap_or("")),
        label: attrs.get("label").cloned(),
        curve: parse_curve_str(attrs.get("curve").map(|s| s.as_str()).unwrap_or("")),
        z_index: attrs
            .get("z_index")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0),
        source_order,
        attrs,
    })
}

fn mark_template_layout_decoration(node: &mut wcl_wdoc::shapes::ShapeNode) {
    let role = node.attrs.shift_remove("_wdoc_layout_role");
    if role.as_deref() != Some("node") {
        node.attrs
            .insert("_wdoc_layout_decoration".to_string(), "true".to_string());
    }
    for child in &mut node.children {
        mark_template_layout_decoration(child);
    }
}

fn scope_connections(
    connections: Vec<wcl_wdoc::shapes::Connection>,
    scope: Option<&str>,
) -> Vec<wcl_wdoc::shapes::Connection> {
    let Some(scope) = scope.filter(|scope| !scope.is_empty()) else {
        return connections;
    };
    connections
        .into_iter()
        .map(|mut conn| {
            conn.from_id = scope_connection_endpoint(&conn.from_id, scope);
            conn.to_id = scope_connection_endpoint(&conn.to_id, scope);
            conn
        })
        .collect()
}

fn scope_connection_endpoint(endpoint: &str, scope: &str) -> String {
    if endpoint.is_empty() || endpoint == scope || endpoint.starts_with(&format!("{scope}.")) {
        endpoint.to_string()
    } else {
        format!("{scope}.{endpoint}")
    }
}

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// Convert Value map to string map without erroring — for diagram attributes.
fn value_map_to_string_map_lossy(map: &IndexMap<String, Value>) -> IndexMap<String, String> {
    let mut result = IndexMap::new();
    for (k, v) in map {
        if k.starts_with('_') {
            continue;
        }
        let s = match v {
            Value::String(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            Value::BlockRef(_) => continue, // skip child blocks
            _ => format!("{v}"),
        };
        result.insert(k.clone(), s);
    }
    result
}

fn collect_diagram_classes(
    values: &IndexMap<String, Value>,
) -> IndexMap<String, wcl_wdoc::shapes::DiagramClass> {
    let mut classes = IndexMap::new();
    for value in values.values() {
        if let Value::BlockRef(block) = value {
            collect_diagram_classes_in_block(block, &mut classes);
        }
    }
    classes
}

fn collect_diagram_classes_in_block(
    block: &BlockRef,
    classes: &mut IndexMap<String, wcl_wdoc::shapes::DiagramClass>,
) {
    if is_draw_class_block(block) {
        let Some(name) = block.id.clone() else {
            return;
        };
        let mut attrs = value_map_to_string_map_lossy(&block.attributes);
        let mut states = IndexMap::new();
        let mut animations = IndexMap::new();
        for child in all_child_blocks(block) {
            if is_draw_state_block(child) {
                if let Some(state_name) = child.id.clone() {
                    states.insert(
                        state_name.clone(),
                        wcl_wdoc::shapes::DiagramState {
                            name: state_name,
                            attrs: value_map_to_string_map_lossy(&child.attributes),
                        },
                    );
                }
            } else if is_draw_animation_block(child) {
                if let Some(animation) = parse_diagram_animation(child) {
                    animations.insert(animation.name.clone(), animation);
                }
            }
        }
        attrs.shift_remove("state");
        attrs.shift_remove("animation");
        classes.insert(
            name.clone(),
            wcl_wdoc::shapes::DiagramClass {
                name,
                attrs,
                states,
                animations,
            },
        );
        return;
    }
    for child in all_child_blocks(block) {
        collect_diagram_classes_in_block(child, classes);
    }
}

fn collect_diagram_events(block: &BlockRef) -> Vec<wcl_wdoc::shapes::DiagramEvent> {
    all_child_blocks(block)
        .into_iter()
        .filter(|child| is_draw_event_block(child))
        .filter_map(|child| {
            let trigger = child.attributes.get("trigger")?.as_string()?.to_string();
            let state = child.attributes.get("state")?.as_string()?.to_string();
            let target = child
                .attributes
                .get("target")
                .and_then(|v| v.as_string())
                .map(str::to_string);
            let button = child
                .attributes
                .get("button")
                .and_then(|v| v.as_string())
                .map(str::to_string);
            let mode = child
                .attributes
                .get("mode")
                .and_then(|v| v.as_string())
                .map(str::to_string);
            let duration_ms = child.attributes.get("duration_ms").and_then(|v| match v {
                Value::Int(i) => Some(*i as i32),
                Value::Float(f) => Some(*f as i32),
                Value::String(s) => s.parse().ok(),
                _ => None,
            });
            let prevent_default = child
                .attributes
                .get("prevent_default")
                .and_then(|v| match v {
                    Value::Bool(b) => Some(*b),
                    Value::String(s) => s.parse().ok(),
                    _ => None,
                });
            Some(wcl_wdoc::shapes::DiagramEvent {
                name: child.id.clone(),
                trigger,
                state,
                target,
                button,
                mode,
                duration_ms,
                prevent_default,
            })
        })
        .collect()
}

fn parse_diagram_animation(block: &BlockRef) -> Option<wcl_wdoc::shapes::DiagramAnimation> {
    let name = block.id.clone()?;
    let attrs = &block.attributes;
    let mut keyframes = all_child_blocks(block)
        .into_iter()
        .filter(|child| is_draw_keyframe_block(child))
        .filter_map(parse_diagram_keyframe)
        .collect::<Vec<_>>();
    keyframes.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    if keyframes.is_empty() {
        return None;
    }
    Some(wcl_wdoc::shapes::DiagramAnimation {
        name,
        duration_ms: value_as_i32(attrs.get("duration_ms")).unwrap_or(1000),
        delay_ms: value_as_i32(attrs.get("delay_ms")).unwrap_or(0),
        timing_function: value_as_string(attrs.get("timing_function"))
            .unwrap_or("ease")
            .to_string(),
        iteration_count: value_as_string(attrs.get("iteration_count"))
            .unwrap_or("1")
            .to_string(),
        direction: value_as_string(attrs.get("direction"))
            .unwrap_or("normal")
            .to_string(),
        fill_mode: value_as_string(attrs.get("fill_mode"))
            .unwrap_or("none")
            .to_string(),
        keyframes,
    })
}

fn parse_diagram_keyframe(block: &BlockRef) -> Option<wcl_wdoc::shapes::DiagramKeyframe> {
    let offset = block
        .attributes
        .get("offset")
        .and_then(value_as_f64)
        .or_else(|| {
            block.id.as_deref().and_then(|id| match id {
                "from" => Some(0.0),
                "to" => Some(100.0),
                other => other.parse::<f64>().ok(),
            })
        })
        .filter(|offset| (0.0..=100.0).contains(offset))?;
    Some(wcl_wdoc::shapes::DiagramKeyframe {
        offset,
        x: block.attributes.get("x").and_then(value_as_f64),
        y: block.attributes.get("y").and_then(value_as_f64),
        width: block.attributes.get("width").and_then(value_as_f64),
        height: block.attributes.get("height").and_then(value_as_f64),
    })
}

fn value_as_i32(v: Option<&Value>) -> Option<i32> {
    v.and_then(|value| match value {
        Value::Int(i) => Some(*i as i32),
        Value::Float(f) => Some(*f as i32),
        Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

fn value_as_string(v: Option<&Value>) -> Option<&str> {
    v.and_then(|value| value.as_string())
}

fn is_draw_class_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::class" | "draw::class" | "class"
    )
}

fn is_draw_state_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::state" | "draw::state" | "state"
    )
}

fn is_draw_animation_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::animation" | "draw::animation" | "animation"
    )
}

fn is_draw_keyframe_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::keyframe" | "draw::keyframe" | "keyframe"
    )
}

fn is_draw_event_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::event" | "draw::event" | "event"
    )
}

fn val_f64(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Int(i)) => Some(*i as f64),
        Some(Value::Float(f)) => Some(*f),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Extraction: BlockRef → wdoc model (with template function calls)
// ---------------------------------------------------------------------------

struct ExtractCtx {
    template_map: HashMap<(String, String), String>,
    template_fns: HashMap<String, TemplateFn>,
    builtins: HashMap<String, BuiltinFn>,
    css_registry: Rc<RefCell<DiagramCssRegistry>>,
    diagram_classes: Rc<RefCell<IndexMap<String, wcl_wdoc::shapes::DiagramClass>>>,
    svg_search_dirs: Vec<PathBuf>,
}

impl ExtractCtx {
    fn render_block(&self, block: &BlockRef) -> Result<String, String> {
        let kind = &block.kind;
        let fn_name = self
            .template_map
            .get(&("html".to_string(), kind.clone()))
            .ok_or_else(|| format!("no @template(\"html\", ...) found for block kind '{kind}'"))?;

        let func = self
            .template_fns
            .get(fn_name)
            .ok_or_else(|| format!("template function '{fn_name}' not found for '{kind}'"))?;

        call_template(func, block, &self.builtins)
    }
}

#[derive(Default)]
struct DiagramCssRegistry {
    css_by_scope: BTreeMap<String, BTreeSet<String>>,
    global_css: BTreeSet<String>,
    font_faces: BTreeSet<String>,
}

impl DiagramCssRegistry {
    fn register(&mut self, scope_class: &str, css: &str) {
        let css = css.trim();
        if css.is_empty() {
            return;
        }
        let scoped = wcl_wdoc::shapes::scope_css_to_selector(css, &format!(".{scope_class}"));
        self.css_by_scope
            .entry(scope_class.to_string())
            .or_default()
            .insert(scoped);
    }

    fn register_global(&mut self, css: &str) {
        let css = css.trim();
        if !css.is_empty() {
            self.global_css.insert(css.to_string());
        }
    }

    fn register_font_face(&mut self, css: &str) {
        let css = css.trim();
        if !css.is_empty() {
            self.font_faces.insert(css.to_string());
        }
    }

    fn render_css(&self) -> String {
        let mut blocks = Vec::new();
        for css in &self.font_faces {
            if !css.trim().is_empty() {
                blocks.push(css.trim());
            }
        }
        for css in &self.global_css {
            if !css.trim().is_empty() {
                blocks.push(css.trim());
            }
        }
        for set in self.css_by_scope.values() {
            for css in set {
                if !css.trim().is_empty() {
                    blocks.push(css.trim());
                }
            }
        }
        blocks.join("\n")
    }
}

fn register_css_fragment(block: &BlockRef, ctx: &ExtractCtx) -> Result<(), String> {
    let scope = block
        .attributes
        .get("scope")
        .and_then(|v| v.as_string())
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .ok_or_else(|| {
            format!(
                "css_fragment '{}' missing non-empty 'scope' attribute",
                block.id.as_deref().unwrap_or("(anonymous)")
            )
        })?;
    let css = block
        .attributes
        .get("css")
        .and_then(|v| v.as_string())
        .ok_or_else(|| {
            format!(
                "css_fragment '{}' missing 'css' attribute",
                block.id.as_deref().unwrap_or("(anonymous)")
            )
        })?;

    let scope_class = design_system_class(scope);
    ctx.css_registry.borrow_mut().register(&scope_class, css);
    Ok(())
}

fn register_global_css(block: &BlockRef, ctx: &ExtractCtx) -> Result<(), String> {
    let css = block
        .attributes
        .get("css")
        .and_then(|v| v.as_string())
        .ok_or_else(|| {
            format!(
                "global_css '{}' missing 'css' attribute",
                block.id.as_deref().unwrap_or("(anonymous)")
            )
        })?;

    ctx.css_registry.borrow_mut().register_global(css);
    Ok(())
}

fn register_font_asset(block: &BlockRef, ctx: &ExtractCtx) -> Result<(), String> {
    let family = block
        .attributes
        .get("family")
        .and_then(|v| v.as_string())
        .map(str::trim)
        .filter(|family| !family.is_empty())
        .ok_or_else(|| {
            format!(
                "font_asset '{}' missing non-empty 'family' attribute",
                block.id.as_deref().unwrap_or("(anonymous)")
            )
        })?;
    let src = block
        .attributes
        .get("src")
        .and_then(|v| v.as_string())
        .map(str::trim)
        .filter(|src| !src.is_empty())
        .ok_or_else(|| {
            format!(
                "font_asset '{}' missing non-empty 'src' attribute",
                block.id.as_deref().unwrap_or("(anonymous)")
            )
        })?;

    if is_remote_or_data_url(src) {
        eprintln!(
            "wdoc: warning: font_asset '{}' uses a remote/data src and was skipped",
            block.id.as_deref().unwrap_or("(anonymous)")
        );
        return Ok(());
    }

    let Some(format) = font_format_for_src(src) else {
        eprintln!(
            "wdoc: warning: font_asset '{}' uses unsupported font extension in '{}'",
            block.id.as_deref().unwrap_or("(anonymous)"),
            src
        );
        return Ok(());
    };

    let weight = block
        .attributes
        .get("weight")
        .and_then(|v| v.as_string())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("400");
    let style = block
        .attributes
        .get("style")
        .and_then(|v| v.as_string())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("normal");
    let display = block
        .attributes
        .get("display")
        .and_then(|v| v.as_string())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("swap");

    let css = format!(
        "@font-face {{\n  font-family: \"{}\";\n  src: url(\"{}\") format(\"{}\");\n  font-weight: {};\n  font-style: {};\n  font-display: {};\n}}",
        css_string_escape(family),
        css_string_escape(src),
        css_string_escape(format),
        css_declaration_value(weight),
        css_declaration_value(style),
        css_declaration_value(display)
    );

    ctx.css_registry.borrow_mut().register_font_face(&css);
    Ok(())
}

fn is_remote_or_data_url(src: &str) -> bool {
    src.starts_with("http://")
        || src.starts_with("https://")
        || src.starts_with("data:")
        || src.starts_with("//")
}

fn font_format_for_src(src: &str) -> Option<&'static str> {
    let path = src.split(['?', '#']).next().unwrap_or(src);
    let ext = Path::new(path).extension()?.to_str()?;
    match ext.to_ascii_lowercase().as_str() {
        "woff2" => Some("woff2"),
        "woff" => Some("woff"),
        "ttf" => Some("truetype"),
        "otf" => Some("opentype"),
        "eot" => Some("embedded-opentype"),
        _ => None,
    }
}

fn css_string_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn css_declaration_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, ';' | '{' | '}' | '<' | '>'))
        .collect::<String>()
}

fn register_css_assets_in_block(block: &BlockRef, ctx: &ExtractCtx) -> Result<(), String> {
    match block.kind.as_str() {
        "wdoc::css_fragment" => register_css_fragment(block, ctx)?,
        "wdoc::global_css" => register_global_css(block, ctx)?,
        "wdoc::font_asset" => register_font_asset(block, ctx)?,
        _ => {}
    }
    for child in all_child_blocks(block) {
        register_css_assets_in_block(child, ctx)?;
    }
    Ok(())
}

fn register_css_assets(values: &IndexMap<String, Value>, ctx: &ExtractCtx) -> Result<(), String> {
    for value in values.values() {
        if let Value::BlockRef(block) = value {
            register_css_assets_in_block(block, ctx)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod wdoc_draw_tests {
    use super::*;
    use crate::Span;

    fn block(
        kind: &str,
        id: Option<&str>,
        attributes: IndexMap<String, Value>,
        children: Vec<BlockRef>,
    ) -> BlockRef {
        BlockRef {
            kind: kind.to_string(),
            id: id.map(str::to_string),
            qualified_id: id.map(str::to_string),
            attributes,
            children,
            decorators: vec![],
            span: Span::dummy(),
        }
    }

    fn string_attr(attrs: &mut IndexMap<String, Value>, key: &str, value: &str) {
        attrs.insert(key.to_string(), Value::String(value.to_string()));
    }

    fn empty_ctx() -> ExtractCtx {
        ExtractCtx {
            template_map: HashMap::new(),
            template_fns: HashMap::new(),
            builtins: HashMap::new(),
            css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
            diagram_classes: Rc::new(RefCell::new(IndexMap::new())),
            svg_search_dirs: Vec::new(),
        }
    }

    fn custom_shape_ctx(kind: &str, template_name: &str, template: BuiltinFn) -> ExtractCtx {
        let mut ctx = empty_ctx();
        ctx.template_map.insert(
            ("shape".to_string(), kind.to_string()),
            template_name.to_string(),
        );
        ctx.template_fns
            .insert(template_name.to_string(), TemplateFn::Builtin(template));
        ctx
    }

    fn int_attr(attrs: &mut IndexMap<String, Value>, key: &str, value: i64) {
        attrs.insert(key.to_string(), Value::Int(value));
    }

    #[test]
    fn measure_text_builtin_accepts_map_and_block_ref() {
        let functions = wdoc_functions();
        let measure = functions
            .functions
            .get("measure_text")
            .expect("measure_text should be registered");

        let mut attrs = IndexMap::new();
        string_attr(&mut attrs, "content", "Inline text");
        int_attr(&mut attrs, "font_size", 14);

        let map_result = measure(&[Value::Map(attrs.clone())]).unwrap();
        let block_result = measure(&[Value::BlockRef(block(
            "wdoc::draw::text",
            Some("label"),
            attrs,
            vec![],
        ))])
        .unwrap();

        let Value::Map(map_metrics) = map_result else {
            panic!("measure_text should return a map");
        };
        let Value::Map(block_metrics) = block_result else {
            panic!("measure_text should return a map");
        };
        assert_eq!(
            map_metrics.get("width").unwrap().as_float(),
            block_metrics.get("width").unwrap().as_float()
        );
        assert!(map_metrics.get("height").unwrap().as_float().unwrap() > 0.0);
        assert!(map_metrics.get("baseline").unwrap().as_float().unwrap() > 0.0);

        assert!(functions.functions.contains_key("wdoc::measure_text"));
    }

    #[test]
    fn measure_text_can_drive_inline_drawing_position_expression() {
        let functions = wdoc_functions();
        let doc = crate::parse(
            r#"
            text label {
                content = "Inline text"
                font_size = 14
            }

            export let icon_x = 20 + measure_text(label).width + 8
            "#,
            crate::ParseOptions {
                functions,
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );

        let icon_x = match doc.values.get("icon_x") {
            Some(Value::Float(v)) => *v,
            Some(Value::Int(v)) => *v as f64,
            other => panic!("expected icon_x to evaluate to a number, got {other:?}"),
        };
        assert!(icon_x > 28.0);
    }

    #[test]
    fn wdoc_template_lambda_can_read_filtered_children() {
        let functions = wdoc_functions();
        let doc = crate::parse(
            r#"
            export let menu_labels = (b) =>
                join("|", map(children(b, "UiMenuItem"), item => item.label))
            "#,
            crate::ParseOptions {
                functions: functions.clone(),
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );

        let func = match doc.values.get("menu_labels") {
            Some(Value::Function(func)) => func,
            other => panic!("expected menu_labels function, got {other:?}"),
        };

        let mut file_attrs = IndexMap::new();
        string_attr(&mut file_attrs, "label", "File");
        let mut divider_attrs = IndexMap::new();
        string_attr(&mut divider_attrs, "label", "-");
        let mut edit_attrs = IndexMap::new();
        string_attr(&mut edit_attrs, "label", "Edit");

        let menu = block(
            "UiMenu",
            Some("main"),
            IndexMap::new(),
            vec![
                block("UiMenuItem", Some("file"), file_attrs, vec![]),
                block("UiDivider", Some("divider"), divider_attrs, vec![]),
                block("UiMenuItem", Some("edit"), edit_attrs, vec![]),
            ],
        );

        let rendered =
            crate::call_lambda(func, &[Value::BlockRef(menu)], &functions.functions).unwrap();
        assert_eq!(rendered, Value::String("File|Edit".to_string()));
    }

    #[test]
    fn diagram_css_and_group_flow_through_cli_extraction() {
        let mut rect_attrs = IndexMap::new();
        int_attr(&mut rect_attrs, "x", 0);
        int_attr(&mut rect_attrs, "y", 0);
        int_attr(&mut rect_attrs, "width", 120);
        int_attr(&mut rect_attrs, "height", 36);
        string_attr(&mut rect_attrs, "class", "ui-button-bg");
        string_attr(&mut rect_attrs, "fill", "#5E81AC");

        let mut text_attrs = IndexMap::new();
        int_attr(&mut text_attrs, "x", 0);
        int_attr(&mut text_attrs, "y", 0);
        int_attr(&mut text_attrs, "width", 120);
        int_attr(&mut text_attrs, "height", 36);
        string_attr(&mut text_attrs, "content", "Preview");

        let mut group_attrs = IndexMap::new();
        int_attr(&mut group_attrs, "x", 20);
        int_attr(&mut group_attrs, "y", 16);
        int_attr(&mut group_attrs, "width", 120);
        int_attr(&mut group_attrs, "height", 36);
        string_attr(&mut group_attrs, "class", "ui-button");
        string_attr(&mut group_attrs, "cursor", "pointer");
        string_attr(&mut group_attrs, "pointer_events", "all");

        let group = block(
            "wdoc::draw::group",
            Some("button"),
            group_attrs,
            vec![
                block("wdoc::draw::rect", Some("bg"), rect_attrs, vec![]),
                block("wdoc::draw::text", Some("label"), text_attrs, vec![]),
            ],
        );

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 180);
        int_attr(&mut diagram_attrs, "height", 70);
        string_attr(
            &mut diagram_attrs,
            "css",
            ".ui-button:hover .ui-button-bg { fill: #81A1C1; }",
        );
        let diagram = block(
            "wdoc::draw::diagram",
            Some("button_preview"),
            diagram_attrs,
            vec![group],
        );

        let ctx = empty_ctx();

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("id=\"wdoc-diagram-button-preview\""));
        assert!(html.contains("<g transform=\"translate(20,16)\" class=\"ui-button\""));
        assert!(html.contains("pointer-events=\"all\""));
        assert!(html.contains("#wdoc-diagram-button-preview .ui-button:hover .ui-button-bg"));
    }

    #[test]
    fn design_system_diagram_css_is_registered_once() {
        let mut rect_attrs = IndexMap::new();
        int_attr(&mut rect_attrs, "x", 0);
        int_attr(&mut rect_attrs, "y", 0);
        int_attr(&mut rect_attrs, "width", 40);
        int_attr(&mut rect_attrs, "height", 20);
        string_attr(&mut rect_attrs, "class", "ui-button-bg");

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 80);
        int_attr(&mut diagram_attrs, "height", 40);
        string_attr(&mut diagram_attrs, "design_system", "wad_interface");
        string_attr(
            &mut diagram_attrs,
            "css",
            ".ui-button:hover .ui-button-bg { fill: #81A1C1; }",
        );

        let diagram = block(
            "wdoc::draw::diagram",
            Some("button_preview"),
            diagram_attrs,
            vec![block("wdoc::draw::rect", Some("bg"), rect_attrs, vec![])],
        );
        let ctx = empty_ctx();

        let first = render_diagram_with_ctx(&diagram, &ctx);
        let second = render_diagram_with_ctx(&diagram, &ctx);
        let css = ctx.css_registry.borrow().render_css();

        assert!(first.contains("class=\"wad-ds-wad_interface\""));
        assert!(second.contains("class=\"wad-ds-wad_interface\""));
        assert!(!first.contains("<style>"));
        assert!(!second.contains("<style>"));
        assert!(css.contains(".wad-ds-wad_interface .ui-button:hover .ui-button-bg"));
        assert_eq!(
            css.matches(".wad-ds-wad_interface .ui-button:hover .ui-button-bg")
                .count(),
            1
        );
    }

    #[test]
    fn css_fragment_registers_scoped_extra_css() {
        let mut doc_attrs = IndexMap::new();
        string_attr(&mut doc_attrs, "title", "Docs");
        let doc = block("wdoc::doc", Some("docs"), doc_attrs, vec![]);

        let mut fragment_attrs = IndexMap::new();
        string_attr(&mut fragment_attrs, "scope", "wad_interface");
        string_attr(
            &mut fragment_attrs,
            "css",
            ".token-swatch { fill: var(--wad-token-frost); }",
        );
        let fragment = block("wdoc::css_fragment", Some("tokens"), fragment_attrs, vec![]);

        let mut values = IndexMap::new();
        values.insert("docs".to_string(), Value::BlockRef(doc));
        values.insert("tokens".to_string(), Value::BlockRef(fragment.clone()));
        values.insert("tokens_duplicate".to_string(), Value::BlockRef(fragment));
        let ctx = empty_ctx();

        let document = extract(&values, &ctx).expect("extract");

        assert!(document
            .extra_css
            .contains(".wad-ds-wad_interface .token-swatch"));
        assert_eq!(
            document
                .extra_css
                .matches(".wad-ds-wad_interface .token-swatch")
                .count(),
            1
        );
    }

    #[test]
    fn font_asset_and_global_css_register_extra_css_once() {
        let mut doc_attrs = IndexMap::new();
        string_attr(&mut doc_attrs, "title", "Docs");
        let doc = block("wdoc::doc", Some("docs"), doc_attrs, vec![]);

        let mut font_attrs = IndexMap::new();
        string_attr(&mut font_attrs, "family", "Inter");
        string_attr(&mut font_attrs, "src", "fonts/Inter-Regular.woff2");
        string_attr(&mut font_attrs, "weight", "400");
        string_attr(&mut font_attrs, "style", "normal");
        string_attr(&mut font_attrs, "display", "swap");
        let font = block(
            "wdoc::font_asset",
            Some("inter_regular"),
            font_attrs,
            vec![],
        );

        let mut global_attrs = IndexMap::new();
        string_attr(
            &mut global_attrs,
            "css",
            ":root { --font-body: \"Inter\", system-ui, sans-serif; }",
        );
        let global = block("wdoc::global_css", Some("app_fonts"), global_attrs, vec![]);

        let mut values = IndexMap::new();
        values.insert("docs".to_string(), Value::BlockRef(doc));
        values.insert("font".to_string(), Value::BlockRef(font.clone()));
        values.insert("font_duplicate".to_string(), Value::BlockRef(font));
        values.insert("global".to_string(), Value::BlockRef(global));
        let ctx = empty_ctx();

        let document = extract(&values, &ctx).expect("extract");

        assert!(document.extra_css.contains("@font-face"));
        assert!(document.extra_css.contains("font-family: \"Inter\";"));
        assert!(document
            .extra_css
            .contains("src: url(\"fonts/Inter-Regular.woff2\") format(\"woff2\");"));
        assert!(document.extra_css.contains("font-weight: 400;"));
        assert!(document.extra_css.contains("font-style: normal;"));
        assert!(document.extra_css.contains("font-display: swap;"));
        assert_eq!(document.extra_css.matches("@font-face").count(), 1);
        assert!(document
            .extra_css
            .contains(":root { --font-body: \"Inter\""));
        assert!(!document.extra_css.contains(".wad-ds-"));
    }

    #[test]
    fn diagram_z_index_flows_through_cli_extraction() {
        let mut back_attrs = IndexMap::new();
        int_attr(&mut back_attrs, "x", 0);
        int_attr(&mut back_attrs, "y", 0);
        int_attr(&mut back_attrs, "width", 40);
        int_attr(&mut back_attrs, "height", 40);
        int_attr(&mut back_attrs, "z_index", 10);
        string_attr(&mut back_attrs, "fill", "red");

        let mut front_attrs = IndexMap::new();
        int_attr(&mut front_attrs, "x", 0);
        int_attr(&mut front_attrs, "y", 0);
        int_attr(&mut front_attrs, "width", 40);
        int_attr(&mut front_attrs, "height", 40);
        int_attr(&mut front_attrs, "z_index", -1);
        string_attr(&mut front_attrs, "fill", "blue");

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 80);
        int_attr(&mut diagram_attrs, "height", 80);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("z_preview"),
            diagram_attrs,
            vec![
                block("wdoc::draw::rect", Some("red"), back_attrs, vec![]),
                block("wdoc::draw::rect", Some("blue"), front_attrs, vec![]),
            ],
        );

        let ctx = empty_ctx();

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.find("fill=\"blue\"").unwrap() < html.find("fill=\"red\"").unwrap());
        assert!(!html.contains("z-index"));
        assert!(!html.contains("z_index"));
    }

    #[test]
    fn user_defined_shape_template_is_graph_node() {
        let template = std::sync::Arc::new(|_args: &[Value]| {
            let mut bg = IndexMap::new();
            bg.insert("kind".to_string(), Value::String("rect".to_string()));
            bg.insert("x".to_string(), Value::Int(0));
            bg.insert("y".to_string(), Value::Int(0));
            bg.insert("width".to_string(), Value::Int(100));
            bg.insert("height".to_string(), Value::Int(40));
            bg.insert("fill".to_string(), Value::String("#bada55".to_string()));
            Ok(Value::List(vec![Value::Map(bg)]))
        }) as BuiltinFn;
        let ctx = custom_shape_ctx("my::task", "my::task_template", template);

        let mut a_attrs = IndexMap::new();
        int_attr(&mut a_attrs, "width", 100);
        int_attr(&mut a_attrs, "height", 40);
        let mut b_attrs = IndexMap::new();
        int_attr(&mut b_attrs, "width", 100);
        int_attr(&mut b_attrs, "height", 40);
        let mut conn_attrs = IndexMap::new();
        string_attr(&mut conn_attrs, "from", "a");
        string_attr(&mut conn_attrs, "to", "b");
        string_attr(&mut conn_attrs, "direction", "to");
        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 240);
        int_attr(&mut diagram_attrs, "height", 160);
        string_attr(&mut diagram_attrs, "align", "layered");

        let diagram = block(
            "wdoc::draw::diagram",
            Some("custom_flow"),
            diagram_attrs,
            vec![
                block("my::task", Some("a"), a_attrs, vec![]),
                block("my::task", Some("b"), b_attrs, vec![]),
                block("wdoc::draw::connection", Some("ab"), conn_attrs, vec![]),
            ],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert_eq!(html.matches("fill=\"#bada55\"").count(), 2);
        assert!(html.contains("marker-end=\"url(#wdoc-arrow)\""));
    }

    #[test]
    fn nested_connection_blocks_are_scoped_to_parent_shape() {
        let mut a_attrs = IndexMap::new();
        int_attr(&mut a_attrs, "width", 80);
        int_attr(&mut a_attrs, "height", 30);
        let mut b_attrs = IndexMap::new();
        int_attr(&mut b_attrs, "width", 80);
        int_attr(&mut b_attrs, "height", 30);
        let mut conn_attrs = IndexMap::new();
        string_attr(&mut conn_attrs, "from", "a");
        string_attr(&mut conn_attrs, "to", "b");
        string_attr(&mut conn_attrs, "direction", "to");
        let mut group_attrs = IndexMap::new();
        int_attr(&mut group_attrs, "width", 180);
        int_attr(&mut group_attrs, "height", 140);
        string_attr(&mut group_attrs, "align", "layered");

        let group = block(
            "wdoc::draw::group",
            Some("phase"),
            group_attrs,
            vec![
                block("wdoc::draw::rect", Some("a"), a_attrs, vec![]),
                block("wdoc::draw::rect", Some("b"), b_attrs, vec![]),
                block("wdoc::draw::connection", Some("ab"), conn_attrs, vec![]),
            ],
        );

        let ctx = empty_ctx();
        let mut shapes = Vec::new();
        let mut connections = Vec::new();
        collect_shape_or_connection(&group, &mut shapes, &mut connections, &ctx, 0);

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].from_id, "phase.a");
        assert_eq!(connections[0].to_id, "phase.b");
    }

    #[test]
    fn template_connection_descriptors_are_scoped_to_instance() {
        let template = std::sync::Arc::new(|_args: &[Value]| {
            let mut a = IndexMap::new();
            a.insert("kind".to_string(), Value::String("rect".to_string()));
            a.insert("id".to_string(), Value::String("start".to_string()));
            a.insert("width".to_string(), Value::Int(80));
            a.insert("height".to_string(), Value::Int(30));
            a.insert("layout_role".to_string(), Value::String("node".to_string()));

            let mut b = IndexMap::new();
            b.insert("kind".to_string(), Value::String("rect".to_string()));
            b.insert("id".to_string(), Value::String("end".to_string()));
            b.insert("width".to_string(), Value::Int(80));
            b.insert("height".to_string(), Value::Int(30));
            b.insert("layout_role".to_string(), Value::String("node".to_string()));

            let mut edge = IndexMap::new();
            edge.insert("kind".to_string(), Value::String("connection".to_string()));
            edge.insert("from".to_string(), Value::String("start".to_string()));
            edge.insert("to".to_string(), Value::String("end".to_string()));
            edge.insert("direction".to_string(), Value::String("to".to_string()));

            Ok(Value::List(vec![
                Value::Map(a),
                Value::Map(b),
                Value::Map(edge),
            ]))
        }) as BuiltinFn;
        let ctx = custom_shape_ctx("my::flow_box", "my::flow_box_template", template);

        let mut flow_attrs = IndexMap::new();
        int_attr(&mut flow_attrs, "width", 180);
        int_attr(&mut flow_attrs, "height", 120);
        string_attr(&mut flow_attrs, "align", "layered");
        let flow = block("my::flow_box", Some("flow"), flow_attrs, vec![]);

        let mut shapes = Vec::new();
        let mut connections = Vec::new();
        collect_shape_or_connection(&flow, &mut shapes, &mut connections, &ctx, 0);

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].from_id, "flow.start");
        assert_eq!(connections[0].to_id, "flow.end");
        assert_eq!(shapes[0].children.len(), 2);
        assert!(!shapes[0].children[0]
            .attrs
            .contains_key("_wdoc_layout_decoration"));

        let mut diagram = wcl_wdoc::shapes::Diagram {
            id: None,
            width: 220.0,
            height: 160.0,
            padding: 0.0,
            align: wcl_wdoc::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes,
            connections,
            classes: IndexMap::new(),
        };
        wcl_wdoc::shapes::render_diagram_svg(&mut diagram);
        let flow_children = &diagram.shapes[0].children;
        assert!(flow_children[0].resolved.y < flow_children[1].resolved.y);
    }

    #[test]
    fn diagram_classes_and_events_flow_through_cli_extraction() {
        let mut class_attrs = IndexMap::new();
        string_attr(&mut class_attrs, "fill", "#ffffff");
        string_attr(&mut class_attrs, "stroke", "#94a3b8");
        int_attr(&mut class_attrs, "z_index", 5);

        let mut state_attrs = IndexMap::new();
        string_attr(&mut state_attrs, "fill", "#eef6ff");
        string_attr(&mut state_attrs, "stroke", "#3b82f6");
        int_attr(&mut state_attrs, "z_index", 20);

        let card_class = block(
            "class",
            Some("card"),
            class_attrs,
            vec![block("state", Some("hovered"), state_attrs, vec![])],
        );

        let mut event_attrs = IndexMap::new();
        string_attr(&mut event_attrs, "trigger", "hover");
        string_attr(&mut event_attrs, "state", "hovered");

        let mut rect_attrs = IndexMap::new();
        int_attr(&mut rect_attrs, "x", 0);
        int_attr(&mut rect_attrs, "y", 0);
        int_attr(&mut rect_attrs, "width", 40);
        int_attr(&mut rect_attrs, "height", 40);
        string_attr(&mut rect_attrs, "class", "card");

        let rect = block(
            "wdoc::draw::rect",
            Some("task"),
            rect_attrs,
            vec![block("event", Some("hover_card"), event_attrs, vec![])],
        );

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 80);
        int_attr(&mut diagram_attrs, "height", 80);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("class_preview"),
            diagram_attrs,
            vec![rect],
        );

        let ctx = empty_ctx();
        ctx.diagram_classes
            .borrow_mut()
            .extend(collect_diagram_classes(&IndexMap::from([(
                "card".to_string(),
                Value::BlockRef(card_class),
            )])));

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("fill=\"#ffffff\""));
        assert!(html.contains("stroke=\"#94a3b8\""));
        assert!(html.contains(".card.wdoc-state-hovered"));
        assert!(html.contains("data-wdoc-events=\"hover|hovered|self||left|0|false\""));
        assert!(html.contains("data-wdoc-state-z=\"hovered:20\""));
        assert!(!html.contains("<event"));
    }

    #[test]
    fn diagram_class_animations_flow_through_cli_extraction() {
        let mut anim_attrs = IndexMap::new();
        int_attr(&mut anim_attrs, "duration_ms", 750);
        string_attr(&mut anim_attrs, "iteration_count", "infinite");
        string_attr(&mut anim_attrs, "direction", "alternate");
        string_attr(&mut anim_attrs, "fill_mode", "both");

        let mut from_attrs = IndexMap::new();
        int_attr(&mut from_attrs, "x", 10);
        int_attr(&mut from_attrs, "y", 10);
        int_attr(&mut from_attrs, "width", 40);
        int_attr(&mut from_attrs, "height", 30);

        let mut to_attrs = IndexMap::new();
        int_attr(&mut to_attrs, "x", 90);
        int_attr(&mut to_attrs, "y", 30);
        int_attr(&mut to_attrs, "width", 50);
        int_attr(&mut to_attrs, "height", 35);

        let animation = block(
            "animation",
            Some("slide"),
            anim_attrs,
            vec![
                block("keyframe", Some("0"), from_attrs, vec![]),
                block("keyframe", Some("100"), to_attrs, vec![]),
            ],
        );

        let mut state_attrs = IndexMap::new();
        string_attr(&mut state_attrs, "animation", "slide");
        let card_class = block(
            "class",
            Some("card"),
            IndexMap::new(),
            vec![
                animation,
                block("state", Some("active"), state_attrs, vec![]),
            ],
        );

        let mut event_attrs = IndexMap::new();
        string_attr(&mut event_attrs, "trigger", "click");
        string_attr(&mut event_attrs, "state", "active");
        string_attr(&mut event_attrs, "mode", "toggle");

        let mut rect_attrs = IndexMap::new();
        int_attr(&mut rect_attrs, "x", 10);
        int_attr(&mut rect_attrs, "y", 10);
        int_attr(&mut rect_attrs, "width", 40);
        int_attr(&mut rect_attrs, "height", 30);
        string_attr(&mut rect_attrs, "class", "card");
        let rect = block(
            "wdoc::draw::rect",
            Some("task"),
            rect_attrs,
            vec![block("event", Some("start"), event_attrs, vec![])],
        );

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 160);
        int_attr(&mut diagram_attrs, "height", 80);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("animation_preview"),
            diagram_attrs,
            vec![rect],
        );

        let ctx = empty_ctx();
        ctx.diagram_classes
            .borrow_mut()
            .extend(collect_diagram_classes(&IndexMap::from([(
                "card".to_string(),
                Value::BlockRef(card_class),
            )])));

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("data-wdoc-state-animation=\"active:slide\""));
        assert!(html.contains("slide|750|0|ease|infinite|alternate|both|"));
        assert!(html.contains("data-wdoc-events=\"click|active|self|toggle|left|0|false\""));
        assert!(!html.contains("<animation"));
        assert!(!html.contains("<keyframe"));
    }

    #[test]
    fn descriptor_z_index_is_structural_and_sorts_composite_children() {
        let mut high = IndexMap::new();
        high.insert("kind".to_string(), Value::String("rect".to_string()));
        high.insert("width".to_string(), Value::Int(40));
        high.insert("height".to_string(), Value::Int(40));
        high.insert("z_index".to_string(), Value::Int(2));
        high.insert("fill".to_string(), Value::String("red".to_string()));

        let mut low = IndexMap::new();
        low.insert("kind".to_string(), Value::String("rect".to_string()));
        low.insert("width".to_string(), Value::Int(40));
        low.insert("height".to_string(), Value::Int(40));
        low.insert("z_index".to_string(), Value::Int(-1));
        low.insert("fill".to_string(), Value::String("blue".to_string()));

        let mut group = IndexMap::new();
        group.insert("kind".to_string(), Value::String("group".to_string()));
        group.insert("width".to_string(), Value::Int(40));
        group.insert("height".to_string(), Value::Int(40));
        group.insert(
            "children".to_string(),
            Value::List(vec![Value::Map(high), Value::Map(low)]),
        );

        let mut diagram = wcl_wdoc::shapes::Diagram {
            id: None,
            width: 80.0,
            height: 80.0,
            padding: 0.0,
            align: wcl_wdoc::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes: vec![
                descriptor_to_shape_node_with_order(&Value::Map(group), 0).expect("descriptor")
            ],
            connections: vec![],
            classes: IndexMap::new(),
        };

        let svg = wcl_wdoc::shapes::render_diagram_svg(&mut diagram);
        assert!(svg.find("fill=\"blue\"").unwrap() < svg.find("fill=\"red\"").unwrap());
        assert!(!svg.contains("z_index"));
    }

    #[test]
    fn descriptor_inline_svg_renders_sanitized_content() {
        let mut descriptor = IndexMap::new();
        descriptor.insert("kind".to_string(), Value::String("inline_svg".to_string()));
        descriptor.insert("x".to_string(), Value::Int(0));
        descriptor.insert("y".to_string(), Value::Int(0));
        descriptor.insert("width".to_string(), Value::Int(24));
        descriptor.insert("height".to_string(), Value::Int(24));
        descriptor.insert(
            "class".to_string(),
            Value::String("generated-icon".to_string()),
        );
        descriptor.insert(
            "content".to_string(),
            Value::String(
                r#"<svg viewBox="0 0 24 24"><path d="M0 0L24 24" onclick="bad()"/></svg>"#
                    .to_string(),
            ),
        );

        let mut diagram = wcl_wdoc::shapes::Diagram {
            id: None,
            width: 24.0,
            height: 24.0,
            padding: 0.0,
            align: wcl_wdoc::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes: vec![
                descriptor_to_shape_node_with_order(&Value::Map(descriptor), 0)
                    .expect("descriptor"),
            ],
            connections: vec![],
            classes: IndexMap::new(),
        };

        let svg = wcl_wdoc::shapes::render_diagram_svg(&mut diagram);
        assert!(svg.contains("class=\"generated-icon\""));
        assert!(svg.contains("<path d=\"M0 0L24 24\""));
        assert!(!svg.contains("onclick"));
    }

    #[test]
    fn diagram_image_flows_through_cli_extraction() {
        let mut image_attrs = IndexMap::new();
        int_attr(&mut image_attrs, "x", 20);
        int_attr(&mut image_attrs, "y", 10);
        int_attr(&mut image_attrs, "width", 160);
        int_attr(&mut image_attrs, "height", 90);
        string_attr(&mut image_attrs, "src", "images/hero.png");
        string_attr(&mut image_attrs, "fit", "cover");
        string_attr(&mut image_attrs, "alt", "Hero image");

        let image = block("wdoc::draw::image", Some("hero"), image_attrs, vec![]);

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 200);
        int_attr(&mut diagram_attrs, "height", 120);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("image_preview"),
            diagram_attrs,
            vec![image],
        );

        let ctx = empty_ctx();

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("<image href=\"images/hero.png\""));
        assert!(html.contains("preserveAspectRatio=\"xMidYMid slice\""));
        assert!(html.contains("role=\"img\" aria-label=\"Hero image\""));
    }

    #[test]
    fn diagram_inline_svg_flows_through_cli_extraction() {
        let mut inline_attrs = IndexMap::new();
        int_attr(&mut inline_attrs, "x", 5);
        int_attr(&mut inline_attrs, "y", 6);
        int_attr(&mut inline_attrs, "width", 24);
        int_attr(&mut inline_attrs, "height", 24);
        string_attr(&mut inline_attrs, "class", "inline-mark");
        string_attr(
            &mut inline_attrs,
            "content",
            r#"<svg viewBox="0 0 24 24"><path d="M1 1L2 2"/></svg>"#,
        );

        let inline_svg = block("wdoc::draw::inline_svg", Some("mark"), inline_attrs, vec![]);

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 50);
        int_attr(&mut diagram_attrs, "height", 50);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("inline_svg_preview"),
            diagram_attrs,
            vec![inline_svg],
        );

        let ctx = empty_ctx();

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("class=\"inline-mark\""));
        assert!(html.contains("<path d=\"M1 1L2 2\""));
        assert!(html.contains("transform=\"translate(5,6) scale(1,1) translate(-0,-0)\""));
    }
}

/// Iterate all child BlockRefs from a parent — checks both `children` and `attributes`
/// (WCL stores named child blocks as attributes, anonymous/duplicate as children).
fn all_child_blocks(block: &BlockRef) -> Vec<&BlockRef> {
    let mut result: Vec<&BlockRef> = Vec::new();
    for val in block.attributes.values() {
        if let Value::BlockRef(child) = val {
            result.push(child);
        }
    }
    for child in &block.children {
        result.push(child);
    }
    result
}

fn extract(values: &IndexMap<String, Value>, ctx: &ExtractCtx) -> Result<WdocDocument, String> {
    let mut wdoc_block = None;
    let mut pages = Vec::new();
    let mut styles = Vec::new();

    register_css_assets(values, ctx)?;

    for value in values.values() {
        if let Value::BlockRef(block) = value {
            match block.kind.as_str() {
                "wdoc::doc" => wdoc_block = Some(block),
                "wdoc::page" => pages.push(extract_page(block, ctx)?),
                "wdoc::style" => styles.push(extract_style(block)),
                _ => {}
            }
        }
    }

    let wdoc = wdoc_block.ok_or("no wdoc::doc block found in document")?;

    let title = wdoc
        .attributes
        .get("title")
        .and_then(|v| v.as_string())
        .ok_or("wdoc block missing 'title' attribute")?
        .to_string();

    let name = wdoc.id.clone().unwrap_or_default();
    let version = wdoc
        .attributes
        .get("version")
        .and_then(|v| v.as_string())
        .map(|s| s.to_string());
    let author = wdoc
        .attributes
        .get("author")
        .and_then(|v| v.as_string())
        .map(|s| s.to_string());

    let mut sections = Vec::new();
    for child in all_child_blocks(wdoc) {
        match child.kind.as_str() {
            "wdoc::section" => sections.push(extract_section(child, &name)?),
            "wdoc::page" => pages.push(extract_page(child, ctx)?),
            "wdoc::style" => styles.push(extract_style(child)),
            _ => {}
        }
    }

    Ok(WdocDocument {
        name,
        title,
        version,
        author,
        sections,
        pages,
        styles,
        extra_css: ctx.css_registry.borrow().render_css(),
    })
}

fn extract_section(block: &BlockRef, parent_path: &str) -> Result<Section, String> {
    let short_id = block.id.clone().unwrap_or_default();
    let id = if parent_path.is_empty() {
        short_id.clone()
    } else {
        format!("{parent_path}.{short_id}")
    };

    // _args[0] is the display title (inline arg after the block ID)
    let title = block
        .attributes
        .get("_args")
        .and_then(|v| match v {
            Value::List(list) => list
                .first()
                .and_then(|v| v.as_string())
                .map(|s| s.to_string()),
            _ => None,
        })
        .or_else(|| {
            block
                .attributes
                .get("title")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| short_id.clone());

    let mut children = Vec::new();
    for child in all_child_blocks(block) {
        if child.kind == "wdoc::section" {
            children.push(extract_section(child, &id)?);
        }
    }

    Ok(Section {
        id,
        short_id,
        title,
        children,
    })
}

fn extract_page(block: &BlockRef, ctx: &ExtractCtx) -> Result<Page, String> {
    let id = block.id.clone().unwrap_or_default();

    let section_id = block
        .attributes
        .get("section")
        .and_then(|v| v.as_string())
        .ok_or_else(|| format!("page '{id}' missing 'section' attribute"))?
        .to_string();

    let title = block
        .attributes
        .get("title")
        .and_then(|v| v.as_string())
        .ok_or_else(|| format!("page '{id}' missing 'title' attribute"))?
        .to_string();

    let all_children = all_child_blocks(block);
    let layout = all_children
        .iter()
        .find(|c| c.kind == "wdoc::layout")
        .map(|c| extract_layout(c, ctx))
        .unwrap_or(Layout {
            children: Vec::new(),
        });

    Ok(Page {
        id,
        section_id,
        title,
        layout,
    })
}

fn extract_layout(block: &BlockRef, ctx: &ExtractCtx) -> Layout {
    Layout {
        children: extract_layout_children(block, ctx),
    }
}

fn extract_layout_children(block: &BlockRef, ctx: &ExtractCtx) -> Vec<LayoutItem> {
    let mut items = Vec::new();
    for child in all_child_blocks(block) {
        match child.kind.as_str() {
            "vsplit" => items.push(LayoutItem::SplitGroup(extract_split_group(
                child,
                SplitDirection::Vertical,
                ctx,
            ))),
            "hsplit" => items.push(LayoutItem::SplitGroup(extract_split_group(
                child,
                SplitDirection::Horizontal,
                ctx,
            ))),
            // Known structural blocks are not content
            "wdoc::layout" | "wdoc::section" | "wdoc::page" | "wdoc::doc" | "wdoc::style"
            | "split" => {}
            // Callout — container with header + nested content blocks
            "wdoc::callout" => {
                let html = render_callout_html(child, ctx);
                items.push(LayoutItem::Content(ContentBlock {
                    kind: "wdoc::callout".to_string(),
                    id: child.id.clone(),
                    rendered_html: html,
                    style: get_style_decorator(child),
                }));
            }
            // Diagram — needs ctx so shape templates can be dispatched.
            // Cannot go through the html template path because templates can't
            // see ctx.
            "wdoc::draw::diagram" => {
                let html = render_diagram_with_ctx(child, ctx);
                items.push(LayoutItem::Content(ContentBlock {
                    kind: "wdoc::draw::diagram".to_string(),
                    id: child.id.clone(),
                    rendered_html: html,
                    style: get_style_decorator(child),
                }));
            }
            // Everything else is a content block — try to render via template
            kind => {
                let rendered = ctx.render_block(child);
                match rendered {
                    Ok(html) => items.push(LayoutItem::Content(ContentBlock {
                        kind: kind.to_string(),
                        id: child.id.clone(),
                        rendered_html: html,
                        style: get_style_decorator(child),
                    })),
                    Err(e) => {
                        eprintln!("wdoc: warning: skipping '{kind}' block: {e}");
                    }
                }
            }
        }
    }
    items
}

fn extract_split_group(
    block: &BlockRef,
    direction: SplitDirection,
    ctx: &ExtractCtx,
) -> SplitGroup {
    let mut splits = Vec::new();
    for child in all_child_blocks(block) {
        if child.kind == "split" {
            splits.push(extract_split(child, ctx));
        }
    }
    SplitGroup { direction, splits }
}

fn extract_split(block: &BlockRef, ctx: &ExtractCtx) -> Split {
    let size_percent = block
        .attributes
        .get("size")
        .and_then(|v| match v {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        })
        .unwrap_or(0.0);

    Split {
        size_percent,
        children: extract_layout_children(block, ctx),
    }
}

fn get_style_decorator(block: &BlockRef) -> Option<String> {
    block
        .decorators
        .iter()
        .find(|d| d.name == "style")
        .and_then(|d| {
            d.args
                .get("_0")
                .or_else(|| d.args.values().next())
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
        })
}

fn extract_style(block: &BlockRef) -> WdocStyle {
    let name = block.id.clone().unwrap_or_else(|| "default".to_string());
    let mut rules = Vec::new();

    for child in all_child_blocks(block) {
        let mut properties = IndexMap::new();
        for (key, val) in &child.attributes {
            if let Some(s) = val.as_string() {
                properties.insert(key.clone(), s.to_string());
            }
        }
        rules.push(StyleRule {
            target: child.kind.clone(),
            properties,
        });
    }

    WdocStyle { name, rules }
}

// ---------------------------------------------------------------------------
// CLI entry points
// ---------------------------------------------------------------------------

struct ExtractedWdoc {
    document: WdocDocument,
    watch_paths: HashSet<PathBuf>,
}

fn setup_lib_dir() -> Result<PathBuf, String> {
    let lib_dir = std::env::temp_dir().join(format!("wdoc-lib-{}", std::process::id()));
    std::fs::create_dir_all(&lib_dir).map_err(|e| format!("failed to create wdoc lib dir: {e}"))?;
    std::fs::write(
        lib_dir.join("wdoc.wcl"),
        wcl_wdoc::library::WDOC_LIBRARY_WCL,
    )
    .map_err(|e| format!("failed to write wdoc.wcl: {e}"))?;
    Ok(lib_dir)
}

fn parse_and_extract(
    files: &[PathBuf],
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<WdocDocument, String> {
    parse_and_extract_with_watch(files, vars, lib_args).map(|extracted| extracted.document)
}

fn parse_and_extract_with_watch(
    files: &[PathBuf],
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<ExtractedWdoc, String> {
    let variables = parse_var_args(vars)?;
    let functions = wdoc_functions();
    let lib_dir = setup_lib_dir()?;

    let mut all_values = IndexMap::new();
    let mut last_doc: Option<crate::Document> = None;
    let mut watch_paths = HashSet::new();

    for file in files {
        let source = std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

        let mut options = crate::ParseOptions {
            root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
            variables: variables.clone(),
            functions: functions.clone(),
            ..Default::default()
        };
        lib_args.apply(&mut options);
        options.lib_paths.push(lib_dir.clone());

        let doc = crate::parse(&source, options);

        let errors: Vec<_> = doc.diagnostics.iter().filter(|d| d.is_error()).collect();
        if !errors.is_empty() {
            let mut msg = String::new();
            for diag in &errors {
                msg.push_str(&super::format_diagnostic(diag, &doc.source_map, file));
                msg.push('\n');
            }
            return Err(msg);
        }

        watch_paths.extend(
            doc.imported_paths
                .iter()
                .filter(|path| !path.starts_with(&lib_dir))
                .cloned(),
        );
        all_values.extend(doc.values.clone());
        last_doc = Some(doc);
    }

    let doc = last_doc.ok_or("no input files")?;

    // Build template dispatch context
    let template_map = collect_template_map(&doc);
    let builtins: HashMap<String, BuiltinFn> = functions.functions;
    let template_fns = collect_template_fns(&doc, &builtins);
    let svg_search_dirs = wdoc_source_dirs(files, &doc.imported_paths, &lib_dir);
    let ctx = ExtractCtx {
        template_map,
        template_fns,
        builtins,
        css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
        diagram_classes: Rc::new(RefCell::new(collect_diagram_classes(&all_values))),
        svg_search_dirs,
    };

    let wdoc_doc = extract(&all_values, &ctx)?;
    let warnings = wcl_wdoc::validate_doc(&wdoc_doc)?;
    for w in &warnings {
        eprintln!("{w}");
    }

    // Clean up temp lib dir
    let _ = std::fs::remove_dir_all(&lib_dir);

    Ok(ExtractedWdoc {
        document: wdoc_doc,
        watch_paths,
    })
}

pub fn run_build(
    files: &[PathBuf],
    output: &Path,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let extracted = parse_and_extract_with_watch(files, vars, lib_args)?;
    let doc = extracted.document;
    let asset_dirs: Vec<&Path> = files
        .iter()
        .filter_map(|f| f.parent())
        .chain(extracted.watch_paths.iter().filter_map(|f| f.parent()))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    wcl_wdoc::render_to(&doc, output, &asset_dirs)?;
    println!(
        "wdoc: built {} page(s) to {}",
        doc.pages.len(),
        output.display()
    );
    Ok(())
}

/// Install the embedded wdoc standard library (`wdoc.wcl`) into the user's
/// library directory so editors, LSP, and `wcl validate` can resolve
/// `import <wdoc.wcl>` without the `wdoc` subcommand's temp-dir bootstrap.
pub fn run_install_library(force: bool) -> Result<(), String> {
    let lib_dir = wcl_lang::library::user_library_dir();
    std::fs::create_dir_all(&lib_dir)
        .map_err(|e| format!("failed to create library dir {}: {e}", lib_dir.display()))?;
    let target = lib_dir.join("wdoc.wcl");
    if target.exists() && !force {
        return Err(format!(
            "{} already exists (use --force to overwrite)",
            target.display()
        ));
    }
    std::fs::write(&target, wcl_wdoc::library::WDOC_LIBRARY_WCL)
        .map_err(|e| format!("failed to write {}: {e}", target.display()))?;
    println!("installed wdoc library to {}", target.display());
    Ok(())
}

fn wdoc_source_dirs(
    files: &[PathBuf],
    imported_paths: &HashSet<PathBuf>,
    lib_dir: &Path,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();
    for dir in files.iter().filter_map(|file| file.parent()) {
        let dir = dir.to_path_buf();
        if seen.insert(dir.clone()) {
            dirs.push(dir);
        }
    }
    let mut imported_dirs: Vec<PathBuf> = imported_paths
        .iter()
        .filter(|path| !path.starts_with(lib_dir))
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect();
    imported_dirs.sort();
    imported_dirs.dedup();
    for dir in imported_dirs {
        if seen.insert(dir.clone()) {
            dirs.push(dir);
        }
    }
    dirs
}

pub fn run_validate(
    files: &[PathBuf],
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let doc = parse_and_extract(files, vars, lib_args)?;
    println!(
        "wdoc: valid ({} section(s), {} page(s))",
        count_sections(&doc.sections),
        doc.pages.len()
    );
    Ok(())
}

pub fn run_serve(
    files: &[PathBuf],
    port: u16,
    open: bool,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let files = files.to_vec();
    let vars = vars.to_vec();
    let lib_args = lib_args.clone();

    let output_dir = std::env::temp_dir().join(format!("wdoc-serve-{}", std::process::id()));

    // Watch the specific input files, not entire directories
    let watch_paths: Vec<PathBuf> = files.clone();

    // Asset directories = parent dirs of input files
    let asset_dirs: Vec<PathBuf> = files
        .iter()
        .filter_map(|f| f.parent().map(|p| p.to_path_buf()))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let build_fn = move || {
        let extracted = parse_and_extract_with_watch(&files, &vars, &lib_args)?;
        Ok(wcl_wdoc::serve::ServeBuild {
            document: extracted.document,
            watch_paths: extracted.watch_paths.into_iter().collect(),
        })
    };

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

    rt.block_on(wcl_wdoc::serve::serve(
        build_fn,
        watch_paths,
        asset_dirs,
        output_dir,
        port,
        open,
    ))
}

fn count_sections(sections: &[Section]) -> usize {
    sections
        .iter()
        .map(|s| 1 + count_sections(&s.children))
        .sum()
}
