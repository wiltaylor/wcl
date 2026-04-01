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
    block_attrs: &IndexMap<String, Value>,
    builtins: &HashMap<String, BuiltinFn>,
) -> Result<String, String> {
    let arg = Value::Map(block_attrs.clone());
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

    // Inline formatting
    reg.register(
        "bold",
        std::sync::Arc::new(|args: &[Value]| {
            let t = args
                .first()
                .and_then(|v| v.as_string())
                .ok_or("bold() expects a string argument")?;
            Ok(Value::String(format!("<strong>{t}</strong>")))
        }) as BuiltinFn,
        mk("bold", vec!["text: string"], "Wrap text in <strong> tags"),
    );

    reg.register(
        "italic",
        std::sync::Arc::new(|args: &[Value]| {
            let t = args
                .first()
                .and_then(|v| v.as_string())
                .ok_or("italic() expects a string argument")?;
            Ok(Value::String(format!("<em>{t}</em>")))
        }) as BuiltinFn,
        mk("italic", vec!["text: string"], "Wrap text in <em> tags"),
    );

    reg.register(
        "link",
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
            "link",
            vec!["text: string", "url: string"],
            "Create an <a> link",
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
        "wdoc_render_heading",
        std::sync::Arc::new(|args: &[Value]| {
            let attrs = value_map_to_string_map(args.first())?;
            Ok(Value::String(wcl_wdoc::templates::render_heading(&attrs)))
        }) as BuiltinFn,
        mk("wdoc_render_heading", "Render a heading element"),
    );

    reg.register(
        "wdoc_render_paragraph",
        std::sync::Arc::new(|args: &[Value]| {
            let attrs = value_map_to_string_map(args.first())?;
            Ok(Value::String(wcl_wdoc::templates::render_paragraph(&attrs)))
        }) as BuiltinFn,
        mk("wdoc_render_paragraph", "Render a paragraph element"),
    );

    reg.register(
        "wdoc_render_image",
        std::sync::Arc::new(|args: &[Value]| {
            let attrs = value_map_to_string_map(args.first())?;
            Ok(Value::String(wcl_wdoc::templates::render_image(&attrs)))
        }) as BuiltinFn,
        mk("wdoc_render_image", "Render an image element"),
    );

    reg.register(
        "wdoc_render_code",
        std::sync::Arc::new(|args: &[Value]| {
            let attrs = value_map_to_string_map(args.first())?;
            Ok(Value::String(wcl_wdoc::templates::render_code(&attrs)))
        }) as BuiltinFn,
        mk("wdoc_render_code", "Render a code block"),
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

        call_template(func, &block.attributes, &self.builtins)
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
                "wdoc" => wdoc_block = Some(block),
                "wdoc_page" => pages.push(extract_page(block, ctx)?),
                "wdoc_style" => styles.push(extract_style(block)),
                _ => {}
            }
        }
    }

    let wdoc = wdoc_block.ok_or("no wdoc block found in document")?;

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
            "wdoc_section" => sections.push(extract_section(child, &name)?),
            "wdoc_page" => pages.push(extract_page(child, ctx)?),
            "wdoc_style" => styles.push(extract_style(child)),
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
        if child.kind == "wdoc_section" {
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
        .ok_or_else(|| format!("wdoc_page '{id}' missing 'section' attribute"))?
        .to_string();

    let title = block
        .attributes
        .get("title")
        .and_then(|v| v.as_string())
        .ok_or_else(|| format!("wdoc_page '{id}' missing 'title' attribute"))?
        .to_string();

    let all_children = all_child_blocks(block);
    let layout = all_children
        .iter()
        .find(|c| c.kind == "wdoc_layout")
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
            "wdoc_layout" | "wdoc_section" | "wdoc_page" | "wdoc" | "wdoc_style" | "split" => {}
            // Everything else is a content block — try to render via template
            kind => {
                let rendered = ctx.render_block(child);
                match rendered {
                    Ok(html) => items.push(LayoutItem::Content(ContentBlock {
                        kind: kind.to_string(),
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
    wcl_wdoc::render_to(&doc, output)?;
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

    let watch_paths: Vec<PathBuf> = files
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
