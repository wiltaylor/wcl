use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::model::*;
use wcl_lang::ast;
use wcl_lang::{BlockRef, BuiltinFn, FunctionRegistry, FunctionSignature, FunctionValue, Value};

/// Options for parsing WCL source files into a WDoc document.
#[derive(Clone, Debug, Default)]
pub struct SourceOptions {
    /// External variables injected before evaluation.
    pub variables: IndexMap<String, Value>,
    /// Extra WCL library search paths.
    pub lib_paths: Vec<PathBuf>,
    /// Disable default XDG/system WCL library search paths.
    pub no_default_lib_paths: bool,
}

/// Result of parsing and extracting WDoc source files.
pub struct ExtractedWdoc {
    pub document: WdocDocument,
    pub watch_paths: HashSet<PathBuf>,
}

/// Result of rendering a WDoc build.
pub struct BuildResult {
    pub pages: usize,
    pub output: PathBuf,
}

/// Result of validating WDoc source.
pub struct ValidationResult {
    pub sections: usize,
    pub pages: usize,
}

// ---------------------------------------------------------------------------
// Template function dispatch
// ---------------------------------------------------------------------------

/// Map from (format, schema_name) → function_name, built from AST @template decorators.
fn collect_template_map(doc: &wcl_lang::Document) -> HashMap<(String, String), String> {
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

fn collect_template_helpers(doc: &wcl_lang::Document) -> HashMap<String, FunctionValue> {
    doc.values
        .iter()
        .filter_map(|(name, value)| match value {
            Value::Function(func) => Some((name.clone(), func.clone())),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Debug)]
struct MarkupRule {
    name: String,
    func: FunctionValue,
    parts: Vec<MarkupPatternPart>,
    priority: i64,
    order: usize,
}

#[derive(Clone, Debug)]
enum MarkupPatternPart {
    Literal(String),
    Capture(String),
}

fn collect_markup_rules(
    doc: &wcl_lang::Document,
    helpers: &HashMap<String, FunctionValue>,
) -> Result<Vec<MarkupRule>, String> {
    let mut rules = Vec::new();
    for (order, (name, value)) in doc.values.iter().enumerate() {
        let Value::Function(_) = value else {
            continue;
        };
        let Some(func) = helpers.get(name) else {
            continue;
        };
        let Some(dec) = func.decorators.iter().find(|d| d.name == "markup") else {
            continue;
        };
        let pattern = dec
            .args
            .get("pattern")
            .or_else(|| dec.args.get("_0"))
            .and_then(Value::as_string)
            .ok_or_else(|| format!("@markup on '{name}' requires a string pattern"))?;
        let priority = dec.args.get("priority").and_then(value_as_i64).unwrap_or(0);
        let parts = compile_markup_pattern(pattern)
            .map_err(|err| format!("invalid @markup pattern on '{name}': {err}"))?;
        validate_markup_pattern(name, func, &parts)?;
        rules.push(MarkupRule {
            name: name.clone(),
            func: func.clone(),
            parts,
            priority,
            order,
        });
    }
    rules.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| b.literal_prefix_len().cmp(&a.literal_prefix_len()))
            .then_with(|| a.order.cmp(&b.order))
    });
    Ok(rules)
}

impl MarkupRule {
    fn literal_prefix_len(&self) -> usize {
        match self.parts.first() {
            Some(MarkupPatternPart::Literal(lit)) => lit.len(),
            _ => 0,
        }
    }
}

fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

fn compile_markup_pattern(pattern: &str) -> Result<Vec<MarkupPatternPart>, String> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '{' {
            literal.push(ch);
            continue;
        }
        if !literal.is_empty() {
            parts.push(MarkupPatternPart::Literal(std::mem::take(&mut literal)));
        }
        let mut name = String::new();
        let mut closed = false;
        for inner in chars.by_ref() {
            if inner == '}' {
                closed = true;
                break;
            }
            name.push(inner);
        }
        if !closed {
            return Err("unterminated capture".into());
        }
        if name.is_empty() {
            return Err("empty capture name".into());
        }
        if !name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
            || name.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            return Err(format!("invalid capture name '{name}'"));
        }
        parts.push(MarkupPatternPart::Capture(name));
    }
    if !literal.is_empty() {
        parts.push(MarkupPatternPart::Literal(literal));
    }
    if !parts
        .iter()
        .any(|part| matches!(part, MarkupPatternPart::Capture(_)))
    {
        return Err("pattern must include at least one capture".into());
    }
    if parts.windows(2).any(|window| {
        matches!(
            window,
            [MarkupPatternPart::Capture(_), MarkupPatternPart::Capture(_)]
        )
    }) {
        return Err("adjacent captures need a literal delimiter".into());
    }
    Ok(parts)
}

fn validate_markup_pattern(
    name: &str,
    func: &FunctionValue,
    parts: &[MarkupPatternPart],
) -> Result<(), String> {
    let captures: HashSet<&str> = parts
        .iter()
        .filter_map(|part| match part {
            MarkupPatternPart::Capture(capture) => Some(capture.as_str()),
            _ => None,
        })
        .collect();
    for capture in &captures {
        if !func.params.iter().any(|param| param == capture) {
            return Err(format!(
                "@markup on '{name}' captures '{{{capture}}}', but the lambda has no matching parameter"
            ));
        }
    }
    for param in &func.params {
        if !captures.contains(param.as_str()) {
            return Err(format!(
                "@markup on '{name}' is missing capture '{{{param}}}' for lambda parameter"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// WCL custom functions (inline formatting + template rendering)
// ---------------------------------------------------------------------------

fn wdoc_functions() -> FunctionRegistry {
    let mut reg = FunctionRegistry::new();
    let measure_text = std::sync::Arc::new(|args: &[Value]| {
        if args.len() != 1 {
            return Err("measure_text() expects 1 argument (text attributes or text block)".into());
        }
        let attrs = value_map_to_string_map(args.first())?;
        let metrics = crate::shapes::measure_text_attrs(&attrs);
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

    register_renderer_helpers(&mut reg);

    reg
}

fn register_renderer_helpers(reg: &mut FunctionRegistry) {
    reg.register(
        "wdoc::slugify",
        std::sync::Arc::new(|args: &[Value]| {
            if args.len() != 1 {
                return Err("wdoc::slugify() expects 1 argument".into());
            }
            Ok(Value::String(crate::templates::slugify(&value_to_string(
                &args[0],
            ))))
        }) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::slugify".into(),
            params: vec!["value: any".into()],
            return_type: "string".into(),
            doc: "Generate a URL-safe slug from text".into(),
        },
    );

    reg.register(
        "wdoc::table_rows",
        std::sync::Arc::new(|args: &[Value]| {
            if args.len() != 1 {
                return Err("wdoc::table_rows() expects 1 argument".into());
            }
            Ok(wdoc_table_rows(&args[0]))
        }) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::table_rows".into(),
            params: vec!["block: any".into()],
            return_type: "map".into(),
            doc: "Extract headers and cell rows from a WDoc data_table block".into(),
        },
    );

    reg.register(
        "wdoc::render_children",
        std::sync::Arc::new(|args: &[Value]| {
            if args.len() != 1 {
                return Err("wdoc::render_children() expects 1 argument".into());
            }
            wdoc_render_children(&args[0]).map(Value::String)
        }) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::render_children".into(),
            params: vec!["block: any".into()],
            return_type: "string".into(),
            doc: "Render a block's child content with the current WDoc renderer context".into(),
        },
    );

    reg.register(
        "wdoc::render_markup",
        std::sync::Arc::new(|args: &[Value]| {
            if args.len() != 1 {
                return Err("wdoc::render_markup() expects 1 argument".into());
            }
            let text = args[0]
                .as_string()
                .ok_or("wdoc::render_markup() argument must be a string")?;
            wdoc_render_markup(text).map(Value::String)
        }) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::render_markup".into(),
            params: vec!["text: string".into()],
            return_type: "string".into(),
            doc: "Render WDoc text markup using @markup formatter lambdas".into(),
        },
    );

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

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::BigInt(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => format!("{other}"),
    }
}

fn wdoc_table_rows(value: &Value) -> Value {
    let attrs = match value {
        Value::BlockRef(br) => &br.attributes,
        Value::Map(map) => map,
        _ => return table_rows_result(String::new(), true, Vec::new(), Vec::new()),
    };
    let caption = attrs
        .get("caption")
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_string();
    let rows = attrs.values().find_map(|v| match v {
        Value::List(list) => Some(list),
        _ => None,
    });
    let Some(rows) = rows.filter(|rows| !rows.is_empty()) else {
        return table_rows_result(caption, true, Vec::new(), Vec::new());
    };

    let headers = match &rows[0] {
        Value::Map(row) => row.keys().cloned().collect(),
        _ => Vec::new(),
    };
    let body_rows = rows
        .iter()
        .filter_map(|row| match row {
            Value::Map(map) => Some(map.values().map(value_to_string).collect()),
            _ => None,
        })
        .collect();
    table_rows_result(caption, false, headers, body_rows)
}

fn table_rows_result(
    caption: String,
    empty: bool,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
) -> Value {
    let mut result = IndexMap::new();
    result.insert("caption".to_string(), Value::String(caption));
    result.insert("empty".to_string(), Value::Bool(empty));
    result.insert(
        "headers".to_string(),
        Value::List(headers.into_iter().map(Value::String).collect()),
    );
    result.insert(
        "rows".to_string(),
        Value::List(
            rows.into_iter()
                .map(|row| Value::List(row.into_iter().map(Value::String).collect()))
                .collect(),
        ),
    );
    Value::Map(result)
}

/// Render a `wdoc::draw::diagram` block to inline SVG. Walks the diagram's child
/// blocks, dispatching shape templates via `ctx`, and feeds the resulting
/// `ShapeNode` tree to `shapes::render_diagram_svg`.
fn render_diagram_with_ctx(br: &BlockRef, ctx: &ExtractCtx) -> String {
    use crate::shapes::*;

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
    let graph_node_connected_ports = diagram_graph_node_connected_ports(br);

    let mut source_order = 0;
    for val in br.attributes.values() {
        if let Value::BlockRef(child) = val {
            if let Some(annotated) =
                graph_node_with_connected_ports(child, &graph_node_connected_ports)
            {
                collect_shape_or_connection(
                    &annotated,
                    &mut shapes,
                    &mut connections,
                    ctx,
                    source_order,
                );
            } else {
                collect_shape_or_connection(
                    child,
                    &mut shapes,
                    &mut connections,
                    ctx,
                    source_order,
                );
            }
            source_order += 1;
        }
    }
    for child in &br.children {
        if let Some(annotated) = graph_node_with_connected_ports(child, &graph_node_connected_ports)
        {
            collect_shape_or_connection(
                &annotated,
                &mut shapes,
                &mut connections,
                ctx,
                source_order,
            );
        } else {
            collect_shape_or_connection(child, &mut shapes, &mut connections, ctx, source_order);
        }
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

fn diagram_graph_node_connected_ports(diagram: &BlockRef) -> HashMap<String, Vec<String>> {
    let mut graph_node_ids = HashSet::new();
    for val in diagram.attributes.values() {
        if let Value::BlockRef(child) = val {
            collect_graph_node_id(child, &mut graph_node_ids);
        }
    }
    for child in &diagram.children {
        collect_graph_node_id(child, &mut graph_node_ids);
    }

    let mut connected_ports: HashMap<String, Vec<String>> = HashMap::new();
    if graph_node_ids.is_empty() {
        return connected_ports;
    }

    for val in diagram.attributes.values() {
        if let Value::BlockRef(child) = val {
            collect_connection_port_usage(child, &graph_node_ids, &mut connected_ports);
        }
    }
    for child in &diagram.children {
        collect_connection_port_usage(child, &graph_node_ids, &mut connected_ports);
    }

    connected_ports
}

fn collect_graph_node_id(block: &BlockRef, ids: &mut HashSet<String>) {
    if is_draw_graph_node_block(block) {
        if let Some(id) = block.id.as_deref().filter(|id| !id.is_empty()) {
            ids.insert(id.to_string());
        }
    }
}

fn collect_connection_port_usage(
    block: &BlockRef,
    graph_node_ids: &HashSet<String>,
    connected_ports: &mut HashMap<String, Vec<String>>,
) {
    if block.kind != "wdoc::draw::connection" {
        return;
    }
    collect_endpoint_port_usage(
        block.attributes.get("from"),
        graph_node_ids,
        connected_ports,
    );
    collect_endpoint_port_usage(block.attributes.get("to"), graph_node_ids, connected_ports);
}

fn collect_endpoint_port_usage(
    endpoint_value: Option<&Value>,
    graph_node_ids: &HashSet<String>,
    connected_ports: &mut HashMap<String, Vec<String>>,
) {
    let Some(endpoint) = value_as_string(endpoint_value).map(str::trim) else {
        return;
    };
    let Some((node_id, port_id)) = endpoint.split_once('.') else {
        return;
    };
    let port_id = port_id.trim();
    if !graph_node_ids.contains(node_id) || port_id.is_empty() {
        return;
    }
    let ports = connected_ports.entry(node_id.to_string()).or_default();
    if !ports.iter().any(|existing| existing == port_id) {
        ports.push(port_id.to_string());
    }
}

fn graph_node_with_connected_ports(
    block: &BlockRef,
    connected_ports: &HashMap<String, Vec<String>>,
) -> Option<BlockRef> {
    if !is_draw_graph_node_block(block) {
        return None;
    }
    let id = block.id.as_deref()?;
    let ports = connected_ports.get(id)?;
    if ports.is_empty() {
        return None;
    }

    let mut annotated = block.clone();
    annotated.attributes.insert(
        "_wdoc_connected_ports".to_string(),
        Value::String(ports.join(",")),
    );
    Some(annotated)
}

fn collect_shape_or_connection(
    br: &BlockRef,
    shapes: &mut Vec<crate::shapes::ShapeNode>,
    connections: &mut Vec<crate::shapes::Connection>,
    ctx: &ExtractCtx,
    source_order: usize,
) {
    use crate::shapes::*;

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
        || is_draw_graph_node_structural_block(br)
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
                    || is_draw_graph_node_structural_block(child_br)
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
                || is_draw_graph_node_structural_block(child_br)
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

        // Composite shape containers are invisible — the template provides all
        // visuals. Default fill/stroke to none so the wrapping rect doesn't
        // double up on the template's drawing. Apply widget defaults before
        // parsing structural attrs so defaults like flowchart align/gap affect
        // the ShapeNode fields as well as the attrs map.
        if is_composite {
            a.entry("fill".to_string())
                .or_insert_with(|| "none".to_string());
            a.entry("stroke".to_string())
                .or_insert_with(|| "none".to_string());
            a.insert("_wdoc_composite".to_string(), "true".to_string());
            assign_default_widget_class(&mut a, br);
            apply_builtin_widget_content_insets(&mut a, br);
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
        let z_index = pf(&a, "z_index").unwrap_or(0.0);

        if kind == ShapeKind::InlineSvg {
            hydrate_inline_svg_attrs(&mut a, ctx);
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
            text_block_items: if kind == ShapeKind::TextBlock {
                text_block_items_from_block(br, ctx)
            } else {
                Vec::new()
            },
            align,
            gap,
            padding: pad,
            z_index,
            source_order,
        });
    }
}

fn text_block_items_from_block(
    br: &BlockRef,
    ctx: &ExtractCtx,
) -> Vec<crate::shapes::TextBlockItem> {
    let mut items = Vec::new();
    if let Some(content) = value_as_string(br.attributes.get("content")) {
        items.push(crate::shapes::TextBlockItem::Paragraph {
            html: render_markup_string(content, ctx).unwrap_or_else(|_| html_escape(content)),
        });
    }

    for child in all_child_blocks(br) {
        match child.kind.as_str() {
            "wdoc::paragraph" | "paragraph" | "wdoc::p" | "p" => {
                if let Some(content) = value_as_string(child.attributes.get("content")) {
                    items.push(crate::shapes::TextBlockItem::Paragraph {
                        html: render_markup_string(content, ctx)
                            .unwrap_or_else(|_| html_escape(content)),
                    });
                }
            }
            "wdoc::code" | "code" => {
                if let Some(content) = value_as_string(child.attributes.get("content")) {
                    items.push(crate::shapes::TextBlockItem::Code {
                        content: content.to_string(),
                        language: value_as_string(child.attributes.get("language"))
                            .map(str::to_string),
                    });
                }
            }
            _ => {}
        }
    }

    items
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn apply_builtin_widget_content_insets(attrs: &mut IndexMap<String, String>, br: &BlockRef) {
    let kind = br.kind.rsplit("::").next().unwrap_or(br.kind.as_str());
    match kind {
        "flowchart" => {
            attrs
                .entry("align".to_string())
                .or_insert_with(|| "layered".to_string());
            attrs
                .entry("gap".to_string())
                .or_insert_with(|| "48".to_string());
            insert_default_content_inset(attrs, "left", 16.0);
            insert_default_content_inset(attrs, "top", 36.0);
            insert_default_content_inset(attrs, "right", 16.0);
            insert_default_content_inset(attrs, "bottom", 16.0);
        }
        "card" => {
            if br
                .attributes
                .get("title")
                .and_then(|v| v.as_string())
                .is_some_and(|title| !title.is_empty())
            {
                insert_default_content_inset(attrs, "top", 36.0);
            }
        }
        "phone" => {
            insert_default_content_inset(attrs, "top", 74.0);
            insert_default_content_inset(attrs, "bottom", 50.0);
        }
        "browser" => {
            insert_default_content_inset(attrs, "top", 72.0);
        }
        _ => {}
    }
}

fn assign_default_widget_class(attrs: &mut IndexMap<String, String>, br: &BlockRef) {
    if attrs
        .get("class")
        .is_some_and(|class_name| !class_name.trim().is_empty())
    {
        return;
    }

    let kind = br.kind.rsplit("::").next().unwrap_or(br.kind.as_str());
    attrs.insert("class".to_string(), format!("wdoc-widget-{kind}"));
}

fn insert_default_content_inset(attrs: &mut IndexMap<String, String>, edge: &str, value: f64) {
    let public_key = format!("content_{edge}");
    let private_key = format!("_wdoc_content_{edge}");
    let value = attrs
        .get(&public_key)
        .cloned()
        .unwrap_or_else(|| format_number(value));
    attrs.entry(private_key).or_insert(value);
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        (value as i64).to_string()
    } else {
        value.to_string()
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
    shapes: Vec<crate::shapes::ShapeNode>,
    connections: Vec<crate::shapes::Connection>,
}

/// Look up a `@template("shape", "fn")` function for `br.kind` and call it.
/// The function receives the BlockRef as its single argument and must return a
/// list of "shape descriptor" values (maps describing primitive shapes).
fn dispatch_shape_template(br: &BlockRef, ctx: &ExtractCtx) -> Result<ShapeTemplateResult, String> {
    let fn_name = ctx
        .template_map
        .get(&("shape".to_string(), br.kind.clone()))
        .ok_or_else(|| format!("no @template(\"shape\", ...) on schema '{}'", br.kind))?;

    let func = ctx.template_helpers.get(fn_name).ok_or_else(|| {
        if ctx.builtins.contains_key(fn_name) {
            format!("shape template function '{fn_name}' must be an exported WCL function")
        } else {
            format!("shape template function '{fn_name}' not registered")
        }
    })?;

    let themed_block = apply_widget_theme_class_attrs(br, ctx);
    let arg = Value::BlockRef(themed_block);
    let result =
        wcl_lang::call_lambda_with_env(func, &[arg], &ctx.builtins, &ctx.template_helpers)?;

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

fn apply_widget_theme_class_attrs(br: &BlockRef, ctx: &ExtractCtx) -> BlockRef {
    let mut themed = br.clone();
    let class_names = widget_theme_class_names(br);
    if class_names.is_empty() {
        return themed;
    }

    let classes = ctx.diagram_classes.borrow();
    for class_name in class_names {
        let Some(class) = classes.get(&class_name) else {
            continue;
        };
        for (key, value) in &class.attrs {
            themed
                .attributes
                .entry(key.clone())
                .or_insert_with(|| Value::String(value.clone()));
        }
    }
    themed
}

fn widget_theme_class_names(br: &BlockRef) -> Vec<String> {
    if let Some(class_attr) = br.attributes.get("class").and_then(|v| v.as_string()) {
        let names = class_attr
            .split_whitespace()
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !names.is_empty() {
            return names;
        }
    }

    let kind = br.kind.rsplit("::").next().unwrap_or(br.kind.as_str());
    vec![format!("wdoc-widget-{kind}")]
}

/// Convert a shape descriptor `Value::Map` (returned from a WCL shape template)
/// into a `ShapeNode`. Recognized fields: kind, x, y, width, height, top, bottom,
/// left, right, align, gap, padding, children (list of more descriptors), and
/// arbitrary visual attributes (fill, stroke, rx, content, font_size, ...).
#[cfg(test)]
fn descriptor_to_shape_node_with_order(
    val: &Value,
    source_order: usize,
) -> Option<crate::shapes::ShapeNode> {
    descriptor_to_shape_node_and_connections(val, source_order).map(|(node, _)| node)
}

fn descriptor_to_shape_node_and_connections(
    val: &Value,
    source_order: usize,
) -> Option<(crate::shapes::ShapeNode, Vec<crate::shapes::Connection>)> {
    use crate::shapes::*;

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
                | "events"
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
    let events = descriptor_events(map);

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
        events,
        children,
        text_block_items: Vec::new(),
        align,
        gap,
        padding,
        z_index,
        source_order,
    };

    Some((node, scope_connections(connections, id.as_deref())))
}

fn descriptor_events(map: &IndexMap<String, Value>) -> Vec<crate::shapes::DiagramEvent> {
    let Some(Value::List(items)) = map.get("events") else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let Value::Map(event) = item else {
                return None;
            };
            let trigger = event.get("trigger")?.as_string()?.to_string();
            let state = event.get("state")?.as_string()?.to_string();
            let target = event
                .get("target")
                .and_then(|v| v.as_string())
                .map(str::to_string);
            let button = event
                .get("button")
                .and_then(|v| v.as_string())
                .map(str::to_string);
            let mode = event
                .get("mode")
                .and_then(|v| v.as_string())
                .map(str::to_string);
            let duration_ms = event.get("duration_ms").and_then(|v| match v {
                Value::Int(i) => Some(*i as i32),
                Value::Float(f) => Some(*f as i32),
                Value::String(s) => s.parse().ok(),
                _ => None,
            });
            let prevent_default = event.get("prevent_default").and_then(|v| match v {
                Value::Bool(b) => Some(*b),
                Value::String(s) => s.parse().ok(),
                _ => None,
            });
            let guard_targets = event
                .get("guard_targets")
                .and_then(|v| v.as_string())
                .map(str::to_string);
            Some(crate::shapes::DiagramEvent {
                name: event
                    .get("name")
                    .and_then(|v| v.as_string())
                    .map(str::to_string),
                trigger,
                state,
                target,
                button,
                mode,
                duration_ms,
                prevent_default,
                guard_targets,
            })
        })
        .collect()
}

fn descriptor_to_connection_with_order(
    val: &Value,
    source_order: usize,
) -> Option<crate::shapes::Connection> {
    use crate::shapes::*;

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

fn mark_template_layout_decoration(node: &mut crate::shapes::ShapeNode) {
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
    connections: Vec<crate::shapes::Connection>,
    scope: Option<&str>,
) -> Vec<crate::shapes::Connection> {
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
) -> IndexMap<String, crate::shapes::DiagramClass> {
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
    classes: &mut IndexMap<String, crate::shapes::DiagramClass>,
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
                        crate::shapes::DiagramState {
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
            crate::shapes::DiagramClass {
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

fn collect_diagram_events(block: &BlockRef) -> Vec<crate::shapes::DiagramEvent> {
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
            let guard_targets = child
                .attributes
                .get("guard_targets")
                .and_then(|v| v.as_string())
                .map(str::to_string);
            Some(crate::shapes::DiagramEvent {
                name: child.id.clone(),
                trigger,
                state,
                target,
                button,
                mode,
                duration_ms,
                prevent_default,
                guard_targets,
            })
        })
        .collect()
}

fn parse_diagram_animation(block: &BlockRef) -> Option<crate::shapes::DiagramAnimation> {
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
    Some(crate::shapes::DiagramAnimation {
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

fn parse_diagram_keyframe(block: &BlockRef) -> Option<crate::shapes::DiagramKeyframe> {
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
    Some(crate::shapes::DiagramKeyframe {
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

fn is_draw_graph_row_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::graph_row" | "draw::graph_row" | "graph_row"
    )
}

fn is_draw_graph_divider_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::graph_divider" | "draw::graph_divider" | "graph_divider"
    )
}

fn is_draw_graph_node_structural_block(block: &BlockRef) -> bool {
    is_draw_graph_row_block(block) || is_draw_graph_divider_block(block)
}

fn is_draw_graph_node_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::graph_node" | "draw::graph_node" | "graph_node"
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
    template_helpers: HashMap<String, FunctionValue>,
    markup_rules: Vec<MarkupRule>,
    builtins: HashMap<String, BuiltinFn>,
    css_registry: Rc<RefCell<DiagramCssRegistry>>,
    diagram_classes: Rc<RefCell<IndexMap<String, crate::shapes::DiagramClass>>>,
    svg_search_dirs: Vec<PathBuf>,
}

impl ExtractCtx {
    fn render_block(&self, block: &BlockRef) -> Result<String, String> {
        let kind = &block.kind;
        let fn_name = self
            .template_map
            .get(&("html".to_string(), kind.clone()))
            .ok_or_else(|| format!("no @template(\"html\", ...) found for block kind '{kind}'"))?;

        let func = self.template_helpers.get(fn_name).ok_or_else(|| {
            if self.builtins.contains_key(fn_name) {
                format!("template function '{fn_name}' must be an exported WCL function")
            } else {
                format!("template function '{fn_name}' not found for '{kind}'")
            }
        })?;

        let _guard = enter_current_wdoc_ctx(self);
        let result = wcl_lang::call_lambda_with_env(
            func,
            &[Value::BlockRef(block.clone())],
            &self.builtins,
            &self.template_helpers,
        )?;
        match result {
            Value::String(s) => Ok(s),
            other => Ok(format!("{other}")),
        }
    }
}

thread_local! {
    static CURRENT_WDOC_CTX: RefCell<Vec<*const ExtractCtx>> = const { RefCell::new(Vec::new()) };
}

struct CurrentWdocCtxGuard;

impl Drop for CurrentWdocCtxGuard {
    fn drop(&mut self) {
        CURRENT_WDOC_CTX.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

fn enter_current_wdoc_ctx(ctx: &ExtractCtx) -> CurrentWdocCtxGuard {
    CURRENT_WDOC_CTX.with(|stack| {
        stack.borrow_mut().push(ctx as *const ExtractCtx);
    });
    CurrentWdocCtxGuard
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
        let scoped = crate::shapes::scope_css_to_selector(css, &format!(".{scope_class}"));
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
    use crate::library::WDOC_LIBRARY_WCL;
    use wcl_lang::Span;

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
            template_helpers: HashMap::new(),
            markup_rules: Vec::new(),
            builtins: HashMap::new(),
            css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
            diagram_classes: Rc::new(RefCell::new(IndexMap::new())),
            svg_search_dirs: Vec::new(),
        }
    }

    fn custom_shape_ctx(source: &str, kind: &str, template_name: &str) -> ExtractCtx {
        let functions = wdoc_functions();
        let doc = wcl_lang::parse(
            source,
            wcl_lang::ParseOptions {
                functions: functions.clone(),
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );

        let mut ctx = empty_ctx();
        ctx.template_map.insert(
            ("shape".to_string(), kind.to_string()),
            template_name.to_string(),
        );
        ctx.template_helpers = collect_template_helpers(&doc);
        ctx.markup_rules = collect_markup_rules(&doc, &ctx.template_helpers).unwrap();
        ctx.builtins = functions.functions;
        ctx
    }

    fn wdoc_library_ctx() -> ExtractCtx {
        let functions = wdoc_functions();
        let doc = wcl_lang::parse(
            WDOC_LIBRARY_WCL,
            wcl_lang::ParseOptions {
                functions: functions.clone(),
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );

        let template_helpers = collect_template_helpers(&doc);
        let markup_rules = collect_markup_rules(&doc, &template_helpers).unwrap();
        ExtractCtx {
            template_map: collect_template_map(&doc),
            template_helpers,
            markup_rules,
            builtins: functions.functions,
            css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
            diagram_classes: Rc::new(RefCell::new(IndexMap::new())),
            svg_search_dirs: Vec::new(),
        }
    }

    fn int_attr(attrs: &mut IndexMap<String, Value>, key: &str, value: i64) {
        attrs.insert(key.to_string(), Value::Int(value));
    }

    #[test]
    fn pure_inline_helpers_are_not_registered_as_host_functions() {
        let functions = wdoc_functions();
        for name in [
            "bold",
            "wdoc::bold",
            "italic",
            "wdoc::italic",
            "link",
            "wdoc::link",
            "wdoc::html_escape",
        ] {
            assert!(
                !functions.functions.contains_key(name),
                "{name} should be implemented by the WDoc library, not Rust"
            );
        }
    }

    #[test]
    fn paragraph_renders_builtin_markup() {
        let ctx = wdoc_library_ctx();
        let mut attrs = IndexMap::new();
        string_attr(
            &mut attrs,
            "content",
            "Use **schemas**, _expressions_, `code`, [imports](guide-imports.html), and :github:",
        );
        let html = ctx
            .render_block(&block("wdoc::paragraph", None, attrs, vec![]))
            .unwrap();
        assert!(html.contains("<strong>schemas</strong>"));
        assert!(html.contains("<em>expressions</em>"));
        assert!(html.contains("<code>code</code>"));
        assert!(html.contains("<a href=\"guide-imports.html\">imports</a>"));
        assert!(html.contains("<i class=\"bi bi-github\"></i>"));
    }

    #[test]
    fn paragraph_renders_custom_markup() {
        let functions = wdoc_functions();
        let source = format!(
            "{}\nnamespace wdoc {{\n@markup(\"=={{text}}==\")\nexport let mark = text => \"<mark>\" + wdoc::html_escape(text) + \"</mark>\"\n}}\n",
            WDOC_LIBRARY_WCL
        );
        let doc = wcl_lang::parse(
            &source,
            wcl_lang::ParseOptions {
                functions: functions.clone(),
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );
        let template_helpers = collect_template_helpers(&doc);
        let ctx = ExtractCtx {
            template_map: collect_template_map(&doc),
            markup_rules: collect_markup_rules(&doc, &template_helpers).unwrap(),
            template_helpers,
            builtins: functions.functions,
            css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
            diagram_classes: Rc::new(RefCell::new(IndexMap::new())),
            svg_search_dirs: Vec::new(),
        };
        let mut attrs = IndexMap::new();
        string_attr(&mut attrs, "content", "This is ==marked== text");
        let html = ctx
            .render_block(&block("wdoc::paragraph", None, attrs, vec![]))
            .unwrap();
        assert!(html.contains("<mark>marked</mark>"));
    }

    #[test]
    fn markup_backslash_escape_keeps_literal_text() {
        let ctx = wdoc_library_ctx();
        let mut attrs = IndexMap::new();
        string_attr(&mut attrs, "content", "\\**not bold**");
        let html = ctx
            .render_block(&block("wdoc::paragraph", None, attrs, vec![]))
            .unwrap();
        assert!(html.contains("**not bold**"));
        assert!(!html.contains("<strong>not bold</strong>"));
    }

    #[test]
    fn markup_does_not_rewrite_existing_html_tags() {
        let ctx = wdoc_library_ctx();
        let mut attrs = IndexMap::new();
        string_attr(
            &mut attrs,
            "content",
            "<i class=\"bi bi-shield-check\" style=\"font-size:1.1em;color:#28a745;\"></i> **Typed schemas**",
        );
        let html = ctx
            .render_block(&block("wdoc::paragraph", None, attrs, vec![]))
            .unwrap();
        assert!(html.contains("style=\"font-size:1.1em;color:#28a745;\""));
        assert!(!html.contains("bi-1.1em;color"));
        assert!(html.contains("<strong>Typed schemas</strong>"));
    }

    #[test]
    fn bundled_templates_resolve_to_wcl_lambdas() {
        let functions = wdoc_functions();
        let doc = wcl_lang::parse(
            WDOC_LIBRARY_WCL,
            wcl_lang::ParseOptions {
                functions: functions.clone(),
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );

        let template_map = collect_template_map(&doc);
        let helpers = collect_template_helpers(&doc);
        assert_eq!(
            template_map
                .get(&(
                    "shape".to_string(),
                    "wdoc::draw::terminal_button".to_string()
                ))
                .map(String::as_str),
            Some("wdoc::terminal_widget_button")
        );
        assert_eq!(
            template_map
                .get(&(
                    "shape".to_string(),
                    "wdoc::draw::terminal_dropdown".to_string()
                ))
                .map(String::as_str),
            Some("wdoc::terminal_widget_dropdown")
        );
        let mut checked = 0;
        for ((format, schema), fn_name) in template_map {
            checked += 1;
            assert!(
                helpers.contains_key(&fn_name),
                "{format} template for {schema} must resolve to exported WCL function {fn_name}"
            );
        }
        assert!(checked > 0, "expected bundled templates");
    }

    #[test]
    fn shape_templates_do_not_fall_back_to_rust_builtins() {
        let template = std::sync::Arc::new(|_args: &[Value]| Ok(Value::Null)) as BuiltinFn;
        let mut ctx = empty_ctx();
        ctx.template_map.insert(
            ("shape".to_string(), "my::legacy".to_string()),
            "legacy_shape_template".to_string(),
        );
        ctx.builtins
            .insert("legacy_shape_template".to_string(), template);

        let err = match dispatch_shape_template(
            &block("my::legacy", Some("legacy"), IndexMap::new(), vec![]),
            &ctx,
        ) {
            Ok(_) => panic!("expected rust builtin shape template to fail"),
            Err(err) => err,
        };
        assert!(err.contains(
            "shape template function 'legacy_shape_template' must be an exported WCL function"
        ));
    }

    #[test]
    fn html_templates_do_not_fall_back_to_rust_builtins() {
        let template =
            std::sync::Arc::new(|_args: &[Value]| Ok(Value::String("legacy".into()))) as BuiltinFn;
        let mut ctx = empty_ctx();
        ctx.template_map.insert(
            ("html".to_string(), "my::legacy".to_string()),
            "legacy_html_template".to_string(),
        );
        ctx.builtins
            .insert("legacy_html_template".to_string(), template);

        let err = ctx
            .render_block(&block(
                "my::legacy",
                Some("legacy"),
                IndexMap::new(),
                vec![],
            ))
            .unwrap_err();
        assert!(err
            .contains("template function 'legacy_html_template' must be an exported WCL function"));
    }

    #[test]
    fn wcl_html_templates_render_standard_content() {
        let ctx = wdoc_library_ctx();

        let mut heading_attrs = IndexMap::new();
        int_attr(&mut heading_attrs, "level", 2);
        string_attr(&mut heading_attrs, "content", "Hello World");
        assert_eq!(
            ctx.render_block(&block("wdoc::heading", Some("h"), heading_attrs, vec![]))
                .unwrap(),
            "<h2 id=\"hello-world\" class=\"wdoc-heading\">Hello World</h2>"
        );

        let mut paragraph_attrs = IndexMap::new();
        string_attr(&mut paragraph_attrs, "content", "<div>Block</div>");
        assert_eq!(
            ctx.render_block(&block(
                "wdoc::paragraph",
                Some("p"),
                paragraph_attrs,
                vec![]
            ))
            .unwrap(),
            "<div class=\"wdoc-paragraph\"><div>Block</div></div>"
        );

        let mut code_attrs = IndexMap::new();
        string_attr(&mut code_attrs, "language", "html");
        string_attr(&mut code_attrs, "content", "<div>hi</div>");
        let code_html = ctx
            .render_block(&block("wdoc::code", Some("c"), code_attrs, vec![]))
            .unwrap();
        assert!(code_html.contains("language-html"));
        assert!(code_html.contains("&lt;div&gt;hi&lt;/div&gt;"));

        let mut row = IndexMap::new();
        string_attr(&mut row, "Name", "Ada");
        int_attr(&mut row, "Age", 37);
        let mut table_attrs = IndexMap::new();
        string_attr(&mut table_attrs, "caption", "People");
        table_attrs.insert("rows".to_string(), Value::List(vec![Value::Map(row)]));
        let table_html = ctx
            .render_block(&block("wdoc::data_table", Some("tbl"), table_attrs, vec![]))
            .unwrap();
        assert!(table_html.contains("<caption>People</caption>"));
        assert!(table_html.contains("<th>Name</th><th>Age</th>"));
        assert!(table_html.contains("<td>Ada</td><td>37</td>"));

        let mut child_attrs = IndexMap::new();
        string_attr(&mut child_attrs, "content", "Nested");
        let mut callout_attrs = IndexMap::new();
        string_attr(&mut callout_attrs, "header", "Note");
        let callout_html = ctx
            .render_block(&block(
                "wdoc::callout",
                Some("call"),
                callout_attrs,
                vec![block("wdoc::paragraph", Some("child"), child_attrs, vec![])],
            ))
            .unwrap();
        assert!(callout_html.contains("wdoc-callout-header"));
        assert!(callout_html.contains("<p class=\"wdoc-paragraph\">Nested</p>"));
    }

    #[test]
    fn builtin_widget_content_insets_are_added_to_composite_container_attrs() {
        let mut attrs = IndexMap::new();
        let mut block_attrs = IndexMap::new();
        string_attr(&mut block_attrs, "title", "Profile");
        let card = block("wdoc::draw::card", Some("panel"), block_attrs, vec![]);

        apply_builtin_widget_content_insets(&mut attrs, &card);

        assert_eq!(
            attrs.get("_wdoc_content_top").map(String::as_str),
            Some("36")
        );
    }

    #[test]
    fn explicit_widget_content_insets_override_defaults() {
        let mut attrs = IndexMap::new();
        attrs.insert("content_top".to_string(), "96".to_string());
        let phone = block("wdoc::draw::phone", Some("screen"), IndexMap::new(), vec![]);

        apply_builtin_widget_content_insets(&mut attrs, &phone);

        assert_eq!(
            attrs.get("_wdoc_content_top").map(String::as_str),
            Some("96")
        );
        assert_eq!(
            attrs.get("_wdoc_content_bottom").map(String::as_str),
            Some("50")
        );
    }

    #[test]
    fn widget_theme_uses_default_class_when_widget_class_is_unset() {
        let button = block(
            "wdoc::draw::button",
            Some("submit"),
            IndexMap::new(),
            vec![],
        );
        let mut class = crate::shapes::DiagramClass {
            name: "wdoc-widget-button".to_string(),
            attrs: IndexMap::new(),
            states: IndexMap::new(),
            animations: IndexMap::new(),
        };
        class
            .attrs
            .insert("background_fill".to_string(), "#0f766e".to_string());

        let ctx = empty_ctx();
        ctx.diagram_classes
            .borrow_mut()
            .insert(class.name.clone(), class);

        let themed = apply_widget_theme_class_attrs(&button, &ctx);
        assert_eq!(
            themed
                .attributes
                .get("background_fill")
                .and_then(Value::as_string),
            Some("#0f766e")
        );
    }

    #[test]
    fn widget_theme_uses_replacement_class_and_explicit_attrs_win() {
        let mut button_attrs = IndexMap::new();
        string_attr(&mut button_attrs, "class", "brand_button");
        string_attr(&mut button_attrs, "background_fill", "#b91c1c");
        let button = block("wdoc::draw::button", Some("delete"), button_attrs, vec![]);

        let mut default_class = crate::shapes::DiagramClass {
            name: "wdoc-widget-button".to_string(),
            attrs: IndexMap::new(),
            states: IndexMap::new(),
            animations: IndexMap::new(),
        };
        default_class
            .attrs
            .insert("background_fill".to_string(), "#0f766e".to_string());

        let mut brand_class = crate::shapes::DiagramClass {
            name: "brand_button".to_string(),
            attrs: IndexMap::new(),
            states: IndexMap::new(),
            animations: IndexMap::new(),
        };
        brand_class
            .attrs
            .insert("background_fill".to_string(), "#2563eb".to_string());
        brand_class
            .attrs
            .insert("label_fill".to_string(), "#ffffff".to_string());

        let ctx = empty_ctx();
        ctx.diagram_classes
            .borrow_mut()
            .insert(default_class.name.clone(), default_class);
        ctx.diagram_classes
            .borrow_mut()
            .insert(brand_class.name.clone(), brand_class);

        let themed = apply_widget_theme_class_attrs(&button, &ctx);
        assert_eq!(
            themed
                .attributes
                .get("background_fill")
                .and_then(Value::as_string),
            Some("#b91c1c")
        );
        assert_eq!(
            themed
                .attributes
                .get("label_fill")
                .and_then(Value::as_string),
            Some("#ffffff")
        );
    }

    #[test]
    fn graph_node_rows_emit_scoped_port_endpoints() {
        let ctx = wdoc_library_ctx();

        let mut api_attrs = IndexMap::new();
        int_attr(&mut api_attrs, "x", 30);
        int_attr(&mut api_attrs, "y", 30);
        int_attr(&mut api_attrs, "width", 220);
        string_attr(&mut api_attrs, "title", "API");
        string_attr(&mut api_attrs, "port_fill", "#10b981");

        let mut api_in = IndexMap::new();
        string_attr(&mut api_in, "label", "HTTP");
        string_attr(&mut api_in, "left_port", "in");

        let mut api_out = IndexMap::new();
        string_attr(&mut api_out, "label", "Repository");
        string_attr(&mut api_out, "right_port", "out");

        let mut db_attrs = IndexMap::new();
        int_attr(&mut db_attrs, "x", 330);
        int_attr(&mut db_attrs, "y", 54);
        int_attr(&mut db_attrs, "width", 220);
        string_attr(&mut db_attrs, "title", "Database");
        string_attr(&mut db_attrs, "port_fill", "#10b981");

        let mut db_in = IndexMap::new();
        string_attr(&mut db_in, "label", "SQL");
        string_attr(&mut db_in, "left_port", "query");

        let mut conn_attrs = IndexMap::new();
        string_attr(&mut conn_attrs, "from", "api.out");
        string_attr(&mut conn_attrs, "to", "db.query");
        string_attr(&mut conn_attrs, "direction", "to");

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 600);
        int_attr(&mut diagram_attrs, "height", 220);

        let diagram = block(
            "wdoc::draw::diagram",
            Some("graph_node_ports"),
            diagram_attrs,
            vec![
                block(
                    "wdoc::draw::graph_node",
                    Some("api"),
                    api_attrs,
                    vec![
                        block("wdoc::draw::graph_row", Some("http"), api_in, vec![]),
                        block("wdoc::draw::graph_row", Some("repo"), api_out, vec![]),
                    ],
                ),
                block(
                    "wdoc::draw::graph_node",
                    Some("db"),
                    db_attrs,
                    vec![block("wdoc::draw::graph_row", Some("sql"), db_in, vec![])],
                ),
                block("wdoc::draw::connection", Some("api_db"), conn_attrs, vec![]),
            ],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains(">API</text>"));
        assert!(html.contains(">Repository</text>"));
        assert!(html.contains("data-wdoc-conn-from=\"api.out\""));
        assert!(html.contains("data-wdoc-conn-to=\"db.query\""));
        assert!(html.contains("marker-end=\"url(#wdoc-arrow)\""));
        assert!(html.contains("fill=\"#10b981\" stroke=\"var(--color-bg)\""));
        assert!(html.contains("fill=\"none\" stroke=\"var(--color-bg)\""));
        assert!(!html.contains("width=\"0\" height=\"0\""));
    }

    #[test]
    fn graph_node_dividers_render_between_rows() {
        let ctx = wdoc_library_ctx();

        let mut node_attrs = IndexMap::new();
        int_attr(&mut node_attrs, "x", 30);
        int_attr(&mut node_attrs, "y", 30);
        int_attr(&mut node_attrs, "width", 220);
        string_attr(&mut node_attrs, "title", "API");

        let mut first_attrs = IndexMap::new();
        string_attr(&mut first_attrs, "label", "Ingress");
        string_attr(&mut first_attrs, "right_port", "in");

        let mut divider_attrs = IndexMap::new();
        int_attr(&mut divider_attrs, "height", 14);
        int_attr(&mut divider_attrs, "inset", 12);
        string_attr(&mut divider_attrs, "stroke", "#ff00aa");

        let mut second_attrs = IndexMap::new();
        string_attr(&mut second_attrs, "label", "Repository");
        string_attr(&mut second_attrs, "right_port", "repo");

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 320);
        int_attr(&mut diagram_attrs, "height", 200);

        let diagram = block(
            "wdoc::draw::diagram",
            Some("graph_node_dividers"),
            diagram_attrs,
            vec![block(
                "wdoc::draw::graph_node",
                Some("api"),
                node_attrs,
                vec![
                    block(
                        "wdoc::draw::graph_row",
                        Some("ingress"),
                        first_attrs,
                        vec![],
                    ),
                    block(
                        "wdoc::draw::graph_divider",
                        Some("boundary"),
                        divider_attrs,
                        vec![],
                    ),
                    block("wdoc::draw::graph_row", Some("repo"), second_attrs, vec![]),
                ],
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("height=\"104\""));
        assert!(html.contains("x1=\"12\" y1=\"69\" x2=\"208\" y2=\"69\""));
        assert!(html.contains("stroke=\"#ff00aa\""));
        assert!(html.contains(">Ingress</text>"));
        assert!(html.contains(">Repository</text>"));
        assert!(!html.contains("width=\"0\" height=\"0\""));
    }

    #[test]
    fn text_block_renders_styled_wdoc_content_inside_shapes() {
        let ctx = wdoc_library_ctx();

        let mut para_attrs = IndexMap::new();
        string_attr(
            &mut para_attrs,
            "content",
            "Routes **authenticated** requests to _domain_ services, links to [docs](docs.html), and emits `AuditEvent`.",
        );

        let mut code_attrs = IndexMap::new();
        string_attr(&mut code_attrs, "language", "wcl");
        string_attr(
            &mut code_attrs,
            "content",
            "connection c { from = \"api.out\" to = \"worker.in\" }",
        );

        let mut text_attrs = IndexMap::new();
        int_attr(&mut text_attrs, "left", 12);
        int_attr(&mut text_attrs, "top", 12);
        int_attr(&mut text_attrs, "right", 12);
        int_attr(&mut text_attrs, "font_size", 12);

        let mut card_attrs = IndexMap::new();
        int_attr(&mut card_attrs, "x", 20);
        int_attr(&mut card_attrs, "y", 20);
        int_attr(&mut card_attrs, "width", 260);
        string_attr(&mut card_attrs, "fill", "#111827");

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 320);
        int_attr(&mut diagram_attrs, "height", 220);

        let diagram = block(
            "wdoc::draw::diagram",
            Some("text_block_styles"),
            diagram_attrs,
            vec![block(
                "wdoc::draw::rect",
                Some("card"),
                card_attrs,
                vec![block(
                    "wdoc::draw::text_block",
                    Some("body"),
                    text_attrs,
                    vec![
                        block("wdoc::paragraph", Some("summary"), para_attrs, vec![]),
                        block("wdoc::code", Some("example"), code_attrs, vec![]),
                    ],
                )],
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("font-weight=\"700\""));
        assert!(html.contains("font-style=\"italic\""));
        assert!(html.contains("text-decoration=\"underline\""));
        assert!(html.contains("AuditEvent"));
        assert!(html.contains("data-language=\"wcl\""));
        assert!(html.contains("class=\"hljs-keyword\""));
        assert!(html.contains("class=\"hljs-attr\""));
        assert!(html.contains("class=\"hljs-string\""));
        assert!(html.contains("connection"));
        assert!(!html.contains("<strong>"));
        assert!(!html.contains("height=\"0\""));
    }

    #[test]
    fn text_block_grows_parent_without_explicit_height() {
        let ctx = wdoc_library_ctx();

        let mut para_attrs = IndexMap::new();
        string_attr(
            &mut para_attrs,
            "content",
            "This text block is intentionally long enough to wrap over several lines so the parent rectangle must grow downward.",
        );

        let mut text_attrs = IndexMap::new();
        int_attr(&mut text_attrs, "left", 10);
        int_attr(&mut text_attrs, "top", 10);
        int_attr(&mut text_attrs, "right", 10);
        int_attr(&mut text_attrs, "font_size", 12);

        let mut card_attrs = IndexMap::new();
        int_attr(&mut card_attrs, "x", 20);
        int_attr(&mut card_attrs, "y", 20);
        int_attr(&mut card_attrs, "width", 150);

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 240);
        int_attr(&mut diagram_attrs, "height", 180);

        let diagram = block(
            "wdoc::draw::diagram",
            Some("text_block_grow"),
            diagram_attrs,
            vec![block(
                "wdoc::draw::rect",
                Some("card"),
                card_attrs,
                vec![block(
                    "wdoc::draw::text_block",
                    Some("body"),
                    text_attrs,
                    vec![block(
                        "wdoc::paragraph",
                        Some("summary"),
                        para_attrs,
                        vec![],
                    )],
                )],
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx);
        let height_prefix = "<rect x=\"20\" y=\"20\" width=\"150\" height=\"";
        let start = html.find(height_prefix).expect("parent rect") + height_prefix.len();
        let end = html[start..].find('"').expect("height end") + start;
        let height = html[start..end].parse::<f64>().expect("numeric height");
        assert!(height > 80.0, "expected parent to grow, got {height}");
    }

    #[test]
    fn widget_theme_properties_flow_into_template_without_svg_leakage() {
        let ctx = custom_shape_ctx(
            r##"
            export let test_button_template = (b) => [
                {
                    kind = "rect",
                    x = 0,
                    y = 0,
                    width = 80,
                    height = 30,
                    fill = attr_or(b, "background_fill", "#cccccc")
                }
            ]
            "##,
            "wdoc::draw::button",
            "test_button_template",
        );
        let mut class = crate::shapes::DiagramClass {
            name: "wdoc-widget-button".to_string(),
            attrs: IndexMap::new(),
            states: IndexMap::new(),
            animations: IndexMap::new(),
        };
        class
            .attrs
            .insert("background_fill".to_string(), "#0f766e".to_string());
        ctx.diagram_classes
            .borrow_mut()
            .insert(class.name.clone(), class);

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 120);
        int_attr(&mut diagram_attrs, "height", 60);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("themed_widget"),
            diagram_attrs,
            vec![block(
                "wdoc::draw::button",
                Some("submit"),
                IndexMap::new(),
                vec![],
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("fill=\"#0f766e\""));
        assert!(html.contains("class=\"wdoc-widget-button\""));
        assert!(!html.contains("background_fill"));
    }

    #[test]
    fn terminal_widget_templates_render_primitives_and_use_classes() {
        let ctx = wdoc_library_ctx();
        let mut class_attrs = IndexMap::new();
        string_attr(&mut class_attrs, "background_fill", "#0f766e");
        string_attr(&mut class_attrs, "hover_background_fill", "#164e63");
        string_attr(&mut class_attrs, "foreground_fill", "#ffffff");
        let button_class = block(
            "class",
            Some("brand_terminal"),
            class_attrs,
            vec![block("state", Some("hovered"), IndexMap::new(), vec![])],
        );
        ctx.diagram_classes
            .borrow_mut()
            .extend(collect_diagram_classes(&IndexMap::from([(
                "brand_terminal".to_string(),
                Value::BlockRef(button_class),
            )])));

        let mut button_attrs = IndexMap::new();
        int_attr(&mut button_attrs, "row", 1);
        int_attr(&mut button_attrs, "col", 2);
        int_attr(&mut button_attrs, "cols", 14);
        string_attr(&mut button_attrs, "label", "Deploy");
        string_attr(&mut button_attrs, "class", "brand_terminal");

        let mut dropdown_attrs = IndexMap::new();
        int_attr(&mut dropdown_attrs, "row", 3);
        int_attr(&mut dropdown_attrs, "col", 2);
        int_attr(&mut dropdown_attrs, "cols", 12);
        string_attr(&mut dropdown_attrs, "value", "prod");

        let mut menubar_attrs = IndexMap::new();
        int_attr(&mut menubar_attrs, "row", 0);
        int_attr(&mut menubar_attrs, "col", 0);
        int_attr(&mut menubar_attrs, "cols", 28);

        let mut run_menu_attrs = IndexMap::new();
        int_attr(&mut run_menu_attrs, "row", 1);
        int_attr(&mut run_menu_attrs, "col", 0);
        int_attr(&mut run_menu_attrs, "rows", 2);
        int_attr(&mut run_menu_attrs, "cols", 12);
        string_attr(
            &mut run_menu_attrs,
            "leave_close_targets",
            "run_menu,build_menu,test_menu",
        );
        string_attr(
            &mut run_menu_attrs,
            "leave_guard_targets",
            "run_menu,build_menu,test_menu",
        );

        let mut file_attrs = IndexMap::new();
        string_attr(&mut file_attrs, "label", "File");
        string_attr(&mut file_attrs, "target", "run_menu");
        let mut run_attrs = IndexMap::new();
        string_attr(&mut run_attrs, "label", "Run");
        string_attr(&mut run_attrs, "target", "test_menu");
        let mut help_attrs = IndexMap::new();
        string_attr(&mut help_attrs, "label", "Help");
        string_attr(&mut help_attrs, "disabled", "true");

        let mut build_attrs = IndexMap::new();
        string_attr(&mut build_attrs, "label", "Build");
        string_attr(&mut build_attrs, "target", "build_menu");
        let mut format_attrs = IndexMap::new();
        string_attr(&mut format_attrs, "label", "Format");

        let mut dev_attrs = IndexMap::new();
        string_attr(&mut dev_attrs, "label", "dev");
        let mut prod_attrs = IndexMap::new();
        string_attr(&mut prod_attrs, "label", "prod");

        let mut terminal_attrs = IndexMap::new();
        int_attr(&mut terminal_attrs, "rows", 8);
        int_attr(&mut terminal_attrs, "cols", 32);
        let terminal = block(
            "wdoc::draw::terminal",
            Some("term"),
            terminal_attrs,
            vec![
                block(
                    "wdoc::draw::terminal_menubar",
                    Some("mainbar"),
                    menubar_attrs,
                    vec![
                        block("wdoc::draw::menu_item", Some("file"), file_attrs, vec![]),
                        block("wdoc::draw::menu_item", Some("run"), run_attrs, vec![]),
                        block("wdoc::draw::menu_item", Some("help"), help_attrs, vec![]),
                    ],
                ),
                block(
                    "wdoc::draw::terminal_menu",
                    Some("run_menu"),
                    run_menu_attrs,
                    vec![
                        block("wdoc::draw::menu_item", Some("build"), build_attrs, vec![]),
                        block(
                            "wdoc::draw::menu_item",
                            Some("format"),
                            format_attrs,
                            vec![],
                        ),
                    ],
                ),
                block(
                    "wdoc::draw::terminal_button",
                    Some("deploy"),
                    button_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::terminal_dropdown",
                    Some("env"),
                    dropdown_attrs,
                    vec![
                        block("wdoc::draw::menu_item", Some("dev"), dev_attrs, vec![]),
                        block("wdoc::draw::menu_item", Some("prod"), prod_attrs, vec![]),
                    ],
                ),
            ],
        );

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 360);
        int_attr(&mut diagram_attrs, "height", 180);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("terminal_widgets"),
            diagram_attrs,
            vec![terminal],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("brand_terminal wdoc-terminal-control"));
        assert!(html.contains("data-wdoc-id=\"deploy_surface\""));
        assert!(html.contains("data-wdoc-id=\"deploy_label\""));
        assert!(html.contains("fill=\"#0f766e\""));
        assert!(html.contains("fill=\"#164e63\""));
        assert!(html.contains("[ Deploy ]"));
        assert!(html.contains("click|shown|env_menu|toggle"));
        assert!(html.contains("data-wdoc-id=\"env_menu\""));
        assert!(html.contains("wdoc-terminal-dropdown-menu"));
        assert!(html.contains("wdoc-terminal-menu"));
        assert!(html.contains("data-wdoc-id=\"mainbar_item_0\""));
        assert!(html.contains("data-wdoc-id=\"run_menu_item_0\""));
        assert!(html.contains("wdoc-terminal-menu-item-disabled"));
        assert!(html.contains("hover|shown|run_menu|add"));
        assert!(html.contains("hover|shown|test_menu|remove"));
        assert!(html.contains("click|shown|run_menu|remove"));
        assert!(html.contains("mouse_leave|shown|run_menu|remove"));
        assert!(html.contains("|run_menu,build_menu,test_menu"));
        assert!(html.contains("&gt;</text>"));
    }

    #[test]
    fn terminal_widget_classes_theme_all_controls_and_animate_root_groups() {
        let ctx = wdoc_library_ctx();

        let terminal_classes = [
            (
                "term_button_theme",
                [
                    ("background_fill", "#0f766e"),
                    ("hover_background_fill", "#164e63"),
                    ("label_fill", "#f8fafc"),
                ]
                .as_slice(),
            ),
            (
                "term_textbox_theme",
                [
                    ("background_fill", "#111827"),
                    ("placeholder_fill", "#c084fc"),
                    ("accent_fill", "#f59e0b"),
                ]
                .as_slice(),
            ),
            (
                "term_checkbox_theme",
                [
                    ("background_fill", "#020617"),
                    ("hover_background_fill", "#334155"),
                    ("label_fill", "#fef3c7"),
                    ("accent_fill", "#22c55e"),
                ]
                .as_slice(),
            ),
            (
                "term_radio_theme",
                [("label_fill", "#bfdbfe"), ("muted_fill", "#94a3b8")].as_slice(),
            ),
            (
                "term_dropdown_theme",
                [
                    ("background_fill", "#172554"),
                    ("foreground_fill", "#e0f2fe"),
                    ("hover_background_fill", "#38bdf8"),
                    ("hover_foreground_fill", "#06121f"),
                ]
                .as_slice(),
            ),
            (
                "term_menu_theme",
                [
                    ("background_fill", "#1e1b4b"),
                    ("foreground_fill", "#ddd6fe"),
                    ("hover_background_fill", "#a78bfa"),
                    ("hover_foreground_fill", "#111827"),
                ]
                .as_slice(),
            ),
        ];
        for (name, attrs) in terminal_classes {
            let mut class_attrs = IndexMap::new();
            for (key, value) in attrs {
                string_attr(&mut class_attrs, key, value);
            }
            let class = block("class", Some(name), class_attrs, vec![]);
            ctx.diagram_classes
                .borrow_mut()
                .extend(collect_diagram_classes(&IndexMap::from([(
                    name.to_string(),
                    Value::BlockRef(class),
                )])));
        }

        let mut anim_attrs = IndexMap::new();
        int_attr(&mut anim_attrs, "duration_ms", 500);
        string_attr(&mut anim_attrs, "direction", "alternate");
        string_attr(&mut anim_attrs, "iteration_count", "infinite");
        string_attr(&mut anim_attrs, "fill_mode", "both");
        let mut from_attrs = IndexMap::new();
        int_attr(&mut from_attrs, "offset", 0);
        int_attr(&mut from_attrs, "x", 28);
        int_attr(&mut from_attrs, "y", 30);
        int_attr(&mut from_attrs, "width", 56);
        int_attr(&mut from_attrs, "height", 18);
        let mut to_attrs = IndexMap::new();
        int_attr(&mut to_attrs, "offset", 100);
        int_attr(&mut to_attrs, "x", 42);
        int_attr(&mut to_attrs, "y", 30);
        int_attr(&mut to_attrs, "width", 56);
        int_attr(&mut to_attrs, "height", 18);
        let mut state_attrs = IndexMap::new();
        string_attr(&mut state_attrs, "animation", "grid_slide");
        let animated_class = block(
            "class",
            Some("term_animated_button"),
            IndexMap::from([
                (
                    "background_fill".to_string(),
                    Value::String("#14532d".to_string()),
                ),
                (
                    "label_fill".to_string(),
                    Value::String("#ffffff".to_string()),
                ),
            ]),
            vec![
                block(
                    "animation",
                    Some("grid_slide"),
                    anim_attrs,
                    vec![
                        block("keyframe", Some("start"), from_attrs, vec![]),
                        block("keyframe", Some("end"), to_attrs, vec![]),
                    ],
                ),
                block("state", Some("hovered"), state_attrs, vec![]),
            ],
        );
        ctx.diagram_classes
            .borrow_mut()
            .extend(collect_diagram_classes(&IndexMap::from([(
                "term_animated_button".to_string(),
                Value::BlockRef(animated_class),
            )])));

        let mut button_attrs = IndexMap::new();
        int_attr(&mut button_attrs, "row", 1);
        int_attr(&mut button_attrs, "col", 2);
        int_attr(&mut button_attrs, "cols", 7);
        string_attr(&mut button_attrs, "label", "Go");
        string_attr(&mut button_attrs, "class", "term_animated_button");

        let mut plain_button_attrs = IndexMap::new();
        int_attr(&mut plain_button_attrs, "row", 2);
        int_attr(&mut plain_button_attrs, "col", 2);
        string_attr(&mut plain_button_attrs, "label", "Ship");
        string_attr(&mut plain_button_attrs, "class", "term_button_theme");

        let mut textbox_attrs = IndexMap::new();
        int_attr(&mut textbox_attrs, "row", 3);
        int_attr(&mut textbox_attrs, "col", 2);
        int_attr(&mut textbox_attrs, "cols", 16);
        string_attr(&mut textbox_attrs, "placeholder", "filter");
        string_attr(&mut textbox_attrs, "class", "term_textbox_theme");
        int_attr(&mut textbox_attrs, "cursor_col", 2);

        let mut checkbox_attrs = IndexMap::new();
        int_attr(&mut checkbox_attrs, "row", 5);
        int_attr(&mut checkbox_attrs, "col", 2);
        string_attr(&mut checkbox_attrs, "label", "Dry run");
        string_attr(&mut checkbox_attrs, "checked", "true");
        string_attr(&mut checkbox_attrs, "class", "term_checkbox_theme");

        let mut radio_attrs = IndexMap::new();
        int_attr(&mut radio_attrs, "row", 6);
        int_attr(&mut radio_attrs, "col", 2);
        string_attr(&mut radio_attrs, "label", "Stage");
        string_attr(&mut radio_attrs, "class", "term_radio_theme");

        let mut dropdown_attrs = IndexMap::new();
        int_attr(&mut dropdown_attrs, "row", 7);
        int_attr(&mut dropdown_attrs, "col", 2);
        int_attr(&mut dropdown_attrs, "cols", 12);
        string_attr(&mut dropdown_attrs, "value", "prod");
        string_attr(&mut dropdown_attrs, "class", "term_dropdown_theme");
        let mut prod_attrs = IndexMap::new();
        string_attr(&mut prod_attrs, "label", "prod");

        let mut menu_attrs = IndexMap::new();
        int_attr(&mut menu_attrs, "row", 9);
        int_attr(&mut menu_attrs, "col", 2);
        int_attr(&mut menu_attrs, "cols", 12);
        string_attr(&mut menu_attrs, "class", "term_menu_theme");
        let mut build_attrs = IndexMap::new();
        string_attr(&mut build_attrs, "label", "Build");

        let mut terminal_attrs = IndexMap::new();
        int_attr(&mut terminal_attrs, "rows", 13);
        int_attr(&mut terminal_attrs, "cols", 32);
        let terminal = block(
            "wdoc::draw::terminal",
            Some("term"),
            terminal_attrs,
            vec![
                block(
                    "wdoc::draw::terminal_button",
                    Some("animated"),
                    button_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::terminal_button",
                    Some("plain"),
                    plain_button_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::terminal_textbox",
                    Some("filter"),
                    textbox_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::terminal_checkbox",
                    Some("dry"),
                    checkbox_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::terminal_radio",
                    Some("stage"),
                    radio_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::terminal_dropdown",
                    Some("env"),
                    dropdown_attrs,
                    vec![block(
                        "wdoc::draw::menu_item",
                        Some("prod"),
                        prod_attrs,
                        vec![],
                    )],
                ),
                block(
                    "wdoc::draw::terminal_menu",
                    Some("task_menu"),
                    menu_attrs,
                    vec![block(
                        "wdoc::draw::menu_item",
                        Some("build"),
                        build_attrs,
                        vec![],
                    )],
                ),
            ],
        );

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 360);
        int_attr(&mut diagram_attrs, "height", 260);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("terminal_class_theme"),
            diagram_attrs,
            vec![terminal],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx);
        assert!(html.contains("data-wdoc-id=\"animated\""));
        assert!(html.contains("class=\"term_animated_button wdoc-terminal-control\""));
        assert!(html.contains("data-wdoc-terminal-grid-group=\"true\""));
        assert!(html.contains("data-wdoc-state-animation=\"hovered:grid_slide\""));
        assert!(html.contains("grid_slide|500|0|ease|infinite|alternate|both|"));
        assert!(html.contains("fill=\"#14532d\""));
        assert!(html.contains("fill=\"#0f766e\""));
        assert!(html.contains("fill=\"#164e63\""));
        assert!(html.contains("fill=\"#f8fafc\""));
        assert!(html.contains("fill=\"#111827\""));
        assert!(html.contains("fill=\"#c084fc\""));
        assert!(html.contains("fill=\"#f59e0b\""));
        assert!(html.contains("fill=\"#020617\""));
        assert!(html.contains("fill=\"#334155\""));
        assert!(html.contains("fill=\"#fef3c7\""));
        assert!(html.contains("fill=\"#22c55e\""));
        assert!(html.contains("fill=\"#bfdbfe\""));
        assert!(html.contains("fill=\"#94a3b8\""));
        assert!(html.contains("fill=\"#172554\""));
        assert!(html.contains("fill=\"#e0f2fe\""));
        assert!(html.contains("fill=\"#38bdf8\""));
        assert!(html.contains("fill=\"#06121f\""));
        assert!(html.contains("fill=\"#1e1b4b\""));
        assert!(html.contains("fill=\"#ddd6fe\""));
        assert!(html.contains("fill=\"#a78bfa\""));
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
        let doc = wcl_lang::parse(
            r#"
            text label {
                content = "Inline text"
                font_size = 14
            }

            export let icon_x = 20 + measure_text(label).width + 8
            "#,
            wcl_lang::ParseOptions {
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
        let doc = wcl_lang::parse(
            r#"
            export let menu_labels = (b) =>
                join("|", map(children(b, "UiMenuItem"), item => item.label))
            "#,
            wcl_lang::ParseOptions {
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

        let rendered = wcl_lang::call_lambda_with_env(
            func,
            &[Value::BlockRef(menu)],
            &functions.functions,
            &HashMap::new(),
        )
        .unwrap();
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
        let ctx = custom_shape_ctx(
            r##"
            export let my_task_template = (_b) => [
                {
                    kind = "rect",
                    x = 0,
                    y = 0,
                    width = 100,
                    height = 40,
                    fill = "#bada55"
                }
            ]
            "##,
            "my::task",
            "my_task_template",
        );

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
        let ctx = custom_shape_ctx(
            r##"
            export let my_flow_box_template = (_b) => [
                {
                    kind = "rect",
                    id = "start",
                    width = 80,
                    height = 32,
                    layout_role = "node"
                },
                {
                    kind = "rect",
                    id = "end",
                    width = 80,
                    height = 32,
                    layout_role = "node"
                },
                {
                    kind = "connection",
                    from = "start",
                    to = "end",
                    direction = "to"
                }
            ]
            "##,
            "my::flow_box",
            "my_flow_box_template",
        );

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

        let mut diagram = crate::shapes::Diagram {
            id: None,
            width: 220.0,
            height: 160.0,
            padding: 0.0,
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes,
            connections,
            classes: IndexMap::new(),
        };
        crate::shapes::render_diagram_svg(&mut diagram);
        let flow_children = &diagram.shapes[0].children;
        assert!(flow_children[0].resolved.y < flow_children[1].resolved.y);
    }

    #[test]
    fn flowchart_widget_defaults_to_layered_nested_container() {
        let ctx = wdoc_library_ctx();
        assert!(ctx
            .template_map
            .contains_key(&("shape".to_string(), "wdoc::draw::flowchart".to_string())));

        let mut start_attrs = IndexMap::new();
        int_attr(&mut start_attrs, "width", 80);
        int_attr(&mut start_attrs, "height", 32);
        let mut end_attrs = IndexMap::new();
        int_attr(&mut end_attrs, "width", 80);
        int_attr(&mut end_attrs, "height", 32);
        let mut conn_attrs = IndexMap::new();
        string_attr(&mut conn_attrs, "from", "start");
        string_attr(&mut conn_attrs, "to", "end");
        string_attr(&mut conn_attrs, "direction", "to");
        let mut flow_attrs = IndexMap::new();
        int_attr(&mut flow_attrs, "width", 180);
        int_attr(&mut flow_attrs, "height", 150);

        let flow = block(
            "wdoc::draw::flowchart",
            Some("flow"),
            flow_attrs,
            vec![
                block(
                    "wdoc::draw::flow_process",
                    Some("start"),
                    start_attrs,
                    vec![],
                ),
                block("wdoc::draw::flow_process", Some("end"), end_attrs, vec![]),
                block("wdoc::draw::connection", Some("step"), conn_attrs, vec![]),
            ],
        );

        let mut shapes = Vec::new();
        let mut connections = Vec::new();
        collect_shape_or_connection(&flow, &mut shapes, &mut connections, &ctx, 0);

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].from_id, "flow.start");
        assert_eq!(connections[0].to_id, "flow.end");
        assert_eq!(
            shapes[0].attrs.get("align").map(String::as_str),
            Some("layered")
        );
        assert_eq!(shapes[0].attrs.get("gap").map(String::as_str), Some("48"));
        assert_eq!(
            shapes[0].attrs.get("_wdoc_content_top").map(String::as_str),
            Some("36")
        );

        let mut diagram = crate::shapes::Diagram {
            id: None,
            width: 220.0,
            height: 190.0,
            padding: 0.0,
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes,
            connections,
            classes: IndexMap::new(),
        };
        crate::shapes::render_diagram_svg(&mut diagram);
        let flow_children = &diagram.shapes[0].children;
        let start = flow_children
            .iter()
            .find(|child| child.id.as_deref() == Some("start"))
            .expect("start child should exist");
        let end = flow_children
            .iter()
            .find(|child| child.id.as_deref() == Some("end"))
            .expect("end child should exist");
        assert!(start.resolved.y < end.resolved.y);
        assert!(start.resolved.y >= 36.0);
    }

    #[test]
    fn flowchart_widget_preserves_explicit_layout_overrides_and_dotted_targets() {
        let ctx = wdoc_library_ctx();

        let mut start_attrs = IndexMap::new();
        int_attr(&mut start_attrs, "width", 80);
        int_attr(&mut start_attrs, "height", 32);
        let mut inner_attrs = IndexMap::new();
        int_attr(&mut inner_attrs, "width", 180);
        int_attr(&mut inner_attrs, "height", 140);
        string_attr(&mut inner_attrs, "align", "grid");
        int_attr(&mut inner_attrs, "gap", 12);
        int_attr(&mut inner_attrs, "content_top", 8);
        let mut inner_start_attrs = IndexMap::new();
        int_attr(&mut inner_start_attrs, "width", 80);
        int_attr(&mut inner_start_attrs, "height", 32);
        let mut inner_end_attrs = IndexMap::new();
        int_attr(&mut inner_end_attrs, "width", 80);
        int_attr(&mut inner_end_attrs, "height", 32);
        let mut outer_conn_attrs = IndexMap::new();
        string_attr(&mut outer_conn_attrs, "from", "start");
        string_attr(&mut outer_conn_attrs, "to", "inner.inner_start");
        string_attr(&mut outer_conn_attrs, "direction", "to");

        let inner = block(
            "wdoc::draw::flowchart",
            Some("inner"),
            inner_attrs,
            vec![
                block(
                    "wdoc::draw::flow_process",
                    Some("inner_start"),
                    inner_start_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::flow_process",
                    Some("inner_end"),
                    inner_end_attrs,
                    vec![],
                ),
            ],
        );
        let outer = block(
            "wdoc::draw::flowchart",
            Some("outer"),
            IndexMap::new(),
            vec![
                block(
                    "wdoc::draw::flow_process",
                    Some("start"),
                    start_attrs,
                    vec![],
                ),
                inner,
                block(
                    "wdoc::draw::connection",
                    Some("to_inner"),
                    outer_conn_attrs,
                    vec![],
                ),
            ],
        );

        let mut shapes = Vec::new();
        let mut connections = Vec::new();
        collect_shape_or_connection(&outer, &mut shapes, &mut connections, &ctx, 0);

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].from_id, "outer.start");
        assert_eq!(connections[0].to_id, "outer.inner.inner_start");
        let inner = shapes[0]
            .children
            .iter()
            .find(|child| child.id.as_deref() == Some("inner"))
            .expect("nested flowchart should be present");
        assert_eq!(inner.attrs.get("align").map(String::as_str), Some("grid"));
        assert_eq!(inner.attrs.get("gap").map(String::as_str), Some("12"));
        assert_eq!(
            inner.attrs.get("_wdoc_content_top").map(String::as_str),
            Some("8")
        );
    }

    #[test]
    fn flowchart_nodes_autosize_and_wrap_labels() {
        let ctx = wdoc_library_ctx();
        let mut attrs = IndexMap::new();
        string_attr(&mut attrs, "label", "Validate customer shipping address");
        int_attr(&mut attrs, "max_width", 150);

        let node = block("wdoc::draw::flow_process", Some("validate"), attrs, vec![]);
        let mut shapes = Vec::new();
        let mut connections = Vec::new();
        collect_shape_or_connection(&node, &mut shapes, &mut connections, &ctx, 0);

        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].width, None);
        assert_eq!(shapes[0].height, None);
        let label = shapes[0]
            .children
            .iter()
            .find(|child| child.kind == crate::shapes::ShapeKind::Text)
            .expect("label text should be present");
        assert!(label.width.unwrap() <= 150.0);
        assert!(label.height.unwrap() > 50.0);
        let label_max_width = label
            .attrs
            .get("max_width")
            .and_then(|value| value.parse::<f64>().ok())
            .expect("label max width should be numeric");
        assert!(label_max_width <= 110.0);
    }

    #[test]
    fn flowchart_nodes_preserve_explicit_dimensions() {
        let ctx = wdoc_library_ctx();
        let mut attrs = IndexMap::new();
        string_attr(
            &mut attrs,
            "label",
            "Long label that still uses manual sizing",
        );
        int_attr(&mut attrs, "width", 220);
        int_attr(&mut attrs, "height", 90);

        let node = block("wdoc::draw::flow_process", Some("manual"), attrs, vec![]);
        let mut shapes = Vec::new();
        let mut connections = Vec::new();
        collect_shape_or_connection(&node, &mut shapes, &mut connections, &ctx, 0);

        assert_eq!(shapes[0].width, Some(220.0));
        assert_eq!(shapes[0].height, Some(90.0));
        assert_eq!(shapes[0].children[0].width, Some(220.0));
        assert_eq!(shapes[0].children[0].height, Some(90.0));
    }

    #[test]
    fn descriptor_events_flow_into_template_shapes() {
        let mut event = IndexMap::new();
        string_attr(&mut event, "trigger", "hover");
        string_attr(&mut event, "state", "hovered");
        string_attr(&mut event, "mode", "while");

        let mut descriptor = IndexMap::new();
        string_attr(&mut descriptor, "kind", "group");
        string_attr(&mut descriptor, "id", "item");
        descriptor.insert("events".to_string(), Value::List(vec![Value::Map(event)]));

        let node = descriptor_to_shape_node_with_order(&Value::Map(descriptor), 0)
            .expect("descriptor should become shape");

        assert_eq!(node.events.len(), 1);
        assert_eq!(node.events[0].trigger, "hover");
        assert_eq!(node.events[0].state, "hovered");
        assert_eq!(node.events[0].mode.as_deref(), Some("while"));
        assert!(!node.attrs.contains_key("events"));
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

        let mut diagram = crate::shapes::Diagram {
            id: None,
            width: 80.0,
            height: 80.0,
            padding: 0.0,
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes: vec![
                descriptor_to_shape_node_with_order(&Value::Map(group), 0).expect("descriptor")
            ],
            connections: vec![],
            classes: IndexMap::new(),
        };

        let svg = crate::shapes::render_diagram_svg(&mut diagram);
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

        let mut diagram = crate::shapes::Diagram {
            id: None,
            width: 24.0,
            height: 24.0,
            padding: 0.0,
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes: vec![
                descriptor_to_shape_node_with_order(&Value::Map(descriptor), 0)
                    .expect("descriptor"),
            ],
            connections: vec![],
            classes: IndexMap::new(),
        };

        let svg = crate::shapes::render_diagram_svg(&mut diagram);
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

fn wdoc_render_children(value: &Value) -> Result<String, String> {
    let Value::BlockRef(block) = value else {
        return Err("wdoc::render_children() expects a block argument".into());
    };
    CURRENT_WDOC_CTX.with(|stack| {
        let Some(ctx_ptr) = stack.borrow().last().copied() else {
            return Err("wdoc::render_children() requires an active WDoc render context".into());
        };
        // The pointer is pushed by `ExtractCtx::render_block` and popped after
        // the template call returns, so it remains valid for this synchronous
        // helper invocation.
        let ctx = unsafe { &*ctx_ptr };
        Ok(render_child_content(block, ctx))
    })
}

fn wdoc_render_markup(text: &str) -> Result<String, String> {
    CURRENT_WDOC_CTX.with(|stack| {
        let Some(ctx_ptr) = stack.borrow().last().copied() else {
            return Err("wdoc::render_markup() requires an active WDoc render context".into());
        };
        // The pointer is pushed by `ExtractCtx::render_block` and popped after
        // the template call returns, so it remains valid for this synchronous
        // helper invocation.
        let ctx = unsafe { &*ctx_ptr };
        render_markup_string(text, ctx)
    })
}

fn render_markup_string(text: &str, ctx: &ExtractCtx) -> Result<String, String> {
    let mut out = String::new();
    let mut pos = 0;
    while pos < text.len() {
        let rest = &text[pos..];
        if rest.starts_with('<') {
            if let Some(end) = rest.find('>') {
                out.push_str(&rest[..=end]);
                pos += end + 1;
                continue;
            }
        }
        if let Some(stripped) = rest.strip_prefix('\\') {
            if let Some(ch) = stripped.chars().next() {
                out.push(ch);
                pos += 1 + ch.len_utf8();
                continue;
            }
        }

        let mut matched = false;
        for rule in &ctx.markup_rules {
            let Some((end, captures)) = match_markup_rule(rule, text, pos) else {
                continue;
            };
            let args = rule
                .func
                .params
                .iter()
                .map(|param| {
                    captures
                        .get(param)
                        .cloned()
                        .map(Value::String)
                        .unwrap_or(Value::Null)
                })
                .collect::<Vec<_>>();
            let value = wcl_lang::call_lambda_with_env(
                &rule.func,
                &args,
                &ctx.builtins,
                &ctx.template_helpers,
            )
            .map_err(|err| format!("in @markup formatter '{}': {err}", rule.name))?;
            out.push_str(&value_to_string(&value));
            pos = end;
            matched = true;
            break;
        }
        if matched {
            continue;
        }

        let ch = rest.chars().next().expect("non-empty string slice");
        out.push(ch);
        pos += ch.len_utf8();
    }
    Ok(out)
}

fn match_markup_rule(
    rule: &MarkupRule,
    text: &str,
    start: usize,
) -> Option<(usize, HashMap<String, String>)> {
    let mut pos = start;
    let mut captures = HashMap::new();
    for (idx, part) in rule.parts.iter().enumerate() {
        match part {
            MarkupPatternPart::Literal(lit) => {
                if !text[pos..].starts_with(lit) {
                    return None;
                }
                pos += lit.len();
            }
            MarkupPatternPart::Capture(name) => {
                let next_lit = rule.parts[idx + 1..].iter().find_map(|part| match part {
                    MarkupPatternPart::Literal(lit) if !lit.is_empty() => Some(lit.as_str()),
                    _ => None,
                });
                let end = if let Some(next_lit) = next_lit {
                    pos + text[pos..].find(next_lit)?
                } else {
                    text.len()
                };
                if end == pos {
                    return None;
                }
                captures.insert(name.clone(), text[pos..end].to_string());
                pos = end;
            }
        }
    }
    Some((pos, captures))
}

fn render_child_content(block: &BlockRef, ctx: &ExtractCtx) -> String {
    let mut html = String::new();
    for child in all_child_blocks(block) {
        match child.kind.as_str() {
            "wdoc::layout" | "wdoc::section" | "wdoc::page" | "wdoc::doc" | "wdoc::style" => {}
            "wdoc::draw::diagram" => {
                html.push_str(&render_diagram_with_ctx(child, ctx));
                html.push('\n');
            }
            _ => {
                if let Ok(child_html) = ctx.render_block(child) {
                    html.push_str(&child_html);
                    html.push('\n');
                }
            }
        }
    }
    html
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
// Source entry points
// ---------------------------------------------------------------------------

fn setup_lib_dir() -> Result<PathBuf, String> {
    let lib_dir = std::env::temp_dir().join(format!("wdoc-lib-{}", std::process::id()));
    std::fs::create_dir_all(&lib_dir).map_err(|e| format!("failed to create wdoc lib dir: {e}"))?;
    std::fs::write(lib_dir.join("wdoc.wcl"), crate::library::WDOC_LIBRARY_WCL)
        .map_err(|e| format!("failed to write wdoc.wcl: {e}"))?;
    Ok(lib_dir)
}

fn parse_and_extract(files: &[PathBuf], options: &SourceOptions) -> Result<WdocDocument, String> {
    parse_extract_from_files(files, options).map(|extracted| extracted.document)
}

fn format_diagnostic(
    diag: &wcl_lang::Diagnostic,
    source_map: &wcl_lang::SourceMap,
    fallback_path: &Path,
) -> String {
    let code = diag
        .code
        .as_deref()
        .map(|code| format!("[{code}]"))
        .unwrap_or_default();
    let sf = source_map.get_file(diag.span.file);
    let path = if sf.path.is_empty() || sf.path == "<input>" {
        fallback_path.display().to_string()
    } else {
        sf.path.clone()
    };
    let (line, col) = sf.line_col(diag.span.start);
    format!(
        "{:?}{code}: {}\n  --> {path}:{line}:{col}",
        diag.severity, diag.message
    )
}

pub fn parse_extract_from_files(
    files: &[PathBuf],
    source_options: &SourceOptions,
) -> Result<ExtractedWdoc, String> {
    let functions = wdoc_functions();
    let lib_dir = setup_lib_dir()?;

    let mut all_values = IndexMap::new();
    let mut last_doc: Option<wcl_lang::Document> = None;
    let mut watch_paths = HashSet::new();

    for file in files {
        let source = std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

        let mut options = wcl_lang::ParseOptions {
            root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
            variables: source_options.variables.clone(),
            functions: functions.clone(),
            ..Default::default()
        };
        options.lib_paths.clone_from(&source_options.lib_paths);
        options.no_default_lib_paths = source_options.no_default_lib_paths;
        options.lib_paths.push(lib_dir.clone());

        let doc = wcl_lang::parse(&source, options);

        let errors: Vec<_> = doc.diagnostics.iter().filter(|d| d.is_error()).collect();
        if !errors.is_empty() {
            let mut msg = String::new();
            for diag in &errors {
                msg.push_str(&format_diagnostic(diag, &doc.source_map, file));
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
    let template_helpers = collect_template_helpers(&doc);
    let markup_rules = collect_markup_rules(&doc, &template_helpers)?;
    let svg_search_dirs = wdoc_source_dirs(files, &doc.imported_paths, &lib_dir);
    let ctx = ExtractCtx {
        template_map,
        template_helpers,
        markup_rules,
        builtins,
        css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
        diagram_classes: Rc::new(RefCell::new(collect_diagram_classes(&all_values))),
        svg_search_dirs,
    };

    let wdoc_doc = extract(&all_values, &ctx)?;
    let warnings = crate::validate_doc(&wdoc_doc)?;
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

pub fn build_from_files(
    files: &[PathBuf],
    output: &Path,
    options: &SourceOptions,
) -> Result<BuildResult, String> {
    let extracted = parse_extract_from_files(files, options)?;
    let doc = extracted.document;
    let pages = doc.pages.len();
    let asset_dirs: Vec<&Path> = files
        .iter()
        .filter_map(|f| f.parent())
        .chain(extracted.watch_paths.iter().filter_map(|f| f.parent()))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    crate::render_to(&doc, output, &asset_dirs)?;
    Ok(BuildResult {
        pages,
        output: output.to_path_buf(),
    })
}

/// Install the embedded wdoc standard library (`wdoc.wcl`) into the user's
/// library directory so editors, LSP, and `wcl validate` can resolve
/// `import <wdoc.wcl>` without the `wdoc` subcommand's temp-dir bootstrap.
pub fn install_library(force: bool) -> Result<PathBuf, String> {
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
    std::fs::write(&target, crate::library::WDOC_LIBRARY_WCL)
        .map_err(|e| format!("failed to write {}: {e}", target.display()))?;
    Ok(target)
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

pub fn validate_from_files(
    files: &[PathBuf],
    options: &SourceOptions,
) -> Result<ValidationResult, String> {
    let doc = parse_and_extract(files, options)?;
    Ok(ValidationResult {
        sections: count_sections(&doc.sections),
        pages: doc.pages.len(),
    })
}

pub fn serve_from_files(
    files: &[PathBuf],
    port: u16,
    open: bool,
    options: &SourceOptions,
) -> Result<(), String> {
    let files = files.to_vec();
    let options = options.clone();

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
        let extracted = parse_extract_from_files(&files, &options)?;
        Ok(crate::serve::ServeBuild {
            document: extracted.document,
            watch_paths: extracted.watch_paths.into_iter().collect(),
        })
    };

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

    rt.block_on(crate::serve::serve(
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
