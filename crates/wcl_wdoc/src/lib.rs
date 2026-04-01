pub mod library;
pub mod model;
pub mod render;
pub mod serve;
pub mod templates;
pub mod validate;

use crate::model::WdocDocument;
use crate::validate::{WdocDiagnostic, WdocSeverity};

/// Validate a `WdocDocument` and return any diagnostics.
/// Returns the document and warnings, or an error string if validation fails.
pub fn validate_doc(doc: &WdocDocument) -> Result<Vec<WdocDiagnostic>, String> {
    let diags = validate::validate(doc);

    let has_errors = diags.iter().any(|d| d.severity == WdocSeverity::Error);
    if has_errors {
        let msg = diags
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(msg);
    }

    Ok(diags)
}

/// Render a `WdocDocument` to HTML in the given output directory.
/// `asset_dirs` are source directories containing images/assets to copy.
pub fn render_to(
    doc: &WdocDocument,
    output: &std::path::Path,
    asset_dirs: &[&std::path::Path],
) -> Result<(), String> {
    render::render_document(doc, output, asset_dirs)
}
