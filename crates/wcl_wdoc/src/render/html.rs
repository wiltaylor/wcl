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
/// (title + favicon + global stylesheet) is owned here regardless of
/// template; templates control the `<body>` contents via `render_template`.
/// `favicon` is the resolved `<link rel="icon">` href (a `_wdoc/…` URL or an
/// external one); `None` emits no favicon link. `head_extra` is verbatim
/// HTML spliced in just before `</head>` — the site's `stylesheets` /
/// `scripts` / `fonts` links plus any head fundamentals a template emits;
/// pass `""` for none.
pub(crate) fn render_page(
    title: &str,
    css: &str,
    body: &str,
    favicon: Option<&str>,
    head_extra: &str,
) -> String {
    let favicon_link = favicon
        .map(|href| {
            // An SVG favicon needs an explicit type; other formats are
            // inferred from the file by the browser.
            let ty = if href.ends_with(".svg") {
                " type=\"image/svg+xml\""
            } else {
                ""
            };
            format!("<link rel=\"icon\"{ty} href=\"{}\">", escape_html(href))
        })
        .unwrap_or_default();
    format!(
        "<!DOCTYPE html>\n\
         <html>\n\
         <head><meta charset=\"utf-8\"><title>{title}</title>{favicon_link}\n\
         <style>{css}</style>{head_extra}</head>\n\
         <body class=\"wdoc-body\">\n\
         {body}</body>\n\
         </html>\n",
        title = escape_html(title),
        favicon_link = favicon_link,
        head_extra = head_extra,
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

/// Recursively read same-kind child blocks nested inside `block` into
/// nodes, preserving source order. `mk` builds each node from the block
/// and its already-read children. Shared by the `toc`/`menu` readers.
/// Recursively read `chapter` blocks nested inside `block` into
/// [`TocNode`]s, preserving source order. A `wdoc_repeater` child (body =
/// `chapter` blocks) is expanded in place into one entry per element of
/// its `each` list — data-driven navigation that tracks a collection.
pub(crate) fn read_chapters(block: &Block<'_>) -> Vec<TocNode> {
    let mut out = Vec::new();
    for ch in block.blocks() {
        push_toc_child(&ch, &mut out);
    }
    out
}

/// Append one TOC child to `out`: a `chapter` becomes a node (recursing for
/// nested chapters / repeaters); a `wdoc_repeater` expands to one node per
/// element, each carrying its binding scope. Other kinds are ignored.
fn push_toc_child(ch: &Block<'_>, out: &mut Vec<TocNode>) {
    match ch.kind() {
        "chapter" => out.push(TocNode {
            title: label_string(ch).unwrap_or_default(),
            page: field_id(ch, "page"),
            children: read_chapters(ch),
        }),
        "wdoc_repeater" if ch.binding_scope_depth() <= MAX_LOWER_DEPTH => {
            for c in expand_repeater_children(ch) {
                push_toc_child(&c, out);
            }
        }
        _ => {}
    }
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

/// Build a `list<Record>` `Value` from `nodes`, marking the entry whose
/// `page` matches `current`. Shared by `toc_to_value`/`menu_to_value`;
/// `ty`/`label_field` name the record and its label key, and the
/// closures read each node's label, resolved href, page, and children.
#[allow(clippy::too_many_arguments)] // cohesive node-projection closures
fn nodes_to_value<T>(
    nodes: &[T],
    current: &str,
    ty: &str,
    label_field: &str,
    label: &impl Fn(&T) -> String,
    href: &impl Fn(&T) -> String,
    page: &impl Fn(&T) -> Option<&str>,
    children: &impl Fn(&T) -> &[T],
) -> Value {
    Value::List(
        nodes
            .iter()
            .map(|n| {
                let mut m = BTreeMap::new();
                m.insert(label_field.to_string(), Value::Utf8(label(n)));
                m.insert("href".to_string(), Value::Utf8(href(n)));
                m.insert("current".to_string(), Value::Bool(page(n) == Some(current)));
                m.insert(
                    "children".to_string(),
                    nodes_to_value(
                        children(n),
                        current,
                        ty,
                        label_field,
                        label,
                        href,
                        page,
                        children,
                    ),
                );
                Value::Record {
                    ty: vec![ty.to_string()],
                    fields: std::sync::Arc::new(m),
                }
            })
            .collect::<Vec<_>>()
            .into(),
    )
}

/// Build a `list<TocEntry>` `Value` from `nodes`, marking the entry
/// (if any) that links to `current`.
pub(crate) fn toc_to_value(nodes: &[TocNode], current: &str) -> Value {
    nodes_to_value(
        nodes,
        current,
        "TocEntry",
        "title",
        &|n: &TocNode| n.title.clone(),
        &|n: &TocNode| {
            n.page
                .as_ref()
                .map(|p| format!("{p}.html"))
                .unwrap_or_default()
        },
        &|n: &TocNode| n.page.as_deref(),
        &|n: &TocNode| n.children.as_slice(),
    )
}

/// One node of a site's navbar menu, read from the `site` block's
/// `menu`. `page` is an internal page link (validated), `href` an
/// external/raw URL; an item with neither (and no children) is a plain
/// label. `children` are nested sub-menu items.
pub(crate) struct MenuNode {
    pub label: String,
    pub page: Option<String>,
    pub href: Option<String>,
    pub children: Vec<MenuNode>,
}

/// Recursively read `item` blocks nested inside `block` into
/// [`MenuNode`]s, preserving source order.
pub(crate) fn read_menu_items(block: &Block<'_>) -> Vec<MenuNode> {
    let mut out = Vec::new();
    for it in block.blocks() {
        push_menu_child(&it, &mut out);
    }
    out
}

/// Append one menu child to `out`: an `item` becomes a node (recursing for
/// nested items / repeaters); a `wdoc_repeater` expands to one node per
/// element of its `each` list — data-driven navigation (e.g. one entry per
/// `included_sites(...)` record). Other kinds are ignored.
fn push_menu_child(it: &Block<'_>, out: &mut Vec<MenuNode>) {
    match it.kind() {
        "item" => out.push(MenuNode {
            label: label_string(it).unwrap_or_default(),
            page: field_id(it, "page"),
            href: field_utf8(it, "href"),
            children: read_menu_items(it),
        }),
        "wdoc_repeater" if it.binding_scope_depth() <= MAX_LOWER_DEPTH => {
            for c in expand_repeater_children(it) {
                push_menu_child(&c, out);
            }
        }
        _ => {}
    }
}

/// Read the site's navbar menu from a `site` block's `menu` child.
/// Empty when there is no `menu` (the `webpage` template then falls
/// back to a flat page list).
pub(crate) fn read_menu(site: &Block<'_>) -> Vec<MenuNode> {
    match site.block("menu") {
        Some(menu) => read_menu_items(&menu),
        None => Vec::new(),
    }
}

/// Build a `list<MenuEntry>` `Value` from `nodes`, resolving each item's
/// `href` (an internal `page` wins → `<page>.html`, else the raw `href`,
/// else empty for a parent/grouping item) and marking the entry that
/// links to `current`.
pub(crate) fn menu_to_value(nodes: &[MenuNode], current: &str) -> Value {
    nodes_to_value(
        nodes,
        current,
        "MenuEntry",
        "label",
        &|n: &MenuNode| n.label.clone(),
        &|n: &MenuNode| match (&n.page, &n.href) {
            (Some(p), _) => format!("{p}.html"),
            (None, Some(h)) => h.clone(),
            (None, None) => String::new(),
        },
        &|n: &MenuNode| n.page.as_deref(),
        &|n: &MenuNode| n.children.as_slice(),
    )
}

/// One pinned sidebar-footer button, read from the `site` block's
/// `sidebar_footer`. `page` is an internal page link (validated), `href`
/// an external/raw URL; `icon` is the icon name (`set.name`) to draw
/// beside the label, resolved to SVG markup at render time.
pub(crate) struct FooterButtonNode {
    pub label: String,
    pub page: Option<String>,
    pub href: Option<String>,
    pub icon: Option<String>,
}

/// Read the pinned sidebar-footer buttons from a `site` block's
/// `sidebar_footer` child. Empty when there is none (templates then
/// render no footer).
pub(crate) fn read_sidebar_footer(site: &Block<'_>) -> Vec<FooterButtonNode> {
    let Some(footer) = site.block("sidebar_footer") else {
        return Vec::new();
    };
    footer
        .blocks()
        .filter(|b| b.kind() == "button")
        .map(|b| FooterButtonNode {
            label: label_string(&b).unwrap_or_default(),
            page: field_id(&b, "page"),
            href: field_utf8(&b, "href"),
            icon: field_utf8(&b, "icon"),
        })
        .collect()
}

/// One section of a presentation deck, read from the `site` block's
/// `deck`. `title` is the section heading; `slides` are the page names
/// it shows, in order.
pub(crate) struct DeckSectionNode {
    pub title: String,
    pub slides: Vec<String>,
}

/// Read the presentation deck from a `site` block's `deck` child: each
/// `section` becomes a [`DeckSectionNode`] holding its `slide` page
/// names (a slide's page is its inline label). Empty when there is no
/// `deck`.
pub(crate) fn read_deck(site: &Block<'_>) -> Vec<DeckSectionNode> {
    let Some(deck) = site.block("deck") else {
        return Vec::new();
    };
    deck.blocks()
        .filter(|b| b.kind() == "section")
        .map(|sec| DeckSectionNode {
            title: label_string(&sec).unwrap_or_default(),
            slides: sec
                .blocks()
                .filter(|b| b.kind() == "slide")
                .filter_map(|s| label_string(&s))
                .collect(),
        })
        .collect()
}

/// A rendered template, split into the page `<body>` and any `<head>`
/// content the template emitted via top-level `HtmlFundamental::Head`
/// fundamentals (hoisted out of the body — see [`head_fundamental_html`]).
#[derive(Default)]
pub(crate) struct Rendered {
    pub body: String,
    pub head: String,
}

/// Render a page through `template`'s `render` function. Builds a
/// `TemplateCtx` record (content + regions + title + page_name + pages +
/// toc) and invokes the WCL function, then renders the returned
/// fundamentals — partitioning top-level `Head` fundamentals into
/// [`Rendered::head`] and the rest into [`Rendered::body`].
/// Best-effort: a missing/failed `render` yields an empty body, like
/// the rest of the lowering pipeline. When `toc_nodes` is empty the
/// `toc` falls back to a flat entry per page.
#[allow(clippy::too_many_arguments)] // cohesive per-page render inputs
pub(crate) fn render_template(
    doc: &Document,
    template: &Block<'_>,
    content: &str,
    regions: Value,
    title: &str,
    page_name: &str,
    pages: &[(String, String, String)],
    toc_nodes: &[TocNode],
    menu_nodes: &[MenuNode],
    footer_nodes: &[FooterButtonNode],
    deck: Value,
    theme_toggle: bool,
    home_href: &str,
    home_title: &str,
    search: bool,
    patterns: &InlinePatterns,
) -> Rendered {
    let Some(field) = template.field("render") else {
        return Rendered::default();
    };
    let Ok(Value::Function(fv)) = field.value() else {
        return Rendered::default();
    };
    let fv = fv.clone();
    let page_ref = |n: &str, h: &str| {
        let mut m = BTreeMap::new();
        m.insert("name".to_string(), Value::Utf8(n.to_string()));
        m.insert("href".to_string(), Value::Utf8(h.to_string()));
        Value::Record {
            ty: vec!["PageRef".to_string()],
            fields: std::sync::Arc::new(m),
        }
    };
    let pages_val = Value::list(pages.iter().map(|(n, h, _)| page_ref(n, h)).collect());
    // `toc`: the declared book TOC, or a flat entry per page as a
    // fallback so a templated page without a `toc` still gets a nav.
    let toc_val = if toc_nodes.is_empty() {
        Value::List(
            pages
                .iter()
                .map(|(n, h, t)| {
                    let mut m = BTreeMap::new();
                    m.insert("title".to_string(), Value::Utf8(t.clone()));
                    m.insert("href".to_string(), Value::Utf8(h.clone()));
                    m.insert("current".to_string(), Value::Bool(n == page_name));
                    m.insert(
                        "children".to_string(),
                        Value::List(std::sync::Arc::new(Vec::new())),
                    );
                    Value::Record {
                        ty: vec!["TocEntry".to_string()],
                        fields: std::sync::Arc::new(m),
                    }
                })
                .collect::<Vec<_>>()
                .into(),
        )
    } else {
        toc_to_value(toc_nodes, page_name)
    };
    // Stamp heading anchors + section markers and collect the in-page
    // heading list for the "on this page" rail (book template), then link
    // any `[^id]` footnote references to their definitions.
    let (content, page_headings) = super::headings::process_page_headings(content);
    let content = super::headings::process_footnotes(&content);
    let mut ctx = BTreeMap::new();
    ctx.insert("content".to_string(), Value::Utf8(content));
    ctx.insert(
        "on_this_page".to_string(),
        super::headings::on_this_page_value(&page_headings),
    );
    ctx.insert("regions".to_string(), regions);
    ctx.insert("title".to_string(), Value::Utf8(title.to_string()));
    ctx.insert("page_name".to_string(), Value::Utf8(page_name.to_string()));
    ctx.insert("pages".to_string(), pages_val);
    ctx.insert("toc".to_string(), toc_val);
    ctx.insert("menu".to_string(), menu_to_value(menu_nodes, page_name));
    // `footer`: the resolved sidebar-footer buttons. Each button's href is
    // its internal `page` (→ `<page>.html`), else its raw `href`, else
    // empty; `current` marks the one linking to the page being rendered;
    // `icon` is resolved to inline SVG markup via the icon registry (which
    // also records it into the shared sprite), empty when unset/unresolved.
    let footer_val = Value::list(
        footer_nodes
            .iter()
            .map(|b| {
                let href = match (&b.page, &b.href) {
                    (Some(p), _) => format!("{p}.html"),
                    (None, Some(h)) => h.clone(),
                    (None, None) => String::new(),
                };
                let icon = b
                    .icon
                    .as_deref()
                    .and_then(|name| patterns.icons().resolve_html_icon(name, &[]))
                    .unwrap_or_default();
                let mut m = BTreeMap::new();
                m.insert("label".to_string(), Value::Utf8(b.label.clone()));
                m.insert("href".to_string(), Value::Utf8(href));
                m.insert(
                    "current".to_string(),
                    Value::Bool(b.page.as_deref() == Some(page_name)),
                );
                m.insert("icon".to_string(), Value::Utf8(icon));
                Value::Record {
                    ty: vec!["FooterButton".to_string()],
                    fields: std::sync::Arc::new(m),
                }
            })
            .collect(),
    );
    ctx.insert("footer".to_string(), footer_val);
    // The resolved presentation deck — populated only on the
    // presentation build path, empty (an empty list) for normal pages.
    ctx.insert("deck".to_string(), deck);
    ctx.insert("theme_toggle".to_string(), Value::Bool(theme_toggle));
    ctx.insert("search".to_string(), Value::Bool(search));
    ctx.insert("home_href".to_string(), Value::Utf8(home_href.to_string()));
    ctx.insert(
        "home_title".to_string(),
        Value::Utf8(home_title.to_string()),
    );
    let arg = Value::Record {
        ty: vec!["TemplateCtx".to_string()],
        fields: std::sync::Arc::new(ctx),
    };
    let Ok(Value::List(items)) = doc.call_value(&fv, &[arg]) else {
        return Rendered::default();
    };
    // Partition the template's top-level fundamentals: a `Head` hoists its
    // children into `<head>`; everything else renders into the `<body>`.
    let mut rendered = Rendered::default();
    for v in items.iter() {
        match head_fundamental_html(doc, v, patterns) {
            Some(h) => rendered.head.push_str(&h),
            None => rendered
                .body
                .push_str(&render_html_variant(doc, v, 0, patterns)),
        }
    }
    rendered
}

pub(crate) fn render_block(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> Option<String> {
    // The structural kinds every backend shares — visibility filtering,
    // `notes` / `frontmatter`, `partial` deposits, and cycle-guarded
    // `collect` gathering — dispatch through the common walker; their
    // children (if any) feed back through this function.
    let mut buf = String::new();
    let structural = crate::render::walk_structural(doc, block, patterns, &mut |b| {
        if let Some(s) = render_block(doc, b, patterns, base_dir) {
            buf.push_str(&s);
        }
        Ok::<(), std::convert::Infallible>(())
    });
    if structural.is_some() {
        return Some(buf);
    }
    let rendered = match block.kind() {
        "column" => Some(render_column(doc, block, patterns, base_dir)),
        // A `region` is a named content slot pulled out and rendered
        // separately by `build_normal_page` (it becomes a `TemplateCtx`
        // region). Reached here only when nested outside the page top
        // level — render nothing so it never leaks into the default body.
        "region" => Some(String::new()),
        // A presentation `fragment` wraps its children in a step-reveal
        // box (`<div class="wdoc-fragment">`); the deck player reveals
        // them one keypress at a time. Like `column`, the `@children`
        // wrapping can't live in a WCL `lower`, so it's rendered here.
        "fragment" => Some(render_fragment(doc, block, patterns, base_dir)),
        "table" => Some(render_table(doc, block, patterns)),
        // Lists are fundamental HTML blocks rendered directly (like
        // `table`): a pure-WCL lower can't see `@children`, so it can't
        // wrap nested items in the `<ul>/<ol>` that valid HTML and the
        // CSS-counter "1.1" numbering need. An `li` is normally reached
        // via `render_list`; the arm here is defensive.
        "list" => Some(render_list(doc, block, patterns, base_dir)),
        "li" => Some(render_li(doc, block, false, patterns, base_dir)),
        // A page image — the asset copy + src rewrite is special-cased in
        // Rust (the same `image` block is also a diagram shape; see
        // render_shape). Records usage in the image registry.
        "image" => Some(crate::image::render_html(block, patterns.images())),
        // A `file` block ships an arbitrary file into the output (copied
        // into its `dir`, default `_wdoc/`) and renders a download link when
        // `as` is set. Special-cased in Rust like `image` — the copy + path
        // rewrite aren't expressible in WCL.
        "file" => Some(crate::file::render_html(block, patterns.files())),
        // A page video — like `image`, the asset copy, URL classification,
        // and click-to-play facade are special-cased in Rust. Records usage
        // in the video registry (driving the local-file/poster copy).
        "video" => Some(crate::video::render_html(block, patterns.videos())),
        "diagram" => Some(render_diagram(doc, block, patterns, base_dir)),
        // Page-level SVG blocks whose geometry is a WCL `lower_svg` fn
        // (fitted viewBox, auto height) — see render/svg/standalone.rs.
        kind @ ("sequence_diagram" | "state_diagram") => {
            Some(render_lowered_svg_block(doc, block, kind, patterns))
        }
        // The terminal is special-cased in Rust: its grid model, ANSI
        // handling, and asciinema replay aren't expressible in WCL.
        // `base_dir` lets a `source` recording path resolve relative to
        // the source file.
        "terminal" => Some(crate::terminal::render_terminal(doc, block, base_dir)),
        // Renders its body to a Markdown string (the same output the Markdown /
        // skill backend produces) and shows it in a highlighted `code` block.
        // Special-cased here because reaching into the Markdown emitter from a
        // WCL `lower` isn't possible. Book-only; in other backends its stub
        // `lower` makes it render empty.
        "markdown_source" => Some(render_markdown_source(doc, block, patterns, base_dir)),
        // An example source listing + a live preview of the same children
        // under both palettes (side by side). Special-cased in Rust: it reads
        // the children's source text and re-renders them into themed wrappers.
        "demo" => Some(crate::demo::render_html(doc, block, patterns, base_dir)),
        // Wireframe widgets (`wf_*`) are diagram shapes now — they render only
        // inside a `diagram` (via `render_shape`), never as a page block.
        // A `wdoc_repeater` renders its body once per element of `each`.
        "wdoc_repeater" => Some(render_repeat(doc, block, patterns, base_dir)),
        // A `wdoc_instance` renders the component named by its `component`
        // value (render-by-reference) — a different component per data element.
        "wdoc_instance" => Some(render_instance(doc, block, patterns, base_dir)),
        // A `wdoc_content` marks where a component instance's own children
        // render — emit the sentinel; `render_component` substitutes it.
        // (Outside a component it has no effect; the sentinel is invisible.)
        "wdoc_content" => Some(WF_CONTENT_SLOT.to_string()),
        // Everything else lowers via WCL — `text` / `code` (which emit the
        // new `Inline` / `Highlighted` leaf fundamentals), the headings,
        // `callout`, and any custom block — UNLESS the kind names a
        // user-defined `wdoc_component`, in which case we expand its
        // declarative body with the instance's slots bound. Lowering
        // declarations are top-level fields, not blocks, so they never
        // reach here.
        kind => {
            if let Some(def) = doc.component_def(kind) {
                Some(render_component(doc, block, &def, patterns, base_dir))
            } else {
                Some(lower_html_block(doc, block, kind, patterns, base_dir))
            }
        }
    };
    rendered.map(|html| anchor_block(block, html, patterns))
}

/// In comment / edit mode (the `--comment` / `--edit` dev server), stamp a
/// rendered block's root tag so the injected JS client can locate it:
///
/// - both modes emit `data-wcl-block` + `data-wcl-kind` — the comment client
///   keys off these to compute a positional locator;
/// - edit mode additionally emits `data-wcl-span="start:end"` (byte offsets
///   into the declaring file) and `data-wcl-file="<path>"` so the WYSIWYG
///   client can map the block back to the exact AST node to mutate.
///
/// A no-op outside both modes, so normal builds emit no extra markup.
fn anchor_block(block: &Block<'_>, html: String, patterns: &InlinePatterns) -> String {
    if !patterns.anchor_mode() {
        return html;
    }
    let kind = block.kind();
    // Synthetic wrappers / multi-root expansions have no single root tag to
    // stamp; their content children are anchored individually when rendered.
    if matches!(
        kind,
        "region" | "column" | "fragment" | "wdoc_repeater" | "wdoc_instance" | "wdoc_content"
    ) {
        return html;
    }
    let mut attrs = format!(" data-wcl-block data-wcl-kind=\"{}\"", escape_html(kind));
    if patterns.edit_mode() {
        let span = block.span();
        attrs.push_str(&format!(
            " data-wcl-span=\"{}:{}\" data-wcl-file=\"{}\"",
            span.start,
            span.end,
            escape_html(block.named_source().name()),
        ));
    }
    splice_attrs(&html, &attrs)
}

/// Insert `attrs` into the first opening tag of `html`. Falls back to a
/// `display:contents` wrapper when there's no leading element tag (rare —
/// keeps the anchor present without disturbing layout).
fn splice_attrs(html: &str, attrs: &str) -> String {
    if let Some(lt) = html.find('<')
        && html[lt + 1..].starts_with(|c: char| c.is_ascii_alphabetic())
        && let Some(rel_gt) = find_tag_end(&html[lt..])
    {
        // Insert before a self-closing `/` if present, else before `>`.
        let gt = lt + rel_gt;
        let at = if html[..gt].ends_with('/') {
            gt - 1
        } else {
            gt
        };
        let mut out = String::with_capacity(html.len() + attrs.len());
        out.push_str(&html[..at]);
        out.push_str(attrs);
        out.push_str(&html[at..]);
        return out;
    }
    format!("<span{attrs} style=\"display:contents\">{html}</span>")
}

/// Byte offset of the `>` that closes the opening tag starting at `s[0]`,
/// honouring quoted attribute values so a `>` inside an attribute is skipped.
fn find_tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            (None, _) => {}
        }
    }
    None
}

/// Expand a `wdoc_component` instance: bind each declared slot to the
/// instance's matching field (or the slot's `default`), then render the
/// definition's `wdoc_body` children under those bindings. A `wdoc_content`
/// block inside the body is replaced with the instance's own nested
/// children (a layout content slot).
pub(crate) fn render_component(
    doc: &Document,
    instance: &Block<'_>,
    def: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    // Stop runaway self-referential components.
    if instance.binding_scope_depth() > MAX_LOWER_DEPTH {
        return depth_marker();
    }
    let children = expand_component_children(instance, def);
    let mut out: String = children
        .iter()
        .filter_map(|b| render_block(doc, b, patterns, base_dir))
        .collect();
    if out.contains(WF_CONTENT_SLOT) {
        let content: String = instance
            .blocks()
            .filter_map(|b| render_block(doc, &b, patterns, base_dir))
            .collect();
        out = out.replace(WF_CONTENT_SLOT, &content);
    }
    out
}

/// Render a `wdoc_instance`: resolve the `wdoc_component` named by the
/// instance's `component` value and render it with the instance's fields
/// bound to the component's slots (render-by-reference). A `component` that
/// names no declared component renders nothing.
pub(crate) fn render_instance(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    if block.binding_scope_depth() > MAX_LOWER_DEPTH {
        return depth_marker();
    }
    match instance_target_def(block) {
        Some(def) => render_component(doc, block, &def, patterns, base_dir),
        None => String::new(),
    }
}

/// Render a `wdoc_repeater`: evaluate `each` to a list and render the
/// repeater's body blocks once per element, binding the element to the
/// symbol named by `as`. A non-list `each` renders nothing.
pub(crate) fn render_repeat(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    if block.binding_scope_depth() > MAX_LOWER_DEPTH {
        return depth_marker();
    }
    expand_repeater_children(block)
        .iter()
        .filter_map(|b| render_block(doc, b, patterns, base_dir))
        .collect()
}

/// Render a presentation `fragment` → `<div class="wdoc-fragment">`
/// wrapping its rendered children. The deck player (`presentation.js`)
/// reveals each `.wdoc-fragment` in turn before advancing the slide.
/// Any author `class`es are appended after the `wdoc-fragment` marker.
pub(crate) fn render_fragment(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    let mut classes = vec!["wdoc-fragment".to_string()];
    classes.extend(field_utf8_list(block, "class"));
    let children: String = block
        .blocks()
        .filter_map(|b| render_block(doc, &b, patterns, base_dir))
        .collect();
    let cls = classes_attr_from_names(&classes);
    let mut out = format!("<div{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    write!(out, ">{children}</div>").expect("write to String");
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

/// Render a `@block("list")` instance → `<ul>` (bullet, the default) or
/// `<ol class="wdoc-list-numbered">` (when `style = :numbered`). Its `li`
/// children render via [`render_li`]; the numbered class drives the
/// CSS-counter "1.1" sublist numbering in the bundled `wdoc-list` stylesheet.
pub(crate) fn render_list(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    let numbered = field_symbol(block, "style").as_deref() == Some("numbered");
    let mut classes: Vec<String> = Vec::new();
    if numbered {
        classes.push("wdoc-list-numbered".to_string());
    }
    classes.extend(field_utf8_list(block, "class"));
    let items: String = block
        .blocks()
        .filter(|b| b.kind() == "li")
        .map(|b| render_li(doc, &b, numbered, patterns, base_dir))
        .collect();
    list_html(numbered, &classes, field_id(block, "id").as_deref(), &items)
}

/// Emit `<ul|ol …>{inner}</ul|ol>` — `ol` when `numbered`.
fn list_html(numbered: bool, classes: &[String], id: Option<&str>, inner: &str) -> String {
    let tag = if numbered { "ol" } else { "ul" };
    let cls = classes_attr_from_names(classes);
    let mut out = format!("<{tag}{cls}");
    append_attr(&mut out, "id", id);
    write!(out, ">{inner}</{tag}>").expect("write to String");
    out
}

/// Render a single `li` → `<li>inline-text + optional sublist</li>`. The
/// item text runs through the inline-pattern engine (bold / italic / code /
/// links / icons / math). `numbered` is the enclosing list's style, so an
/// `li`-under-`li` sublist keeps the parent's numbering (a numbered list
/// nests as "1.1"); a nested `list` block instead sets its own style.
pub(crate) fn render_li(
    doc: &Document,
    block: &Block<'_>,
    numbered: bool,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    let cls = class_attr(block);
    let mut out = format!("<li{cls}");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    out.push('>');
    // `@inline(0) text` arrives as the block's label, not a named field.
    out.push_str(&patterns.render(doc, &label_string(block).unwrap_or_default()));
    // A sublist built from nested `li`s, wrapped in a list of the same
    // style (so numbered sublists count as "1.1").
    let sub_items: String = block
        .blocks()
        .filter(|b| b.kind() == "li")
        .map(|b| render_li(doc, &b, numbered, patterns, base_dir))
        .collect();
    if !sub_items.is_empty() {
        let mut classes: Vec<String> = Vec::new();
        if numbered {
            classes.push("wdoc-list-numbered".to_string());
        }
        out.push_str(&list_html(numbered, &classes, None, &sub_items));
    }
    // A nested `list` block carries its own bullet/numbered style.
    for b in block.blocks().filter(|b| b.kind() == "list") {
        out.push_str(&render_list(doc, &b, patterns, base_dir));
    }
    out.push_str("</li>");
    out
}

/// Render a `@block("table")` instance. Two authoring forms:
///
/// - **Computed** — a `rows` value field (a list of cell-lists, e.g.
///   `map`ped from data) plus an optional `header` row. Taken when a
///   `rows` field is present, so a component/repeater can feed a table.
/// - **Pipe rows** — WCL's pipe-table syntax (`rows: | a | b |`), read
///   via `Block::tables()`; the first row is the header.
///
/// Either way each cell is rendered by `cell_to_html`, so utf8 cells pick
/// up inline patterns and other scalars stringify.
pub(crate) fn render_table(doc: &Document, block: &Block<'_>, patterns: &InlinePatterns) -> String {
    let classes_for = |block: &Block<'_>| -> Vec<String> {
        let mut classes: Vec<String> = vec!["wdoc-table".to_string()];
        classes.extend(field_utf8_list(block, "class"));
        classes
    };

    // Computed-rows form: `rows = <list of cell-lists>` (+ optional
    // `header`). Reads the field on the passed-in Block view, so a value
    // fed from a component slot / repeater binding resolves here.
    if let Some(Value::List(body_rows)) = computed_field(block, "rows") {
        let to_cells = |row: &Value| -> Vec<String> {
            match row {
                Value::List(cells) => cells
                    .iter()
                    .map(|c| cell_to_html(doc, patterns, c))
                    .collect(),
                // A non-list row degrades to a single cell rather than vanishing.
                other => vec![cell_to_html(doc, patterns, other)],
            }
        };
        let header: Vec<String> = match computed_field(block, "header") {
            Some(Value::List(cells)) => cells
                .iter()
                .map(|c| cell_to_html(doc, patterns, c))
                .collect(),
            _ => Vec::new(),
        };
        let body: Vec<Vec<String>> = body_rows.iter().map(to_cells).collect();
        if header.is_empty() && body.is_empty() {
            return String::new();
        }
        return table_html(
            field_id(block, "id").as_deref(),
            &classes_for(block),
            &header,
            &body,
        );
    }

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
    let header = &rows[0];
    let body = &rows[1..];
    table_html(
        field_id(block, "id").as_deref(),
        &classes_for(block),
        header,
        body,
    )
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

/// Render an `HtmlFundamental::Inline { text }` by running the inline-
/// pattern engine (bold / italic / link / icon) over `text`. The Rust
/// regex engine stays the leaf; the `<p>` / `<span>` wrappers around it
/// are emitted by the WCL `text` lower (`lib/text.wcl`).
pub(crate) fn render_inline_fundamental(
    doc: &Document,
    map: &BTreeMap<String, Value>,
    patterns: &InlinePatterns,
) -> String {
    let text = map_utf8(map, "text").unwrap_or_default();
    patterns.render(doc, &text)
}

/// Render a `markdown_source` block: lower its body to a Markdown string (via
/// the Markdown backend's emitter, so the output matches `wcl wdoc markdown` /
/// `skill`) and present it in a highlighted `code` block, mirroring the WCL
/// `code` lower's `<pre class="code-block"><code class="language-markdown">…`.
///
/// When `start_page` is set, the body is lowered under the skill-folder link
/// layout (`reference` ⇒ a `references/…` page with `../` prefixes), with
/// internal links validated against `pages` (the skill site's page names) — so
/// a skill page's Markdown reproduces faithfully even inside the book build.
fn render_markdown_source(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    let children: Vec<Block<'_>> = block.blocks().collect();
    let out_dir = patterns.output_dir();
    let pages: std::collections::HashSet<String> =
        field_utf8_list(block, "pages").into_iter().collect();
    let reference = field_bool(block, "reference").unwrap_or(true);
    // Stem for any SVGs the body's Markdown writes — the previewed page's
    // name, so `![](_wdoc/…)` refs match the real skill output.
    let stem = field_utf8(block, "id").unwrap_or_else(|| "markdown_source".to_string());

    let emit = || {
        crate::markdown::emit::body_to_markdown(doc, &children, &stem, patterns, base_dir, &out_dir)
    };
    let md = match field_utf8(block, "start_page") {
        Some(start) => patterns.with_skill_layout(start, reference, pages, emit),
        None => emit(),
    };
    let body = match md {
        Ok(s) => highlight::highlight_html(s.trim_end(), "markdown", false),
        // Surface a lowering failure in the preview rather than failing the
        // whole build — the block is a documentation aid, not load-bearing.
        Err(_) => escape_html("markdown_source: failed to render body to Markdown"),
    };
    format!("<pre class=\"code-block\"><code class=\"language-markdown\">{body}</code></pre>")
}

/// Render an `HtmlFundamental::Highlighted { source, language }` to the
/// syntect-produced `<span class="tok-…">` token runs (the code body, no
/// wrapper). The WCL `code` lower wraps it in `<pre><code class="language-…">`.
pub(crate) fn render_highlighted_fundamental(map: &BTreeMap<String, Value>) -> String {
    let source = map_utf8(map, "source").unwrap_or_default();
    let language = map_utf8(map, "language").unwrap_or_default();
    // Code blocks render as the book code-card with a line-number gutter;
    // each line is wrapped in `<span class="code-line">` for the counter.
    highlight::highlight_html(&source, &language, true)
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
    patterns: &InlinePatterns,
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
        for a in attrs.iter() {
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
        for child in children.iter() {
            out.push_str(&render_html_variant(doc, child, depth + 1, patterns));
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
