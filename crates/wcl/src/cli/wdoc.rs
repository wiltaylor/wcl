use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

    reg.register(
        "wdoc::render_diagram",
        std::sync::Arc::new(|args: &[Value]| Ok(Value::String(render_diagram_html(args))))
            as BuiltinFn,
        mk("wdoc::render_diagram", "Render a diagram as inline SVG"),
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

/// Render a `wdoc_diagram` block to inline SVG.
/// Converts WCL Value tree → ShapeNode tree, then calls shapes::render_diagram_svg.
fn render_diagram_html(args: &[Value]) -> String {
    use wcl_wdoc::shapes::*;

    let br = match args.first() {
        Some(Value::BlockRef(br)) => br,
        _ => return "<div class=\"wdoc-diagram\">(invalid diagram)</div>".to_string(),
    };

    let str_attrs = value_map_to_string_map_lossy(&br.attributes);

    let diagram_w = val_f64(br.attributes.get("width")).unwrap_or(600.0);
    let diagram_h = val_f64(br.attributes.get("height")).unwrap_or(400.0);
    let padding = val_f64(br.attributes.get("padding")).unwrap_or(0.0);
    let gap = val_f64(br.attributes.get("gap")).unwrap_or(40.0);
    let align = parse_alignment_str(str_attrs.get("align").map(|s| s.as_str()).unwrap_or("none"));

    let mut shapes = Vec::new();
    let mut connections = Vec::new();

    // Walk child blocks from both attributes and children
    for val in br.attributes.values() {
        if let Value::BlockRef(child) = val {
            collect_shape_or_connection(child, &mut shapes, &mut connections);
        }
    }
    for child in &br.children {
        collect_shape_or_connection(child, &mut shapes, &mut connections);
    }

    let mut diagram = Diagram {
        width: diagram_w,
        height: diagram_h,
        shapes,
        connections,
        padding,
        align,
        gap,
        options: str_attrs,
    };

    render_diagram_svg(&mut diagram)
}

fn collect_shape_or_connection(
    br: &BlockRef,
    shapes: &mut Vec<wcl_wdoc::shapes::ShapeNode>,
    connections: &mut Vec<wcl_wdoc::shapes::Connection>,
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
            attrs: a,
        });
        return;
    }

    if let Some(kind) = parse_shape_kind(&br.kind) {
        let mut a = value_map_to_string_map_lossy(&br.attributes);

        // For widgets, get template-generated children (shape primitives)
        let mut children = if wcl_wdoc::widgets::is_widget(&br.kind) {
            let w = a
                .get("width")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(200.0);
            let h = a
                .get("height")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(100.0);
            wcl_wdoc::widgets::build_widget(&br.kind, w, h, &a)
        } else {
            Vec::new()
        };

        // Also collect any user-defined child shapes from the block
        let mut child_connections = Vec::new();
        for val in br.attributes.values() {
            if let Value::BlockRef(child_br) = val {
                collect_shape_or_connection(child_br, &mut children, &mut child_connections);
            }
        }
        for child_br in &br.children {
            collect_shape_or_connection(child_br, &mut children, &mut child_connections);
        }

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

        // Widgets are invisible containers — their template provides all visuals
        if wcl_wdoc::widgets::is_widget(&br.kind) {
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
            children,
            align,
            gap,
            padding: pad,
        });
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
    })
}

fn extract_section(block: &BlockRef, parent_path: &str) -> Result<Section, String> {
    let short_id = block.id.clone().unwrap_or_default();
    let id = if parent_path.is_empty() {
        short_id.clone()
    } else {
        format!("{parent_path}.{short_id}")
    };

    // _args[0] is the block ID, _args[1] is the display title (inline arg)
    let title = block
        .attributes
        .get("_args")
        .and_then(|v| match v {
            Value::List(list) => list
                .get(1)
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
    let variables = parse_var_args(vars)?;
    let functions = wdoc_functions();
    let lib_dir = setup_lib_dir()?;

    let mut all_values = IndexMap::new();
    let mut last_doc: Option<crate::Document> = None;

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

        all_values.extend(doc.values.clone());
        last_doc = Some(doc);
    }

    let doc = last_doc.ok_or("no input files")?;

    // Build template dispatch context
    let template_map = collect_template_map(&doc);
    let builtins: HashMap<String, BuiltinFn> = functions.functions;
    let template_fns = collect_template_fns(&doc, &builtins);
    let ctx = ExtractCtx {
        template_map,
        template_fns,
        builtins,
    };

    let wdoc_doc = extract(&all_values, &ctx)?;
    let warnings = wcl_wdoc::validate_doc(&wdoc_doc)?;
    for w in &warnings {
        eprintln!("{w}");
    }

    // Clean up temp lib dir
    let _ = std::fs::remove_dir_all(&lib_dir);

    Ok(wdoc_doc)
}

pub fn run_build(
    files: &[PathBuf],
    output: &Path,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let doc = parse_and_extract(files, vars, lib_args)?;
    let asset_dirs: Vec<&Path> = files
        .iter()
        .filter_map(|f| f.parent())
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

    let build_fn = move || parse_and_extract(&files, &vars, &lib_args);

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
