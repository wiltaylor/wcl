//! HTML page rendering: the document shell, templates, the page-level
//! blocks (text / span / column / code / table), and the HTML fundamentals.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use wcl_lang::{Block, Document, Value};

use crate::highlight;
use crate::icons::IconRegistry;
use crate::inline::InlinePatterns;

use super::*;

/// Wrap a page's `body` HTML in the document shell. The `<head>`
/// (title + global stylesheet) is owned here regardless of template;
/// templates control the `<body>` contents via `render_template`.
pub(crate) fn render_page(name: &str, css: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n\
         <html>\n\
         <head><meta charset=\"utf-8\"><title>{title}</title>\n\
         <style>{css}</style></head>\n\
         <body class=\"wdoc-body\">\n\
         {body}</body>\n\
         </html>\n",
        title = escape_html(name),
        body = body,
    )
}

/// Find a `@block("template")` instance by its inline name.
pub(crate) fn find_template<'a>(doc: &'a Document, name: &str) -> Option<Block<'a>> {
    doc.blocks()
        .find(|b| b.kind() == "template" && label_string(b).as_deref() == Some(name))
}

/// One node of a book's table of contents, read from the `site`
/// block's `toc`. `page` is the linked page name (None ⇒ a grouping
/// heading); `children` are nested entries.
pub(crate) struct TocNode {
    pub title: String,
    pub page: Option<String>,
    pub children: Vec<TocNode>,
}

/// Recursively read `chapter` blocks nested inside `block` into
/// [`TocNode`]s, preserving source order.
pub(crate) fn read_chapters(block: &Block<'_>) -> Vec<TocNode> {
    block
        .blocks()
        .filter(|b| b.kind() == "chapter")
        .map(|ch| TocNode {
            title: label_string(&ch).unwrap_or_default(),
            page: field_id(&ch, "page"),
            children: read_chapters(&ch),
        })
        .collect()
}

/// Read the book's table of contents from a `site` block's `toc`
/// child. Empty when there is no `toc` (callers fall back to a flat
/// page list).
pub(crate) fn read_toc(site: &Block<'_>) -> Vec<TocNode> {
    match site.block("toc") {
        Some(toc) => read_chapters(&toc),
        None => Vec::new(),
    }
}

/// Build a `list<TocEntry>` `Value` from `nodes`, marking the entry
/// (if any) that links to `current`.
pub(crate) fn toc_to_value(nodes: &[TocNode], current: &str) -> Value {
    Value::List(
        nodes
            .iter()
            .map(|n| {
                let href = n
                    .page
                    .as_ref()
                    .map(|p| format!("{p}.html"))
                    .unwrap_or_default();
                let mut m = BTreeMap::new();
                m.insert("title".to_string(), Value::Utf8(n.title.clone()));
                m.insert("href".to_string(), Value::Utf8(href));
                m.insert(
                    "current".to_string(),
                    Value::Bool(n.page.as_deref() == Some(current)),
                );
                m.insert("children".to_string(), toc_to_value(&n.children, current));
                Value::Record {
                    ty: vec!["TocEntry".to_string()],
                    fields: m,
                }
            })
            .collect(),
    )
}

/// Render a page through `template`'s `render` function. Builds a
/// `TemplateCtx` record (content + title + page_name + pages + toc) and
/// invokes the WCL function, then renders the returned fundamentals.
/// Best-effort: a missing/failed `render` yields an empty body, like
/// the rest of the lowering pipeline. When `toc_nodes` is empty the
/// `toc` falls back to a flat entry per page.
#[allow(clippy::too_many_arguments)] // cohesive per-page render inputs
pub(crate) fn render_template(
    doc: &Document,
    template: &Block<'_>,
    content: &str,
    title: &str,
    page_name: &str,
    pages: &[(String, String)],
    toc_nodes: &[TocNode],
    theme_toggle: bool,
    icons: &IconRegistry,
) -> String {
    let Some(field) = template.field("render") else {
        return String::new();
    };
    let Ok(Value::Function(fv)) = field.value() else {
        return String::new();
    };
    let fv = fv.clone();
    let page_ref = |n: &str, h: &str| {
        let mut m = BTreeMap::new();
        m.insert("name".to_string(), Value::Utf8(n.to_string()));
        m.insert("href".to_string(), Value::Utf8(h.to_string()));
        Value::Record {
            ty: vec!["PageRef".to_string()],
            fields: m,
        }
    };
    let pages_val = Value::List(pages.iter().map(|(n, h)| page_ref(n, h)).collect());
    // `toc`: the declared book TOC, or a flat entry per page as a
    // fallback so a templated page without a `toc` still gets a nav.
    let toc_val = if toc_nodes.is_empty() {
        Value::List(
            pages
                .iter()
                .map(|(n, h)| {
                    let mut m = BTreeMap::new();
                    m.insert("title".to_string(), Value::Utf8(n.clone()));
                    m.insert("href".to_string(), Value::Utf8(h.clone()));
                    m.insert("current".to_string(), Value::Bool(n == page_name));
                    m.insert("children".to_string(), Value::List(Vec::new()));
                    Value::Record {
                        ty: vec!["TocEntry".to_string()],
                        fields: m,
                    }
                })
                .collect(),
        )
    } else {
        toc_to_value(toc_nodes, page_name)
    };
    let mut ctx = BTreeMap::new();
    ctx.insert("content".to_string(), Value::Utf8(content.to_string()));
    ctx.insert("title".to_string(), Value::Utf8(title.to_string()));
    ctx.insert("page_name".to_string(), Value::Utf8(page_name.to_string()));
    ctx.insert("pages".to_string(), pages_val);
    ctx.insert("toc".to_string(), toc_val);
    ctx.insert("theme_toggle".to_string(), Value::Bool(theme_toggle));
    let arg = Value::Record {
        ty: vec!["TemplateCtx".to_string()],
        fields: ctx,
    };
    let Ok(Value::List(items)) = doc.call_value(&fv, &[arg]) else {
        return String::new();
    };
    items
        .iter()
        .map(|v| render_html_variant(doc, v, 0, icons))
        .collect()
}

pub(crate) fn render_block(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> Option<String> {
    match block.kind() {
        "text" => Some(render_text(doc, block, patterns)),
        "column" => Some(render_column(doc, block, patterns, base_dir)),
        "table" => Some(render_table(doc, block, patterns)),
        "diagram" => Some(render_diagram(
            doc,
            block,
            patterns.icons(),
            patterns.tilesets(),
        )),
        "code" => Some(render_code(block)),
        // The terminal is special-cased in Rust (like `code`): its grid
        // model, ANSI handling, and asciinema replay aren't expressible
        // in WCL. `base_dir` lets a `source` recording path resolve
        // relative to the source file.
        "terminal" => Some(crate::terminal::render_terminal(doc, block, base_dir)),
        // Skip the lowering function declarations — they're top-level
        // fields, not blocks, so they don't reach render_block.
        kind => Some(lower_html_block(doc, block, kind, patterns.icons())),
    }
}

pub(crate) fn render_text(doc: &Document, block: &Block<'_>, patterns: &InlinePatterns) -> String {
    let cls = class_attr(block);
    let spans: String = block
        .blocks()
        .filter(|b| b.kind() == "span")
        .map(|b| render_span(doc, &b, patterns))
        .collect();
    let mut out = format!("<p{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, ">{spans}</p>").expect("write to String");
    out
}

pub(crate) fn render_span(doc: &Document, block: &Block<'_>, patterns: &InlinePatterns) -> String {
    let cls = class_attr(block);
    let text = label_string(block).unwrap_or_default();
    let mut out = format!("<span{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, ">{}</span>", patterns.render(doc, &text)).expect("write to String");
    out
}

pub(crate) fn render_column(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    let cls = class_attr(block);
    let widths = field_f64_list(block, "widths");
    let grid_cols: String = widths
        .iter()
        .map(|w| format!("{w}%"))
        .collect::<Vec<_>>()
        .join(" ");
    let children: String = block
        .blocks()
        .filter_map(|b| render_block(doc, &b, patterns, base_dir))
        .collect();
    let mut out = format!("<div{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(
        out,
        " style=\"display:grid;grid-template-columns:{grid_cols};\">{children}</div>"
    )
    .expect("write to String");
    out
}

/// Render a `@block("code")` instance to a `<pre><code>` element
/// with syntect-produced `<span class="tok-…">` tokens inside. The
/// `code-block` class is always present so the bundled theme CSS
/// can style the container; user-declared `class` entries are
/// appended after it.
pub(crate) fn render_code(block: &Block<'_>) -> String {
    // `language` is declared `@inline(0)` on @block("code"), so it
    // arrives as the block's label rather than a named field.
    let language = label_string(block).unwrap_or_default();
    let source = field_utf8(block, "source").unwrap_or_default();
    let mut classes: Vec<String> = vec!["code-block".to_string()];
    classes.extend(field_utf8_list(block, "class"));
    let cls = classes_attr_from_names(&classes);
    let inner = highlight::highlight_html(&source, &language);
    let mut out = format!("<pre{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(
        out,
        "><code class=\"language-{}\">{inner}</code></pre>",
        escape_html(&language),
    )
    .expect("write to String");
    out
}

/// Render a `@block("table")` instance. Rows are authored with WCL's
/// pipe-table syntax (`rows: | a | b |`) and read here via
/// `Block::tables()` — a `table` declares no typed `rows` field, so
/// the rows are arbitrary-width and never schema-validated. The first
/// row is the header (`<th>` inside `<thead>`); the rest are body
/// rows (`<td>` inside `<tbody>`). Each cell is rendered by
/// `cell_to_html`, so utf8 cells pick up inline patterns.
pub(crate) fn render_table(doc: &Document, block: &Block<'_>, patterns: &InlinePatterns) -> String {
    // Collect every pipe-row in source order. A `table` normally holds
    // a single `rows:` table; if several are present we concatenate.
    let mut rows: Vec<Vec<String>> = Vec::new();
    for table in block.tables() {
        for row in table.rows() {
            let Ok(values) = row.values() else {
                // A row whose cells fail to evaluate is skipped rather
                // than aborting the whole table.
                continue;
            };
            rows.push(
                values
                    .iter()
                    .map(|v| cell_to_html(doc, patterns, v))
                    .collect(),
            );
        }
    }
    if rows.is_empty() {
        return String::new();
    }
    let mut classes: Vec<String> = vec!["wdoc-table".to_string()];
    classes.extend(field_utf8_list(block, "class"));
    let header = &rows[0];
    let body = &rows[1..];
    table_html(field_id(block, "id").as_deref(), &classes, header, body)
}

/// Render a single table cell to inner HTML. utf8 cells flow through
/// the inline-pattern engine (bold / italic / code / links); every
/// other value kind is stringified via `Value`'s `Display` and
/// HTML-escaped.
pub(crate) fn cell_to_html(doc: &Document, patterns: &InlinePatterns, value: &Value) -> String {
    match value {
        Value::Utf8(s) | Value::Ascii(s) => patterns.render(doc, s),
        other => escape_html(&other.to_string()),
    }
}

/// Shared `<table>` builder. `header` and `body` cells are already
/// rendered inner HTML (not escaped again here). An empty `header`
/// omits the `<thead>` entirely so the lowering path can emit a
/// header-less table.
pub(crate) fn table_html(
    id: Option<&str>,
    classes: &[String],
    header: &[String],
    body: &[Vec<String>],
) -> String {
    let cls = classes_attr_from_names(classes);
    let mut out = format!("<table{cls}");
    append_attr(&mut out, "id", id);
    out.push('>');
    if !header.is_empty() {
        out.push_str("<thead><tr>");
        for cell in header {
            write!(out, "<th>{cell}</th>").expect("write to String");
        }
        out.push_str("</tr></thead>");
    }
    if !body.is_empty() {
        out.push_str("<tbody>");
        for row in body {
            out.push_str("<tr>");
            for cell in row {
                write!(out, "<td>{cell}</td>").expect("write to String");
            }
            out.push_str("</tr>");
        }
        out.push_str("</tbody>");
    }
    out.push_str("</table>");
    out
}

/// Render an `HtmlFundamental::Icon { name, class? }` by resolving the
/// name against the icon registry (which records it for the shared
/// sprite). A miss renders nothing.
pub(crate) fn render_icon_fundamental(
    map: &BTreeMap<String, Value>,
    icons: &IconRegistry,
) -> String {
    let Some(name) = map_utf8(map, "name") else {
        return String::new();
    };
    let classes = map_utf8_list(map, "class");
    icons.resolve_html_icon(&name, &classes).unwrap_or_default()
}

pub(crate) fn render_paragraph_payload(map: &BTreeMap<String, Value>) -> String {
    let cls = class_attr_from_map(map);
    let spans = map_utf8_list(map, "spans");
    let inner: String = spans
        .iter()
        .map(|s| format!("<span>{}</span>", escape_html(s)))
        .collect();
    let mut out = format!("<p{cls}");
    append_attr(&mut out, "id", map_id(map, "id").as_deref());
    write!(out, ">{inner}</p>").expect("write to String");
    out
}

/// Render an `HtmlFundamental::Table` variant produced by a custom
/// block's `lower`. `header` is the (optional) heading row and `rows`
/// is `list<list<utf8>>` of body rows. Cells are plain escaped text
/// on this path (no inline patterns), mirroring `Paragraph`'s spans.
pub(crate) fn render_table_payload(map: &BTreeMap<String, Value>) -> String {
    let mut classes: Vec<String> = vec!["wdoc-table".to_string()];
    classes.extend(map_utf8_list(map, "class"));
    let header: Vec<String> = map_utf8_list(map, "header")
        .iter()
        .map(|s| escape_html(s))
        .collect();
    let body: Vec<Vec<String>> = match map.get("rows") {
        Some(Value::List(rows)) => rows
            .iter()
            .map(|row| match row {
                Value::List(cells) => cells
                    .iter()
                    .filter_map(value_as_str)
                    .map(|s| escape_html(&s))
                    .collect(),
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    };
    table_html(map_id(map, "id").as_deref(), &classes, &header, &body)
}

/// Render an `HtmlFundamental::Element` — `<tag id class attrs>…</tag>`
/// with its `children` rendered recursively as fundamentals. Powers
/// template layout (header / nav / main / a / …).
pub(crate) fn render_element_payload(
    doc: &Document,
    map: &BTreeMap<String, Value>,
    depth: usize,
    icons: &IconRegistry,
) -> String {
    let tag = map_utf8(map, "tag").unwrap_or_else(|| "div".to_string());
    // Only allow simple alphanumeric tag names so a stray value can't
    // inject markup; fall back to `div` otherwise.
    let tag = if !tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric()) {
        tag
    } else {
        "div".to_string()
    };
    let cls = class_attr_from_map(map);
    let mut out = format!("<{tag}{cls}");
    append_attr(&mut out, "id", map_id(map, "id").as_deref());
    // `attrs` is a list of `[name, value]` pairs.
    if let Some(Value::List(attrs)) = map.get("attrs") {
        for a in attrs {
            if let Value::List(pair) = a
                && let (Some(name), Some(value)) = (
                    pair.first().and_then(value_as_str),
                    pair.get(1).and_then(value_as_str),
                )
            {
                append_attr(&mut out, &name, Some(&value));
            }
        }
    }
    out.push('>');
    if let Some(Value::List(children)) = map.get("children") {
        for child in children {
            out.push_str(&render_html_variant(doc, child, depth + 1, icons));
        }
    }
    write!(out, "</{tag}>").expect("write to String");
    out
}

/// Render an `HtmlFundamental::Raw` — pre-rendered HTML embedded
/// verbatim (NOT escaped). Used to splice already-rendered content
/// (e.g. a page's body) into a template.
pub(crate) fn render_raw_payload(map: &BTreeMap<String, Value>) -> String {
    map_utf8(map, "html").unwrap_or_default()
}

// ── Resolution helpers (block-side) ───────────────────────────────
