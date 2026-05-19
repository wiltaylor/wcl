use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::transform::codec;
use crate::wdoc::markup::{self, elem, raw_html, s};
use crate::wdoc::model::{self, Page, Section, WdocDocument};
use crate::Value;
use indexmap::IndexMap;

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
    let mut css = crate::wdoc::assets::base_css()?;
    css.push('\n');
    css.push_str(&crate::wdoc::assets::style_css(&doc.styles)?);
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

    // Render HTML outputs through the bundled WDoc template library.
    let mut written_html = HashSet::new();
    let context = render_context_value(doc)?;
    let used_templates = used_page_templates(doc);
    for (filename, html) in crate::wdoc::assets::render_html_outputs(context, &used_templates)? {
        collect_referenced_image_assets(&html, &asset_extensions, &mut referenced_assets);
        write_html_with_codec(output, &filename, &html)?;
        written_html.insert(filename);
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

fn used_page_templates(doc: &WdocDocument) -> Vec<String> {
    let mut templates = BTreeSet::new();
    for page in doc.pages.iter().filter(|page| !page.draft) {
        templates.insert(page.template.unwrap_or(doc.template).as_str().to_string());
    }
    templates.into_iter().collect()
}

fn render_context_value(doc: &WdocDocument) -> Result<Value, String> {
    let mut root = IndexMap::new();
    let runtime = runtime_context_value()?;
    let mut doc_value = document_context_value(doc)?;
    if let Value::Map(doc_map) = &mut doc_value {
        doc_map.insert("runtime".to_string(), runtime.clone());
    }
    root.insert("doc".to_string(), doc_value);
    root.insert("runtime".to_string(), runtime);
    root.insert(
        "site_outputs".to_string(),
        Value::List(site_output_contexts(doc)?),
    );
    Ok(Value::Map(root))
}

fn runtime_context_value() -> Result<Value, String> {
    let mut map = IndexMap::new();
    map.insert(
        "mathjax_config".to_string(),
        Value::String(crate::wdoc::assets::mathjax_config_js()?.to_string()),
    );
    map.insert(
        "theme".to_string(),
        Value::String(crate::wdoc::assets::theme_runtime_js()?.to_string()),
    );
    map.insert(
        "presentation".to_string(),
        Value::String(crate::wdoc::assets::presentation_runtime_js()?.to_string()),
    );
    Ok(Value::Map(map))
}

fn document_context_value(doc: &WdocDocument) -> Result<Value, String> {
    let mut map = IndexMap::new();
    map.insert("name".to_string(), Value::String(doc.name.clone()));
    map.insert("title".to_string(), Value::String(doc.title.clone()));
    map.insert(
        "template".to_string(),
        Value::String(doc.template.as_str().to_string()),
    );
    map.insert(
        "version".to_string(),
        doc.version
            .as_ref()
            .map(|value| Value::String(value.clone()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "author".to_string(),
        doc.author
            .as_ref()
            .map(|value| Value::String(value.clone()))
            .unwrap_or(Value::Null),
    );
    map.insert("site".to_string(), site_context_value(doc));
    map.insert(
        "sections".to_string(),
        Value::List(
            doc.sections
                .iter()
                .map(|section| section_context_value(doc, section))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    map.insert(
        "pages".to_string(),
        Value::List(
            doc.pages
                .iter()
                .filter(|page| !page.draft)
                .map(|page| page_context_value(doc, page))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    Ok(Value::Map(map))
}

fn site_context_value(doc: &WdocDocument) -> Value {
    let mut map = IndexMap::new();
    map.insert(
        "header_html".to_string(),
        doc.site
            .header_html
            .as_ref()
            .map(|html| Value::String(html.clone()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "nav_html".to_string(),
        doc.site
            .nav_html
            .as_ref()
            .map(|html| Value::String(html.clone()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "footer_html".to_string(),
        doc.site
            .footer_html
            .as_ref()
            .map(|html| Value::String(html.clone()))
            .unwrap_or(Value::Null),
    );
    Value::Map(map)
}

fn section_context_value(doc: &WdocDocument, section: &Section) -> Result<Value, String> {
    let output_path = section_output_path(section);
    let mut map = IndexMap::new();
    map.insert("id".to_string(), Value::String(section.id.clone()));
    map.insert(
        "short_id".to_string(),
        Value::String(section.short_id.clone()),
    );
    map.insert("title".to_string(), Value::String(section.title.clone()));
    map.insert(
        "output_path".to_string(),
        Value::String(output_path.clone()),
    );
    map.insert(
        "css_path".to_string(),
        Value::String(css_path_for(&output_path)),
    );
    map.insert(
        "pages".to_string(),
        Value::List(
            pages_for_section(doc, section)
                .into_iter()
                .map(|page| page_summary_context_value(page, &output_path))
                .collect(),
        ),
    );
    map.insert(
        "first_page_path".to_string(),
        doc.pages
            .iter()
            .find(|page| !page.draft && page.section_id == section.id)
            .map(|page| Value::String(page_output_path(page)))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "children".to_string(),
        Value::List(
            section
                .children
                .iter()
                .map(|child| section_context_value(doc, child))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    Ok(Value::Map(map))
}

fn page_context_value(doc: &WdocDocument, page: &Page) -> Result<Value, String> {
    let output_path = page_output_path(page);
    let mut map = page_base_context_value(page);
    map.insert(
        "template".to_string(),
        Value::String(page.template.unwrap_or(doc.template).as_str().to_string()),
    );
    map.insert(
        "output_path".to_string(),
        Value::String(output_path.clone()),
    );
    map.insert(
        "css_path".to_string(),
        Value::String(css_path_for(&output_path)),
    );
    map.insert(
        "layout_items".to_string(),
        model::layout_items_to_value(&page.layout.children),
    );
    map.insert(
        "signal_runtime".to_string(),
        if page_has_runtime(page) {
            Value::String(page_signal_runtime(page)?)
        } else {
            Value::Null
        },
    );
    map.insert(
        "presentation_nav".to_string(),
        presentation_nav_context_value(doc, page),
    );
    Ok(Value::Map(map))
}

fn page_summary_context_value(page: &Page, from_path: &str) -> Value {
    let mut map = page_base_context_value(page);
    let output_path = page_output_path(page);
    map.insert(
        "output_path".to_string(),
        Value::String(output_path.clone()),
    );
    map.insert(
        "href".to_string(),
        Value::String(relative_href(from_path, &output_path)),
    );
    Value::Map(map)
}

fn page_base_context_value(page: &Page) -> IndexMap<String, Value> {
    let mut map = IndexMap::new();
    map.insert("id".to_string(), Value::String(page.id.clone()));
    map.insert(
        "section_id".to_string(),
        Value::String(page.section_id.clone()),
    );
    map.insert("title".to_string(), Value::String(page.title.clone()));
    map.insert(
        "date".to_string(),
        page.date
            .as_ref()
            .map(|date| Value::String(date.clone()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "summary".to_string(),
        page.summary
            .as_ref()
            .map(|summary| Value::String(summary.clone()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "tags".to_string(),
        Value::List(page.tags.iter().cloned().map(Value::String).collect()),
    );
    map.insert(
        "categories".to_string(),
        Value::List(page.categories.iter().cloned().map(Value::String).collect()),
    );
    map.insert(
        "params".to_string(),
        Value::Map(
            page.params
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                .collect(),
        ),
    );
    map
}

fn site_output_contexts(doc: &WdocDocument) -> Result<Vec<Value>, String> {
    let mut outputs = Vec::new();
    collect_section_output_contexts(doc, &doc.sections, &mut outputs)?;
    outputs.extend(taxonomy_output_contexts(doc, "tags"));
    outputs.extend(taxonomy_output_contexts(doc, "categories"));
    Ok(outputs)
}

fn collect_section_output_contexts(
    doc: &WdocDocument,
    sections: &[Section],
    out: &mut Vec<Value>,
) -> Result<(), String> {
    for section in sections {
        let section_value = section_context_value(doc, section)?;
        let section_pages = pages_for_section(doc, section);
        if !section_pages.is_empty() || !section.children.is_empty() {
            let mut map = IndexMap::new();
            map.insert("kind".to_string(), Value::String("section".to_string()));
            map.insert("section".to_string(), section_value);
            out.push(Value::Map(map));
        }
        collect_section_output_contexts(doc, &section.children, out)?;
    }
    Ok(())
}

fn taxonomy_output_contexts(doc: &WdocDocument, kind: &str) -> Vec<Value> {
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
            let mut map = IndexMap::new();
            map.insert("kind".to_string(), Value::String("taxonomy".to_string()));
            map.insert("path".to_string(), Value::String(path.clone()));
            map.insert(
                "title".to_string(),
                Value::String(format!("{kind}: {term}")),
            );
            map.insert(
                "pages".to_string(),
                Value::List(
                    pages
                        .iter()
                        .map(|page| page_summary_context_value(page, &path))
                        .collect(),
                ),
            );
            Value::Map(map)
        })
        .collect()
}

fn page_has_runtime(page: &Page) -> bool {
    !page.signals.is_empty() || !page.bindings.is_empty()
}

fn page_signal_runtime(page: &Page) -> Result<String, String> {
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
    crate::wdoc::assets::page_signal_runtime_js(&data)
}

fn presentation_nav_context_value(doc: &WdocDocument, page: &Page) -> Value {
    let nav = presentation_nav(doc, page);
    let mut map = IndexMap::new();
    for (key, target) in [
        ("left", nav.left),
        ("right", nav.right),
        ("up", nav.up),
        ("down", nav.down),
    ] {
        map.insert(
            key.to_string(),
            target
                .map(|page| Value::String(page_output_path(page)))
                .unwrap_or(Value::Null),
        );
    }
    Value::Map(map)
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
        if !page.draft
            && !groups
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
        if let Some(page) = all_pages
            .iter()
            .find(|p| !p.draft && p.section_id == section.id)
        {
            out.push(page);
        }
        collect_pages_by_section(&section.children, all_pages, out);
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
        ContentBlock, Layout, LayoutItem, Page, Section, SiteConfig, WdocDocument, WdocTemplate,
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
