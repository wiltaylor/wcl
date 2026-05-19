use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast;
use crate::wdoc::model::*;
use crate::{
    BlockRef, BuiltinFn, FileId, FunctionRegistry, FunctionSignature, FunctionValue, Value,
};

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
fn collect_template_map(doc: &crate::Document) -> HashMap<(String, String), String> {
    let mut map = HashMap::new();
    for item in &doc.ast.items {
        if let ast::DocItem::Body(ast::BodyItem::Schema(schema)) = item {
            let schema_name = schema_name_literal(schema);

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

/// Map from schema_name → base_schema_name, built from AST @extends decorators.
fn collect_template_extends_map(doc: &crate::Document) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for item in &doc.ast.items {
        if let ast::DocItem::Body(ast::BodyItem::Schema(schema)) = item {
            let schema_name = schema_name_literal(schema);

            for dec in &schema.decorators {
                if dec.name.name == "extends" && !dec.args.is_empty() {
                    if let Some(base_name) = extract_string_arg(&dec.args[0]) {
                        map.insert(schema_name.clone(), base_name);
                    }
                }
            }
        }
    }
    map
}

fn collect_draw_schema_names(doc: &crate::Document) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &doc.ast.items {
        if let ast::DocItem::Body(ast::BodyItem::Schema(schema)) = item {
            let schema_name = schema_name_literal(schema);
            if schema_name.starts_with("wdoc::draw::") {
                names.insert(schema_name);
            }
        }
    }
    names
}

fn collect_structural_shape_schema_names(doc: &crate::Document) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &doc.ast.items {
        if let ast::DocItem::Body(ast::BodyItem::Schema(schema)) = item {
            if schema
                .decorators
                .iter()
                .any(|dec| dec.name.name == "structural")
            {
                names.insert(schema_name_literal(schema));
            }
        }
    }
    names
}

fn schema_name_literal(schema: &ast::Schema) -> String {
    schema
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
        .collect::<String>()
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

pub(crate) fn collect_template_helpers(doc: &crate::Document) -> HashMap<String, FunctionValue> {
    doc.values
        .iter()
        .filter_map(|(name, value)| match value {
            Value::Function(func) => Some((name.clone(), func.clone())),
            _ => None,
        })
        .collect()
}

fn collect_layout_helpers(doc: &crate::Document) -> Result<HashMap<String, FunctionValue>, String> {
    let mut helpers: HashMap<String, (String, FunctionValue)> = HashMap::new();
    for (fn_name, value) in &doc.values {
        let Value::Function(func) = value else {
            continue;
        };
        for decorator in &func.decorators {
            if decorator.name != "layout" {
                continue;
            }
            let Some(layout_name) = decorator
                .args
                .get("_0")
                .or_else(|| decorator.args.values().next())
                .and_then(|value| value.as_string())
                .map(str::to_string)
            else {
                continue;
            };
            if let Some((existing_name, _)) =
                helpers.insert(layout_name.clone(), (fn_name.clone(), func.clone()))
            {
                return Err(format!(
                    "duplicate @layout(\"{layout_name}\") definition on '{fn_name}' and '{existing_name}'; layout names must be unique"
                ));
            }
        }
    }
    Ok(helpers
        .into_iter()
        .map(|(layout_name, (_, func))| (layout_name, func))
        .collect())
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
    doc: &crate::Document,
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

pub fn wdoc_functions() -> FunctionRegistry {
    let mut reg = FunctionRegistry::new();
    let measure_text = std::sync::Arc::new(|args: &[Value]| {
        if args.len() != 1 {
            return Err("measure_text() expects 1 argument (text attributes or text block)".into());
        }
        let attrs = value_map_to_string_map(args.first())?;
        let metrics = crate::wdoc::shapes::measure_text_attrs(&attrs);
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

/// Parse options for editor/LSP tooling that should understand WDoc files.
///
/// This registers WDoc host functions and exposes the embedded `wdoc.wcl`
/// library, so `import <wdoc.wcl>` works without a separate install step.
pub fn lsp_parse_options() -> Result<crate::ParseOptions, String> {
    let options = crate::ParseOptions {
        functions: wdoc_functions(),
        ..Default::default()
    };
    Ok(options)
}

#[cfg(test)]
mod lsp_parse_options_tests {
    use super::*;

    #[test]
    fn lsp_parse_options_resolve_embedded_wdoc_library_and_icons() {
        let options = lsp_parse_options().unwrap();
        let doc = crate::parse(
            "import <wdoc.wcl>\nuse wdoc::{icon}\nlet x = icon(\"home\")",
            options,
        );
        assert!(
            doc.diagnostics.iter().all(|d| !d.is_error()),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );
    }
}

fn register_renderer_helpers(reg: &mut FunctionRegistry) {
    for (name, align) in [
        ("wdoc::layout_stack", crate::wdoc::shapes::Alignment::Stack),
        ("wdoc::layout_flow", crate::wdoc::shapes::Alignment::Flow),
        (
            "wdoc::layout_center",
            crate::wdoc::shapes::Alignment::Center,
        ),
        ("wdoc::layout_grid", crate::wdoc::shapes::Alignment::Grid),
        (
            "wdoc::layout_layered",
            crate::wdoc::shapes::Alignment::Layered,
        ),
        ("wdoc::layout_force", crate::wdoc::shapes::Alignment::Force),
        (
            "wdoc::layout_radial",
            crate::wdoc::shapes::Alignment::Radial,
        ),
    ] {
        let builtin_name = name.to_string();
        reg.register(
            name,
            std::sync::Arc::new(move |args: &[Value]| wdoc_layout_helper(align, args)) as BuiltinFn,
            FunctionSignature {
                name: builtin_name,
                params: vec!["ctx: map".into()],
                return_type: "map".into(),
                doc: "Resolve a WDoc diagram layout request".into(),
            },
        );
    }

    reg.register(
        "wdoc::route_connections",
        std::sync::Arc::new(wdoc_route_connections_helper) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::route_connections".into(),
            params: vec!["ctx: map".into()],
            return_type: "list(map)".into(),
            doc: "Route WDoc diagram connections for resolved shapes".into(),
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

    let block_children = std::sync::Arc::new(|args: &[Value]| {
        if args.len() != 1 {
            return Err("block_children() expects 1 argument".into());
        }
        let Value::BlockRef(block) = &args[0] else {
            return Err("block_children() expects a block argument".into());
        };
        Ok(Value::List(
            all_child_blocks(block)
                .into_iter()
                .cloned()
                .map(Value::BlockRef)
                .collect(),
        ))
    }) as BuiltinFn;
    let block_children_sig = |name: &str| FunctionSignature {
        name: name.into(),
        params: vec!["block: any".into()],
        return_type: "list(any)".into(),
        doc: "Return all direct child blocks, including named child blocks".into(),
    };
    reg.register(
        "block_children",
        block_children.clone(),
        block_children_sig("block_children"),
    );
    reg.register(
        "wdoc::block_children",
        block_children,
        block_children_sig("wdoc::block_children"),
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

    register_icon_helper(
        reg,
        "wdoc::icon",
        "Render a named SVG icon from the active WDoc icon set",
        |args| {
            if args.len() != 1 {
                return Err("wdoc::icon() expects 1 argument".into());
            }
            Ok(icon_placeholder(
                &value_to_string(&args[0]),
                "1em",
                None,
                IndexMap::new(),
            ))
        },
    );
    register_icon_helper(reg, "icon", "Render a named SVG icon", |args| {
        if args.len() != 1 {
            return Err("icon() expects 1 argument".into());
        }
        Ok(icon_placeholder(
            &value_to_string(&args[0]),
            "1em",
            None,
            IndexMap::new(),
        ))
    });
    register_icon_helper(
        reg,
        "wdoc::icon_styled",
        "Render a named SVG icon with size and colour",
        |args| {
            if args.len() != 3 {
                return Err("wdoc::icon_styled() expects 3 arguments".into());
            }
            let mut props = IndexMap::new();
            props.insert("fill".to_string(), value_to_string(&args[2]));
            Ok(icon_placeholder(
                &value_to_string(&args[0]),
                &value_to_string(&args[1]),
                None,
                props,
            ))
        },
    );
    register_icon_helper(reg, "icon_styled", "Render a styled SVG icon", |args| {
        if args.len() != 3 {
            return Err("icon_styled() expects 3 arguments".into());
        }
        let mut props = IndexMap::new();
        props.insert("fill".to_string(), value_to_string(&args[2]));
        Ok(icon_placeholder(
            &value_to_string(&args[0]),
            &value_to_string(&args[1]),
            None,
            props,
        ))
    });
    register_icon_helper(
        reg,
        "wdoc::icon_props",
        "Render a named SVG icon with CSS variable properties",
        |args| {
            if args.len() != 3 {
                return Err("wdoc::icon_props() expects 3 arguments".into());
            }
            Ok(icon_placeholder(
                &value_to_string(&args[0]),
                &value_to_string(&args[1]),
                None,
                value_to_string_props(args.get(2)),
            ))
        },
    );
    register_icon_helper(
        reg,
        "icon_props",
        "Render an SVG icon with properties",
        |args| {
            if args.len() != 3 {
                return Err("icon_props() expects 3 arguments".into());
            }
            Ok(icon_placeholder(
                &value_to_string(&args[0]),
                &value_to_string(&args[1]),
                None,
                value_to_string_props(args.get(2)),
            ))
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

fn register_icon_helper<F>(reg: &mut FunctionRegistry, name: &str, doc: &str, func: F)
where
    F: Fn(&[Value]) -> Result<String, String> + Send + Sync + 'static,
{
    let function_name = name.to_string();
    reg.register(
        name,
        std::sync::Arc::new(move |args: &[Value]| func(args).map(Value::String)) as BuiltinFn,
        FunctionSignature {
            name: function_name,
            params: vec!["name: string".into()],
            return_type: "string".into(),
            doc: doc.into(),
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

fn value_to_string_props(value: Option<&Value>) -> IndexMap<String, String> {
    let Some(Value::Map(map)) = value else {
        return IndexMap::new();
    };
    map.iter()
        .filter_map(|(key, value)| {
            if key.trim().is_empty() {
                None
            } else {
                Some((key.clone(), value_to_string(value)))
            }
        })
        .collect()
}

fn icon_placeholder(
    name: &str,
    size: &str,
    set: Option<&str>,
    props: IndexMap<String, String>,
) -> String {
    let mut attrs = IndexMap::new();
    attrs.insert("tag".to_string(), crate::wdoc::markup::s("span"));
    attrs.insert("data_wdoc_icon".to_string(), crate::wdoc::markup::s("true"));
    attrs.insert("data_name".to_string(), crate::wdoc::markup::s(name));
    attrs.insert("data_size".to_string(), crate::wdoc::markup::s(size));
    if let Some(set) = set.filter(|set| !set.trim().is_empty()) {
        attrs.insert("data_set".to_string(), crate::wdoc::markup::s(set));
    }
    if !props.is_empty() {
        let encoded = props
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    url_component_escape(key),
                    url_component_escape(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        attrs.insert("data_props".to_string(), crate::wdoc::markup::s(encoded));
    }
    crate::wdoc::markup::render_html(&Value::Map(attrs))
        .expect("wdoc icon placeholder should serialize as HTML")
}

fn url_component_escape(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn url_component_unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
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
fn render_diagram_with_ctx(br: &BlockRef, ctx: &ExtractCtx) -> Result<String, String> {
    use crate::wdoc::shapes::*;

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
    let connected_ports = diagram_connected_ports(br);
    let dopesheets = diagram_dopesheets(br);

    let mut source_order = 0;
    for val in br.attributes.values() {
        if let Value::BlockRef(child) = val {
            if let Some(annotated) = block_with_connected_ports(child, &connected_ports) {
                collect_shape_or_connection(
                    &annotated,
                    &mut shapes,
                    &mut connections,
                    ctx,
                    source_order,
                    &dopesheets,
                );
            } else {
                collect_shape_or_connection(
                    child,
                    &mut shapes,
                    &mut connections,
                    ctx,
                    source_order,
                    &dopesheets,
                );
            }
            source_order += 1;
        }
    }
    for child in &br.children {
        if let Some(annotated) = block_with_connected_ports(child, &connected_ports) {
            collect_shape_or_connection(
                &annotated,
                &mut shapes,
                &mut connections,
                ctx,
                source_order,
                &dopesheets,
            );
        } else {
            collect_shape_or_connection(
                child,
                &mut shapes,
                &mut connections,
                ctx,
                source_order,
                &dopesheets,
            );
        }
        source_order += 1;
    }

    let mut diagram = Diagram {
        id: br.id.clone(),
        width: diagram_w,
        height: diagram_h,
        shapes,
        connections,
        classes: diagram_classes_for_file(ctx, br.span.file),
        padding,
        align,
        gap,
        options: str_attrs,
    };

    apply_wcl_layouts(&mut diagram, ctx)?;
    Ok(crate::wdoc::shapes::render_resolved_diagram_svg(
        &mut diagram,
    ))
}

fn apply_wcl_layouts(
    diagram: &mut crate::wdoc::shapes::Diagram,
    ctx: &ExtractCtx,
) -> Result<(), String> {
    crate::wdoc::shapes::resolve_diagram_layout_with(diagram, |children, connections, request| {
        let layout_name = crate::wdoc::shapes::alignment_name(request.align);
        let Some(func) = ctx.layout_helpers.get(layout_name) else {
            return Err(format!(
                "no @layout(\"{layout_name}\") registered for WDoc diagram layout"
            ));
        };
        let request_value = layout_request_to_value(children, connections, request);
        let result = crate::call_lambda_with_env(
            func,
            &[request_value],
            &ctx.builtins,
            &ctx.template_helpers,
        )?;
        apply_layout_result(children, request.indices, result)
    })
}

fn wdoc_layout_helper(
    align: crate::wdoc::shapes::Alignment,
    args: &[Value],
) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("wdoc layout helper expects 1 argument".into());
    }
    let Value::Map(ctx) = &args[0] else {
        return Err("wdoc layout helper argument must be a map".into());
    };
    let mut children = value_shapes(ctx.get("shapes"))?;
    let mut connections = value_connections(ctx.get("connections"))?;
    let indices: Vec<usize> = (0..children.len()).collect();
    let parent = value_bounds(ctx.get("parent")).unwrap_or_default();
    let gap = ctx.get("gap").and_then(value_as_f64).unwrap_or(0.0);
    let options = value_string_map(ctx.get("options"));
    crate::wdoc::shapes::prepare_layout_inputs(&mut children, &connections, &parent);
    crate::wdoc::shapes::apply_builtin_layout(
        &mut children,
        &mut connections,
        crate::wdoc::shapes::LayoutRequest {
            indices: &indices,
            parent: &parent,
            scope_path: ctx
                .get("scope_path")
                .and_then(|value| value.as_string())
                .unwrap_or(""),
            align,
            gap,
            options: &options,
        },
    );
    let mut result = IndexMap::new();
    result.insert(
        "shapes".to_string(),
        Value::List(children.iter().map(shape_node_to_value).collect()),
    );
    result.insert(
        "connections".to_string(),
        Value::List(connections.iter().map(connection_to_value).collect()),
    );
    Ok(Value::Map(result))
}

fn wdoc_route_connections_helper(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("wdoc::route_connections() expects 1 argument".into());
    }
    let Value::Map(ctx) = &args[0] else {
        return Err("wdoc::route_connections() argument must be a map".into());
    };
    let shapes = value_shapes(ctx.get("shapes"))?;
    let connections = value_connections(ctx.get("connections"))?;
    Ok(Value::List(
        crate::wdoc::shapes::route_connection_infos(&shapes, &connections)
            .into_iter()
            .map(|route| {
                let mut map = IndexMap::new();
                map.insert("from".to_string(), Value::String(route.from_id));
                map.insert("to".to_string(), Value::String(route.to_id));
                map.insert("d".to_string(), Value::String(route.d));
                map.insert(
                    "points".to_string(),
                    Value::List(
                        route
                            .points
                            .into_iter()
                            .map(|(x, y)| Value::List(vec![Value::Float(x), Value::Float(y)]))
                            .collect(),
                    ),
                );
                map.insert("label_x".to_string(), Value::Float(route.label_x));
                map.insert("label_y".to_string(), Value::Float(route.label_y));
                Value::Map(map)
            })
            .collect(),
    ))
}

fn layout_request_to_value(
    children: &[crate::wdoc::shapes::ShapeNode],
    connections: &[crate::wdoc::shapes::Connection],
    request: crate::wdoc::shapes::LayoutRequest<'_>,
) -> Value {
    let mut map = IndexMap::new();
    map.insert(
        "layout".to_string(),
        Value::String(crate::wdoc::shapes::alignment_name(request.align).to_string()),
    );
    map.insert("gap".to_string(), Value::Float(request.gap));
    map.insert(
        "scope_path".to_string(),
        Value::String(request.scope_path.to_string()),
    );
    map.insert("parent".to_string(), bounds_to_value(request.parent));
    map.insert(
        "options".to_string(),
        Value::Map(
            request
                .options
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                .collect(),
        ),
    );
    map.insert(
        "shapes".to_string(),
        Value::List(
            request
                .indices
                .iter()
                .filter_map(|idx| children.get(*idx))
                .map(shape_node_to_value)
                .collect(),
        ),
    );
    map.insert(
        "connections".to_string(),
        Value::List(connections.iter().map(connection_to_value).collect()),
    );
    Value::Map(map)
}

fn bounds_to_value(bounds: &crate::wdoc::shapes::Bounds) -> Value {
    Value::Map(IndexMap::from([
        ("x".to_string(), Value::Float(bounds.x)),
        ("y".to_string(), Value::Float(bounds.y)),
        ("width".to_string(), Value::Float(bounds.width)),
        ("height".to_string(), Value::Float(bounds.height)),
    ]))
}

fn shape_node_to_value(node: &crate::wdoc::shapes::ShapeNode) -> Value {
    let mut map: IndexMap<String, Value> = node
        .attrs
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect();
    map.insert("kind".to_string(), Value::String(node.kind_name.clone()));
    if let Some(id) = &node.id {
        map.insert("id".to_string(), Value::String(id.clone()));
    }
    map.insert("x".to_string(), Value::Float(node.resolved.x));
    map.insert("y".to_string(), Value::Float(node.resolved.y));
    map.insert("width".to_string(), Value::Float(node.resolved.width));
    map.insert("height".to_string(), Value::Float(node.resolved.height));
    map.insert(
        "align".to_string(),
        Value::String(crate::wdoc::shapes::alignment_name(node.align).to_string()),
    );
    map.insert("gap".to_string(), Value::Float(node.gap));
    map.insert("padding".to_string(), Value::Float(node.padding));
    map.insert(
        "children".to_string(),
        Value::List(node.children.iter().map(shape_node_to_value).collect()),
    );
    Value::Map(map)
}

fn connection_to_value(conn: &crate::wdoc::shapes::Connection) -> Value {
    let mut map: IndexMap<String, Value> = conn
        .attrs
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect();
    map.insert("kind".to_string(), Value::String("connection".to_string()));
    map.insert("from".to_string(), Value::String(conn.from_id.clone()));
    map.insert("to".to_string(), Value::String(conn.to_id.clone()));
    if let Some(label) = &conn.label {
        map.insert("label".to_string(), Value::String(label.clone()));
    }
    Value::Map(map)
}

fn value_shapes(value: Option<&Value>) -> Result<Vec<crate::wdoc::shapes::ShapeNode>, String> {
    let Some(Value::List(items)) = value else {
        return Ok(Vec::new());
    };
    let mut shapes = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        let Some((mut shape, _)) =
            descriptor_to_shape_node_and_connections(item, idx, None, None, &HashMap::new())
        else {
            return Err("layout shape descriptors must be maps with a kind".into());
        };
        shape.resolved.x = shape.x.unwrap_or(shape.resolved.x);
        shape.resolved.y = shape.y.unwrap_or(shape.resolved.y);
        shape.resolved.width = shape.width.unwrap_or(shape.resolved.width);
        shape.resolved.height = shape.height.unwrap_or(shape.resolved.height);
        shapes.push(shape);
    }
    Ok(shapes)
}

fn value_connections(
    value: Option<&Value>,
) -> Result<Vec<crate::wdoc::shapes::Connection>, String> {
    let Some(Value::List(items)) = value else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| descriptor_to_connection_with_order(item, idx))
        .collect())
}

fn value_bounds(value: Option<&Value>) -> Option<crate::wdoc::shapes::Bounds> {
    let Some(Value::Map(map)) = value else {
        return None;
    };
    Some(crate::wdoc::shapes::Bounds {
        x: map.get("x").and_then(value_as_f64).unwrap_or(0.0),
        y: map.get("y").and_then(value_as_f64).unwrap_or(0.0),
        width: map.get("width").and_then(value_as_f64).unwrap_or(0.0),
        height: map.get("height").and_then(value_as_f64).unwrap_or(0.0),
    })
}

fn value_string_map(value: Option<&Value>) -> IndexMap<String, String> {
    let Some(Value::Map(map)) = value else {
        return IndexMap::new();
    };
    map.iter()
        .map(|(key, value)| (key.clone(), value_to_string(value)))
        .collect()
}

fn apply_layout_result(
    children: &mut [crate::wdoc::shapes::ShapeNode],
    indices: &[usize],
    result: Value,
) -> Result<(), String> {
    let shapes = match result {
        Value::Map(mut map) => map.shift_remove("shapes").unwrap_or(Value::Null),
        Value::List(_) => result,
        Value::Null => return Ok(()),
        other => {
            return Err(format!(
                "@layout lambda must return a map or list, got {}",
                other.type_name()
            ))
        }
    };
    let Value::List(items) = shapes else {
        return Ok(());
    };
    for (position, item) in items.iter().enumerate() {
        let Some(child_index) = indices.get(position).copied() else {
            break;
        };
        let Some(child) = children.get_mut(child_index) else {
            continue;
        };
        apply_shape_layout_value(child, item)?;
    }
    Ok(())
}

fn apply_shape_layout_value(
    node: &mut crate::wdoc::shapes::ShapeNode,
    value: &Value,
) -> Result<(), String> {
    let Value::Map(map) = value else {
        return Err("@layout shape results must be maps".into());
    };
    if let Some(x) = map.get("x").and_then(value_as_f64) {
        node.resolved.x = x;
    }
    if let Some(y) = map.get("y").and_then(value_as_f64) {
        node.resolved.y = y;
    }
    if let Some(width) = map.get("width").and_then(value_as_f64) {
        node.resolved.width = width;
    }
    if let Some(height) = map.get("height").and_then(value_as_f64) {
        node.resolved.height = height;
    }
    for (key, value) in map {
        if matches!(
            key.as_str(),
            "kind" | "id" | "x" | "y" | "width" | "height" | "children"
        ) {
            continue;
        }
        match value {
            Value::String(s) => {
                node.attrs.insert(key.clone(), s.clone());
            }
            Value::Int(i) => {
                node.attrs.insert(key.clone(), i.to_string());
            }
            Value::Float(f) => {
                node.attrs.insert(key.clone(), f.to_string());
            }
            Value::Bool(b) => {
                node.attrs.insert(key.clone(), b.to_string());
            }
            Value::Null => {}
            _ => {}
        }
    }
    if let Some(Value::List(items)) = map.get("children") {
        for (idx, item) in items.iter().enumerate() {
            if let Some(child) = node.children.get_mut(idx) {
                apply_shape_layout_value(child, item)?;
            }
        }
    }
    Ok(())
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

fn diagram_connected_ports(diagram: &BlockRef) -> HashMap<String, Vec<String>> {
    let mut shape_ids = HashSet::new();
    for val in diagram.attributes.values() {
        if let Value::BlockRef(child) = val {
            collect_block_ids(child, &mut shape_ids);
        }
    }
    for child in &diagram.children {
        collect_block_ids(child, &mut shape_ids);
    }

    let mut connected_ports: HashMap<String, Vec<String>> = HashMap::new();
    if shape_ids.is_empty() {
        return connected_ports;
    }

    for val in diagram.attributes.values() {
        if let Value::BlockRef(child) = val {
            collect_connection_port_usage(child, &shape_ids, &mut connected_ports);
        }
    }
    for child in &diagram.children {
        collect_connection_port_usage(child, &shape_ids, &mut connected_ports);
    }

    connected_ports
}

fn collect_block_ids(block: &BlockRef, ids: &mut HashSet<String>) {
    if let Some(id) = block.id.as_deref().filter(|id| !id.is_empty()) {
        ids.insert(id.to_string());
    }
    for value in block.attributes.values() {
        if let Value::BlockRef(child) = value {
            collect_block_ids(child, ids);
        }
    }
    for child in &block.children {
        collect_block_ids(child, ids);
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

fn block_with_connected_ports(
    block: &BlockRef,
    connected_ports: &HashMap<String, Vec<String>>,
) -> Option<BlockRef> {
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

fn diagram_dopesheets(diagram: &BlockRef) -> HashMap<String, IndexMap<String, String>> {
    let mut sheets = HashMap::new();
    for val in diagram.attributes.values() {
        if let Value::BlockRef(child) = val {
            collect_dopesheet(child, &mut sheets);
        }
    }
    for child in &diagram.children {
        collect_dopesheet(child, &mut sheets);
    }
    sheets
}

fn collect_dopesheet(block: &BlockRef, sheets: &mut HashMap<String, IndexMap<String, String>>) {
    if is_draw_dopesheet_block(block) {
        if let Some(id) = block.id.as_deref().filter(|id| !id.is_empty()) {
            sheets.insert(
                id.to_string(),
                value_map_to_string_map_lossy(&block.attributes),
            );
        }
        return;
    }
    for child in all_child_blocks(block) {
        collect_dopesheet(child, sheets);
    }
}

fn collect_shape_or_connection(
    br: &BlockRef,
    shapes: &mut Vec<crate::wdoc::shapes::ShapeNode>,
    connections: &mut Vec<crate::wdoc::shapes::Connection>,
    ctx: &ExtractCtx,
    source_order: usize,
    dopesheets: &HashMap<String, IndexMap<String, String>>,
) {
    use crate::wdoc::shapes::*;

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
        || is_draw_dopesheet_block(br)
        || is_draw_widget_structural_block(br, ctx)
    {
        return;
    }

    let is_composite = ctx
        .template_map
        .contains_key(&("shape".to_string(), br.kind.clone()));

    let kind = parse_shape_kind(&br.kind).or_else(|| {
        // User-defined @template("shape", ...) schemas are composite shape
        // containers even when they do not live under wdoc::draw::*.
        (is_composite || ctx.draw_schema_names.contains(&br.kind)).then_some(ShapeKind::Custom)
    });

    if let Some(kind) = kind {
        let mut a = value_map_to_string_map_lossy(&br.attributes);
        let tooltip_blocks = all_child_blocks(br)
            .into_iter()
            .filter(|child| is_draw_tooltip_block(child))
            .collect::<Vec<_>>();
        if kind == ShapeKind::Sprite
            || kind == ShapeKind::DopesheetView
            || kind == ShapeKind::Tilemap
        {
            apply_sprite_dopesheet_attrs(&mut a, dopesheets);
        }
        if kind == ShapeKind::Tilemap {
            apply_tilemap_rows_attrs(&mut a, br);
        }

        // Composite shape: any block whose schema declares @template("shape", "fn").
        // Call the function and convert its returned shape descriptors into the
        // widget container's children.
        let mut children: Vec<ShapeNode> = if is_composite {
            match dispatch_shape_template(br, ctx, dopesheets) {
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
                    || is_draw_widget_structural_block(child_br, ctx)
                {
                    continue;
                }
                collect_shape_or_connection(
                    child_br,
                    &mut children,
                    &mut child_connections,
                    ctx,
                    child_source_order,
                    dopesheets,
                );
                child_source_order += 1;
            }
        }
        for child_br in &br.children {
            if is_draw_event_block(child_br)
                || is_draw_animation_block(child_br)
                || is_draw_keyframe_block(child_br)
                || is_draw_widget_structural_block(child_br, ctx)
            {
                continue;
            }
            collect_shape_or_connection(
                child_br,
                &mut children,
                &mut child_connections,
                ctx,
                child_source_order,
                dopesheets,
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
            apply_wcl_shape_default_attrs(&mut a, br, ctx);
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
        } else if kind == ShapeKind::Icon {
            hydrate_icon_shape_attrs(&mut a, br.attributes.get("props"), ctx);
        }
        if br
            .id
            .as_deref()
            .is_some_and(|id| ctx.binding_targets.borrow().contains(id))
        {
            a.insert("_wdoc_runtime".to_string(), "true".to_string());
        }
        if !tooltip_blocks.is_empty() {
            if let Some(source_id) = br.id.as_deref() {
                let tooltip = tooltip_blocks[0];
                let tooltip_id = tooltip
                    .id
                    .as_deref()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{source_id}_tooltip"));
                a.insert("data_wdoc_tooltip_id".to_string(), tooltip_id);
                a.insert(
                    "data_wdoc_tooltip_delay_ms".to_string(),
                    tooltip_attr_string(tooltip, "delay_ms", "500"),
                );
                a.insert(
                    "data_wdoc_tooltip_offset_x".to_string(),
                    tooltip_attr_string(tooltip, "offset_x", "12"),
                );
                a.insert(
                    "data_wdoc_tooltip_offset_y".to_string(),
                    tooltip_attr_string(tooltip, "offset_y", "12"),
                );
                a.entry("tabindex".to_string())
                    .or_insert_with(|| "0".to_string());
                a.insert("_wdoc_runtime".to_string(), "true".to_string());
            }
        }

        let source_node = ShapeNode {
            kind,
            kind_name: br.kind.clone(),
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
        };
        shapes.push(source_node);

        if let Some(source_id) = br.id.as_deref() {
            for (idx, tooltip) in tooltip_blocks.iter().take(1).enumerate() {
                shapes.push(tooltip_overlay_shape(
                    source_id,
                    tooltip,
                    ctx,
                    dopesheets,
                    source_order + idx + 1,
                    z_index,
                ));
            }
        }
    }
}

fn tooltip_overlay_shape(
    source_id: &str,
    tooltip: &BlockRef,
    ctx: &ExtractCtx,
    dopesheets: &HashMap<String, IndexMap<String, String>>,
    source_order: usize,
    source_z: f64,
) -> crate::wdoc::shapes::ShapeNode {
    use crate::wdoc::shapes::*;

    let attrs = value_map_to_string_map_lossy(&tooltip.attributes);
    let width = attrs
        .get("width")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(240.0);
    let height = attrs
        .get("height")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(96.0);
    let padding = attrs
        .get("padding")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(10.0);
    let radius = attrs
        .get("radius")
        .cloned()
        .unwrap_or_else(|| "6".to_string());
    let fill = attrs
        .get("surface_fill")
        .cloned()
        .unwrap_or_else(|| "var(--color-bg)".to_string());
    let stroke = attrs
        .get("border_stroke")
        .cloned()
        .unwrap_or_else(|| "var(--color-nav-border)".to_string());
    let stroke_width = attrs
        .get("border_width")
        .cloned()
        .unwrap_or_else(|| "1".to_string());
    let id = tooltip
        .id
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{source_id}_tooltip"));

    let mut children = vec![ShapeNode {
        kind: ShapeKind::Rect,
        kind_name: "wdoc::draw::rect".to_string(),
        id: Some(format!("{id}_frame")),
        x: Some(0.0),
        y: Some(0.0),
        width: Some(width),
        height: Some(height),
        top: None,
        bottom: None,
        left: None,
        right: None,
        resolved: Bounds::default(),
        attrs: IndexMap::from([
            ("rx".to_string(), radius),
            ("fill".to_string(), fill),
            ("stroke".to_string(), stroke),
            ("stroke_width".to_string(), stroke_width),
            ("_wdoc_layout_decoration".to_string(), "true".to_string()),
            ("_wdoc_full_container".to_string(), "true".to_string()),
        ]),
        events: Vec::new(),
        children: Vec::new(),
        text_block_items: Vec::new(),
        align: Alignment::None,
        gap: 0.0,
        padding: 0.0,
        z_index: -1.0,
        source_order: 0,
    }];

    let mut tooltip_connections = Vec::new();
    let mut child_source_order = 1;
    for child in all_child_blocks(tooltip) {
        collect_shape_or_connection(
            child,
            &mut children,
            &mut tooltip_connections,
            ctx,
            child_source_order,
            dopesheets,
        );
        child_source_order += 1;
    }

    let mut overlay_attrs = IndexMap::new();
    overlay_attrs.insert("visibility".to_string(), "hidden".to_string());
    overlay_attrs.insert("pointer_events".to_string(), "none".to_string());
    overlay_attrs.insert("data_wdoc_tooltip_overlay".to_string(), "true".to_string());
    overlay_attrs.insert("data_wdoc_tooltip_for".to_string(), source_id.to_string());
    overlay_attrs.insert("_wdoc_runtime".to_string(), "true".to_string());
    if let Some(class_name) = attrs.get("class").filter(|value| !value.trim().is_empty()) {
        overlay_attrs.insert("class".to_string(), class_name.clone());
    }

    ShapeNode {
        kind: ShapeKind::Group,
        kind_name: "wdoc::draw::group".to_string(),
        id: Some(id),
        x: Some(0.0),
        y: Some(0.0),
        width: Some(width),
        height: Some(height),
        top: None,
        bottom: None,
        left: None,
        right: None,
        resolved: Bounds::default(),
        attrs: overlay_attrs,
        events: Vec::new(),
        children,
        text_block_items: Vec::new(),
        align: Alignment::Stack,
        gap: attrs
            .get("gap")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(8.0),
        padding,
        z_index: attrs
            .get("z_index")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(source_z + 10_000.0),
        source_order,
    }
}

fn tooltip_attr_string(block: &BlockRef, key: &str, default: &str) -> String {
    block
        .attributes
        .get(key)
        .and_then(shape_default_attr_value)
        .unwrap_or_else(|| default.to_string())
}

fn text_block_items_from_block(
    br: &BlockRef,
    ctx: &ExtractCtx,
) -> Vec<crate::wdoc::shapes::TextBlockItem> {
    let mut items = Vec::new();
    if let Some(content) = value_as_string(br.attributes.get("content")) {
        items.push(crate::wdoc::shapes::TextBlockItem::Paragraph {
            html: render_markup_string(content, ctx).unwrap_or_else(|_| html_escape(content)),
        });
    }

    for child in all_child_blocks(br) {
        match child.kind.as_str() {
            "wdoc::paragraph" | "paragraph" | "wdoc::p" | "p" => {
                if let Some(content) = value_as_string(child.attributes.get("content")) {
                    items.push(crate::wdoc::shapes::TextBlockItem::Paragraph {
                        html: render_markup_string(content, ctx)
                            .unwrap_or_else(|_| html_escape(content)),
                    });
                }
            }
            "wdoc::code" | "code" => {
                if let Some(content) = value_as_string(child.attributes.get("content")) {
                    items.push(crate::wdoc::shapes::TextBlockItem::Code {
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

fn text_block_items_from_descriptor(
    map: &IndexMap<String, Value>,
    ctx: Option<&ExtractCtx>,
) -> Vec<crate::wdoc::shapes::TextBlockItem> {
    let mut items = Vec::new();
    if let Some(content) = value_as_string(map.get("content")) {
        items.push(crate::wdoc::shapes::TextBlockItem::Paragraph {
            html: ctx
                .and_then(|ctx| render_markup_string(content, ctx).ok())
                .unwrap_or_else(|| html_escape(content)),
        });
    }

    if let Some(Value::List(descriptor_items)) = map.get("items") {
        for item in descriptor_items {
            let Value::Map(item_map) = item else {
                continue;
            };
            match item_map
                .get("kind")
                .and_then(|value| value.as_string())
                .unwrap_or("paragraph")
            {
                "code" | "wdoc::code" => {
                    if let Some(content) = value_as_string(item_map.get("content")) {
                        items.push(crate::wdoc::shapes::TextBlockItem::Code {
                            content: content.to_string(),
                            language: value_as_string(item_map.get("language")).map(str::to_string),
                        });
                    }
                }
                _ => {
                    if let Some(content) = value_as_string(item_map.get("content")) {
                        items.push(crate::wdoc::shapes::TextBlockItem::Paragraph {
                            html: ctx
                                .and_then(|ctx| render_markup_string(content, ctx).ok())
                                .unwrap_or_else(|| html_escape(content)),
                        });
                    }
                }
            }
        }
    }

    items
}

fn apply_sprite_dopesheet_attrs(
    attrs: &mut IndexMap<String, String>,
    dopesheets: &HashMap<String, IndexMap<String, String>>,
) {
    let Some(sheet_id) = attrs.get("sheet").cloned() else {
        return;
    };
    let Some(sheet) = dopesheets.get(&sheet_id) else {
        return;
    };
    attrs.insert("_wdoc_sheet_id".to_string(), sheet_id);
    for (src, dest) in [
        ("src", "_wdoc_sheet_src"),
        ("columns", "_wdoc_sheet_columns"),
        ("frame_width", "_wdoc_sheet_frame_width"),
        ("frame_height", "_wdoc_sheet_frame_height"),
        ("offset_x", "_wdoc_sheet_offset_x"),
        ("offset_y", "_wdoc_sheet_offset_y"),
        ("gap_x", "_wdoc_sheet_gap_x"),
        ("gap_y", "_wdoc_sheet_gap_y"),
        ("frame_count", "_wdoc_sheet_frame_count"),
        ("sheet_width", "_wdoc_sheet_width"),
        ("sheet_height", "_wdoc_sheet_height"),
    ] {
        if let Some(value) = sheet.get(src) {
            attrs.insert(dest.to_string(), value.clone());
        }
    }
    for (src, dest) in [
        ("src", "_wdoc_sheet_src"),
        ("columns", "_wdoc_sheet_columns"),
        ("frame_width", "_wdoc_sheet_frame_width"),
        ("frame_height", "_wdoc_sheet_frame_height"),
        ("offset_x", "_wdoc_sheet_offset_x"),
        ("offset_y", "_wdoc_sheet_offset_y"),
        ("gap_x", "_wdoc_sheet_gap_x"),
        ("gap_y", "_wdoc_sheet_gap_y"),
        ("frame_count", "_wdoc_sheet_frame_count"),
        ("sheet_width", "_wdoc_sheet_width"),
        ("sheet_height", "_wdoc_sheet_height"),
    ] {
        if let Some(value) = attrs.get(src).cloned() {
            attrs.insert(dest.to_string(), value);
        }
    }
}

fn apply_tilemap_rows_attrs(attrs: &mut IndexMap<String, String>, br: &BlockRef) {
    let Some((rows, columns, row_count)) = tilemap_rows_attr(br.attributes.get("rows")) else {
        return;
    };
    attrs.insert("_wdoc_tilemap_rows".to_string(), rows);
    attrs.insert("_wdoc_tilemap_columns".to_string(), columns.to_string());
    attrs.insert(
        "_wdoc_tilemap_rows_count".to_string(),
        row_count.to_string(),
    );

    let tile_w = attrs
        .get("tile_width")
        .and_then(|value| value.parse::<f64>().ok());
    let tile_h = attrs
        .get("tile_height")
        .and_then(|value| value.parse::<f64>().ok());
    let tile_render_h = attrs
        .get("tile_render_height")
        .and_then(|value| value.parse::<f64>().ok());
    let orientation = attrs
        .get("orientation")
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "orthogonal".to_string());
    if !attrs.contains_key("width") {
        if let Some(tile_w) = tile_w {
            let width = if orientation == "isometric" {
                (columns + row_count) as f64 * tile_w / 2.0
            } else {
                columns as f64 * tile_w
            };
            attrs.insert("width".to_string(), width.to_string());
        }
    }
    if !attrs.contains_key("height") {
        if let Some(tile_h) = tile_h {
            let extra_render_h = tile_render_h
                .map(|value| (value - tile_h).max(0.0))
                .unwrap_or(0.0);
            let height = if orientation == "isometric" {
                (columns + row_count) as f64 * tile_h / 2.0 + extra_render_h
            } else {
                row_count as f64 * tile_h + extra_render_h
            };
            attrs.insert("height".to_string(), height.to_string());
        }
    }
}

fn tilemap_rows_attr(value: Option<&Value>) -> Option<(String, usize, usize)> {
    let Value::List(rows) = value? else {
        return None;
    };
    let mut encoded_rows = Vec::new();
    let mut columns = 0usize;
    for row in rows {
        let Value::List(cells) = row else {
            continue;
        };
        columns = columns.max(cells.len());
        encoded_rows.push(
            cells
                .iter()
                .map(|cell| match cell {
                    Value::Int(frame) if *frame >= 0 => frame.to_string(),
                    Value::BigInt(frame) if *frame >= 0 => frame.to_string(),
                    Value::Float(frame) if *frame >= 0.0 => frame.floor().to_string(),
                    Value::Null => String::new(),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    Some((encoded_rows.join(";"), columns, rows.len()))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn html_unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

fn apply_wcl_shape_default_attrs(
    attrs: &mut IndexMap<String, String>,
    br: &BlockRef,
    ctx: &ExtractCtx,
) {
    let Some(func) = ctx.template_helpers.get("wdoc::shape_default_attrs") else {
        return;
    };

    let Ok(Value::Map(defaults)) = crate::call_lambda_with_env(
        func,
        &[Value::BlockRef(br.clone())],
        &ctx.builtins,
        &ctx.template_helpers,
    ) else {
        return;
    };

    for (key, value) in defaults {
        let Some(value) = shape_default_attr_value(&value) else {
            continue;
        };
        if let Some(edge) = key.strip_prefix("content_") {
            insert_default_content_inset(attrs, edge, value);
        } else {
            attrs.entry(key).or_insert(value);
        }
    }
}

fn shape_default_attr_value(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Int(i) => Some(i.to_string()),
        Value::Float(f) => Some(format_number(*f)),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null | Value::BlockRef(_) => None,
        _ => Some(format!("{value}")),
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

fn insert_default_content_inset(attrs: &mut IndexMap<String, String>, edge: &str, value: String) {
    let public_key = format!("content_{edge}");
    let private_key = format!("_wdoc_content_{edge}");
    let value = attrs.get(&public_key).cloned().unwrap_or(value);
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

fn hydrate_icon_shape_attrs(
    attrs: &mut IndexMap<String, String>,
    props_value: Option<&Value>,
    ctx: &ExtractCtx,
) {
    let Some(name) = attrs
        .get("name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let set = attrs
        .get("icon_set")
        .or_else(|| attrs.get("set"))
        .map(|s| s.as_str());
    let normalize_width = attrs
        .get("normalize_width")
        .and_then(|value| value.parse::<f64>().ok());
    let normalize_height = attrs
        .get("normalize_height")
        .and_then(|value| value.parse::<f64>().ok());
    let normalize_mode = attrs.get("normalize_mode").map(|s| s.as_str());
    let mut props = value_to_string_props(props_value);
    if let Some(fill) = attrs.get("fill").filter(|value| !value.trim().is_empty()) {
        props.insert("fill".to_string(), fill.clone());
    }
    if let Some(stroke) = attrs.get("stroke").filter(|value| !value.trim().is_empty()) {
        props.insert("stroke".to_string(), stroke.clone());
    }
    match resolve_icon_reference(
        &ctx.icon_registry,
        set,
        &name,
        normalize_width,
        normalize_height,
        normalize_mode,
    ) {
        Ok(icon) => {
            attrs.insert("_wdoc_icon_content".to_string(), icon.content);
            attrs.insert("_wdoc_icon_css".to_string(), icon.css);
            attrs.insert(
                "_wdoc_icon_normalize_mode".to_string(),
                icon.normalize_mode.to_string(),
            );
            if let Some(width) = icon.normalize_width {
                attrs.insert("_wdoc_icon_normalize_width".to_string(), width.to_string());
            }
            if let Some(height) = icon.normalize_height {
                attrs.insert(
                    "_wdoc_icon_normalize_height".to_string(),
                    height.to_string(),
                );
            }
            let style_vars = icon_style_vars(&props);
            if !style_vars.is_empty() {
                let mut existing = attrs.get("style").cloned().unwrap_or_default();
                if !existing.trim().is_empty() && !existing.trim_end().ends_with(';') {
                    existing.push(';');
                }
                attrs.insert("style".to_string(), format!("{existing}{style_vars}"));
            }
        }
        Err(err) => {
            attrs.insert("_wdoc_icon_missing".to_string(), name.clone());
            eprintln!("wdoc: warning: icon '{name}' could not be loaded: {err}");
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

#[derive(Clone, Default)]
struct IconRegistry {
    sets: IndexMap<String, IconSet>,
    default_set: Option<String>,
}

#[derive(Clone)]
struct IconSet {
    dir: PathBuf,
    normalize_width: Option<f64>,
    normalize_height: Option<f64>,
    normalize_mode: String,
    parts: IndexMap<String, IconPart>,
}

#[derive(Clone)]
struct IconPart {
    selector: String,
    property: String,
    default: Option<String>,
}

struct ResolvedIcon {
    name: String,
    content: String,
    normalize_width: Option<f64>,
    normalize_height: Option<f64>,
    normalize_mode: String,
    css: String,
}

fn collect_icon_sets(
    values: &IndexMap<String, Value>,
    search_dirs: &[PathBuf],
) -> Result<IconRegistry, String> {
    let mut registry = IconRegistry::default();
    for value in values.values() {
        if let Value::BlockRef(block) = value {
            collect_icon_sets_in_block(block, search_dirs, &mut registry)?;
        }
    }
    Ok(registry)
}

fn collect_icon_sets_in_block(
    block: &BlockRef,
    search_dirs: &[PathBuf],
    registry: &mut IconRegistry,
) -> Result<(), String> {
    if block.kind == "wdoc::icon_set" {
        let Some(id) = block.id.as_deref().filter(|id| !id.trim().is_empty()) else {
            return Err("wdoc::icon_set requires an id".to_string());
        };
        let path = value_as_string(block.attributes.get("path"))
            .ok_or_else(|| format!("icon_set '{id}' requires a path"))?;
        let dir = resolve_local_icon_dir(path, search_dirs)
            .map_err(|err| format!("icon_set '{id}' path '{path}' is invalid: {err}"))?;
        let normalize_width = val_f64(block.attributes.get("normalize_width"));
        let normalize_height = val_f64(block.attributes.get("normalize_height"));
        let normalize_mode = value_as_string(block.attributes.get("normalize_mode"))
            .unwrap_or("viewbox")
            .to_string();
        let mut parts = IndexMap::new();
        for child in all_child_blocks(block) {
            if child.kind != "wdoc::icon_part" {
                continue;
            }
            let Some(name) = child.id.as_deref().filter(|name| !name.trim().is_empty()) else {
                continue;
            };
            let Some(selector) = value_as_string(child.attributes.get("selector")) else {
                continue;
            };
            let Some(property) = value_as_string(child.attributes.get("property")) else {
                continue;
            };
            if !is_safe_css_selector(selector) || !is_safe_css_property(property) {
                continue;
            }
            parts.insert(
                name.to_string(),
                IconPart {
                    selector: selector.to_string(),
                    property: property.to_string(),
                    default: value_as_string(child.attributes.get("default")).map(str::to_string),
                },
            );
        }
        if value_as_bool(block.attributes.get("default")).unwrap_or(false) {
            registry.default_set = Some(id.to_string());
        }
        if registry.default_set.is_none() {
            registry.default_set = Some(id.to_string());
        }
        registry.sets.insert(
            id.to_string(),
            IconSet {
                dir,
                normalize_width,
                normalize_height,
                normalize_mode,
                parts,
            },
        );
        return Ok(());
    }
    for child in all_child_blocks(block) {
        collect_icon_sets_in_block(child, search_dirs, registry)?;
    }
    Ok(())
}

fn resolve_local_icon_dir(path: &str, search_dirs: &[PathBuf]) -> Result<PathBuf, String> {
    let lower = path.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("data:") {
        return Err("remote and data URLs are not supported".to_string());
    }
    let canonical_dirs: Vec<PathBuf> = search_dirs
        .iter()
        .filter_map(|dir| dir.canonicalize().ok())
        .collect();
    if canonical_dirs.is_empty() {
        return Err("no source directories are available for icon lookup".to_string());
    }
    let raw_path = Path::new(path);
    if raw_path.is_absolute() {
        let canonical = raw_path
            .canonicalize()
            .map_err(|_| "directory was not found in WDoc source directories".to_string())?;
        if !canonical.is_dir() {
            return Err("path is not a directory".to_string());
        }
        if !canonical_dirs.iter().any(|dir| canonical.starts_with(dir)) {
            return Err("path escapes the WDoc source directory".to_string());
        }
        return Ok(canonical);
    }
    for dir in &canonical_dirs {
        let candidate = dir.join(raw_path);
        let Ok(canonical) = candidate.canonicalize() else {
            continue;
        };
        if !canonical.starts_with(dir) {
            return Err("path escapes the WDoc source directory".to_string());
        }
        if canonical.is_dir() {
            return Ok(canonical);
        }
    }
    Err("directory was not found in WDoc source directories".to_string())
}

fn resolve_icon_reference(
    registry: &IconRegistry,
    set_name: Option<&str>,
    icon_name: &str,
    normalize_width: Option<f64>,
    normalize_height: Option<f64>,
    normalize_mode: Option<&str>,
) -> Result<ResolvedIcon, String> {
    let (name_set, name) = split_icon_name(icon_name);
    let set_name = set_name
        .filter(|set| !set.trim().is_empty())
        .or(name_set)
        .or(registry.default_set.as_deref())
        .ok_or_else(|| "no icon_set is configured".to_string())?;
    if !is_safe_icon_name(name) {
        return Err(format!("invalid icon name '{name}'"));
    }
    let set = registry
        .sets
        .get(set_name)
        .ok_or_else(|| format!("icon_set '{set_name}' is not configured"))?;
    let candidate = set.dir.join(format!("{name}.svg"));
    let canonical = candidate
        .canonicalize()
        .map_err(|_| format!("icon '{name}' was not found in icon_set '{set_name}'"))?;
    if !canonical.starts_with(&set.dir) {
        return Err("icon path escapes its icon_set directory".to_string());
    }
    let content = std::fs::read_to_string(&canonical)
        .map_err(|e| format!("failed to read {}: {e}", canonical.display()))?;
    Ok(ResolvedIcon {
        name: name.to_string(),
        content,
        normalize_width: normalize_width.or(set.normalize_width),
        normalize_height: normalize_height.or(set.normalize_height),
        normalize_mode: normalize_mode.unwrap_or(&set.normalize_mode).to_string(),
        css: icon_css(&set.parts),
    })
}

fn split_icon_name(name: &str) -> (Option<&str>, &str) {
    if let Some((set, icon)) = name.split_once(':') {
        (Some(set.trim()), icon.trim())
    } else {
        (None, name.trim())
    }
}

fn is_safe_icon_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn icon_css(parts: &IndexMap<String, IconPart>) -> String {
    let mut css = String::from(
        ":where(path,rect,circle,ellipse,polygon,polyline,line,text,tspan):not([fill]){fill:var(--wdoc-icon-fill,currentColor);}:where(path,rect,circle,ellipse,polygon,polyline,line,text,tspan):not([stroke]){stroke:var(--wdoc-icon-stroke,none);}",
    );
    for (name, part) in parts {
        let var = css_var_suffix(name);
        let default = part
            .default
            .as_deref()
            .filter(|value| is_safe_css_value(value))
            .unwrap_or("currentColor");
        css.push_str(&part.selector);
        css.push('{');
        css.push_str(&part.property);
        css.push_str(":var(--wdoc-icon-");
        css.push_str(&var);
        css.push(',');
        css.push_str(default);
        css.push_str(");}");
    }
    css
}

fn css_var_suffix(name: &str) -> String {
    name.chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch == '-' || ch == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
}

fn is_safe_css_selector(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(|ch| matches!(ch, '<' | '{' | '}' | ';'))
}

fn is_safe_css_property(value: &str) -> bool {
    !value.trim().is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphabetic() || ch == '-')
}

fn is_safe_css_value(value: &str) -> bool {
    !value
        .chars()
        .any(|ch| matches!(ch, '<' | '>' | '{' | '}' | ';'))
}

#[derive(Debug)]
struct ShapeTemplateResult {
    shapes: Vec<crate::wdoc::shapes::ShapeNode>,
    connections: Vec<crate::wdoc::shapes::Connection>,
}

/// Look up a `@template("shape", "fn")` function for `br.kind` and call it.
/// Plain shape templates receive the BlockRef as their single argument.
/// Templates with `@extends` receive `(BlockRef, base_descriptors)`.
fn dispatch_shape_template(
    br: &BlockRef,
    ctx: &ExtractCtx,
    dopesheets: &HashMap<String, IndexMap<String, String>>,
) -> Result<ShapeTemplateResult, String> {
    let descriptors = dispatch_shape_template_descriptors(br, ctx, &mut Vec::new())?;

    let mut result = ShapeTemplateResult {
        shapes: Vec::new(),
        connections: Vec::new(),
    };
    for (idx, desc) in descriptors.iter().enumerate() {
        if let Some(conn) = descriptor_to_connection_with_order(desc, idx) {
            result.connections.push(conn);
        } else if let Some((node, connections)) = descriptor_to_shape_node_and_connections(
            desc,
            idx,
            Some(&ctx.draw_schema_names),
            Some(ctx),
            dopesheets,
        ) {
            result.shapes.push(node);
            result.connections.extend(connections);
        }
    }
    Ok(result)
}

fn dispatch_shape_template_descriptors(
    br: &BlockRef,
    ctx: &ExtractCtx,
    stack: &mut Vec<String>,
) -> Result<Vec<Value>, String> {
    let schema_name = br.kind.clone();
    if let Some(pos) = stack.iter().position(|kind| kind == &schema_name) {
        let mut cycle = stack[pos..].to_vec();
        cycle.push(schema_name.clone());
        return Err(format!(
            "shape template extension cycle: {}",
            cycle.join(" -> ")
        ));
    }
    stack.push(schema_name.clone());

    let fn_name = ctx
        .template_map
        .get(&("shape".to_string(), schema_name.clone()))
        .ok_or_else(|| format!("no @template(\"shape\", ...) on schema '{}'", schema_name))?;

    let func = ctx.template_helpers.get(fn_name).ok_or_else(|| {
        if ctx.builtins.contains_key(fn_name) {
            format!("shape template function '{fn_name}' must be an exported WCL function")
        } else {
            format!("shape template function '{fn_name}' not registered")
        }
    })?;

    let themed_block = apply_widget_theme_class_attrs(br, ctx);
    let result = if let Some(base_kind) = ctx.template_extends_map.get(&schema_name) {
        let mut base_block = themed_block.clone();
        base_block.kind = base_kind.clone();
        let base_descriptors = dispatch_shape_template_descriptors(&base_block, ctx, stack)?;
        crate::call_lambda_with_env(
            func,
            &[
                Value::BlockRef(themed_block),
                Value::List(base_descriptors.clone()),
            ],
            &ctx.builtins,
            &ctx.template_helpers,
        )
    } else {
        crate::call_lambda_with_env(
            func,
            &[Value::BlockRef(themed_block)],
            &ctx.builtins,
            &ctx.template_helpers,
        )
    }?;

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
    stack.pop();
    Ok(descriptors)
}

fn apply_widget_theme_class_attrs(br: &BlockRef, ctx: &ExtractCtx) -> BlockRef {
    let mut themed = br.clone();
    let class_names = widget_theme_class_names(br);
    if class_names.is_empty() {
        return themed;
    }

    let classes = diagram_classes_for_file(ctx, br.span.file);
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
) -> Option<crate::wdoc::shapes::ShapeNode> {
    descriptor_to_shape_node_and_connections(val, source_order, None, None, &HashMap::new())
        .map(|(node, _)| node)
}

fn descriptor_to_shape_node_and_connections(
    val: &Value,
    source_order: usize,
    draw_schema_names: Option<&HashSet<String>>,
    ctx: Option<&ExtractCtx>,
    dopesheets: &HashMap<String, IndexMap<String, String>>,
) -> Option<(
    crate::wdoc::shapes::ShapeNode,
    Vec<crate::wdoc::shapes::Connection>,
)> {
    use crate::wdoc::shapes::*;

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
    let is_connection = map
        .get("kind")
        .and_then(|v| v.as_string())
        .is_some_and(|kind| kind == "connection" || kind == "wdoc::draw::connection");
    let has_children = matches!(map.get("children"), Some(Value::List(items)) if !items.is_empty());
    let kind = parse_shape_kind(&qualified).or_else(|| {
        let is_declared_draw_schema =
            draw_schema_names.is_some_and(|schema_names| schema_names.contains(&qualified));
        if !is_connection && (has_children || is_declared_draw_schema) {
            Some(ShapeKind::Custom)
        } else {
            None
        }
    })?;

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
                | "items"
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
            } else if let (Value::BlockRef(child_br), Some(ctx)) = (item, ctx) {
                let mut child_shapes = Vec::new();
                let mut child_connections = Vec::new();
                collect_shape_or_connection(
                    child_br,
                    &mut child_shapes,
                    &mut child_connections,
                    ctx,
                    idx,
                    dopesheets,
                );
                children.extend(child_shapes);
                connections.extend(child_connections);
            } else if let Some((child, child_connections)) =
                descriptor_to_shape_node_and_connections(
                    item,
                    idx,
                    draw_schema_names,
                    ctx,
                    dopesheets,
                )
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
        kind_name: qualified,
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
        text_block_items: if kind == ShapeKind::TextBlock {
            text_block_items_from_descriptor(map, ctx)
        } else {
            Vec::new()
        },
        align,
        gap,
        padding,
        z_index,
        source_order,
    };

    Some((node, scope_connections(connections, id.as_deref())))
}

fn descriptor_events(map: &IndexMap<String, Value>) -> Vec<crate::wdoc::shapes::DiagramEvent> {
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
            let state = event
                .get("state")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
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
            Some(crate::wdoc::shapes::DiagramEvent {
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
                signal_actions: descriptor_signal_actions(event),
            })
        })
        .collect()
}

fn descriptor_signal_actions(
    event: &IndexMap<String, Value>,
) -> Vec<crate::wdoc::shapes::SignalAction> {
    let mut actions = Vec::new();
    if let Some(value) = event.get("set_signal") {
        match value {
            Value::Map(map) => {
                if let Some(action) = descriptor_signal_action(map) {
                    actions.push(action);
                }
            }
            Value::List(items) => {
                for item in items {
                    if let Value::Map(map) = item {
                        if let Some(action) = descriptor_signal_action(map) {
                            actions.push(action);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(Value::List(items)) = event.get("actions") {
        for item in items {
            let Value::Map(map) = item else {
                continue;
            };
            if map
                .get("kind")
                .and_then(|value| value.as_string())
                .is_some_and(|kind| kind == "set_signal")
            {
                if let Some(action) = descriptor_signal_action(map) {
                    actions.push(action);
                }
            }
        }
    }
    actions
}

fn descriptor_signal_action(
    map: &IndexMap<String, Value>,
) -> Option<crate::wdoc::shapes::SignalAction> {
    Some(crate::wdoc::shapes::SignalAction {
        signal: map.get("signal")?.as_string()?.to_string(),
        value: map
            .get("value")
            .map(crate::json::value_to_json)
            .unwrap_or(serde_json::Value::Null),
        path: map
            .get("path")
            .and_then(|value| value.as_string())
            .map(str::to_string),
    })
}

fn descriptor_to_connection_with_order(
    val: &Value,
    source_order: usize,
) -> Option<crate::wdoc::shapes::Connection> {
    use crate::wdoc::shapes::*;

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

fn mark_template_layout_decoration(node: &mut crate::wdoc::shapes::ShapeNode) {
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
    connections: Vec<crate::wdoc::shapes::Connection>,
    scope: Option<&str>,
) -> Vec<crate::wdoc::shapes::Connection> {
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
) -> IndexMap<String, crate::wdoc::shapes::DiagramClass> {
    let mut classes = IndexMap::new();
    for value in values.values() {
        if let Value::BlockRef(block) = value {
            collect_diagram_classes_in_block(block, &mut classes);
        }
    }
    classes
}

fn collect_diagram_classes_by_file(
    values: &IndexMap<String, Value>,
) -> HashMap<FileId, IndexMap<String, crate::wdoc::shapes::DiagramClass>> {
    let mut classes = HashMap::new();
    for value in values.values() {
        if let Value::BlockRef(block) = value {
            collect_diagram_classes_by_file_in_block(block, &mut classes);
        }
    }
    classes
}

fn collect_diagram_classes_by_file_in_block(
    block: &BlockRef,
    classes: &mut HashMap<FileId, IndexMap<String, crate::wdoc::shapes::DiagramClass>>,
) {
    if let Some(class) = diagram_class_from_block(block) {
        classes
            .entry(block.span.file)
            .or_default()
            .insert(class.name.clone(), class);
        return;
    }
    for child in all_child_blocks(block) {
        collect_diagram_classes_by_file_in_block(child, classes);
    }
}

fn collect_diagram_classes_in_block(
    block: &BlockRef,
    classes: &mut IndexMap<String, crate::wdoc::shapes::DiagramClass>,
) {
    if let Some(class) = diagram_class_from_block(block) {
        classes.insert(class.name.clone(), class);
        return;
    }
    for child in all_child_blocks(block) {
        collect_diagram_classes_in_block(child, classes);
    }
}

fn diagram_class_from_block(block: &BlockRef) -> Option<crate::wdoc::shapes::DiagramClass> {
    if !is_draw_class_block(block) {
        return None;
    }
    let name = block.id.clone()?;
    let mut attrs = value_map_to_string_map_lossy(&block.attributes);
    let mut states = IndexMap::new();
    let mut animations = IndexMap::new();
    for child in all_child_blocks(block) {
        if is_draw_state_block(child) {
            if let Some(state_name) = child.id.clone() {
                states.insert(
                    state_name.clone(),
                    crate::wdoc::shapes::DiagramState {
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
    Some(crate::wdoc::shapes::DiagramClass {
        name,
        attrs,
        states,
        animations,
    })
}

fn diagram_classes_for_file(
    ctx: &ExtractCtx,
    file: FileId,
) -> IndexMap<String, crate::wdoc::shapes::DiagramClass> {
    let classes_by_file = ctx.diagram_classes_by_file.borrow();
    if classes_by_file.is_empty() {
        ctx.diagram_classes.borrow().clone()
    } else {
        classes_by_file.get(&file).cloned().unwrap_or_default()
    }
}

fn collect_diagram_events(block: &BlockRef) -> Vec<crate::wdoc::shapes::DiagramEvent> {
    all_child_blocks(block)
        .into_iter()
        .filter(|child| is_draw_event_block(child))
        .filter_map(|child| {
            let trigger = child.attributes.get("trigger")?.as_string()?.to_string();
            let state = child
                .attributes
                .get("state")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
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
            Some(crate::wdoc::shapes::DiagramEvent {
                name: child.id.clone(),
                trigger,
                state,
                target,
                button,
                mode,
                duration_ms,
                prevent_default,
                guard_targets,
                signal_actions: collect_signal_actions(child),
            })
        })
        .collect()
}

fn collect_signal_actions(block: &BlockRef) -> Vec<crate::wdoc::shapes::SignalAction> {
    all_child_blocks(block)
        .into_iter()
        .filter(|child| is_draw_set_signal_block(child))
        .filter_map(|child| {
            let signal = child.attributes.get("signal")?.as_string()?.to_string();
            let value = child
                .attributes
                .get("value")
                .map(crate::json::value_to_json)
                .unwrap_or(serde_json::Value::Null);
            let path = child
                .attributes
                .get("path")
                .and_then(|value| value.as_string())
                .map(str::to_string);
            Some(crate::wdoc::shapes::SignalAction {
                signal,
                value,
                path,
            })
        })
        .collect()
}

fn parse_diagram_animation(block: &BlockRef) -> Option<crate::wdoc::shapes::DiagramAnimation> {
    let name = block.id.clone()?;
    let attrs = &block.attributes;
    let frames = attrs
        .get("frames")
        .map(value_as_i32_list)
        .unwrap_or_default();
    let frame_rate = attrs.get("frame_rate").and_then(value_as_f64);
    let mut keyframes = all_child_blocks(block)
        .into_iter()
        .filter(|child| is_draw_keyframe_block(child))
        .filter_map(parse_diagram_keyframe)
        .collect::<Vec<_>>();
    keyframes.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    if keyframes.is_empty() && frames.is_empty() {
        return None;
    }
    let duration_ms = value_as_i32(attrs.get("duration_ms")).unwrap_or_else(|| {
        frame_rate
            .filter(|rate| *rate > 0.0)
            .map(|rate| ((frames.len() as f64 / rate) * 1000.0).round() as i32)
            .filter(|duration| *duration > 0)
            .unwrap_or(1000)
    });
    Some(crate::wdoc::shapes::DiagramAnimation {
        name,
        duration_ms,
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
        frame_rate,
        frames,
    })
}

fn parse_diagram_keyframe(block: &BlockRef) -> Option<crate::wdoc::shapes::DiagramKeyframe> {
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
    Some(crate::wdoc::shapes::DiagramKeyframe {
        offset,
        x: block.attributes.get("x").and_then(value_as_f64),
        y: block.attributes.get("y").and_then(value_as_f64),
        width: block.attributes.get("width").and_then(value_as_f64),
        height: block.attributes.get("height").and_then(value_as_f64),
        rotate: block.attributes.get("rotate").and_then(value_as_f64),
        rotate_origin_x: block
            .attributes
            .get("rotate_origin_x")
            .and_then(value_as_f64),
        rotate_origin_y: block
            .attributes
            .get("rotate_origin_y")
            .and_then(value_as_f64),
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

fn value_as_i32_list(value: &Value) -> Vec<i32> {
    match value {
        Value::List(values) => values
            .iter()
            .filter_map(|value| match value {
                Value::Int(i) => Some(*i as i32),
                Value::Float(f) => Some(*f as i32),
                Value::String(s) => s.parse().ok(),
                _ => None,
            })
            .collect(),
        Value::String(s) => s
            .split(',')
            .filter_map(|part| part.trim().parse::<i32>().ok())
            .collect(),
        _ => Vec::new(),
    }
}

fn value_as_string(v: Option<&Value>) -> Option<&str> {
    v.and_then(|value| value.as_string())
}

fn value_as_bool(v: Option<&Value>) -> Option<bool> {
    match v {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::String(value)) => value.parse().ok(),
        _ => None,
    }
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

fn is_draw_dopesheet_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::dopesheet" | "draw::dopesheet" | "dopesheet"
    )
}

fn is_draw_event_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::event" | "draw::event" | "event"
    )
}

fn is_draw_set_signal_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::set_signal" | "draw::set_signal" | "set_signal"
    )
}

fn is_wdoc_signal_block(block: &BlockRef) -> bool {
    matches!(block.kind.as_str(), "wdoc::signal" | "signal")
}

fn is_wdoc_binding_block(block: &BlockRef) -> bool {
    matches!(block.kind.as_str(), "wdoc::binding" | "binding")
}

fn is_draw_widget_structural_block(block: &BlockRef, ctx: &ExtractCtx) -> bool {
    ctx.structural_shape_schema_names.contains(&block.kind)
}

fn is_draw_tooltip_block(block: &BlockRef) -> bool {
    matches!(
        block.kind.as_str(),
        "wdoc::draw::tooltip" | "draw::tooltip" | "tooltip"
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
    template_extends_map: HashMap<String, String>,
    draw_schema_names: HashSet<String>,
    structural_shape_schema_names: HashSet<String>,
    template_helpers: HashMap<String, FunctionValue>,
    layout_helpers: HashMap<String, FunctionValue>,
    markup_rules: Vec<MarkupRule>,
    builtins: HashMap<String, BuiltinFn>,
    css_registry: Rc<RefCell<DiagramCssRegistry>>,
    diagram_classes: Rc<RefCell<IndexMap<String, crate::wdoc::shapes::DiagramClass>>>,
    diagram_classes_by_file:
        Rc<RefCell<HashMap<FileId, IndexMap<String, crate::wdoc::shapes::DiagramClass>>>>,
    binding_targets: Rc<RefCell<HashSet<String>>>,
    svg_search_dirs: Vec<PathBuf>,
    icon_registry: IconRegistry,
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
        let result = crate::call_lambda_with_env(
            func,
            &[Value::BlockRef(block.clone())],
            &self.builtins,
            &self.template_helpers,
        )?;
        let output = match result {
            Value::String(s) => s,
            other => format!("{other}"),
        };
        Ok(resolve_icon_placeholders(&output, self))
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
        let scoped = crate::wdoc::shapes::scope_css_to_selector(css, &format!(".{scope_class}"));
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
                blocks.push(crate::wdoc::markup::raw_css(css.trim()));
            }
        }
        for css in &self.global_css {
            if !css.trim().is_empty() {
                blocks.push(crate::wdoc::markup::raw_css(css.trim()));
            }
        }
        for set in self.css_by_scope.values() {
            for css in set {
                if !css.trim().is_empty() {
                    blocks.push(crate::wdoc::markup::raw_css(css.trim()));
                }
            }
        }
        crate::wdoc::markup::render_css(&crate::wdoc::markup::css_stylesheet(blocks))
            .expect("wdoc diagram CSS should serialize as CSS")
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

    let css = crate::wdoc::markup::render_css(&crate::wdoc::markup::css_at(
        "font_face",
        &[
            (
                "font_family",
                crate::wdoc::markup::s(format!("\"{}\"", css_string_escape(family))),
            ),
            (
                "src",
                crate::wdoc::markup::s(format!(
                    "url(\"{}\") format(\"{}\")",
                    css_string_escape(src),
                    css_string_escape(format)
                )),
            ),
            (
                "font_weight",
                crate::wdoc::markup::s(css_declaration_value(weight)),
            ),
            (
                "font_style",
                crate::wdoc::markup::s(css_declaration_value(style)),
            ),
            (
                "font_display",
                crate::wdoc::markup::s(css_declaration_value(display)),
            ),
        ],
        vec![],
    ))
    .expect("wdoc font asset CSS should serialize as CSS");

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
    use crate::wdoc::library::WDOC_LIBRARY_WCL;
    use crate::Span;

    fn with_large_stack(test: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(test)
            .expect("spawn large-stack test thread")
            .join()
            .expect("large-stack test thread panicked");
    }

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
            attribute_decorators: IndexMap::new(),
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
            template_extends_map: HashMap::new(),
            draw_schema_names: HashSet::new(),
            structural_shape_schema_names: HashSet::new(),
            template_helpers: HashMap::new(),
            layout_helpers: HashMap::new(),
            markup_rules: Vec::new(),
            builtins: HashMap::new(),
            css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
            diagram_classes: Rc::new(RefCell::new(IndexMap::new())),
            diagram_classes_by_file: Rc::new(RefCell::new(HashMap::new())),
            binding_targets: Rc::new(RefCell::new(HashSet::new())),
            svg_search_dirs: Vec::new(),
            icon_registry: IconRegistry::default(),
        }
    }

    fn ctx_with_icon_set(dir: PathBuf) -> ExtractCtx {
        let mut ctx = empty_ctx();
        let mut parts = IndexMap::new();
        parts.insert(
            "accent".to_string(),
            IconPart {
                selector: ".accent".to_string(),
                property: "fill".to_string(),
                default: Some("currentColor".to_string()),
            },
        );
        ctx.icon_registry.default_set = Some("test".to_string());
        ctx.icon_registry.sets.insert(
            "test".to_string(),
            IconSet {
                dir,
                normalize_width: Some(24.0),
                normalize_height: Some(24.0),
                normalize_mode: "viewbox".to_string(),
                parts,
            },
        );
        ctx
    }

    #[test]
    fn page_signals_and_bindings_extract_full_wdoc_values() {
        let mut initial = IndexMap::new();
        initial.insert(
            "items".to_string(),
            Value::List(vec![Value::Map(IndexMap::from([(
                "label".to_string(),
                Value::String("Alpha".to_string()),
            )]))]),
        );
        let signal = block(
            "wdoc::signal",
            Some("selection"),
            IndexMap::from([("initial".to_string(), Value::Map(initial))]),
            vec![],
        );
        let binding = block(
            "wdoc::binding",
            Some("label_binding"),
            IndexMap::from([
                ("signal".to_string(), Value::String("selection".to_string())),
                ("target".to_string(), Value::String("label".to_string())),
                ("property".to_string(), Value::String("text".to_string())),
                (
                    "path".to_string(),
                    Value::String("items[0].label".to_string()),
                ),
                (
                    "format".to_string(),
                    Value::String("Selected: {value}".to_string()),
                ),
            ]),
            vec![],
        );
        let page = block(
            "wdoc::page",
            Some("interactive"),
            IndexMap::from([
                (
                    "section".to_string(),
                    Value::String("docs.interactive".to_string()),
                ),
                (
                    "title".to_string(),
                    Value::String("Interactive".to_string()),
                ),
            ]),
            vec![signal, binding],
        );

        let extracted = extract_page(&page, &empty_ctx()).expect("page extracts");

        assert_eq!(extracted.signals.len(), 1);
        assert_eq!(extracted.signals[0].name, "selection");
        assert_eq!(extracted.signals[0].initial["items"][0]["label"], "Alpha");
        assert_eq!(extracted.bindings.len(), 1);
        assert_eq!(
            extracted.bindings[0].path.as_deref(),
            Some("items[0].label")
        );
        assert_eq!(
            extracted.bindings[0].format.as_deref(),
            Some("Selected: {value}")
        );
    }

    #[test]
    fn page_binding_marks_diagram_shape_as_runtime_target() {
        let rect = block(
            "wdoc::draw::rect",
            Some("meter"),
            IndexMap::from([
                ("width".to_string(), Value::Int(40)),
                ("height".to_string(), Value::Int(20)),
            ]),
            vec![],
        );
        let diagram = block(
            "wdoc::draw::diagram",
            Some("preview"),
            IndexMap::from([
                ("width".to_string(), Value::Int(100)),
                ("height".to_string(), Value::Int(40)),
            ]),
            vec![rect],
        );
        let layout = block("wdoc::layout", None, IndexMap::new(), vec![diagram]);
        let binding = block(
            "wdoc::binding",
            None,
            IndexMap::from([
                ("signal".to_string(), Value::String("progress".to_string())),
                ("target".to_string(), Value::String("meter".to_string())),
                ("property".to_string(), Value::String("width".to_string())),
            ]),
            vec![],
        );
        let page = block(
            "wdoc::page",
            Some("interactive"),
            IndexMap::from([
                (
                    "section".to_string(),
                    Value::String("docs.interactive".to_string()),
                ),
                (
                    "title".to_string(),
                    Value::String("Interactive".to_string()),
                ),
            ]),
            vec![binding, layout],
        );

        let extracted = extract_page(&page, &empty_ctx()).expect("page extracts");
        let LayoutItem::Content(content) = &extracted.layout.children[0] else {
            panic!("expected diagram content");
        };

        assert!(content.rendered_html.contains("data-wdoc-id=\"meter\""));
        assert!(content.rendered_html.contains("data-wdoc-width=\"40\""));
        assert!(content.rendered_html.contains("<script>"));
    }

    fn custom_shape_ctx(source: &str, kind: &str, template_name: &str) -> ExtractCtx {
        custom_shape_ctx_with_templates(source, &[(kind, template_name)], &[])
    }

    fn custom_shape_ctx_with_templates(
        source: &str,
        templates: &[(&str, &str)],
        extends: &[(&str, &str)],
    ) -> ExtractCtx {
        let functions = wdoc_functions();
        let doc = crate::parse(
            source,
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

        let mut ctx = empty_ctx();
        for (kind, template_name) in templates {
            ctx.template_map.insert(
                ("shape".to_string(), (*kind).to_string()),
                (*template_name).to_string(),
            );
        }
        for (kind, base_kind) in extends {
            ctx.template_extends_map
                .insert((*kind).to_string(), (*base_kind).to_string());
        }
        ctx.template_helpers = collect_template_helpers(&doc);
        ctx.layout_helpers = collect_layout_helpers(&doc).unwrap();
        ctx.markup_rules = collect_markup_rules(&doc, &ctx.template_helpers).unwrap();
        ctx.builtins = functions.functions;
        ctx.draw_schema_names = collect_draw_schema_names(&doc);
        ctx.structural_shape_schema_names = collect_structural_shape_schema_names(&doc);
        ctx
    }

    fn custom_shape_ctx_from_source(source: &str) -> ExtractCtx {
        let functions = wdoc_functions();
        let doc = crate::parse(
            source,
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

        let template_helpers = collect_template_helpers(&doc);
        let mut ctx = empty_ctx();
        ctx.template_map = collect_template_map(&doc);
        ctx.template_extends_map = collect_template_extends_map(&doc);
        ctx.template_helpers = template_helpers;
        ctx.layout_helpers = collect_layout_helpers(&doc).unwrap();
        ctx.markup_rules = collect_markup_rules(&doc, &ctx.template_helpers).unwrap();
        ctx.builtins = functions.functions;
        ctx.draw_schema_names = collect_draw_schema_names(&doc);
        ctx.structural_shape_schema_names = collect_structural_shape_schema_names(&doc);
        ctx
    }

    fn wdoc_library_ctx() -> ExtractCtx {
        let functions = wdoc_functions();
        let doc = crate::parse(
            WDOC_LIBRARY_WCL,
            crate::ParseOptions {
                root_dir: PathBuf::from(crate::eval::imports::EMBEDDED_LIBRARY_ROOT),
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
        let layout_helpers = collect_layout_helpers(&doc).unwrap();
        let markup_rules = collect_markup_rules(&doc, &template_helpers).unwrap();
        ExtractCtx {
            template_map: collect_template_map(&doc),
            template_extends_map: collect_template_extends_map(&doc),
            draw_schema_names: collect_draw_schema_names(&doc),
            structural_shape_schema_names: collect_structural_shape_schema_names(&doc),
            template_helpers,
            layout_helpers,
            markup_rules,
            builtins: functions.functions,
            css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
            diagram_classes: Rc::new(RefCell::new(IndexMap::new())),
            diagram_classes_by_file: Rc::new(RefCell::new(HashMap::new())),
            binding_targets: Rc::new(RefCell::new(HashSet::new())),
            svg_search_dirs: Vec::new(),
            icon_registry: IconRegistry::default(),
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
            "wdoc::slugify",
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
        assert!(html.contains("wdoc-icon"));
        assert!(html.contains("github"));
    }

    #[test]
    fn equation_block_renders_display_mathjax_wrapper() {
        let ctx = wdoc_library_ctx();
        let mut attrs = IndexMap::new();
        string_attr(&mut attrs, "content", "E = mc^2 <script>");
        let html = ctx
            .render_block(&block("wdoc::equation", None, attrs, vec![]))
            .unwrap();
        assert!(html.contains("class=\"wdoc-equation\""));
        assert!(html.contains("data-wdoc-equation=\"display\""));
        assert!(html.contains("\\[E = mc^2 &lt;script&gt;\\]"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn paragraph_renders_inline_equation_markup() {
        let ctx = wdoc_library_ctx();
        let mut attrs = IndexMap::new();
        string_attr(&mut attrs, "content", "Use $E = mc^2$ carefully.");
        let html = ctx
            .render_block(&block("wdoc::paragraph", None, attrs, vec![]))
            .unwrap();
        assert!(html.contains("class=\"wdoc-equation-inline\""));
        assert!(html.contains("data-wdoc-equation=\"inline\""));
        assert!(html.contains("\\(E = mc^2\\)"));
    }

    #[test]
    fn inline_icon_set_renders_normalized_svg_with_custom_properties() {
        let temp = tempfile::tempdir().unwrap();
        let icon_dir = temp.path().join("icons");
        std::fs::create_dir(&icon_dir).unwrap();
        std::fs::write(
            icon_dir.join("sample.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 24"><path class="accent" d="M0 0h48v24H0z"/></svg>"#,
        )
        .unwrap();
        let ctx = ctx_with_icon_set(icon_dir);
        let mut props = IndexMap::new();
        props.insert("accent".to_string(), "#f85149".to_string());

        let html = render_inline_icon("sample", "2em", None, props, &ctx);

        assert!(html.contains("<svg class=\"wdoc-icon\""));
        assert!(html.contains("width=\"2em\" height=\"2em\""));
        assert!(html.contains("viewBox=\"0 0 24 24\""));
        assert!(html.contains("--wdoc-icon-accent:#f85149;"));
        assert!(html.contains(".accent{fill:var(--wdoc-icon-accent,currentColor);}"));
        assert!(html.contains("<path class=\"accent\""));
    }

    #[test]
    fn paragraph_renders_custom_markup() {
        let functions = wdoc_functions();
        let source = format!(
            "{}\nnamespace wdoc {{\n@markup(\"=={{text}}==\")\nexport let mark = text => \"<mark>\" + wdoc::html_escape(text) + \"</mark>\"\n}}\n",
            WDOC_LIBRARY_WCL
        );
        let doc = crate::parse(
            &source,
            crate::ParseOptions {
                root_dir: PathBuf::from(crate::eval::imports::EMBEDDED_LIBRARY_ROOT),
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
        let layout_helpers = collect_layout_helpers(&doc).unwrap();
        let ctx = ExtractCtx {
            template_map: collect_template_map(&doc),
            template_extends_map: collect_template_extends_map(&doc),
            draw_schema_names: collect_draw_schema_names(&doc),
            structural_shape_schema_names: collect_structural_shape_schema_names(&doc),
            markup_rules: collect_markup_rules(&doc, &template_helpers).unwrap(),
            template_helpers,
            layout_helpers,
            builtins: functions.functions,
            css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
            diagram_classes: Rc::new(RefCell::new(IndexMap::new())),
            diagram_classes_by_file: Rc::new(RefCell::new(HashMap::new())),
            binding_targets: Rc::new(RefCell::new(HashSet::new())),
            svg_search_dirs: Vec::new(),
            icon_registry: IconRegistry::default(),
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
            "<span class=\"status-icon\" style=\"font-size:1.1em;color:#28a745;\"></span> **Typed schemas**",
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
        let doc = crate::parse(
            WDOC_LIBRARY_WCL,
            crate::ParseOptions {
                root_dir: PathBuf::from(crate::eval::imports::EMBEDDED_LIBRARY_ROOT),
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
            &HashMap::new(),
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
        with_large_stack(|| {
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
        });
    }

    #[test]
    fn wcl_widget_content_insets_are_added_to_composite_container_attrs() {
        let ctx = wdoc_library_ctx();
        let mut attrs = IndexMap::new();
        let mut block_attrs = IndexMap::new();
        string_attr(&mut block_attrs, "title", "Profile");
        let card = block("wdoc::draw::card", Some("panel"), block_attrs, vec![]);

        apply_wcl_shape_default_attrs(&mut attrs, &card, &ctx);

        assert_eq!(
            attrs.get("_wdoc_content_top").map(String::as_str),
            Some("36")
        );

        let mut window_attrs = IndexMap::new();
        let window = block(
            "wdoc::draw::window",
            Some("desktop"),
            IndexMap::new(),
            vec![],
        );
        apply_wcl_shape_default_attrs(&mut window_attrs, &window, &ctx);
        assert_eq!(
            window_attrs.get("_wdoc_content_top").map(String::as_str),
            Some("36")
        );

        let mut tablet_attrs = IndexMap::new();
        let tablet = block(
            "wdoc::draw::tablet",
            Some("device"),
            IndexMap::new(),
            vec![],
        );
        apply_wcl_shape_default_attrs(&mut tablet_attrs, &tablet, &ctx);
        assert_eq!(
            tablet_attrs.get("_wdoc_content_left").map(String::as_str),
            Some("18")
        );
        assert_eq!(
            tablet_attrs.get("_wdoc_content_top").map(String::as_str),
            Some("42")
        );
        assert_eq!(
            tablet_attrs.get("_wdoc_content_right").map(String::as_str),
            Some("18")
        );
        assert_eq!(
            tablet_attrs.get("_wdoc_content_bottom").map(String::as_str),
            Some("24")
        );

        let mut phone_landscape_attrs = IndexMap::new();
        let phone_landscape = block(
            "wdoc::draw::phone_landscape",
            Some("mobile"),
            IndexMap::new(),
            vec![],
        );
        apply_wcl_shape_default_attrs(&mut phone_landscape_attrs, &phone_landscape, &ctx);
        assert_eq!(
            phone_landscape_attrs
                .get("_wdoc_content_top")
                .map(String::as_str),
            Some("68")
        );
        assert_eq!(
            phone_landscape_attrs
                .get("_wdoc_content_right")
                .map(String::as_str),
            Some("28")
        );

        let mut tablet_landscape_attrs = IndexMap::new();
        let tablet_landscape = block(
            "wdoc::draw::tablet_landscape",
            Some("wide"),
            IndexMap::new(),
            vec![],
        );
        apply_wcl_shape_default_attrs(&mut tablet_landscape_attrs, &tablet_landscape, &ctx);
        assert_eq!(
            tablet_landscape_attrs
                .get("_wdoc_content_top")
                .map(String::as_str),
            Some("42")
        );
        assert_eq!(
            tablet_landscape_attrs
                .get("_wdoc_content_right")
                .map(String::as_str),
            Some("24")
        );
    }

    #[test]
    fn explicit_widget_content_insets_override_defaults() {
        let ctx = wdoc_library_ctx();
        let mut attrs = IndexMap::new();
        attrs.insert("content_top".to_string(), "96".to_string());
        let phone = block("wdoc::draw::phone", Some("screen"), IndexMap::new(), vec![]);

        apply_wcl_shape_default_attrs(&mut attrs, &phone, &ctx);

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
        let mut class = crate::wdoc::shapes::DiagramClass {
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

        let mut default_class = crate::wdoc::shapes::DiagramClass {
            name: "wdoc-widget-button".to_string(),
            attrs: IndexMap::new(),
            states: IndexMap::new(),
            animations: IndexMap::new(),
        };
        default_class
            .attrs
            .insert("background_fill".to_string(), "#0f766e".to_string());

        let mut brand_class = crate::wdoc::shapes::DiagramClass {
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("height=\"128\""));
        assert!(html.contains("x1=\"12\" y1=\"81\" x2=\"208\" y2=\"81\""));
        assert!(html.contains("stroke=\"#ff00aa\""));
        assert!(html.contains(">Ingress</text>"));
        assert!(html.contains(">Repository</text>"));
        assert!(!html.contains("width=\"0\" height=\"0\""));
    }

    #[test]
    fn chart_widgets_render_points_without_leaking_structural_children() {
        let ctx = wdoc_library_ctx();
        assert!(ctx
            .template_map
            .contains_key(&("shape".to_string(), "wdoc::draw::bar_chart".to_string())));

        let mut chart_attrs = IndexMap::new();
        int_attr(&mut chart_attrs, "x", 20);
        int_attr(&mut chart_attrs, "y", 20);
        int_attr(&mut chart_attrs, "width", 300);
        int_attr(&mut chart_attrs, "height", 200);
        int_attr(&mut chart_attrs, "y_min", 0);
        int_attr(&mut chart_attrs, "y_max", 100);
        int_attr(&mut chart_attrs, "tick_count", 2);
        string_attr(&mut chart_attrs, "title", "Revenue");

        let mut q1_attrs = IndexMap::new();
        string_attr(&mut q1_attrs, "label", "Q1");
        int_attr(&mut q1_attrs, "value", 40);

        let mut q2_attrs = IndexMap::new();
        string_attr(&mut q2_attrs, "label", "Q2");
        int_attr(&mut q2_attrs, "value", 80);

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 360);
        int_attr(&mut diagram_attrs, "height", 250);

        let diagram = block(
            "wdoc::draw::diagram",
            Some("chart_widgets"),
            diagram_attrs,
            vec![block(
                "wdoc::draw::bar_chart",
                Some("revenue"),
                chart_attrs,
                vec![
                    block("wdoc::draw::chart_point", Some("q1"), q1_attrs, vec![]),
                    block("wdoc::draw::chart_point", Some("q2"), q2_attrs, vec![]),
                ],
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("class=\"wdoc-widget-bar_chart\""));
        assert!(html.contains(">Revenue</text>"));
        assert!(html.contains(">Q1</text>"));
        assert!(html.contains("height=\"51.2\""));
        assert!(!html.contains("width=\"0\" height=\"0\""));
        assert!(!html.contains("wdoc-widget-chart_point"));
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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
        let mut class = crate::wdoc::shapes::DiagramClass {
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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
    fn collapsible_panel_widget_emits_runtime_metadata() {
        let ctx = wdoc_library_ctx();
        let mut panel_attrs = IndexMap::new();
        int_attr(&mut panel_attrs, "x", 20);
        int_attr(&mut panel_attrs, "y", 16);
        int_attr(&mut panel_attrs, "width", 260);
        int_attr(&mut panel_attrs, "height", 180);
        string_attr(&mut panel_attrs, "title", "Filters");
        string_attr(&mut panel_attrs, "collapse_group", "settings");
        panel_attrs.insert("collapsed".to_string(), Value::Bool(true));

        let mut input_attrs = IndexMap::new();
        int_attr(&mut input_attrs, "height", 55);
        string_attr(&mut input_attrs, "label", "Search");
        let input = block("wdoc::draw::input", Some("search"), input_attrs, vec![]);

        let panel = block(
            "wdoc::draw::collapsible_panel",
            Some("filters"),
            panel_attrs,
            vec![input],
        );
        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 320);
        int_attr(&mut diagram_attrs, "height", 230);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("collapse_demo"),
            diagram_attrs,
            vec![panel],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("data-wdoc-id=\"filters\""));
        assert!(html.contains("data-wdoc-collapsible-panel=\"true\""));
        assert!(html.contains("data-wdoc-collapse-group=\"settings\""));
        assert!(html.contains("data-wdoc-collapse-header-height=\"36\""));
        assert!(html.contains("data-wdoc-collapse-collapsed=\"true\""));
        assert!(html.contains("data-wdoc-collapse-header=\"true\""));
        assert!(html.contains("data-wdoc-collapse-frame=\"true\""));
        assert!(html.contains("data-wdoc-collapse-keep=\"true\""));
        assert!(html.contains("data-wdoc-id=\"search\""));
        assert!(html.contains("initCollapsiblePanel"));
    }

    #[test]
    fn nested_tooltip_renders_delayed_overlay_with_text_block() {
        let ctx = wdoc_library_ctx();
        let mut rect_attrs = IndexMap::new();
        int_attr(&mut rect_attrs, "x", 40);
        int_attr(&mut rect_attrs, "y", 30);
        int_attr(&mut rect_attrs, "width", 120);
        int_attr(&mut rect_attrs, "height", 60);
        string_attr(&mut rect_attrs, "fill", "#dbeafe");

        let mut tooltip_attrs = IndexMap::new();
        int_attr(&mut tooltip_attrs, "width", 220);
        int_attr(&mut tooltip_attrs, "height", 90);
        int_attr(&mut tooltip_attrs, "delay_ms", 650);

        let mut paragraph_attrs = IndexMap::new();
        string_attr(
            &mut paragraph_attrs,
            "content",
            "Runs **API** requests and publishes events.",
        );
        let paragraph = block("paragraph", Some("summary"), paragraph_attrs, vec![]);
        let mut text_block_attrs = IndexMap::new();
        int_attr(&mut text_block_attrs, "width", 200);
        let text_block = block(
            "wdoc::draw::text_block",
            Some("body"),
            text_block_attrs,
            vec![paragraph],
        );
        let tooltip = block(
            "wdoc::draw::tooltip",
            Some("server_tip"),
            tooltip_attrs,
            vec![text_block],
        );
        let rect = block(
            "wdoc::draw::rect",
            Some("server"),
            rect_attrs,
            vec![tooltip],
        );

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 420);
        int_attr(&mut diagram_attrs, "height", 220);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("tooltip_demo"),
            diagram_attrs,
            vec![rect],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("data-wdoc-id=\"server\""));
        assert!(html.contains("data-wdoc-tooltip-id=\"server_tip\""));
        assert!(html.contains("data-wdoc-tooltip-delay-ms=\"650\""));
        assert!(html.contains("tabindex=\"0\""));
        assert!(html.contains("data-wdoc-id=\"server_tip\""));
        assert!(html.contains("data-wdoc-tooltip-overlay=\"true\""));
        assert!(html.contains("data-wdoc-tooltip-for=\"server\""));
        assert!(html.contains("visibility=\"hidden\""));
        assert!(html.contains("Runs"));
        assert!(html.contains("font-weight=\"700\""));
        assert!(html.contains("initTooltip"));
        assert!(html.contains("setTimeout"));
        assert!(!html.contains("wdoc::draw::tooltip"));
    }

    #[test]
    fn datatable_widget_renders_compact_rows() {
        let ctx = wdoc_library_ctx();
        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 520);
        int_attr(&mut diagram_attrs, "height", 180);

        let mut first = IndexMap::new();
        string_attr(&mut first, "Name", "Ada");
        string_attr(&mut first, "Role", "Admin");
        string_attr(&mut first, "Status", "Active");
        let mut second = IndexMap::new();
        string_attr(&mut second, "Name", "Lin");
        string_attr(&mut second, "Role", "Editor");
        string_attr(&mut second, "Status", "Pending");

        let mut table_attrs = IndexMap::new();
        int_attr(&mut table_attrs, "x", 20);
        int_attr(&mut table_attrs, "y", 20);
        int_attr(&mut table_attrs, "width", 460);
        table_attrs.insert(
            "rows".to_string(),
            Value::List(vec![Value::Map(first), Value::Map(second)]),
        );

        let diagram = block(
            "wdoc::draw::diagram",
            Some("datatable_compact"),
            diagram_attrs,
            vec![block(
                "wdoc::draw::datatable",
                Some("users"),
                table_attrs,
                vec![],
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("Ada"));
        assert!(html.contains("Role"));
        assert!(html.contains("Pending"));
        assert!(!html.contains("datatable_row"));
    }

    #[test]
    fn datatable_widget_renders_rich_cell_controls_and_markup() {
        let ctx = wdoc_library_ctx();
        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 620);
        int_attr(&mut diagram_attrs, "height", 180);

        let columns = vec![
            {
                let mut attrs = IndexMap::new();
                string_attr(&mut attrs, "label", "Name");
                int_attr(&mut attrs, "width", 180);
                block("wdoc::draw::datatable_column", Some("name"), attrs, vec![])
            },
            {
                let mut attrs = IndexMap::new();
                string_attr(&mut attrs, "label", "Status");
                int_attr(&mut attrs, "width", 120);
                block(
                    "wdoc::draw::datatable_column",
                    Some("status"),
                    attrs,
                    vec![],
                )
            },
            {
                let mut attrs = IndexMap::new();
                string_attr(&mut attrs, "label", "");
                int_attr(&mut attrs, "width", 110);
                block(
                    "wdoc::draw::datatable_column",
                    Some("action"),
                    attrs,
                    vec![],
                )
            },
        ];

        let mut name_cell_attrs = IndexMap::new();
        string_attr(&mut name_cell_attrs, "content", "**Ada**");
        let name_cell = block(
            "wdoc::draw::datatable_cell",
            Some("name"),
            name_cell_attrs,
            vec![],
        );
        let badge = block(
            "wdoc::draw::badge",
            Some("ok"),
            IndexMap::from([("label".to_string(), Value::String("Active".to_string()))]),
            vec![],
        );
        let status_cell = block(
            "wdoc::draw::datatable_cell",
            Some("status"),
            IndexMap::new(),
            vec![badge],
        );
        let button = block(
            "wdoc::draw::button",
            Some("edit"),
            IndexMap::from([("label".to_string(), Value::String("Edit".to_string()))]),
            vec![],
        );
        let action_cell = block(
            "wdoc::draw::datatable_cell",
            Some("action"),
            IndexMap::new(),
            vec![button],
        );
        let row = block(
            "wdoc::draw::datatable_row",
            Some("r1"),
            IndexMap::new(),
            vec![name_cell, status_cell, action_cell],
        );

        let mut table_children = columns;
        table_children.push(row);
        let mut table_attrs = IndexMap::new();
        int_attr(&mut table_attrs, "x", 20);
        int_attr(&mut table_attrs, "y", 20);
        int_attr(&mut table_attrs, "width", 460);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("datatable_rich"),
            diagram_attrs,
            vec![block(
                "wdoc::draw::datatable",
                Some("users"),
                table_attrs,
                table_children,
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("Ada"));
        assert!(html.contains("font-weight=\"700\""));
        assert!(html.contains("Active"));
        assert!(html.contains("Edit"));
        assert!(!html.contains("datatable_cell"));
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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

        let rendered = crate::call_lambda_with_env(
            func,
            &[Value::BlockRef(menu)],
            &functions.functions,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(rendered, Value::String("File|Edit".to_string()));
    }

    #[test]
    fn custom_shape_template_preserves_open_kind() {
        let ctx = custom_shape_ctx(
            r##"
            export let my_task_template = (b) => [
                { kind = "rect", x = 0, y = 0, width = 100, height = 40, fill = "#bada55" }
            ]
            "##,
            "my::task",
            "my_task_template",
        );
        let mut attrs = IndexMap::new();
        int_attr(&mut attrs, "width", 100);
        int_attr(&mut attrs, "height", 40);

        let mut shapes = Vec::new();
        let mut connections = Vec::new();
        collect_shape_or_connection(
            &block("my::task", Some("task"), attrs, vec![]),
            &mut shapes,
            &mut connections,
            &ctx,
            0,
            &HashMap::new(),
        );

        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].kind, crate::wdoc::shapes::ShapeKind::Custom);
        assert_eq!(shapes[0].kind_name, "my::task");
        assert_eq!(shapes[0].children.len(), 1);
        assert_eq!(
            shapes[0].children[0].kind,
            crate::wdoc::shapes::ShapeKind::Rect
        );
    }

    #[test]
    fn shape_template_can_extend_base_widget_descriptors() {
        let ctx = custom_shape_ctx_with_templates(
            r##"
            export let base_button = (b) => [
                { kind = "rect", x = 0, y = 0, width = attr_or(b, "width", 120), height = 40, fill = "#111111" }
            ]

            export let danger_button = (b, base) => concat(base, [
                { kind = "text", x = 0, y = 0, width = attr_or(b, "width", 120), height = 40, content = "Danger", fill = "#ff0000" }
            ])
            "##,
            &[
                ("my::base_button", "base_button"),
                ("my::danger_button", "danger_button"),
            ],
            &[("my::danger_button", "my::base_button")],
        );

        let mut attrs = IndexMap::new();
        int_attr(&mut attrs, "width", 140);
        int_attr(&mut attrs, "height", 40);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("extended_widget"),
            IndexMap::new(),
            vec![block("my::danger_button", Some("danger"), attrs, vec![])],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");

        assert!(html.contains("fill=\"#111111\""));
        assert!(html.contains(">Danger</text>"));
        assert!(html.contains("fill=\"#ff0000\""));
    }

    #[test]
    fn shape_template_extends_uses_schema_decorator_metadata() {
        let ctx = custom_shape_ctx_from_source(
            r##"
            @template("shape", "base_card")
            @open schema "my::base_card" { }

            @extends("my::base_card")
            @template("shape", "accent_card")
            @open schema "my::accent_card" { }

            export let base_card = (_b) => [
                { kind = "rect", x = 0, y = 0, width = 90, height = 32, fill = "#445566" }
            ]
            export let accent_card = (_b, base) => concat(base, [
                { kind = "rect", x = 4, y = 4, width = 12, height = 12, fill = "#abcdef" }
            ])
            "##,
        );

        let diagram = block(
            "wdoc::draw::diagram",
            Some("decorator_extends"),
            IndexMap::new(),
            vec![block(
                "my::accent_card",
                Some("card"),
                IndexMap::new(),
                vec![],
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");

        assert!(html.contains("fill=\"#445566\""));
        assert!(html.contains("fill=\"#abcdef\""));
    }

    #[test]
    fn shape_template_extends_normalizes_single_map_base_result() {
        let ctx = custom_shape_ctx_with_templates(
            r##"
            export let base_badge = (_b) => { kind = "rect", x = 0, y = 0, width = 80, height = 28, fill = "#222222" }
            export let labeled_badge = (_b, base) => concat(base, [
                { kind = "text", x = 0, y = 0, width = 80, height = 28, content = "1", fill = "#ffffff" }
            ])
            "##,
            &[
                ("my::base_badge", "base_badge"),
                ("my::labeled_badge", "labeled_badge"),
            ],
            &[("my::labeled_badge", "my::base_badge")],
        );

        let diagram = block(
            "wdoc::draw::diagram",
            Some("single_map_base"),
            IndexMap::new(),
            vec![block(
                "my::labeled_badge",
                Some("badge"),
                IndexMap::new(),
                vec![],
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");

        assert!(html.contains("fill=\"#222222\""));
        assert!(html.contains(">1</text>"));
    }

    #[test]
    fn shape_template_extends_supports_chains() {
        let ctx = custom_shape_ctx_with_templates(
            r##"
            export let base_box = (_b) => [
                { kind = "rect", x = 0, y = 0, width = 100, height = 40, fill = "#101010" }
            ]
            export let middle_box = (_b, base) => concat(base, [
                { kind = "circle", x = 8, y = 8, r = 6, fill = "#202020" }
            ])
            export let final_box = (_b, base) => concat(base, [
                { kind = "text", x = 0, y = 0, width = 100, height = 40, content = "Final", fill = "#303030" }
            ])
            "##,
            &[
                ("my::base_box", "base_box"),
                ("my::middle_box", "middle_box"),
                ("my::final_box", "final_box"),
            ],
            &[
                ("my::middle_box", "my::base_box"),
                ("my::final_box", "my::middle_box"),
            ],
        );

        let diagram = block(
            "wdoc::draw::diagram",
            Some("extend_chain"),
            IndexMap::new(),
            vec![block("my::final_box", Some("box"), IndexMap::new(), vec![])],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");

        assert!(html.contains("fill=\"#101010\""));
        assert!(html.contains("fill=\"#202020\""));
        assert!(html.contains(">Final</text>"));
    }

    #[test]
    fn shape_template_extends_reports_wrong_arity() {
        let ctx = custom_shape_ctx_with_templates(
            r##"
            export let base_box = (_b) => []
            export let bad_box = (_b) => []
            "##,
            &[("my::base_box", "base_box"), ("my::bad_box", "bad_box")],
            &[("my::bad_box", "my::base_box")],
        );

        let err = dispatch_shape_template(
            &block("my::bad_box", Some("bad"), IndexMap::new(), vec![]),
            &ctx,
            &HashMap::new(),
        )
        .unwrap_err();

        assert!(err.contains("expected 1 arguments, got 2"));
    }

    #[test]
    fn shape_template_extends_reports_cycles() {
        let ctx = custom_shape_ctx_with_templates(
            r##"
            export let a_box = (_b, base) => base
            export let b_box = (_b, base) => base
            "##,
            &[("my::a_box", "a_box"), ("my::b_box", "b_box")],
            &[("my::a_box", "my::b_box"), ("my::b_box", "my::a_box")],
        );

        let err = dispatch_shape_template(
            &block("my::a_box", Some("a"), IndexMap::new(), vec![]),
            &ctx,
            &HashMap::new(),
        )
        .unwrap_err();

        assert!(err.contains("shape template extension cycle: my::a_box -> my::b_box -> my::a_box"));
    }

    #[test]
    fn structural_shape_schemas_are_metadata_driven() {
        let ctx = custom_shape_ctx(
            r##"
            namespace wdoc::draw {
                @structural @open schema "widget_data" { }
            }

            export let widget_box = (b) => [
                { kind = "rect", x = 0, y = 0, width = 120, height = 50, fill = "#123456" }
            ]
            "##,
            "wdoc::draw::widget_box",
            "widget_box",
        );

        let mut attrs = IndexMap::new();
        int_attr(&mut attrs, "width", 120);
        int_attr(&mut attrs, "height", 50);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("custom_structural"),
            IndexMap::new(),
            vec![block(
                "wdoc::draw::widget_box",
                Some("box"),
                attrs,
                vec![block(
                    "wdoc::draw::widget_data",
                    Some("metadata"),
                    IndexMap::new(),
                    vec![],
                )],
            )],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("#123456"));
        assert!(!html.contains("metadata"));
    }

    #[test]
    fn connected_port_context_is_available_to_any_shape_template() {
        let ctx = custom_shape_ctx(
            r##"
            export let widget_socket = (b) => {
                let ports = attr_or(b, "_wdoc_connected_ports", "")
                [
                    { kind = "text", x = 0, y = 0, width = 120, height = 20, content = ports }
                ]
            }
            "##,
            "wdoc::draw::socket",
            "widget_socket",
        );

        let mut socket_attrs = IndexMap::new();
        int_attr(&mut socket_attrs, "width", 120);
        int_attr(&mut socket_attrs, "height", 20);
        let mut sink_attrs = IndexMap::new();
        int_attr(&mut sink_attrs, "x", 180);
        int_attr(&mut sink_attrs, "width", 20);
        int_attr(&mut sink_attrs, "height", 20);
        let mut connection_attrs = IndexMap::new();
        string_attr(&mut connection_attrs, "from", "source.out");
        string_attr(&mut connection_attrs, "to", "sink");

        let diagram = block(
            "wdoc::draw::diagram",
            Some("ports"),
            IndexMap::new(),
            vec![
                block("wdoc::draw::socket", Some("source"), socket_attrs, vec![]),
                block("wdoc::draw::rect", Some("sink"), sink_attrs, vec![]),
                block(
                    "wdoc::draw::connection",
                    Some("edge"),
                    connection_attrs,
                    vec![],
                ),
            ],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains(">out</text>"));
    }

    #[test]
    fn unknown_wdoc_draw_block_without_template_is_skipped() {
        let mut shapes = Vec::new();
        let mut connections = Vec::new();
        collect_shape_or_connection(
            &block(
                "wdoc::draw::not_a_builtin",
                Some("mystery"),
                IndexMap::new(),
                vec![],
            ),
            &mut shapes,
            &mut connections,
            &empty_ctx(),
            0,
            &HashMap::new(),
        );

        assert!(shapes.is_empty());
        assert!(connections.is_empty());
    }

    #[test]
    fn unknown_descriptor_with_children_becomes_custom_container() {
        let mut child = IndexMap::new();
        string_attr(&mut child, "kind", "rect");
        int_attr(&mut child, "width", 20);
        int_attr(&mut child, "height", 10);

        let mut descriptor = IndexMap::new();
        string_attr(&mut descriptor, "kind", "callout_badge");
        descriptor.insert("children".to_string(), Value::List(vec![Value::Map(child)]));

        let (node, connections) = descriptor_to_shape_node_and_connections(
            &Value::Map(descriptor),
            0,
            None,
            None,
            &HashMap::new(),
        )
        .expect("custom container descriptor");
        assert_eq!(node.kind, crate::wdoc::shapes::ShapeKind::Custom);
        assert_eq!(node.kind_name, "wdoc::draw::callout_badge");
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].kind, crate::wdoc::shapes::ShapeKind::Rect);
        assert!(connections.is_empty());
    }

    #[test]
    fn unknown_leaf_descriptor_is_rejected() {
        let descriptor = IndexMap::from([(
            "kind".to_string(),
            Value::String("callout_badge".to_string()),
        )]);

        assert!(descriptor_to_shape_node_and_connections(
            &Value::Map(descriptor),
            0,
            None,
            None,
            &HashMap::new(),
        )
        .is_none());
    }

    #[test]
    fn declared_draw_leaf_descriptor_becomes_custom_without_rust_kind() {
        let mut draw_schemas = HashSet::new();
        draw_schemas.insert("wdoc::draw::terminal_text".to_string());
        let descriptor = IndexMap::from([(
            "kind".to_string(),
            Value::String("terminal_text".to_string()),
        )]);

        let (node, _) = descriptor_to_shape_node_and_connections(
            &Value::Map(descriptor),
            0,
            Some(&draw_schemas),
            None,
            &HashMap::new(),
        )
        .expect("declared draw descriptor");
        assert_eq!(node.kind, crate::wdoc::shapes::ShapeKind::Custom);
        assert_eq!(node.kind_name, "wdoc::draw::terminal_text");
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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

        let first = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        let second = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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
    fn extracts_document_and_page_templates() {
        let mut doc_attrs = IndexMap::new();
        string_attr(&mut doc_attrs, "title", "Docs");
        string_attr(&mut doc_attrs, "template", "presentation");
        let doc = block("wdoc::doc", Some("docs"), doc_attrs, vec![]);

        let mut page_attrs = IndexMap::new();
        string_attr(&mut page_attrs, "section", "docs.overview");
        string_attr(&mut page_attrs, "title", "Overview");
        string_attr(&mut page_attrs, "template", "book");
        let page = block("wdoc::page", Some("overview"), page_attrs, vec![]);

        let mut values = IndexMap::new();
        values.insert("docs".to_string(), Value::BlockRef(doc));
        values.insert("overview".to_string(), Value::BlockRef(page));
        let ctx = empty_ctx();

        let document = extract(&values, &ctx).expect("extract");

        assert_eq!(document.template, WdocTemplate::Presentation);
        assert_eq!(document.pages[0].template, Some(WdocTemplate::Book));
    }

    #[test]
    fn extracts_site_template_and_page_metadata() {
        let mut doc_attrs = IndexMap::new();
        string_attr(&mut doc_attrs, "title", "Docs");
        string_attr(&mut doc_attrs, "template", "site");
        let doc = block("wdoc::doc", Some("docs"), doc_attrs, vec![]);

        let mut params = IndexMap::new();
        params.insert("audience".to_string(), Value::String("users".to_string()));
        let mut page_attrs = IndexMap::new();
        string_attr(&mut page_attrs, "section", "docs.overview");
        string_attr(&mut page_attrs, "title", "Overview");
        string_attr(&mut page_attrs, "path", "guides/overview");
        string_attr(&mut page_attrs, "date", "2026-05-17");
        page_attrs.insert("draft".to_string(), Value::Bool(true));
        page_attrs.insert("weight".to_string(), Value::Int(10));
        string_attr(&mut page_attrs, "summary", "Short summary");
        page_attrs.insert(
            "tags".to_string(),
            Value::List(vec![Value::String("wdoc".to_string())]),
        );
        page_attrs.insert(
            "categories".to_string(),
            Value::List(vec![Value::String("docs".to_string())]),
        );
        page_attrs.insert("params".to_string(), Value::Map(params));
        let page = block("wdoc::page", Some("overview"), page_attrs, vec![]);

        let mut values = IndexMap::new();
        values.insert("docs".to_string(), Value::BlockRef(doc));
        values.insert("overview".to_string(), Value::BlockRef(page));
        let ctx = empty_ctx();

        let document = extract(&values, &ctx).expect("extract");
        let page = &document.pages[0];

        assert_eq!(document.template, WdocTemplate::Site);
        assert_eq!(page.path.as_deref(), Some("guides/overview"));
        assert_eq!(page.date.as_deref(), Some("2026-05-17"));
        assert!(page.draft);
        assert_eq!(page.weight, Some(10));
        assert_eq!(page.summary.as_deref(), Some("Short summary"));
        assert_eq!(page.tags, vec!["wdoc"]);
        assert_eq!(page.categories, vec!["docs"]);
        assert_eq!(
            page.params.get("audience").map(String::as_str),
            Some("users")
        );
    }

    #[test]
    fn unknown_wdoc_template_reports_supported_names() {
        let mut doc_attrs = IndexMap::new();
        string_attr(&mut doc_attrs, "title", "Docs");
        string_attr(&mut doc_attrs, "template", "slides");
        let doc = block("wdoc::doc", Some("docs"), doc_attrs, vec![]);

        let mut values = IndexMap::new();
        values.insert("docs".to_string(), Value::BlockRef(doc));
        let ctx = empty_ctx();

        let err = extract(&values, &ctx).expect_err("unknown template should fail");

        assert!(err.contains("unknown wdoc template 'slides'"));
        assert!(err.contains("supported: book, site, presentation"));
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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

            @layout("layered")
            export let test_layered_layout = ctx => wdoc::layout_layered(ctx)
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert_eq!(html.matches("fill=\"#bada55\"").count(), 2);
        assert!(html.contains("marker-end=\"url(#wdoc-arrow)\""));
    }

    #[test]
    fn duplicate_layout_lambdas_are_rejected() {
        let doc = crate::parse(
            r#"
            @layout("flow")
            export let flow_a = ctx => ctx

            @layout("flow")
            export let flow_b = ctx => ctx
            "#,
            crate::ParseOptions {
                functions: wdoc_functions(),
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );

        let err = collect_layout_helpers(&doc).expect_err("duplicate layout should fail");
        assert!(err.contains("duplicate @layout(\"flow\")"));
    }

    #[test]
    fn missing_layout_lambda_is_render_error() {
        let mut a_attrs = IndexMap::new();
        int_attr(&mut a_attrs, "width", 40);
        int_attr(&mut a_attrs, "height", 20);
        let mut b_attrs = IndexMap::new();
        int_attr(&mut b_attrs, "width", 40);
        int_attr(&mut b_attrs, "height", 20);
        let mut diagram_attrs = IndexMap::new();
        string_attr(&mut diagram_attrs, "align", "flow");

        let diagram = block(
            "wdoc::draw::diagram",
            Some("missing_layout"),
            diagram_attrs,
            vec![
                block("wdoc::draw::rect", Some("a"), a_attrs, vec![]),
                block("wdoc::draw::rect", Some("b"), b_attrs, vec![]),
            ],
        );

        let err = render_diagram_with_ctx(&diagram, &empty_ctx()).expect_err("layout should fail");
        assert!(err.contains("no @layout(\"flow\") registered"));
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
        collect_shape_or_connection(
            &group,
            &mut shapes,
            &mut connections,
            &ctx,
            0,
            &HashMap::new(),
        );

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
        collect_shape_or_connection(
            &flow,
            &mut shapes,
            &mut connections,
            &ctx,
            0,
            &HashMap::new(),
        );

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].from_id, "flow.start");
        assert_eq!(connections[0].to_id, "flow.end");
        assert_eq!(shapes[0].children.len(), 2);
        assert!(!shapes[0].children[0]
            .attrs
            .contains_key("_wdoc_layout_decoration"));

        let mut diagram = crate::wdoc::shapes::Diagram {
            id: None,
            width: 220.0,
            height: 160.0,
            padding: 0.0,
            align: crate::wdoc::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes,
            connections,
            classes: IndexMap::new(),
        };
        crate::wdoc::shapes::render_diagram_svg(&mut diagram);
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
        collect_shape_or_connection(
            &flow,
            &mut shapes,
            &mut connections,
            &ctx,
            0,
            &HashMap::new(),
        );

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

        let mut diagram = crate::wdoc::shapes::Diagram {
            id: None,
            width: 220.0,
            height: 190.0,
            padding: 0.0,
            align: crate::wdoc::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes,
            connections,
            classes: IndexMap::new(),
        };
        crate::wdoc::shapes::render_diagram_svg(&mut diagram);
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
        collect_shape_or_connection(
            &outer,
            &mut shapes,
            &mut connections,
            &ctx,
            0,
            &HashMap::new(),
        );

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
        collect_shape_or_connection(
            &node,
            &mut shapes,
            &mut connections,
            &ctx,
            0,
            &HashMap::new(),
        );

        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].width, None);
        assert_eq!(shapes[0].height, None);
        let label = shapes[0]
            .children
            .iter()
            .find(|child| child.kind == crate::wdoc::shapes::ShapeKind::Text)
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
        collect_shape_or_connection(
            &node,
            &mut shapes,
            &mut connections,
            &ctx,
            0,
            &HashMap::new(),
        );

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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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
        int_attr(&mut from_attrs, "rotate", 0);

        let mut to_attrs = IndexMap::new();
        int_attr(&mut to_attrs, "x", 90);
        int_attr(&mut to_attrs, "y", 30);
        int_attr(&mut to_attrs, "width", 50);
        int_attr(&mut to_attrs, "height", 35);
        int_attr(&mut to_attrs, "rotate", 180);
        int_attr(&mut to_attrs, "rotate_origin_x", 80);
        int_attr(&mut to_attrs, "rotate_origin_y", 40);

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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("data-wdoc-state-animation=\"active:slide\""));
        assert!(html.contains("slide|750|0|ease|infinite|alternate|both|"));
        assert!(html.contains("100,90,30,50,35,180,80,40"));
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

        let mut diagram = crate::wdoc::shapes::Diagram {
            id: None,
            width: 80.0,
            height: 80.0,
            padding: 0.0,
            align: crate::wdoc::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes: vec![
                descriptor_to_shape_node_with_order(&Value::Map(group), 0).expect("descriptor")
            ],
            connections: vec![],
            classes: IndexMap::new(),
        };

        let svg = crate::wdoc::shapes::render_diagram_svg(&mut diagram);
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

        let mut diagram = crate::wdoc::shapes::Diagram {
            id: None,
            width: 24.0,
            height: 24.0,
            padding: 0.0,
            align: crate::wdoc::shapes::Alignment::None,
            gap: 0.0,
            options: IndexMap::new(),
            shapes: vec![
                descriptor_to_shape_node_with_order(&Value::Map(descriptor), 0)
                    .expect("descriptor"),
            ],
            connections: vec![],
            classes: IndexMap::new(),
        };

        let svg = crate::wdoc::shapes::render_diagram_svg(&mut diagram);
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("<image href=\"images/hero.png\""));
        assert!(html.contains("preserveAspectRatio=\"xMidYMid slice\""));
        assert!(html.contains("role=\"img\" aria-label=\"Hero image\""));
    }

    #[test]
    fn diagram_map_flows_through_cli_extraction_with_child_shapes() {
        let mut pin_attrs = IndexMap::new();
        int_attr(&mut pin_attrs, "x", 720);
        int_attr(&mut pin_attrs, "y", 460);
        int_attr(&mut pin_attrs, "width", 14);
        int_attr(&mut pin_attrs, "height", 14);
        string_attr(&mut pin_attrs, "fill", "#ef4444");
        let pin = block("wdoc::draw::circle", Some("pin"), pin_attrs, vec![]);

        let mut map_attrs = IndexMap::new();
        int_attr(&mut map_attrs, "x", 0);
        int_attr(&mut map_attrs, "y", 0);
        int_attr(&mut map_attrs, "width", 300);
        int_attr(&mut map_attrs, "height", 180);
        string_attr(&mut map_attrs, "src", "images/world.png");
        int_attr(&mut map_attrs, "content_width", 1200);
        int_attr(&mut map_attrs, "content_height", 800);
        int_attr(&mut map_attrs, "view_x", 300);
        int_attr(&mut map_attrs, "view_y", 200);
        int_attr(&mut map_attrs, "view_width", 300);
        int_attr(&mut map_attrs, "view_height", 180);
        let map = block("wdoc::draw::map", Some("world"), map_attrs, vec![pin]);

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 300);
        int_attr(&mut diagram_attrs, "height", 180);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("map_preview"),
            diagram_attrs,
            vec![map],
        );

        let ctx = empty_ctx();

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("data-wdoc-map=\"true\""));
        assert!(html.contains("<image href=\"images/world.png\""));
        assert!(html.contains("viewBox=\"300 200 300 180\""));
        assert!(html.contains("<circle cx=\"727\" cy=\"467\" r=\"7\" fill=\"#ef4444\""));
    }

    #[test]
    fn sprite_dopesheet_animation_flows_through_cli_extraction() {
        let ctx = wdoc_library_ctx();

        let mut sheet_attrs = IndexMap::new();
        string_attr(&mut sheet_attrs, "src", "images/explosion.png");
        int_attr(&mut sheet_attrs, "columns", 10);
        int_attr(&mut sheet_attrs, "frame_width", 100);
        int_attr(&mut sheet_attrs, "frame_height", 100);
        int_attr(&mut sheet_attrs, "frame_count", 50);

        let mut states = IndexMap::new();
        states.insert(
            "active".to_string(),
            crate::wdoc::shapes::DiagramState {
                name: "active".to_string(),
                attrs: IndexMap::from([("animation".to_string(), "explode".to_string())]),
            },
        );
        let mut animations = IndexMap::new();
        animations.insert(
            "explode".to_string(),
            crate::wdoc::shapes::DiagramAnimation {
                name: "explode".to_string(),
                duration_ms: 150,
                delay_ms: 0,
                timing_function: "ease".to_string(),
                iteration_count: "1".to_string(),
                direction: "normal".to_string(),
                fill_mode: "none".to_string(),
                keyframes: Vec::new(),
                frame_rate: Some(20.0),
                frames: vec![0, 1, 2],
            },
        );
        ctx.diagram_classes.borrow_mut().insert(
            "sprite_fx".to_string(),
            crate::wdoc::shapes::DiagramClass {
                name: "sprite_fx".to_string(),
                attrs: IndexMap::new(),
                states,
                animations,
            },
        );

        let mut sprite_attrs = IndexMap::new();
        int_attr(&mut sprite_attrs, "x", 20);
        int_attr(&mut sprite_attrs, "y", 20);
        int_attr(&mut sprite_attrs, "width", 64);
        int_attr(&mut sprite_attrs, "height", 64);
        string_attr(&mut sprite_attrs, "sheet", "explosion_sheet");
        int_attr(&mut sprite_attrs, "frame", 1);
        string_attr(&mut sprite_attrs, "class", "sprite_fx");
        string_attr(&mut sprite_attrs, "transparent_color", "#ff00ff");

        let mut event_attrs = IndexMap::new();
        string_attr(&mut event_attrs, "trigger", "click");
        string_attr(&mut event_attrs, "state", "active");
        string_attr(&mut event_attrs, "mode", "add");

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 120);
        int_attr(&mut diagram_attrs, "height", 100);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("sprite_demo"),
            diagram_attrs,
            vec![
                block(
                    "wdoc::draw::dopesheet",
                    Some("explosion_sheet"),
                    sheet_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::sprite",
                    Some("explosion"),
                    sprite_attrs,
                    vec![block(
                        "wdoc::draw::event",
                        Some("play"),
                        event_attrs,
                        vec![],
                    )],
                ),
            ],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("data-wdoc-sprite=\"true\""));
        assert!(html.contains("viewBox=\"100 0 100 100\""));
        assert!(html.contains("href=\"images/explosion.png\""));
        assert!(html.contains("data-wdoc-sprite-transparent-color=\"#ff00ff\""));
        assert!(
            html.contains("data-wdoc-animations=\"explode|150|0|ease|1|normal|none||20|0,1,2\"")
        );
        assert!(!html.contains("<dopesheet"));
    }

    #[test]
    fn dopesheet_view_renders_uniform_cropped_frame_grid() {
        let ctx = wdoc_library_ctx();

        let mut sheet_attrs = IndexMap::new();
        string_attr(&mut sheet_attrs, "src", "images/player.png");
        int_attr(&mut sheet_attrs, "columns", 2);
        int_attr(&mut sheet_attrs, "frame_width", 16);
        int_attr(&mut sheet_attrs, "frame_height", 8);
        int_attr(&mut sheet_attrs, "frame_count", 3);

        let mut view_attrs = IndexMap::new();
        int_attr(&mut view_attrs, "x", 10);
        int_attr(&mut view_attrs, "y", 20);
        int_attr(&mut view_attrs, "width", 120);
        int_attr(&mut view_attrs, "height", 80);
        string_attr(&mut view_attrs, "sheet", "player_sheet");
        string_attr(&mut view_attrs, "background_fill", "#111827");
        string_attr(&mut view_attrs, "grid_stroke", "#ffffff");

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 160);
        int_attr(&mut diagram_attrs, "height", 120);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("dopesheet_view_demo"),
            diagram_attrs,
            vec![
                block(
                    "wdoc::draw::dopesheet",
                    Some("player_sheet"),
                    sheet_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::dopesheet_view",
                    Some("preview"),
                    view_attrs,
                    vec![],
                ),
            ],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("data-wdoc-dopesheet-view=\"true\""));
        assert!(html.contains("href=\"images/player.png\""));
        assert!(html.contains("viewBox=\"0 0 16 8\""));
        assert!(html.contains("viewBox=\"16 0 16 8\""));
        assert!(html.contains("viewBox=\"0 8 16 8\""));
        assert!(html.contains("fill=\"#111827\""));
        assert!(html.contains("stroke=\"#ffffff\""));
        assert!(!html.contains("<dopesheet"));
    }

    #[test]
    fn tilemap_rows_render_cropped_tiles_and_empty_cells() {
        let ctx = wdoc_library_ctx();

        let mut sheet_attrs = IndexMap::new();
        string_attr(&mut sheet_attrs, "src", "images/terrain.png");
        int_attr(&mut sheet_attrs, "columns", 4);
        int_attr(&mut sheet_attrs, "frame_width", 16);
        int_attr(&mut sheet_attrs, "frame_height", 8);
        int_attr(&mut sheet_attrs, "frame_count", 12);

        let mut tilemap_attrs = IndexMap::new();
        int_attr(&mut tilemap_attrs, "x", 10);
        int_attr(&mut tilemap_attrs, "y", 20);
        int_attr(&mut tilemap_attrs, "tile_width", 32);
        int_attr(&mut tilemap_attrs, "tile_height", 16);
        string_attr(&mut tilemap_attrs, "sheet", "terrain_sheet");
        string_attr(&mut tilemap_attrs, "background_fill", "#111827");
        string_attr(&mut tilemap_attrs, "grid_stroke", "#ffffff");
        string_attr(&mut tilemap_attrs, "transparent_color", "#ff00ff");
        tilemap_attrs.insert(
            "rows".to_string(),
            Value::List(vec![
                Value::List(vec![Value::Int(0), Value::Int(1), Value::Null]),
                Value::List(vec![Value::Int(4)]),
                Value::List(vec![Value::Null, Value::Int(9), Value::Int(2)]),
            ]),
        );

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 180);
        int_attr(&mut diagram_attrs, "height", 120);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("tilemap_demo"),
            diagram_attrs,
            vec![
                block(
                    "wdoc::draw::dopesheet",
                    Some("terrain_sheet"),
                    sheet_attrs,
                    vec![],
                ),
                block("wdoc::draw::tilemap", Some("level"), tilemap_attrs, vec![]),
            ],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("data-wdoc-tilemap=\"true\""));
        assert!(html.contains("width=\"96\" height=\"48\""));
        assert!(html.contains("href=\"images/terrain.png\""));
        assert!(html.contains("viewBox=\"0 0 16 8\""));
        assert!(html.contains("viewBox=\"16 0 16 8\""));
        assert!(html.contains("viewBox=\"16 16 16 8\""));
        assert!(html.contains("x=\"42\" y=\"52\" width=\"32\" height=\"16\""));
        assert!(html.contains("fill=\"#111827\""));
        assert!(html.contains("stroke=\"#ffffff\""));
        assert!(html.contains("data-wdoc-sprite-transparent-color=\"#ff00ff\""));
        assert_eq!(
            html.matches("<image href=\"images/terrain.png\"").count(),
            5
        );
        assert!(!html.contains("<dopesheet"));
    }

    #[test]
    fn isometric_tilemap_rows_render_in_diamond_layout() {
        let ctx = wdoc_library_ctx();

        let mut sheet_attrs = IndexMap::new();
        string_attr(&mut sheet_attrs, "src", "images/iso.png");
        int_attr(&mut sheet_attrs, "columns", 4);
        int_attr(&mut sheet_attrs, "frame_width", 64);
        int_attr(&mut sheet_attrs, "frame_height", 32);
        int_attr(&mut sheet_attrs, "frame_count", 16);

        let mut tilemap_attrs = IndexMap::new();
        int_attr(&mut tilemap_attrs, "x", 10);
        int_attr(&mut tilemap_attrs, "y", 20);
        int_attr(&mut tilemap_attrs, "tile_width", 64);
        int_attr(&mut tilemap_attrs, "tile_height", 32);
        string_attr(&mut tilemap_attrs, "sheet", "iso_sheet");
        string_attr(&mut tilemap_attrs, "orientation", "isometric");
        string_attr(&mut tilemap_attrs, "background_fill", "#111827");
        string_attr(&mut tilemap_attrs, "grid_stroke", "#ffffff");
        tilemap_attrs.insert(
            "rows".to_string(),
            Value::List(vec![
                Value::List(vec![Value::Int(0), Value::Int(1), Value::Null]),
                Value::List(vec![Value::Int(4), Value::Int(5), Value::Int(6)]),
                Value::List(vec![Value::Null, Value::Int(9)]),
            ]),
        );

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 260);
        int_attr(&mut diagram_attrs, "height", 160);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("iso_tilemap_demo"),
            diagram_attrs,
            vec![
                block(
                    "wdoc::draw::dopesheet",
                    Some("iso_sheet"),
                    sheet_attrs,
                    vec![],
                ),
                block("wdoc::draw::tilemap", Some("iso"), tilemap_attrs, vec![]),
            ],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("data-wdoc-tilemap=\"true\""));
        assert!(html.contains("width=\"192\" height=\"96\""));
        assert!(html.contains("x=\"74\" y=\"20\" width=\"64\" height=\"32\""));
        assert!(html.contains("x=\"106\" y=\"36\" width=\"64\" height=\"32\""));
        assert!(html.contains("x=\"42\" y=\"36\" width=\"64\" height=\"32\""));
        assert!(html.contains("x=\"42\" y=\"68\" width=\"64\" height=\"32\""));
        assert!(html.contains("viewBox=\"64 64 64 32\""));
        assert_eq!(html.matches("<image href=\"images/iso.png\"").count(), 6);
        assert!(!html.contains("stroke=\"#ffffff\""));
        assert!(!html.contains("<dopesheet"));
    }

    #[test]
    fn isometric_tilemap_rows_support_taller_render_height() {
        let ctx = wdoc_library_ctx();

        let mut sheet_attrs = IndexMap::new();
        string_attr(&mut sheet_attrs, "src", "images/elevated-iso.png");
        int_attr(&mut sheet_attrs, "columns", 2);
        int_attr(&mut sheet_attrs, "frame_width", 64);
        int_attr(&mut sheet_attrs, "frame_height", 64);
        int_attr(&mut sheet_attrs, "frame_count", 4);

        let mut tilemap_attrs = IndexMap::new();
        int_attr(&mut tilemap_attrs, "x", 10);
        int_attr(&mut tilemap_attrs, "y", 20);
        int_attr(&mut tilemap_attrs, "tile_width", 64);
        int_attr(&mut tilemap_attrs, "tile_height", 32);
        int_attr(&mut tilemap_attrs, "tile_render_height", 64);
        string_attr(&mut tilemap_attrs, "sheet", "elevated_iso_sheet");
        string_attr(&mut tilemap_attrs, "orientation", "isometric");
        tilemap_attrs.insert(
            "rows".to_string(),
            Value::List(vec![
                Value::List(vec![Value::Int(0), Value::Int(1)]),
                Value::List(vec![Value::Int(2), Value::Int(3)]),
            ]),
        );

        let mut diagram_attrs = IndexMap::new();
        int_attr(&mut diagram_attrs, "width", 220);
        int_attr(&mut diagram_attrs, "height", 160);
        let diagram = block(
            "wdoc::draw::diagram",
            Some("elevated_iso_tilemap_demo"),
            diagram_attrs,
            vec![
                block(
                    "wdoc::draw::dopesheet",
                    Some("elevated_iso_sheet"),
                    sheet_attrs,
                    vec![],
                ),
                block(
                    "wdoc::draw::tilemap",
                    Some("elevated_iso"),
                    tilemap_attrs,
                    vec![],
                ),
            ],
        );

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
        assert!(html.contains("width=\"128\" height=\"96\""));
        assert!(html.contains("x=\"42\" y=\"20\" width=\"64\" height=\"64\""));
        assert!(html.contains("x=\"74\" y=\"36\" width=\"64\" height=\"64\""));
        assert!(html.contains("x=\"10\" y=\"36\" width=\"64\" height=\"64\""));
        assert!(html.contains("x=\"42\" y=\"52\" width=\"64\" height=\"64\""));
        assert!(html.contains("viewBox=\"64 64 64 64\""));
        assert_eq!(
            html.matches("<image href=\"images/elevated-iso.png\"")
                .count(),
            4
        );
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

        let html = render_diagram_with_ctx(&diagram, &ctx).expect("render diagram");
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
        render_child_content(block, ctx)
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
        if rest.starts_with('$') {
            if let Some((end, tex)) = match_inline_equation(text, pos) {
                out.push_str(&crate::wdoc::markup::render_html(
                    &crate::wdoc::markup::elem(
                        "span",
                        &[
                            ("class_name", crate::wdoc::markup::s("wdoc-equation-inline")),
                            ("data_wdoc_equation", crate::wdoc::markup::s("inline")),
                        ],
                        vec![crate::wdoc::markup::text(format!("\\({tex}\\)"))],
                    ),
                )?);
                pos = end;
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
            let value = crate::call_lambda_with_env(
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
    Ok(resolve_icon_placeholders(&out, ctx))
}

fn match_inline_equation(text: &str, start: usize) -> Option<(usize, &str)> {
    let rest = text.get(start..)?;
    if !rest.starts_with('$') {
        return None;
    }
    let content_start = start + 1;
    let first = text.get(content_start..)?.chars().next()?;
    if first.is_whitespace() {
        return None;
    }

    let mut pos = content_start;
    let mut escaped = false;
    while pos < text.len() {
        let ch = text[pos..].chars().next()?;
        if escaped {
            escaped = false;
            pos += ch.len_utf8();
            continue;
        }
        if ch == '\\' {
            escaped = true;
            pos += ch.len_utf8();
            continue;
        }
        if ch == '$' {
            let tex = &text[content_start..pos];
            if tex.is_empty() || tex.chars().last().is_some_and(char::is_whitespace) {
                return None;
            }
            return Some((pos + 1, tex));
        }
        pos += ch.len_utf8();
    }
    None
}

fn resolve_icon_placeholders(html: &str, ctx: &ExtractCtx) -> String {
    let marker = "<span data-wdoc-icon=\"true\"";
    let mut out = String::new();
    let mut pos = 0;
    while let Some(start_rel) = html[pos..].find(marker) {
        let start = pos + start_rel;
        out.push_str(&html[pos..start]);
        let Some(tag_end_rel) = html[start..].find('>') else {
            out.push_str(&html[start..]);
            return out;
        };
        let tag_end = start + tag_end_rel + 1;
        let close = html[tag_end..]
            .find("</span>")
            .map(|offset| tag_end + offset + "</span>".len())
            .unwrap_or(tag_end);
        let tag = &html[start..tag_end];
        let name = html_attr(tag, "data-name").unwrap_or_default();
        let size = html_attr(tag, "data-size").unwrap_or_else(|| "1em".to_string());
        let set = html_attr(tag, "data-set");
        let props = html_attr(tag, "data-props")
            .map(|encoded| decode_props(&encoded))
            .unwrap_or_default();
        out.push_str(&render_inline_icon(
            &name,
            &size,
            set.as_deref(),
            props,
            ctx,
        ));
        pos = close;
    }
    out.push_str(&html[pos..]);
    out
}

fn html_attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(html_unescape(&tag[start..end]))
}

fn decode_props(encoded: &str) -> IndexMap<String, String> {
    let mut props = IndexMap::new();
    for part in encoded.split('&') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        props.insert(url_component_unescape(key), url_component_unescape(value));
    }
    props
}

fn render_inline_icon(
    name: &str,
    size: &str,
    set: Option<&str>,
    props: IndexMap<String, String>,
    ctx: &ExtractCtx,
) -> String {
    match resolve_icon_reference(&ctx.icon_registry, set, name, None, None, None) {
        Ok(icon) => render_inline_icon_svg(&icon, size, &props),
        Err(_) => crate::wdoc::markup::render_html(&crate::wdoc::markup::elem(
            "span",
            &[
                (
                    "class_name",
                    crate::wdoc::markup::s("wdoc-icon wdoc-icon-missing"),
                ),
                ("aria_hidden", crate::wdoc::markup::s("true")),
            ],
            vec![crate::wdoc::markup::text(name)],
        ))
        .expect("missing icon fallback should serialize as HTML"),
    }
}

fn render_inline_icon_svg(
    icon: &ResolvedIcon,
    size: &str,
    props: &IndexMap<String, String>,
) -> String {
    let sanitized = crate::wdoc::shapes::sanitize_inline_svg(&icon.content).unwrap_or_default();
    let native_view_box = crate::wdoc::shapes::svg_source_view_box(&icon.content)
        .and_then(|view_box| crate::wdoc::shapes::parse_svg_view_box(&view_box))
        .unwrap_or((0.0, 0.0, 24.0, 24.0));
    let (min_x, min_y, native_width, native_height) = native_view_box;
    let native_width = native_width.max(1.0);
    let native_height = native_height.max(1.0);
    let ratio = native_width / native_height;
    let (view_min_x, view_min_y, view_width, view_height, transform) =
        if icon.normalize_mode == "none" {
            (min_x, min_y, native_width, native_height, String::new())
        } else {
            let norm_width = icon
                .normalize_width
                .or_else(|| icon.normalize_height.map(|height| height * ratio))
                .unwrap_or(native_width)
                .max(1.0);
            let norm_height = icon
                .normalize_height
                .or_else(|| icon.normalize_width.map(|width| width / ratio))
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
                format!("translate({tx},{ty}) scale({scale})"),
            )
        };
    let mut style = String::from("vertical-align:-0.125em;");
    style.push_str(&icon_style_vars(props));
    let css = icon.css.replace("</style", "<\\/style");
    let body = if transform.is_empty() {
        crate::wdoc::markup::raw_svg(sanitized)
    } else {
        crate::wdoc::markup::svg_elem(
            "g",
            &[("transform", crate::wdoc::markup::s(transform))],
            vec![crate::wdoc::markup::raw_svg(sanitized)],
        )
    };
    crate::wdoc::markup::render_svg(&crate::wdoc::markup::svg_elem(
        "svg",
        &[
            ("class_name", crate::wdoc::markup::s("wdoc-icon")),
            (
                "xmlns",
                crate::wdoc::markup::s("http://www.w3.org/2000/svg"),
            ),
            ("width", crate::wdoc::markup::s(size)),
            ("height", crate::wdoc::markup::s(size)),
            (
                "viewBox",
                crate::wdoc::markup::s(format!(
                    "{view_min_x} {view_min_y} {view_width} {view_height}"
                )),
            ),
            ("style", crate::wdoc::markup::s(style)),
            ("aria_hidden", crate::wdoc::markup::s("true")),
            ("data_wdoc_icon_name", crate::wdoc::markup::s(&icon.name)),
        ],
        vec![
            crate::wdoc::markup::svg_elem("style", &[("raw", crate::wdoc::markup::s(css))], vec![]),
            body,
        ],
    ))
    .expect("inline icon svg should serialize as SVG")
}

fn icon_style_vars(props: &IndexMap<String, String>) -> String {
    let mut style = String::new();
    let fill = props
        .get("fill")
        .map(String::as_str)
        .unwrap_or("currentColor");
    let stroke_prop = props.get("stroke").map(String::as_str);
    let stroke = stroke_prop.unwrap_or("none");
    if is_safe_css_value(fill) {
        style.push_str("color:");
        style.push_str(stroke_prop.filter(|value| *value != "none").unwrap_or(fill));
        style.push(';');
        style.push_str("--wdoc-icon-fill:");
        style.push_str(fill);
        style.push(';');
    }
    if is_safe_css_value(stroke) {
        style.push_str("--wdoc-icon-stroke:");
        style.push_str(stroke);
        style.push(';');
    }
    for (key, value) in props {
        if key == "fill" || key == "stroke" || !is_safe_css_value(value) {
            continue;
        }
        let suffix = css_var_suffix(key);
        if suffix.is_empty() {
            continue;
        }
        style.push_str("--wdoc-icon-");
        style.push_str(&suffix);
        style.push(':');
        style.push_str(value);
        style.push(';');
    }
    style
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

fn render_child_content(block: &BlockRef, ctx: &ExtractCtx) -> Result<String, String> {
    let mut html = String::new();
    for child in all_child_blocks(block) {
        match child.kind.as_str() {
            "wdoc::layout" | "wdoc::section" | "wdoc::page" | "wdoc::doc" | "wdoc::style" => {}
            "wdoc::draw::diagram" => {
                html.push_str(&render_diagram_with_ctx(child, ctx)?);
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
    Ok(html)
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
    let template = extract_template_attr(wdoc, "doc")?.unwrap_or(WdocTemplate::DEFAULT);
    let mut site = SiteConfig::default();

    let mut sections = Vec::new();
    for child in all_child_blocks(wdoc) {
        match child.kind.as_str() {
            "wdoc::section" => sections.push(extract_section(child, &name)?),
            "wdoc::page" => pages.push(extract_page(child, ctx)?),
            "wdoc::style" => styles.push(extract_style(child)),
            "wdoc::site_header" => site.header_html = Some(ctx.render_block(child)?),
            "wdoc::site_nav" => site.nav_html = Some(ctx.render_block(child)?),
            "wdoc::site_footer" => site.footer_html = Some(ctx.render_block(child)?),
            _ => {}
        }
    }

    Ok(WdocDocument {
        name,
        title,
        template,
        version,
        author,
        site,
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
        .and_then(value_ref_id)
        .ok_or_else(|| format!("page '{id}' missing 'section' attribute"))?
        .to_string();

    let title = block
        .attributes
        .get("title")
        .and_then(|v| v.as_string())
        .ok_or_else(|| format!("page '{id}' missing 'title' attribute"))?
        .to_string();
    let template = extract_template_attr(block, &format!("page '{id}'"))?;
    let path = string_attr(block, "path");
    let date = string_attr(block, "date");
    let draft = value_as_bool(block.attributes.get("draft")).unwrap_or(false);
    let weight = block.attributes.get("weight").and_then(value_as_i64);
    let summary = string_attr(block, "summary");
    let tags = string_list_attr(block, "tags");
    let categories = string_list_attr(block, "categories");
    let params = string_map_attr(block, "params");

    let all_children = all_child_blocks(block);
    let signals = extract_page_signals(&all_children);
    let bindings = extract_page_bindings(&all_children);
    {
        let mut targets = ctx.binding_targets.borrow_mut();
        targets.clear();
        for binding in &bindings {
            targets.insert(binding.target.clone());
        }
    }
    let layout = all_children
        .iter()
        .find(|c| c.kind == "wdoc::layout")
        .map(|c| extract_layout(c, ctx))
        .transpose()?
        .unwrap_or(Layout {
            children: Vec::new(),
        });
    ctx.binding_targets.borrow_mut().clear();

    Ok(Page {
        id,
        section_id,
        title,
        template,
        path,
        date,
        draft,
        weight,
        summary,
        tags,
        categories,
        params,
        layout,
        signals,
        bindings,
    })
}

fn string_attr(block: &BlockRef, key: &str) -> Option<String> {
    block
        .attributes
        .get(key)
        .and_then(|v| v.as_string())
        .map(str::to_string)
}

fn string_list_attr(block: &BlockRef, key: &str) -> Vec<String> {
    match block.attributes.get(key) {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|value| value.as_string().map(str::to_string))
            .collect(),
        Some(Value::String(value)) => value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn string_map_attr(block: &BlockRef, key: &str) -> IndexMap<String, String> {
    match block.attributes.get(key) {
        Some(Value::Map(map)) => map
            .iter()
            .filter_map(|(key, value)| value.as_string().map(|s| (key.clone(), s.to_string())))
            .collect(),
        _ => IndexMap::new(),
    }
}

fn extract_template_attr(block: &BlockRef, label: &str) -> Result<Option<WdocTemplate>, String> {
    block
        .attributes
        .get("template")
        .and_then(|v| v.as_string())
        .map(WdocTemplate::parse)
        .transpose()
        .map_err(|e| format!("{label} has {e}"))
}

fn extract_page_signals(blocks: &[&BlockRef]) -> Vec<WdocSignal> {
    blocks
        .iter()
        .filter(|block| is_wdoc_signal_block(block))
        .filter_map(|block| {
            let name = block.id.clone()?;
            let initial = block
                .attributes
                .get("initial")
                .map(crate::json::value_to_json)
                .unwrap_or(serde_json::Value::Null);
            let type_name = block
                .attributes
                .get("type")
                .and_then(|value| value.as_string())
                .map(str::to_string);
            Some(WdocSignal {
                name,
                initial,
                type_name,
            })
        })
        .collect()
}

fn extract_page_bindings(blocks: &[&BlockRef]) -> Vec<WdocBinding> {
    let mut bindings = Vec::new();
    for block in blocks {
        collect_bindings_in_block(block, &mut bindings);
    }
    bindings
}

fn collect_bindings_in_block(block: &BlockRef, bindings: &mut Vec<WdocBinding>) {
    if is_wdoc_binding_block(block) {
        if let (Some(signal), Some(target), Some(property)) = (
            block.attributes.get("signal").and_then(|v| v.as_string()),
            block.attributes.get("target").and_then(|v| v.as_string()),
            block.attributes.get("property").and_then(|v| v.as_string()),
        ) {
            bindings.push(WdocBinding {
                name: block.id.clone(),
                signal: signal.to_string(),
                target: target.to_string(),
                property: property.to_string(),
                path: block
                    .attributes
                    .get("path")
                    .and_then(|v| v.as_string())
                    .map(str::to_string),
                format: block
                    .attributes
                    .get("format")
                    .and_then(|v| v.as_string())
                    .map(str::to_string),
            });
        }
        return;
    }
    for child in all_child_blocks(block) {
        collect_bindings_in_block(child, bindings);
    }
}

fn extract_layout(block: &BlockRef, ctx: &ExtractCtx) -> Result<Layout, String> {
    Ok(Layout {
        children: extract_layout_children(block, ctx)?,
    })
}

fn extract_layout_children(block: &BlockRef, ctx: &ExtractCtx) -> Result<Vec<LayoutItem>, String> {
    let mut items = Vec::new();
    for child in all_child_blocks(block) {
        match child.kind.as_str() {
            "vsplit" => items.push(LayoutItem::SplitGroup(extract_split_group(
                child,
                SplitDirection::Vertical,
                ctx,
            )?)),
            "hsplit" => items.push(LayoutItem::SplitGroup(extract_split_group(
                child,
                SplitDirection::Horizontal,
                ctx,
            )?)),
            // Known structural blocks are not content
            "wdoc::layout" | "wdoc::section" | "wdoc::page" | "wdoc::doc" | "wdoc::style"
            | "wdoc::signal" | "wdoc::binding" | "signal" | "binding" | "split" => {}
            // Diagram — needs ctx so shape templates can be dispatched.
            // Cannot go through the html template path because templates can't
            // see ctx.
            "wdoc::draw::diagram" => {
                let html = render_diagram_with_ctx(child, ctx)?;
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
    Ok(items)
}

fn extract_split_group(
    block: &BlockRef,
    direction: SplitDirection,
    ctx: &ExtractCtx,
) -> Result<SplitGroup, String> {
    let mut splits = Vec::new();
    for child in all_child_blocks(block) {
        if child.kind == "split" {
            splits.push(extract_split(child, ctx)?);
        }
    }
    Ok(SplitGroup { direction, splits })
}

fn extract_split(block: &BlockRef, ctx: &ExtractCtx) -> Result<Split, String> {
    let size_percent = block
        .attributes
        .get("size")
        .and_then(|v| match v {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        })
        .unwrap_or(0.0);

    Ok(Split {
        size_percent,
        children: extract_layout_children(block, ctx)?,
    })
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
                .and_then(value_ref_id)
                .map(|s| s.to_string())
        })
}

fn value_ref_id(value: &Value) -> Option<&str> {
    match value {
        Value::BlockRef(block) => block.qualified_id.as_deref().or(block.id.as_deref()),
        Value::Identifier(value) | Value::String(value) => Some(value.as_str()),
        _ => None,
    }
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

fn parse_and_extract(files: &[PathBuf], options: &SourceOptions) -> Result<WdocDocument, String> {
    parse_extract_from_files(files, options).map(|extracted| extracted.document)
}

fn format_diagnostic(
    diag: &crate::Diagnostic,
    source_map: &crate::SourceMap,
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

    let mut all_values = IndexMap::new();
    let mut last_doc: Option<crate::Document> = None;
    let mut watch_paths = HashSet::new();

    for file in files {
        let source = std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

        let mut options = crate::ParseOptions {
            root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
            variables: source_options.variables.clone(),
            functions: functions.clone(),
            ..Default::default()
        };
        options.lib_paths.clone_from(&source_options.lib_paths);
        options.no_default_lib_paths = source_options.no_default_lib_paths;

        let doc = crate::parse(&source, options);

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
                .filter(|path| !path.starts_with(crate::eval::imports::EMBEDDED_LIBRARY_ROOT))
                .cloned(),
        );
        all_values.extend(doc.values.clone());
        last_doc = Some(doc);
    }

    let doc = last_doc.ok_or("no input files")?;

    // Build template dispatch context
    let template_map = collect_template_map(&doc);
    let draw_schema_names = collect_draw_schema_names(&doc);
    let structural_shape_schema_names = collect_structural_shape_schema_names(&doc);
    let template_extends_map = collect_template_extends_map(&doc);
    let builtins: HashMap<String, BuiltinFn> = functions.functions;
    let template_helpers = collect_template_helpers(&doc);
    let layout_helpers = collect_layout_helpers(&doc)?;
    let markup_rules = collect_markup_rules(&doc, &template_helpers)?;
    let svg_search_dirs = wdoc_source_dirs(files, &doc.imported_paths);
    let icon_registry = collect_icon_sets(&all_values, &svg_search_dirs)?;
    let ctx = ExtractCtx {
        template_map,
        template_extends_map,
        draw_schema_names,
        structural_shape_schema_names,
        template_helpers,
        layout_helpers,
        markup_rules,
        builtins,
        css_registry: Rc::new(RefCell::new(DiagramCssRegistry::default())),
        diagram_classes: Rc::new(RefCell::new(collect_diagram_classes(&all_values))),
        diagram_classes_by_file: Rc::new(RefCell::new(collect_diagram_classes_by_file(
            &all_values,
        ))),
        binding_targets: Rc::new(RefCell::new(HashSet::new())),
        svg_search_dirs,
        icon_registry,
    };

    let wdoc_doc = extract(&all_values, &ctx)?;

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
    let asset_dirs = wdoc_asset_dirs(files, &extracted.watch_paths);
    crate::wdoc::codec::encode_html_document(&doc, output, &asset_dirs)?;
    Ok(BuildResult {
        pages,
        output: output.to_path_buf(),
    })
}

fn wdoc_asset_dirs(files: &[PathBuf], watch_paths: &HashSet<PathBuf>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();
    for dir in files
        .iter()
        .filter_map(|path| path.parent())
        .chain(watch_paths.iter().filter_map(|path| path.parent()))
    {
        let dir = dir.to_path_buf();
        if seen.insert(dir.clone()) {
            dirs.push(dir);
        }
    }
    dirs
}

fn wdoc_source_dirs(files: &[PathBuf], imported_paths: &HashSet<PathBuf>) -> Vec<PathBuf> {
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
        .filter(|path| !path.starts_with(crate::eval::imports::EMBEDDED_LIBRARY_ROOT))
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

fn count_sections(sections: &[Section]) -> usize {
    sections
        .iter()
        .map(|s| 1 + count_sections(&s.children))
        .sum()
}
