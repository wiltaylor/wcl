use std::fmt::Write;

use crate::model::*;
use crate::render::layout::render_layout_items;

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
</head>
<body>
"#,
        title = page.title,
        doc_title = doc.title,
    )
    .unwrap();

    // Nav sidebar
    render_nav(doc, &page.section_id, &mut html);

    // Main content
    html.push_str("<main class=\"wdoc-content\">\n");
    render_layout_items(&page.layout.children, &mut html);
    html.push_str("</main>\n");

    html.push_str("</body>\n</html>\n");
    html
}

/// Render the index page (list of all sections with links to first page).
pub fn render_index(doc: &WdocDocument, css_path: &str) -> String {
    let mut html = String::with_capacity(2048);

    write!(
        html,
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="{css_path}">
</head>
<body>
"#,
        title = doc.title,
    )
    .unwrap();

    render_nav(doc, "", &mut html);

    html.push_str("<main class=\"wdoc-content\">\n");
    writeln!(html, "<h1 class=\"wdoc-heading\">{}</h1>", doc.title).unwrap();

    if let Some(author) = &doc.author {
        writeln!(html, "<p class=\"wdoc-paragraph\">By {author}</p>").unwrap();
    }
    if let Some(version) = &doc.version {
        writeln!(html, "<p class=\"wdoc-paragraph\">Version {version}</p>").unwrap();
    }

    // Section listing
    if !doc.sections.is_empty() {
        html.push_str("<h2 class=\"wdoc-heading\">Contents</h2>\n<ul>\n");
        render_section_list(&doc.sections, &doc.pages, &mut html);
        html.push_str("</ul>\n");
    }

    html.push_str("</main>\n</body>\n</html>\n");
    html
}

fn render_nav(doc: &WdocDocument, active_section: &str, html: &mut String) {
    html.push_str("<nav class=\"wdoc-nav\">\n");
    writeln!(html, "<div class=\"wdoc-nav-title\">{}</div>", doc.title).unwrap();
    html.push_str("<ul>\n");
    writeln!(html, "<li><a href=\"index.html\">Home</a></li>").unwrap();
    render_nav_sections(&doc.sections, &doc.pages, active_section, html);
    html.push_str("</ul>\n");
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

fn render_section_list(sections: &[Section], pages: &[Page], html: &mut String) {
    for section in sections {
        let page_link = pages
            .iter()
            .find(|p| p.section_id == section.id)
            .map(|p| format!("<a href=\"{}.html\">{}</a>", p.id, section.title))
            .unwrap_or_else(|| section.title.clone());

        writeln!(html, "<li>{page_link}").unwrap();
        if !section.children.is_empty() {
            html.push_str("<ul>\n");
            render_section_list(&section.children, pages, html);
            html.push_str("</ul>\n");
        }
        html.push_str("</li>\n");
    }
}
