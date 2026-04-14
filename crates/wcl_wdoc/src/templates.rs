//! Pure HTML rendering functions for standard wdoc content elements.
//!
//! These functions are wrapped as `BuiltinFn` in the CLI handler and called
//! via template function dispatch. They have no dependency on wcl types —
//! they operate on plain strings and maps.

use std::fmt::Write;

use indexmap::IndexMap;

/// Generate a URL-safe slug from text.
pub fn slugify(s: &str) -> String {
    let slug: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    slug.trim_matches('-').to_string()
}

/// Render a heading element with an anchor ID.
/// Expects: `level` (i64), `content` (string)
pub fn render_heading(attrs: &IndexMap<String, String>) -> String {
    let level = attrs
        .get("level")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1);
    render_heading_at(level, attrs)
}

/// Render a heading at a fixed level. Used by `h1`..`h6` schemas which carry
/// only a `content` field (the level is implied by the schema name).
pub fn render_heading_at(level: u8, attrs: &IndexMap<String, String>) -> String {
    let level = level.clamp(1, 6);
    let content = attrs.get("content").map(|s| s.as_str()).unwrap_or("");
    let slug = slugify(content);
    format!("<h{level} id=\"{slug}\" class=\"wdoc-heading\">{content}</h{level}>")
}

/// Render a paragraph element.
/// Expects: `content` (string)
pub fn render_paragraph(attrs: &IndexMap<String, String>) -> String {
    let content = attrs.get("content").map(|s| s.as_str()).unwrap_or("");
    // Use <div> when content contains block-level elements — browsers auto-close
    // <p> before block elements, breaking the wrapper and its CSS.
    let has_block = content.contains("<ul")
        || content.contains("<ol")
        || content.contains("<table")
        || content.contains("<div");
    if has_block {
        format!("<div class=\"wdoc-paragraph\">{content}</div>")
    } else {
        format!("<p class=\"wdoc-paragraph\">{content}</p>")
    }
}

/// Render an image element.
/// Expects: `src` (string), optional `alt`, `width`, `height`
pub fn render_image(attrs: &IndexMap<String, String>) -> String {
    let src = attrs.get("src").map(|s| s.as_str()).unwrap_or("");
    let mut html = format!("<img src=\"{src}\"");
    if let Some(alt) = attrs.get("alt") {
        write!(html, " alt=\"{alt}\"").unwrap();
    }
    if let Some(w) = attrs.get("width") {
        write!(html, " width=\"{w}\"").unwrap();
    }
    if let Some(h) = attrs.get("height") {
        write!(html, " height=\"{h}\"").unwrap();
    }
    html.push_str(" class=\"wdoc-image\">");
    html
}

/// Render a code block.
/// Expects: `content` (string), optional `language`
pub fn render_code(attrs: &IndexMap<String, String>) -> String {
    let content = attrs.get("content").map(|s| s.as_str()).unwrap_or("");
    let lang_class = attrs
        .get("language")
        .map(|l| format!(" class=\"language-{l}\""))
        .unwrap_or_default();
    // HTML-escape the content for code blocks
    let escaped = html_escape(content);
    format!("<pre class=\"wdoc-code\"><code{lang_class}>{escaped}</code></pre>")
}

/// Render a full page HTML document.
pub fn render_page(
    title: &str,
    doc_title: &str,
    nav_html: &str,
    content_html: &str,
    styles_css: &str,
) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — {doc_title}</title>
<style>{styles_css}</style>
</head>
<body>
{nav_html}
<main class="wdoc-content">
{content_html}
</main>
</body>
</html>
"#
    )
}

/// Render the navigation sidebar from section data.
pub fn render_nav(
    doc_title: &str,
    sections: &[NavSection],
    pages: &[(String, String)], // (page_id, section_id)
    active_section: &str,
) -> String {
    let mut html = String::new();
    html.push_str("<nav class=\"wdoc-nav\">\n");
    writeln!(html, "<div class=\"wdoc-nav-title\">{doc_title}</div>").unwrap();
    html.push_str("<ul>\n");
    writeln!(html, "<li><a href=\"index.html\">Home</a></li>").unwrap();
    render_nav_sections(sections, pages, active_section, &mut html);
    html.push_str("</ul>\n</nav>\n");
    html
}

/// Section data for navigation rendering.
#[derive(Debug, Clone)]
pub struct NavSection {
    pub id: String,
    pub title: String,
    pub children: Vec<NavSection>,
}

fn render_nav_sections(
    sections: &[NavSection],
    pages: &[(String, String)],
    active: &str,
    html: &mut String,
) {
    for section in sections {
        let active_class = if active == section.id {
            " class=\"active\""
        } else {
            ""
        };
        let page_file = pages
            .iter()
            .find(|(_, sid)| *sid == section.id)
            .map(|(pid, _)| format!("{pid}.html"))
            .unwrap_or_else(|| "#".to_string());

        writeln!(
            html,
            "<li><a href=\"{page_file}\"{active_class}>{title}</a>",
            title = section.title,
        )
        .unwrap();

        if !section.children.is_empty() {
            html.push_str("<ul>\n");
            render_nav_sections(&section.children, pages, active, html);
            html.push_str("</ul>\n");
        }
        html.push_str("</li>\n");
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_render_heading() {
        let html = render_heading(&attrs(&[("level", "2"), ("content", "Hello")]));
        assert_eq!(html, "<h2 id=\"hello\" class=\"wdoc-heading\">Hello</h2>");
    }

    #[test]
    fn test_render_paragraph() {
        let html = render_paragraph(&attrs(&[("content", "Some text")]));
        assert_eq!(html, "<p class=\"wdoc-paragraph\">Some text</p>");
    }

    #[test]
    fn test_render_code_escapes() {
        let html = render_code(&attrs(&[
            ("language", "html"),
            ("content", "<div>hi</div>"),
        ]));
        assert!(html.contains("&lt;div&gt;hi&lt;/div&gt;"));
        assert!(html.contains("language-html"));
    }

    #[test]
    fn test_render_image() {
        let html = render_image(&attrs(&[("src", "foo.png"), ("alt", "A photo")]));
        assert!(html.contains("src=\"foo.png\""));
        assert!(html.contains("alt=\"A photo\""));
    }
}
