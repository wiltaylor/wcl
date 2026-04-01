use std::fmt::Write;

use crate::model::*;
use crate::render::layout::render_layout_items;

/// highlight.js local assets injected into <head>.
/// Both light and dark themes loaded; JS toggles which is active.
const HLJS_HEAD: &str = r#"<link rel="stylesheet" href="highlight-light.min.css" id="hljs-light">
<link rel="stylesheet" href="highlight-dark.min.css" id="hljs-dark" disabled>
<script defer src="highlight.min.js"></script>
<script defer src="wcl-grammar.js"></script>"#;

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

/// Render a single page as a complete HTML document.
pub fn render_page(doc: &WdocDocument, page: &Page, css_path: &str) -> String {
    let mut html = String::with_capacity(4096);

    // DOCTYPE + head
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
</head>
<body>
"#,
        title = page.title,
        doc_title = doc.title,
        HLJS_HEAD = HLJS_HEAD,
    )
    .unwrap();

    // Nav sidebar
    render_nav(doc, &page.section_id, &mut html);

    // Main content
    html.push_str("<main class=\"wdoc-content\">\n");
    render_layout_items(&page.layout.children, &mut html);
    html.push_str("</main>\n");

    // Theme + highlight.js script
    html.push_str(THEME_SCRIPT);
    html.push_str("\n</body>\n</html>\n");
    html
}

/// Render the index page — redirects to the first page if one exists.
pub fn render_index(doc: &WdocDocument, _css_path: &str) -> String {
    // Redirect to the first page
    let target = doc
        .pages
        .first()
        .map(|p| format!("{}.html", p.id))
        .unwrap_or_else(|| "#".to_string());

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="refresh" content="0; url={target}">
<title>{title}</title>
</head>
<body>
<p>Redirecting to <a href="{target}">{target}</a>...</p>
</body>
</html>
"#,
        title = doc.title,
    )
}

fn render_nav(doc: &WdocDocument, active_section: &str, html: &mut String) {
    html.push_str("<nav class=\"wdoc-nav\">\n");
    writeln!(html, "<div class=\"wdoc-nav-title\">{}</div>", doc.title).unwrap();
    html.push_str("<ul>\n");
    writeln!(html, "<li><a href=\"index.html\">Home</a></li>").unwrap();
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
