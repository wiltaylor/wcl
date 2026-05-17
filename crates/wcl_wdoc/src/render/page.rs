use std::fmt::Write;

use crate::model::*;
use crate::render::layout::render_layout_items;

/// highlight.js local assets injected into <head>.
const HLJS_HEAD: &str = r#"<link rel="stylesheet" href="highlight-light.min.css" id="hljs-light">
<link rel="stylesheet" href="highlight-dark.min.css" id="hljs-dark" disabled>
<script defer src="highlight.min.js"></script>
<script defer src="wcl-grammar.js"></script>"#;

const MATHJAX_HEAD: &str = r#"<script>
window.MathJax = {
    tex: {
        inlineMath: [['\\(', '\\)']],
        displayMath: [['\\[', '\\]']],
        processEscapes: true
    },
    options: {
        skipHtmlTags: ['script', 'noscript', 'style', 'textarea', 'pre', 'code']
    }
};
</script>
<script defer src="https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex-mml-chtml.js"></script>"#;

/// Theme detection + highlight.js init + toggle logic.
const THEME_SCRIPT: &str = r#"<script>
(function() {
    // Determine initial theme: saved preference > system preference > light
    function getPreferred() {
        var saved = localStorage.getItem('wdoc-theme');
        if (saved === 'dark' || saved === 'light') return saved;
        if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) return 'dark';
        return 'light';
    }

    function applyTheme(theme) {
        document.documentElement.setAttribute('data-theme', theme);
        var light = document.getElementById('hljs-light');
        var dark = document.getElementById('hljs-dark');
        if (light && dark) {
            light.disabled = (theme === 'dark');
            dark.disabled = (theme !== 'dark');
        }
        var icon = document.getElementById('wdoc-theme-icon');
        if (icon) icon.textContent = (theme === 'dark') ? '\u{2600}\u{FE0F}' : '\u{1F319}';
        localStorage.setItem('wdoc-theme', theme);
    }

    // Apply immediately (before DOM ready) to prevent flash
    applyTheme(getPreferred());

    document.addEventListener('DOMContentLoaded', function() {
        // highlight.js init
        if (typeof hljs !== 'undefined') {
            if (typeof hljsDefineWcl !== 'undefined') hljs.registerLanguage('wcl', hljsDefineWcl);
            hljs.highlightAll();
        }

        // Toggle button
        var toggle = document.getElementById('wdoc-theme-toggle');
        if (toggle) {
            toggle.addEventListener('click', function() {
                var current = document.documentElement.getAttribute('data-theme') || 'light';
                applyTheme(current === 'dark' ? 'light' : 'dark');
                // Re-highlight with new theme
                if (typeof hljs !== 'undefined') {
                    document.querySelectorAll('pre code').forEach(function(el) {
                        el.removeAttribute('data-highlighted');
                        hljs.highlightElement(el);
                    });
                }
            });
        }

        // Listen for system theme changes
        if (window.matchMedia) {
            window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function(e) {
                if (!localStorage.getItem('wdoc-theme')) {
                    applyTheme(e.matches ? 'dark' : 'light');
                }
            });
        }
    });
})();
</script>"#;

const PRESENTATION_SCRIPT: &str = r#"<script>
(function() {
    function go(selector) {
        var link = document.querySelector(selector);
        if (!link) return false;
        window.location.href = link.getAttribute('href');
        return true;
    }
    document.addEventListener('keydown', function(event) {
        if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
        var tag = event.target && event.target.tagName ? event.target.tagName.toLowerCase() : '';
        if (tag === 'input' || tag === 'textarea' || tag === 'select') return;
        var handled = false;
        if (event.key === 'ArrowRight' || event.key === 'PageDown' || event.key === ' ') {
            handled = go('[data-wdoc-slide-right]');
        } else if (event.key === 'ArrowLeft' || event.key === 'Backspace') {
            handled = go('[data-wdoc-slide-left]');
        } else if (event.key === 'ArrowDown') {
            handled = go('[data-wdoc-slide-down]');
        } else if (event.key === 'ArrowUp' || event.key === 'PageUp') {
            handled = go('[data-wdoc-slide-up]');
        }
        if (handled) {
            event.preventDefault();
        }
    });
})();
</script>"#;

/// Render a single page as a complete HTML document.
pub fn render_page(doc: &WdocDocument, page: &Page, css_path: &str) -> String {
    match page.template.unwrap_or(doc.template) {
        WdocTemplate::Book => render_book_page(doc, page, css_path),
        WdocTemplate::Presentation => render_presentation_page(doc, page, css_path),
    }
}

fn render_book_page(doc: &WdocDocument, page: &Page, css_path: &str) -> String {
    let mut html = String::with_capacity(4096);
    let mut content_html = String::new();
    render_layout_items(&page.layout.children, &mut content_html);

    render_document_head(doc, page, css_path, &content_html, &mut html);
    html.push_str("<body class=\"wdoc-template-book\">\n");

    // Nav sidebar
    render_nav(doc, &page.section_id, &mut html);

    // Main content
    html.push_str("<main class=\"wdoc-content\">\n");
    html.push_str(&content_html);
    html.push_str("</main>\n");

    // Theme + highlight.js script
    if page_has_runtime(page) {
        html.push_str(&page_signal_runtime(page));
    }
    html.push_str(THEME_SCRIPT);
    html.push_str("\n</body>\n</html>\n");
    html
}

fn render_presentation_page(doc: &WdocDocument, page: &Page, css_path: &str) -> String {
    let mut html = String::with_capacity(4096);
    let mut content_html = String::new();
    render_layout_items(&page.layout.children, &mut content_html);
    let nav = presentation_nav(doc, page);

    render_document_head(doc, page, css_path, &content_html, &mut html);
    html.push_str("<body class=\"wdoc-template-presentation\">\n");
    html.push_str("<main class=\"wdoc-presentation\" aria-label=\"Presentation slide\">\n");
    html.push_str("<nav class=\"wdoc-presentation-nav\" aria-hidden=\"true\">\n");
    render_presentation_nav_link(&mut html, nav.up, "up", "Previous section");
    render_presentation_nav_link(&mut html, nav.left, "left", "Previous slide");
    render_presentation_nav_link(&mut html, nav.right, "right", "Next slide");
    render_presentation_nav_link(&mut html, nav.down, "down", "Next section");
    html.push_str("</nav>\n");
    html.push_str("<section class=\"wdoc-slide\">\n");
    html.push_str(&content_html);
    html.push_str("</section>\n</main>\n");
    if page_has_runtime(page) {
        html.push_str(&page_signal_runtime(page));
    }
    html.push_str(THEME_SCRIPT);
    html.push_str(PRESENTATION_SCRIPT);
    html.push_str("\n</body>\n</html>\n");
    html
}

fn render_presentation_nav_link(
    html: &mut String,
    target: Option<&Page>,
    direction: &str,
    aria_label: &str,
) {
    if let Some(target) = target {
        writeln!(
            html,
            "<a href=\"{}.html\" data-wdoc-slide-{} aria-label=\"{}\"></a>",
            target.id, direction, aria_label
        )
        .unwrap();
    }
}

fn render_document_head(
    doc: &WdocDocument,
    page: &Page,
    css_path: &str,
    content_html: &str,
    html: &mut String,
) {
    let mathjax_head = if content_html.contains("data-wdoc-equation=") {
        MATHJAX_HEAD
    } else {
        ""
    };

    write!(
        html,
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — {doc_title}</title>
<link rel="stylesheet" href="{css_path}">
{HLJS_HEAD}
{mathjax_head}
</head>
"#,
        title = page.title,
        doc_title = doc.title,
        HLJS_HEAD = HLJS_HEAD,
        mathjax_head = mathjax_head,
    )
    .unwrap();
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
    format!("<script>(function(cfg){{if(window.__wdocPageSignalsInit){{window.__wdocPageSignalsInit(cfg);return;}}function val(v){{return v&&typeof v==='object'&&Object.prototype.hasOwnProperty.call(v,'initial')?v.initial:v;}}function clone(v){{return v==null||typeof v!=='object'?v:JSON.parse(JSON.stringify(v));}}function text(v){{if(v==null)return'';return typeof v==='string'?v:JSON.stringify(v);}}function readPath(v,p){{if(!p)return v;return String(p).replace(/\\[(\\d+)\\]/g,'.$1').split('.').filter(Boolean).reduce(function(a,k){{return a==null?undefined:a[k];}},v);}}function writePath(v,p,n){{if(!p)return n;var root=clone(v),cur=root,parts=String(p).replace(/\\[(\\d+)\\]/g,'.$1').split('.').filter(Boolean);for(var i=0;i<parts.length-1;i++){{var k=parts[i];if(cur[k]==null)cur[k]=/^\\d+$/.test(parts[i+1])?[]:{{}};cur=cur[k];}}cur[parts[parts.length-1]]=n;return root;}}function fmt(v,f){{var s=text(v);return f?String(f).replace(/\\{{value\\}}/g,s):s;}}function findTarget(id){{return document.querySelector('[data-wdoc-id=\"'+css(id)+'\"]')||document.querySelector('[data-wdoc-content-id=\"'+css(id)+'\"]')||document.getElementById(id);}}function css(s){{return String(s).replace(/\\\\/g,'\\\\\\\\').replace(/\"/g,'\\\\\"');}}function applyProp(el,prop,value){{if(!el)return;var s=text(value);if(prop==='text'||prop==='content'){{el.textContent=s;return;}}if(prop==='html'){{el.innerHTML=s;return;}}if(prop==='class'){{el.setAttribute('class',s);return;}}if(prop.indexOf('style.')===0){{el.style.setProperty(prop.slice(6).replace(/_/g,'-'),s);return;}}if(window.__wdocDiagramApplyProperty&&el.hasAttribute('data-wdoc-id')&&window.__wdocDiagramApplyProperty(el,prop,value))return;el.setAttribute(prop.replace(/_/g,'-'),s);}}function apply(){{bindings.forEach(function(b){{applyProp(findTarget(b.target),b.property,fmt(readPath(signals[b.signal],b.path),b.format));}});}}function setSignal(name,value,path){{signals[name]=writePath(signals[name],path,value);apply();document.dispatchEvent(new CustomEvent('wdoc:signal-change',{{detail:{{name:name,value:signals[name]}}}}));}}var signals={{}},bindings=cfg.bindings||[];(cfg.signals||[]).forEach(function(s){{signals[s.name]=clone(val(s));}});window.__wdocSignals=signals;window.__wdocSetSignal=setSignal;window.__wdocPageSignalsInit=function(next){{cfg=next||cfg;bindings=cfg.bindings||[];signals={{}};(cfg.signals||[]).forEach(function(s){{signals[s.name]=clone(val(s));}});window.__wdocSignals=signals;apply();}};if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',apply);else apply();}})({data});</script>\n")
}

fn render_nav(doc: &WdocDocument, active_section: &str, html: &mut String) {
    html.push_str("<nav class=\"wdoc-nav\">\n");
    writeln!(html, "<div class=\"wdoc-nav-title\">{}</div>", doc.title).unwrap();
    html.push_str("<ul>\n");
    render_nav_sections(&doc.sections, &doc.pages, active_section, html);
    html.push_str("</ul>\n");

    // Theme toggle at bottom of nav
    html.push_str(
        r#"<div class="wdoc-theme-toggle" id="wdoc-theme-toggle">
<span id="wdoc-theme-icon" class="wdoc-theme-icon">&#x1F319;</span>
<div class="wdoc-theme-toggle-track"><div class="wdoc-theme-toggle-knob"></div></div>
<span>Dark mode</span>
</div>
"#,
    );

    html.push_str("</nav>\n");
}

fn render_nav_sections(
    sections: &[Section],
    pages: &[Page],
    active_section: &str,
    html: &mut String,
) {
    for section in sections {
        let active_class = if active_section == section.id {
            " class=\"active\""
        } else {
            ""
        };

        // Find the first page for this section
        let page_file = pages
            .iter()
            .find(|p| p.section_id == section.id)
            .map(|p| format!("{}.html", p.id))
            .unwrap_or_else(|| "#".to_string());

        writeln!(
            html,
            "<li><a href=\"{page_file}\"{active_class}>{title}</a>",
            title = section.title,
        )
        .unwrap();

        if !section.children.is_empty() {
            html.push_str("<ul>\n");
            render_nav_sections(&section.children, pages, active_section, html);
            html.push_str("</ul>\n");
        }
        html.push_str("</li>\n");
    }
}
