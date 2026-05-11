pub mod assets;
pub mod layout;
pub mod page;

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::model::WdocDocument;

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
    let mut css = assets::BASE_CSS.to_string();
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
        crate::library::HIGHLIGHTJS_CORE,
    )
    .map_err(|e| format!("failed to write highlight.min.js: {e}"))?;

    fs::write(
        output.join("highlight-light.min.css"),
        crate::library::HIGHLIGHTJS_THEME_LIGHT_CSS,
    )
    .map_err(|e| format!("failed to write highlight-light.min.css: {e}"))?;

    fs::write(
        output.join("highlight-dark.min.css"),
        crate::library::HIGHLIGHTJS_THEME_DARK_CSS,
    )
    .map_err(|e| format!("failed to write highlight-dark.min.css: {e}"))?;

    fs::write(
        output.join("wcl-grammar.js"),
        crate::library::WCL_HIGHLIGHTJS_GRAMMAR,
    )
    .map_err(|e| format!("failed to write wcl-grammar.js: {e}"))?;

    let font_dir = output.join("fonts");
    fs::create_dir_all(&font_dir).map_err(|e| format!("failed to create fonts directory: {e}"))?;
    for (name, bytes) in [
        (
            "JetBrainsMonoNerdFontMono-Regular.ttf",
            crate::library::JETBRAINS_MONO_NERD_REGULAR,
        ),
        (
            "JetBrainsMonoNerdFontMono-Bold.ttf",
            crate::library::JETBRAINS_MONO_NERD_BOLD,
        ),
        (
            "JetBrainsMonoNerdFontMono-Italic.ttf",
            crate::library::JETBRAINS_MONO_NERD_ITALIC,
        ),
        (
            "JetBrainsMonoNerdFontMono-BoldItalic.ttf",
            crate::library::JETBRAINS_MONO_NERD_BOLD_ITALIC,
        ),
    ] {
        fs::write(font_dir.join(name), bytes)
            .map_err(|e| format!("failed to write bundled font {name}: {e}"))?;
    }

    // Render each page
    for p in &doc.pages {
        let html = page::render_page(doc, p, "styles.css");
        collect_referenced_image_assets(&html, &asset_extensions, &mut referenced_assets);
        let filename = format!("{}.html", p.id);
        fs::write(output.join(&filename), &html)
            .map_err(|e| format!("failed to write {filename}: {e}"))?;
    }

    // index.html redirects to the first page by section order
    if let Some(first) = first_page_by_section_order(&doc.sections, &doc.pages) {
        let target = format!("{}.html", first.id);
        let redirect = format!(
            "<!DOCTYPE html><html><head>\
             <meta http-equiv=\"refresh\" content=\"0;url={target}\">\
             </head><body></body></html>"
        );
        fs::write(output.join("index.html"), redirect)
            .map_err(|e| format!("failed to write index.html: {e}"))?;
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

/// Walk the section tree in declaration order and return the first page found.
fn first_page_by_section_order<'a>(
    sections: &[crate::model::Section],
    pages: &'a [crate::model::Page],
) -> Option<&'a crate::model::Page> {
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
    use crate::model::{ContentBlock, Layout, LayoutItem, Page, Section, WdocDocument};

    fn doc_with_html(html: &str) -> WdocDocument {
        WdocDocument {
            name: "docs".to_string(),
            title: "Docs".to_string(),
            version: None,
            author: None,
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
