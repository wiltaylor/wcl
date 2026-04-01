pub mod assets;
pub mod content;
pub mod layout;
pub mod page;

use std::fs;
use std::path::Path;

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

    // Render each page
    for p in &doc.pages {
        let html = page::render_page(doc, p, "styles.css");
        let filename = format!("{}.html", p.id);
        fs::write(output.join(&filename), &html)
            .map_err(|e| format!("failed to write {filename}: {e}"))?;
    }

    // index.html redirects to the first page
    if let Some(first) = doc.pages.first() {
        let target = format!("{}.html", first.id);
        let redirect = format!(
            "<!DOCTYPE html><html><head>\
             <meta http-equiv=\"refresh\" content=\"0;url={target}\">\
             </head><body></body></html>"
        );
        fs::write(output.join("index.html"), redirect)
            .map_err(|e| format!("failed to write index.html: {e}"))?;
    }

    // Copy asset files (images, etc.) from source directories
    let asset_extensions = ["png", "jpg", "jpeg", "gif", "svg", "webp", "ico"];
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
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if asset_extensions.contains(&ext) {
                        let dest = output.join(path.file_name().unwrap());
                        let _ = fs::copy(&path, &dest);
                    }
                }
            }
        }
    }

    Ok(())
}

fn copy_dir_assets(src: &Path, dest: &Path, extensions: &[&str]) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("mkdir {}: {e}", dest.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("read {}: {e}", src.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext) {
                    let dest_file = dest.join(path.file_name().unwrap());
                    fs::copy(&path, &dest_file)
                        .map_err(|e| format!("copy {}: {e}", path.display()))?;
                }
            }
        }
    }
    Ok(())
}
