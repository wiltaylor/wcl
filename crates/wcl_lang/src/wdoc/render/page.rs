use crate::wdoc::markup::{self, b, elem, raw_html, s, text};
use crate::wdoc::model::*;
use crate::wdoc::render::layout::render_layout_items;
use crate::Value;
use indexmap::IndexMap;

/// Render a single page as a complete HTML document.
pub fn render_page(doc: &WdocDocument, page: &Page, css_path: &str) -> String {
    match page.template.unwrap_or(doc.template) {
        WdocTemplate::Book => render_book_page(doc, page, css_path),
        WdocTemplate::Site => render_site_page(doc, page, css_path),
        WdocTemplate::Presentation => render_presentation_page(doc, page, css_path),
    }
}

fn render_book_page(doc: &WdocDocument, page: &Page, css_path: &str) -> String {
    let mut content_html = String::new();
    render_layout_items(&page.layout.children, &mut content_html);
    render_site_shell(SiteShell {
        doc,
        title: &page.title,
        css_path,
        content_html: &content_html,
        body_class: "wdoc-template-book",
        main_class: "wdoc-content",
        before_main: Some(render_book_nav(doc, &page.section_id)),
        after_main: None,
        runtime: if page_has_runtime(page) {
            Some(page_signal_runtime(page))
        } else {
            None
        },
    })
}

fn render_site_page(doc: &WdocDocument, page: &Page, css_path: &str) -> String {
    let mut content_html = String::new();
    render_layout_items(&page.layout.children, &mut content_html);
    render_site_content_page(
        doc,
        &page.title,
        &content_html,
        css_path,
        if page_has_runtime(page) {
            Some(page_signal_runtime(page))
        } else {
            None
        },
    )
}

pub fn render_generated_site_page(
    doc: &WdocDocument,
    title: &str,
    content_html: &str,
    css_path: &str,
) -> String {
    render_site_content_page(doc, title, content_html, css_path, None)
}

fn render_site_content_page(
    doc: &WdocDocument,
    title: &str,
    content_html: &str,
    css_path: &str,
    runtime: Option<String>,
) -> String {
    let before_main = site_before_main(doc, css_path);
    let after_main = doc
        .site
        .footer_html
        .as_ref()
        .map(|html| relativize_chrome_links(html, css_path));
    render_site_shell(SiteShell {
        doc,
        title,
        css_path,
        content_html,
        body_class: "wdoc-template-site",
        main_class: "wdoc-site-main",
        before_main,
        after_main,
        runtime,
    })
}

fn site_before_main(doc: &WdocDocument, css_path: &str) -> Option<String> {
    let mut html = String::new();
    if let Some(header) = &doc.site.header_html {
        html.push_str(&relativize_chrome_links(header, css_path));
        html.push('\n');
    }
    if let Some(nav) = &doc.site.nav_html {
        html.push_str(&relativize_chrome_links(nav, css_path));
        html.push('\n');
    }
    if html.is_empty() {
        None
    } else {
        Some(html)
    }
}

fn relativize_chrome_links(html: &str, css_path: &str) -> String {
    let prefix = css_path.strip_suffix("styles.css").unwrap_or_default();
    if prefix.is_empty() {
        return html.to_string();
    }
    let html = relativize_attr(html, "href", prefix);
    relativize_attr(&html, "src", prefix)
}

fn relativize_attr(html: &str, attr: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let needle = format!("{attr}=\"");
    while let Some(idx) = rest.find(&needle) {
        let (before, after_before) = rest.split_at(idx);
        out.push_str(before);
        out.push_str(&needle);
        let value_start = needle.len();
        let after = &after_before[value_start..];
        let Some(value_end) = after.find('"') else {
            out.push_str(after);
            return out;
        };
        let value = &after[..value_end];
        if should_rewrite_chrome_url(value) {
            out.push_str(prefix);
        }
        out.push_str(value);
        rest = &after[value_end..];
    }
    out.push_str(rest);
    out
}

fn should_rewrite_chrome_url(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('#')
        && !value.starts_with('/')
        && !value.starts_with("./")
        && !value.starts_with("../")
        && !value.contains(':')
}

struct SiteShell<'a> {
    doc: &'a WdocDocument,
    title: &'a str,
    css_path: &'a str,
    content_html: &'a str,
    body_class: &'a str,
    main_class: &'a str,
    before_main: Option<String>,
    after_main: Option<String>,
    runtime: Option<String>,
}

fn render_site_shell(shell: SiteShell<'_>) -> String {
    let mut body_children = Vec::new();
    if let Some(before_main) = shell.before_main {
        body_children.push(raw_html(before_main));
    }
    body_children.push(elem(
        "main",
        &[("class_name", s(shell.main_class))],
        vec![raw_html(shell.content_html)],
    ));
    if let Some(after_main) = shell.after_main {
        body_children.push(raw_html(after_main));
    }
    if let Some(runtime) = shell.runtime {
        body_children.push(script_node(&runtime));
    }
    body_children.push(script_node(
        crate::wdoc::assets::theme_runtime_js().expect("bundled wdoc theme runtime"),
    ));

    render_document_html(
        shell.doc,
        shell.title,
        shell.css_path,
        shell.content_html,
        shell.body_class,
        body_children,
    )
}

fn render_presentation_page(doc: &WdocDocument, page: &Page, css_path: &str) -> String {
    let mut content_html = String::new();
    render_layout_items(&page.layout.children, &mut content_html);
    let nav = presentation_nav(doc, page);

    let mut body_children = vec![elem(
        "main",
        &[
            ("class_name", s("wdoc-presentation")),
            ("aria_label", s("Presentation slide")),
        ],
        vec![
            elem(
                "nav",
                &[
                    ("class_name", s("wdoc-presentation-nav")),
                    ("aria_hidden", s("true")),
                ],
                presentation_nav_links(nav),
            ),
            elem(
                "section",
                &[("class_name", s("wdoc-slide"))],
                vec![raw_html(content_html.clone())],
            ),
        ],
    )];
    if page_has_runtime(page) {
        body_children.push(script_node(&page_signal_runtime(page)));
    }
    body_children.push(script_node(
        crate::wdoc::assets::theme_runtime_js().expect("bundled wdoc theme runtime"),
    ));
    body_children.push(script_node(
        crate::wdoc::assets::presentation_runtime_js().expect("bundled wdoc presentation runtime"),
    ));

    render_document_html(
        doc,
        &page.title,
        css_path,
        &content_html,
        "wdoc-template-presentation",
        body_children,
    )
}

fn presentation_nav_links(nav: PresentationNav<'_>) -> Vec<Value> {
    [
        (nav.up, "up", "Previous section"),
        (nav.left, "left", "Previous slide"),
        (nav.right, "right", "Next slide"),
        (nav.down, "down", "Next section"),
    ]
    .into_iter()
    .filter_map(|(target, direction, aria_label)| {
        target.map(|target| {
            let mut attrs = IndexMap::new();
            attrs.insert("tag".to_string(), s("a"));
            attrs.insert("href".to_string(), s(format!("{}.html", target.id)));
            attrs.insert(format!("data_wdoc_slide_{direction}"), b(true));
            attrs.insert("aria_label".to_string(), s(aria_label));
            Value::Map(attrs)
        })
    })
    .collect()
}

fn render_document_html(
    doc: &WdocDocument,
    title: &str,
    css_path: &str,
    content_html: &str,
    body_class: &str,
    body_children: Vec<Value>,
) -> String {
    let head = markup::render_html(&document_head(doc, title, css_path, content_html))
        .expect("wdoc document head should serialize as HTML");
    let body = markup::render_html(&elem(
        "body",
        &[("class_name", s(body_class))],
        body_children,
    ))
    .expect("wdoc document body should serialize as HTML");
    format!("<!DOCTYPE html><html lang=\"en\">{head}{body}</html>\n")
}

fn document_head(doc: &WdocDocument, title: &str, css_path: &str, content_html: &str) -> Value {
    let mut children = vec![
        elem("meta", &[("charset", s("utf-8"))], vec![]),
        elem(
            "meta",
            &[
                ("name", s("viewport")),
                ("content_attr", s("width=device-width, initial-scale=1")),
            ],
            vec![],
        ),
        elem("title", &[], vec![text(format!("{title} — {}", doc.title))]),
        elem(
            "link",
            &[("rel", s("stylesheet")), ("href", s(css_path))],
            vec![],
        ),
        elem(
            "link",
            &[
                ("rel", s("stylesheet")),
                ("href", s("highlight-light.min.css")),
                ("id", s("hljs-light")),
            ],
            vec![],
        ),
        elem(
            "link",
            &[
                ("rel", s("stylesheet")),
                ("href", s("highlight-dark.min.css")),
                ("id", s("hljs-dark")),
                ("disabled", b(true)),
            ],
            vec![],
        ),
        elem(
            "script",
            &[("defer", b(true)), ("src", s("highlight.min.js"))],
            vec![],
        ),
        elem(
            "script",
            &[("defer", b(true)), ("src", s("wcl-grammar.js"))],
            vec![],
        ),
    ];
    if content_html.contains("data-wdoc-equation=") {
        children.push(script_node(
            crate::wdoc::assets::mathjax_config_js().expect("bundled wdoc mathjax config"),
        ));
        children.push(elem(
            "script",
            &[
                ("defer", b(true)),
                (
                    "src",
                    s("https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex-mml-chtml.js"),
                ),
            ],
            vec![],
        ));
    }
    elem("head", &[], children)
}

fn script_node(source: &str) -> Value {
    elem(
        "script",
        &[("html", s(source.replace("</", "<\\/")))],
        vec![],
    )
}

#[derive(Debug)]
struct PresentationNav<'a> {
    left: Option<&'a Page>,
    right: Option<&'a Page>,
    up: Option<&'a Page>,
    down: Option<&'a Page>,
}

fn presentation_nav<'a>(doc: &'a WdocDocument, page: &Page) -> PresentationNav<'a> {
    let grid = presentation_grid(doc);
    let (row, col) = grid
        .iter()
        .enumerate()
        .find_map(|(row, group)| {
            group
                .iter()
                .position(|candidate| candidate.id == page.id)
                .map(|col| (row, col))
        })
        .unwrap_or((0, 0));
    let row_pages = grid.get(row).map(Vec::as_slice).unwrap_or(&[]);

    PresentationNav {
        left: col
            .checked_sub(1)
            .and_then(|idx| row_pages.get(idx).copied()),
        right: row_pages.get(col + 1).copied(),
        up: row
            .checked_sub(1)
            .and_then(|idx| nearest_slide_in_group(grid.get(idx), col)),
        down: nearest_slide_in_group(grid.get(row + 1), col),
    }
}

fn nearest_slide_in_group<'a>(group: Option<&Vec<&'a Page>>, col: usize) -> Option<&'a Page> {
    let group = group?;
    let idx = col.min(group.len().saturating_sub(1));
    group.get(idx).copied()
}

fn presentation_grid(doc: &WdocDocument) -> Vec<Vec<&Page>> {
    let mut groups = Vec::new();
    for section in &doc.sections {
        let mut pages = Vec::new();
        collect_pages_by_section(std::slice::from_ref(section), &doc.pages, &mut pages);
        if !pages.is_empty() {
            groups.push(pages);
        }
    }

    let mut uncategorized = Vec::new();
    for page in &doc.pages {
        if !groups
            .iter()
            .flatten()
            .any(|candidate| candidate.id == page.id)
        {
            uncategorized.push(page);
        }
    }
    if !uncategorized.is_empty() {
        groups.push(uncategorized);
    }
    groups
}

fn collect_pages_by_section<'a>(
    sections: &[Section],
    all_pages: &'a [Page],
    out: &mut Vec<&'a Page>,
) {
    for section in sections {
        if let Some(page) = all_pages.iter().find(|p| p.section_id == section.id) {
            out.push(page);
        }
        collect_pages_by_section(&section.children, all_pages, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_with_html(rendered_html: &str) -> Page {
        Page {
            id: "test".to_string(),
            section_id: "section".to_string(),
            title: "Test".to_string(),
            template: None,
            path: None,
            date: None,
            draft: false,
            weight: None,
            summary: None,
            tags: Vec::new(),
            categories: Vec::new(),
            params: Default::default(),
            layout: Layout {
                children: vec![LayoutItem::Content(ContentBlock {
                    kind: "wdoc::paragraph".to_string(),
                    id: None,
                    rendered_html: rendered_html.to_string(),
                    style: None,
                })],
            },
            signals: Vec::new(),
            bindings: Vec::new(),
        }
    }

    fn doc_with_page(page: Page) -> WdocDocument {
        WdocDocument {
            name: "doc".to_string(),
            title: "Doc".to_string(),
            template: WdocTemplate::Book,
            version: None,
            author: None,
            site: SiteConfig::default(),
            sections: vec![Section {
                id: "section".to_string(),
                short_id: "section".to_string(),
                title: "Section".to_string(),
                children: Vec::new(),
            }],
            pages: vec![page],
            styles: Vec::new(),
            extra_css: String::new(),
        }
    }

    fn presentation_page(id: &str, section_id: &str) -> Page {
        Page {
            id: id.to_string(),
            section_id: section_id.to_string(),
            title: id.to_string(),
            template: None,
            path: None,
            date: None,
            draft: false,
            weight: None,
            summary: None,
            tags: Vec::new(),
            categories: Vec::new(),
            params: Default::default(),
            layout: Layout {
                children: Vec::new(),
            },
            signals: Vec::new(),
            bindings: Vec::new(),
        }
    }

    fn section_with_children(id: &str, children: Vec<Section>) -> Section {
        Section {
            id: id.to_string(),
            short_id: id.rsplit('.').next().unwrap_or(id).to_string(),
            title: id.to_string(),
            children,
        }
    }

    #[test]
    fn mathjax_is_loaded_only_when_page_contains_equations() {
        let page = page_with_html(
            "<div class=\"wdoc-equation\" data-wdoc-equation=\"display\">\\[x\\]</div>",
        );
        let doc = doc_with_page(page.clone());
        let html = render_page(&doc, &page, "styles.css");
        assert!(html.contains("MathJax"));
        assert!(html.contains("tex-mml-chtml.js"));

        let plain_page = page_with_html("<p class=\"wdoc-paragraph\">No math</p>");
        let plain_doc = doc_with_page(plain_page.clone());
        let plain_html = render_page(&plain_doc, &plain_page, "styles.css");
        assert!(!plain_html.contains("tex-mml-chtml.js"));
    }

    #[test]
    fn presentation_template_renders_slide_shell_without_book_nav() {
        let mut page = page_with_html("<h1 class=\"wdoc-heading\">Slide</h1>");
        page.template = Some(WdocTemplate::Presentation);
        let doc = doc_with_page(page.clone());

        let html = render_page(&doc, &page, "styles.css");

        assert!(html.contains("wdoc-template-presentation"));
        assert!(html.contains("wdoc-slide"));
        assert!(!html.contains("wdoc-nav"));
        assert!(!html.contains("wdoc-presentation-chrome"));
        assert!(!html.contains("wdoc-presentation-count"));
    }

    #[test]
    fn presentation_navigation_moves_within_and_between_section_rows() {
        let doc = WdocDocument {
            name: "deck".to_string(),
            title: "Deck".to_string(),
            template: WdocTemplate::Presentation,
            version: None,
            author: None,
            site: SiteConfig::default(),
            sections: vec![
                section_with_children(
                    "deck.row_a",
                    vec![
                        section_with_children("deck.row_a.a1", vec![]),
                        section_with_children("deck.row_a.a2", vec![]),
                    ],
                ),
                section_with_children(
                    "deck.row_b",
                    vec![
                        section_with_children("deck.row_b.b1", vec![]),
                        section_with_children("deck.row_b.b2", vec![]),
                    ],
                ),
            ],
            pages: vec![
                presentation_page("a1", "deck.row_a.a1"),
                presentation_page("a2", "deck.row_a.a2"),
                presentation_page("b1", "deck.row_b.b1"),
                presentation_page("b2", "deck.row_b.b2"),
            ],
            styles: Vec::new(),
            extra_css: String::new(),
        };

        let html = render_page(&doc, &doc.pages[1], "styles.css");

        assert!(html.contains("class=\"wdoc-presentation-nav\""));
        assert!(html.contains("href=\"a1.html\" data-wdoc-slide-left"));
        assert!(html.contains("href=\"b2.html\" data-wdoc-slide-down"));
        assert!(!html.contains("href=\"a2.html\" data-wdoc-slide-up"));
        assert!(!html.contains("href=\"b1.html\" data-wdoc-slide-right"));
    }
}

fn page_has_runtime(page: &Page) -> bool {
    !page.signals.is_empty() || !page.bindings.is_empty()
}

fn page_signal_runtime(page: &Page) -> String {
    let signals = page
        .signals
        .iter()
        .map(|signal| {
            serde_json::json!({
                "name": signal.name,
                "initial": signal.initial,
                "type": signal.type_name,
            })
        })
        .collect::<Vec<_>>();
    let bindings = page
        .bindings
        .iter()
        .map(|binding| {
            serde_json::json!({
                "name": binding.name,
                "signal": binding.signal,
                "target": binding.target,
                "property": binding.property,
                "path": binding.path,
                "format": binding.format,
            })
        })
        .collect::<Vec<_>>();
    let data = serde_json::json!({
        "signals": signals,
        "bindings": bindings,
    })
    .to_string()
    .replace("</", "<\\/");
    crate::wdoc::assets::page_signal_runtime_js(&data).expect("bundled wdoc page signal runtime")
}

fn render_book_nav(doc: &WdocDocument, active_section: &str) -> String {
    markup::render_html(&elem(
        "nav",
        &[("class_name", s("wdoc-nav"))],
        vec![
            elem(
                "div",
                &[("class_name", s("wdoc-nav-title"))],
                vec![text(&doc.title)],
            ),
            elem(
                "ul",
                &[],
                render_nav_sections(&doc.sections, &doc.pages, active_section),
            ),
            elem(
                "div",
                &[
                    ("class_name", s("wdoc-theme-toggle")),
                    ("id", s("wdoc-theme-toggle")),
                ],
                vec![
                    elem(
                        "span",
                        &[
                            ("id", s("wdoc-theme-icon")),
                            ("class_name", s("wdoc-theme-icon")),
                        ],
                        vec![raw_html("&#x1F319;")],
                    ),
                    elem(
                        "div",
                        &[("class_name", s("wdoc-theme-toggle-track"))],
                        vec![elem(
                            "div",
                            &[("class_name", s("wdoc-theme-toggle-knob"))],
                            vec![],
                        )],
                    ),
                    elem("span", &[], vec![text("Dark mode")]),
                ],
            ),
        ],
    ))
    .expect("wdoc book nav should serialize as HTML")
}

fn render_nav_sections(sections: &[Section], pages: &[Page], active_section: &str) -> Vec<Value> {
    sections
        .iter()
        .map(|section| {
            let page_file = pages
                .iter()
                .find(|p| p.section_id == section.id)
                .map(|p| format!("{}.html", p.id))
                .unwrap_or_else(|| "#".to_string());
            let mut children = vec![elem(
                "a",
                &[
                    ("href", s(page_file)),
                    (
                        "class_name",
                        if active_section == section.id {
                            s("active")
                        } else {
                            Value::Null
                        },
                    ),
                ],
                vec![text(&section.title)],
            )];
            if !section.children.is_empty() {
                children.push(elem(
                    "ul",
                    &[],
                    render_nav_sections(&section.children, pages, active_section),
                ));
            }
            elem("li", &[], children)
        })
        .collect()
}
