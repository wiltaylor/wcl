pub mod library;
pub mod markup;
pub mod model;
pub mod render;
#[cfg(feature = "wdoc-serve")]
pub mod serve;
pub mod source;

pub use crate::render::{graph_layout, shapes, terminal};

use crate::wdoc::model::WdocDocument;

/// Render a `WdocDocument` to HTML in the given output directory.
/// `asset_dirs` are source directories containing images/assets to copy.
pub fn render_to(
    doc: &WdocDocument,
    output: &std::path::Path,
    asset_dirs: &[&std::path::Path],
) -> Result<(), String> {
    render::render_document(doc, output, asset_dirs)
}
