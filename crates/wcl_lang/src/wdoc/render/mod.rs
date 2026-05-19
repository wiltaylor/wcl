pub mod assets;
pub mod layout;
pub mod page;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::transform::codec;
use crate::wdoc::markup::{self, elem, raw_html, s, text};
use crate::wdoc::model::{Page, Section, WdocDocument, WdocTemplate};
use crate::Value;

/// Render a `WdocDocument` to an output directory as static HTML files.
/// `asset_dirs` are source directories to scan for image/asset files to copy.
pub fn render_document(
    doc: &WdocDocument,
    output: &Path,
    asset_dirs: &[&Path],
) -> Result<(), String> {
    // Create output directory
    fs::create_dir_all(output).map_err(|e| format!("failed to create output directory: {e}"))?;

    // Generate CSS: base + user styles
    let mut css = assets::base_css()?;
    css.push('\n');
    css.push_str(&assets::generate_style_css(&doc.styles));
    let extra_css = doc.extra_css.trim();
    if !extra_css.is_empty() {
        css.push('\n');
        css.push_str(extra_css);
        css.push('\n');
    }

    let asset_extensions = [
        "png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "woff2", "woff", "ttf", "otf", "eot",
    ];
    let mut referenced_assets = HashSet::new();
    collect_referenced_css_assets(&css, &asset_extensions, &mut referenced_assets);

    fs::write(output.join("styles.css"), &css)
        .map_err(|e| format!("failed to write styles.css: {e}"))?;

    // Write highlight.js assets (bundled locally so file:// works)
    fs::write(
        output.join("highlight.min.js"),
        crate::assets::HIGHLIGHTJS_CORE,
    )
    .map_err(|e| format!("failed to write highlight.min.js: {e}"))?;

    fs::write(
        output.join("highlight-light.min.css"),
        crate::assets::HIGHLIGHTJS_THEME_LIGHT_CSS,
    )
    .map_err(|e| format!("failed to write highlight-light.min.css: {e}"))?;

    fs::write(
        output.join("highlight-dark.min.css"),
        crate::assets::HIGHLIGHTJS_THEME_DARK_CSS,
    )
    .map_err(|e| format!("failed to write highlight-dark.min.css: {e}"))?;

    fs::write(
        output.join("wcl-grammar.js"),
        crate::assets::WCL_HIGHLIGHTJS_GRAMMAR,
    )
    .map_err(|e| format!("failed to write wcl-grammar.js: {e}"))?;

    let font_dir = output.join("fonts");
    fs::create_dir_all(&font_dir).map_err(|e| format!("failed to create fonts directory: {e}"))?;
    for (name, bytes) in [
        (
            "JetBrainsMonoNerdFontMono-Regular.ttf",
            crate::assets::JETBRAINS_MONO_NERD_REGULAR,
        ),
        (
            "JetBrainsMonoNerdFontMono-Bold.ttf",
            crate::assets::JETBRAINS_MONO_NERD_BOLD,
        ),
        (
            "JetBrainsMonoNerdFontMono-Italic.ttf",
            crate::assets::JETBRAINS_MONO_NERD_ITALIC,
        ),
        (
            "JetBrainsMonoNerdFontMono-BoldItalic.ttf",
            crate::assets::JETBRAINS_MONO_NERD_BOLD_ITALIC,
        ),
    ] {
        fs::write(font_dir.join(name), bytes)
            .map_err(|e| format!("failed to write bundled font {name}: {e}"))?;
    }

    // Render each page
    let mut written_html = HashSet::new();
    for p in &doc.pages {
        if p.draft {
            continue;
        }
        let filename = page_output_path(p);
        let css_path = css_path_for(&filename);
        let html = page::render_page(doc, p, &css_path);
        collect_referenced_image_assets(&html, &asset_extensions, &mut referenced_assets);
        write_html_with_codec(output, &filename, &html)?;
        written_html.insert(filename);
    }

    if doc.template == WdocTemplate::Site {
        for generated in generated_site_pages(doc) {
            let css_path = css_path_for(&generated.path);
            let html =
                page::render_generated_site_page(doc, &generated.title, &generated.html, &css_path);
            collect_referenced_image_assets(&html, &asset_extensions, &mut referenced_assets);
            write_html_with_codec(output, &generated.path, &html)?;
            written_html.insert(generated.path);
        }
    }

    // index.html redirects to the first page by section order
    if !written_html.contains("index.html") {
        let first = first_page_by_section_order(&doc.sections, &doc.pages)
            .or_else(|| doc.pages.iter().find(|page| !page.draft));
        if let Some(first) = first {
            let target = page_output_path(first);
            let redirect = markup::render_html(&Value::List(vec![
                raw_html("<!DOCTYPE html>"),
                elem(
                    "html",
                    &[],
                    vec![
                        elem(
                            "head",
                            &[],
                            vec![elem(
                                "meta",
                                &[
                                    ("http_equiv", s("refresh")),
                                    ("content_attr", s(format!("0;url={target}"))),
                                ],
                                vec![],
                            )],
                        ),
                        elem("body", &[], vec![]),
                    ],
                ),
            ]))
            .expect("wdoc redirect should serialize as HTML");
            write_html_with_codec(output, "index.html", &redirect)?;
        }
    }

    // Copy referenced assets first so deep paths used by images work in previews.
    copy_referenced_assets(asset_dirs, output, &referenced_assets, &asset_extensions);

    // Preserve the legacy broad copy of image assets from source directories.
    for dir in asset_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Copy subdirectories (e.g., images/)
                    let dir_name = path.file_name().unwrap();
                    let dest_dir = output.join(dir_name);
                    if let Err(e) = copy_dir_assets(&path, &dest_dir, &asset_extensions) {
                        eprintln!(
                            "wdoc: warning: failed to copy assets from {}: {e}",
                            path.display()
                        );
                    }
                } else if has_asset_extension(&path, &asset_extensions) {
                    let dest = output.join(path.file_name().unwrap());
                    let _ = fs::copy(&path, &dest);
                }
            }
        }
    }

    Ok(())
}

struct GeneratedSitePage {
    path: String,
    title: String,
    html: String,
}

fn generated_site_pages(doc: &WdocDocument) -> Vec<GeneratedSitePage> {
    let mut pages = Vec::new();
    collect_section_pages(doc, &doc.sections, &mut pages);
    pages.extend(taxonomy_pages(doc, "tags"));
    pages.extend(taxonomy_pages(doc, "categories"));
    pages
}

fn collect_section_pages(
    doc: &WdocDocument,
    sections: &[Section],
    out: &mut Vec<GeneratedSitePage>,
) {
    for section in sections {
        let output_path = section_output_path(section);
        let section_pages = pages_for_section(doc, section);
        let section_children = section
            .children
            .iter()
            .map(|child| {
                elem(
                    "a",
                    &[
                        ("class_name", s("wdoc-site-list-item")),
                        (
                            "href",
                            s(relative_href(&output_path, &section_output_path(child))),
                        ),
                    ],
                    vec![elem("h3", &[], vec![text(&child.title)])],
                )
            })
            .collect::<Vec<_>>();
        if !section_pages.is_empty() || !section.children.is_empty() {
            let cards = section_pages
                .iter()
                .map(|page| {
                    site_page_card_value(
                        page,
                        &relative_href(&output_path, &page_output_path(page)),
                    )
                })
                .collect::<Vec<_>>();
            let html = markup::render_html(&elem(
                "section",
                &[("class_name", s("wdoc-site-list"))],
                vec![
                    elem("h1", &[], vec![text(&section.title)]),
                    Value::List(section_children),
                    elem("div", &[("class_name", s("wdoc-site-card-grid"))], cards),
                ],
            ))
            .expect("wdoc section page should serialize as HTML");
            out.push(GeneratedSitePage {
                path: output_path,
                title: section.title.clone(),
                html,
            });
        }
        collect_section_pages(doc, &section.children, out);
    }
}

fn pages_for_section<'a>(doc: &'a WdocDocument, section: &Section) -> Vec<&'a Page> {
    let mut pages = doc
        .pages
        .iter()
        .filter(|page| !page.draft && page.section_id == section.id)
        .collect::<Vec<_>>();
    pages.sort_by(|a, b| {
        a.weight
            .unwrap_or(i64::MAX)
            .cmp(&b.weight.unwrap_or(i64::MAX))
            .then_with(|| a.title.cmp(&b.title))
    });
    pages
}

fn taxonomy_pages(doc: &WdocDocument, kind: &str) -> Vec<GeneratedSitePage> {
    let mut terms: BTreeMap<String, Vec<&Page>> = BTreeMap::new();
    for page in doc.pages.iter().filter(|page| !page.draft) {
        let values = if kind == "tags" {
            &page.tags
        } else {
            &page.categories
        };
        for value in values {
            terms.entry(value.clone()).or_default().push(page);
        }
    }

    terms
        .into_iter()
        .map(|(term, mut pages)| {
            let path = format!("{}/{}.html", kind, slug(&term));
            pages.sort_by(|a, b| a.title.cmp(&b.title));
            let cards = pages
                .iter()
                .map(|page| {
                    site_page_card_value(page, &relative_href(&path, &page_output_path(page)))
                })
                .collect::<Vec<_>>();
            let title = format!("{kind}: {term}");
            GeneratedSitePage {
                path,
                title: title.clone(),
                html: markup::render_html(&elem(
                    "section",
                    &[("class_name", s("wdoc-site-list"))],
                    vec![
                        elem("h1", &[], vec![text(&title)]),
                        elem("div", &[("class_name", s("wdoc-site-card-grid"))], cards),
                    ],
                ))
                .expect("wdoc taxonomy page should serialize as HTML"),
            }
        })
        .collect()
}

fn site_page_card_value(page: &Page, href: &str) -> Value {
    let mut children = vec![elem("h3", &[], vec![text(&page.title)])];
    if let Some(summary) = &page.summary {
        children.push(elem("p", &[], vec![text(summary)]));
    }
    if let Some(date) = &page.date {
        children.push(elem("span", &[], vec![text(date)]));
    }
    elem(
        "a",
        &[("class_name", s("wdoc-site-card")), ("href", s(href))],
        children,
    )
}

fn page_output_path(page: &Page) -> String {
    page.path
        .as_deref()
        .map(normalize_html_path)
        .unwrap_or_else(|| format!("{}.html", page.id))
}

fn section_output_path(section: &Section) -> String {
    format!("sections/{}.html", slug(&section.id))
}

fn normalize_html_path(path: &str) -> String {
    let trimmed = path.trim().trim_start_matches('/').to_string();
    if trimmed.is_empty() {
        return "index.html".to_string();
    }
    if trimmed.ends_with('/') {
        return format!("{trimmed}index.html");
    }
    if trimmed.ends_with(".html") {
        trimmed
    } else {
        format!("{trimmed}.html")
    }
}

fn css_path_for(filename: &str) -> String {
    let depth = Path::new(filename)
        .parent()
        .map(|parent| parent.components().count())
        .unwrap_or(0);
    format!("{}styles.css", "../".repeat(depth))
}

fn relative_href(from_file: &str, to_file: &str) -> String {
    if to_file.starts_with("http://") || to_file.starts_with("https://") || to_file.starts_with('/')
    {
        return to_file.to_string();
    }
    let depth = Path::new(from_file)
        .parent()
        .map(|parent| parent.components().count())
        .unwrap_or(0);
    format!("{}{}", "../".repeat(depth), to_file)
}

fn slug(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn write_html_with_codec(output: &Path, filename: &str, html: &str) -> Result<(), String> {
    let mut options = codec::CodecOptions::new();
    options.insert("filename".to_string(), Value::String(filename.to_string()));
    let resolved = codec::native::output_filename(&options, filename);
    codec::native::write_text_output(
        &resolved,
        html,
        codec::native::OutputTarget::Directory(output),
    )
    .map_err(|e| format!("failed to write {filename}: {e}"))
}

/// Walk the section tree in declaration order and return the first page found.
fn first_page_by_section_order<'a>(
    sections: &[crate::wdoc::model::Section],
    pages: &'a [crate::wdoc::model::Page],
) -> Option<&'a crate::wdoc::model::Page> {
    for section in sections {
        if let Some(page) = pages.iter().find(|p| p.section_id == section.id) {
            return Some(page);
        }
        if let Some(page) = first_page_by_section_order(&section.children, pages) {
            return Some(page);
        }
    }
    None
}

fn copy_dir_assets(src: &Path, dest: &Path, extensions: &[&str]) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("mkdir {}: {e}", dest.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("read {}: {e}", src.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() && has_asset_extension(&path, extensions) {
            let dest_file = dest.join(path.file_name().unwrap());
            fs::copy(&path, &dest_file).map_err(|e| format!("copy {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

fn collect_referenced_image_assets(html: &str, extensions: &[&str], out: &mut HashSet<PathBuf>) {
    for attr in ["src", "href", "xlink:href"] {
        let mut rest = html;
        let needle = format!("{attr}=\"");
        while let Some(idx) = rest.find(&needle) {
            let value_start = idx + needle.len();
            let after = &rest[value_start..];
            let Some(value_end) = after.find('"') else {
                break;
            };
            let value = html_unescape_attr(&after[..value_end]);
            if let Some(asset_ref) = local_asset_ref_path(&value, extensions) {
                out.insert(asset_ref);
            }
            rest = &after[value_end + 1..];
        }
    }
}

fn collect_referenced_css_assets(css: &str, extensions: &[&str], out: &mut HashSet<PathBuf>) {
    let mut rest = css;
    while let Some(idx) = rest.find("url(") {
        let after = &rest[idx + 4..];
        let Some(value_end) = after.find(')') else {
            break;
        };
        let value = strip_css_url_quotes(after[..value_end].trim());
        if let Some(asset_ref) = local_asset_ref_path(value, extensions) {
            out.insert(asset_ref);
        }
        rest = &after[value_end + 1..];
    }
}

fn strip_css_url_quotes(value: &str) -> &str {
    if value.len() >= 2 {
        let first = value.as_bytes()[0];
        let last = value.as_bytes()[value.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn copy_referenced_assets(
    asset_dirs: &[&Path],
    output: &Path,
    refs: &HashSet<PathBuf>,
    extensions: &[&str],
) {
    for rel in refs {
        if !is_safe_relative_path(rel) || !has_asset_extension(rel, extensions) {
            continue;
        }

        let dest = output.join(rel);
        if dest.is_file() {
            continue;
        }

        let Some(src) = asset_dirs
            .iter()
            .map(|dir| dir.join(rel))
            .find(|candidate| candidate.is_file())
        else {
            eprintln!(
                "wdoc: warning: referenced asset '{}' was not found",
                rel.display()
            );
            continue;
        };

        if let Some(parent) = dest.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("wdoc: warning: failed to create {}: {e}", parent.display());
                continue;
            }
        }
        if let Err(e) = fs::copy(&src, &dest) {
            eprintln!(
                "wdoc: warning: failed to copy referenced asset {}: {e}",
                src.display()
            );
        }
    }
}

fn has_asset_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            extensions
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
}

fn local_asset_ref_path(value: &str, extensions: &[&str]) -> Option<PathBuf> {
    let path_part = value.split(['?', '#']).next().unwrap_or(value);
    if path_part.is_empty()
        || path_part.starts_with("http://")
        || path_part.starts_with("https://")
        || path_part.starts_with("data:")
        || path_part.starts_with('/')
    {
        return None;
    }

    let path = Path::new(path_part);
    (is_safe_relative_path(path) && has_asset_extension(path, extensions))
        .then(|| path.to_path_buf())
}

fn is_safe_relative_path(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn html_unescape_attr(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wdoc::model::{
        ContentBlock, Layout, LayoutItem, Page, Section, SiteConfig, WdocDocument,
    };

    fn doc_with_html(html: &str) -> WdocDocument {
        WdocDocument {
            name: "docs".to_string(),
            title: "Docs".to_string(),
            template: crate::wdoc::model::WdocTemplate::Book,
            version: None,
            author: None,
            site: SiteConfig::default(),
            sections: vec![Section {
                id: "docs.overview".to_string(),
                short_id: "overview".to_string(),
                title: "Overview".to_string(),
                children: vec![],
            }],
            pages: vec![Page {
                id: "home".to_string(),
                section_id: "docs.overview".to_string(),
                title: "Home".to_string(),
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
                        kind: "wdoc::draw::diagram".to_string(),
                        id: None,
                        rendered_html: html.to_string(),
                        style: None,
                    })],
                },
                signals: vec![],
                bindings: vec![],
            }],
            styles: vec![],
            extra_css: String::new(),
        }
    }

    #[test]
    fn render_document_copies_deep_referenced_image_asset() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("src");
        let output = temp.path().join("out");
        std::fs::create_dir_all(source.join("assets/deep")).expect("mkdir assets");
        std::fs::write(
            source.join("assets/deep/hero.png"),
            [0x89, b'P', b'N', b'G'],
        )
        .expect("write png");

        let doc = doc_with_html(r#"<svg><image href="assets/deep/hero.png"/></svg>"#);
        render_document(&doc, &output, &[source.as_path()]).expect("render");

        assert!(output.join("assets/deep/hero.png").exists());
    }

    #[test]
    fn render_document_writes_extra_css_to_shared_stylesheet() {
        let temp = tempfile::tempdir().expect("tempdir");
        let output = temp.path().join("out");
        let mut doc = doc_with_html(
            r#"<div class="wdoc-diagram"><svg class="wad-ds-wad_interface"></svg></div>"#,
        );
        doc.extra_css = ".wad-ds-wad_interface .button{fill:red;}".to_string();

        render_document(&doc, &output, &[]).expect("render");

        let css = std::fs::read_to_string(output.join("styles.css")).expect("styles.css");
        let html = std::fs::read_to_string(output.join("home.html")).expect("home.html");
        assert!(css.contains(".wad-ds-wad_interface .button{fill:red;}"));
        assert!(html.contains("class=\"wad-ds-wad_interface\""));
    }

    #[test]
    fn render_document_copies_font_asset_referenced_from_css() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let output = temp.path().join("out");
        std::fs::create_dir_all(source.join("fonts")).expect("create fonts dir");
        std::fs::write(source.join("fonts/Inter-Regular.woff2"), [0, 1, 2, 3]).expect("write font");

        let mut doc = doc_with_html("<p>Fonts</p>");
        doc.extra_css =
            "@font-face { src: url(\"fonts/Inter-Regular.woff2\") format(\"woff2\"); }".to_string();

        render_document(&doc, &output, &[source.as_path()]).expect("render");

        let css = std::fs::read_to_string(output.join("styles.css")).expect("styles.css");
        assert!(css.contains("url(\"fonts/Inter-Regular.woff2\")"));
        assert!(output.join("fonts/Inter-Regular.woff2").exists());
    }

    #[test]
    fn render_document_writes_bundled_terminal_fonts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let output = temp.path().join("out");
        let doc = doc_with_html("<p>Terminal fonts</p>");

        render_document(&doc, &output, &[]).expect("render");

        let css = std::fs::read_to_string(output.join("styles.css")).expect("styles.css");
        assert!(css.contains("JetBrainsMono Nerd Font"));
        assert!(output
            .join("fonts/JetBrainsMonoNerdFontMono-Regular.ttf")
            .exists());
        assert!(output
            .join("fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf")
            .exists());
    }

    #[test]
    fn site_template_writes_page_paths_sections_and_taxonomies() {
        let temp = tempfile::tempdir().expect("tempdir");
        let output = temp.path().join("out");
        let mut doc = doc_with_html("<p>Site page</p>");
        doc.template = WdocTemplate::Site;
        doc.site.header_html =
            Some("<header class=\"wdoc-site-header\">Header</header>".to_string());
        doc.sections[0].children.push(Section {
            id: "docs.overview.child".to_string(),
            short_id: "child".to_string(),
            title: "Child".to_string(),
            children: vec![],
        });
        doc.pages[0].template = None;
        doc.pages[0].path = Some("guides/home".to_string());
        doc.pages[0].summary = Some("A page summary".to_string());
        doc.pages[0].tags = vec!["alpha".to_string()];
        doc.pages[0].categories = vec!["docs".to_string()];

        render_document(&doc, &output, &[]).expect("render");

        let page_html =
            std::fs::read_to_string(output.join("guides/home.html")).expect("site page");
        let section_html = std::fs::read_to_string(output.join("sections/docs-overview.html"))
            .expect("section page");
        let tag_html = std::fs::read_to_string(output.join("tags/alpha.html")).expect("tag page");

        assert!(page_html.contains("wdoc-template-site"));
        assert!(page_html.contains("wdoc-site-header"));
        assert!(section_html.contains("A page summary"));
        assert!(tag_html.contains("../guides/home.html"));
    }

    #[test]
    fn referenced_asset_collection_ignores_remote_and_unsafe_paths() {
        let mut refs = HashSet::new();
        collect_referenced_image_assets(
            r#"<img src="https://example.com/a.png"><image href="../secret.png"><img src="images/ok.webp?cache=1">"#,
            &["png", "webp"],
            &mut refs,
        );

        assert!(refs.contains(&PathBuf::from("images/ok.webp")));
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn css_asset_collection_ignores_remote_and_unsafe_paths() {
        let mut refs = HashSet::new();
        collect_referenced_css_assets(
            r#"@font-face{src:url("fonts/Inter.woff2")} .x{background:url(https://example.com/a.png)} .y{background:url('../secret.ttf')} .z{background:url(icons/a.svg?cache=1)}"#,
            &["woff2", "ttf", "svg"],
            &mut refs,
        );

        assert!(refs.contains(&PathBuf::from("fonts/Inter.woff2")));
        assert!(refs.contains(&PathBuf::from("icons/a.svg")));
        assert_eq!(refs.len(), 2);
    }
}
