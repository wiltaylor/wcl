//! WCL-driven static site / document generator.
//!
//! Embeds a bundled `wdoc.wcl` standard library (page + h1..h6 + p +
//! diagram + … blocks) that a user document opts into with
//! `import <wdoc.wcl>`, and renders pages declared in user `.wcl` files to
//! HTML / PDF / Markdown. The `wcl` CLI wires these entry points behind
//! `wcl wdoc build` / `pdf` / `markdown` (and drives its own dev server for
//! `wcl wdoc serve`).

pub mod build;
mod card;
pub mod comments;
mod demo;
mod dopesheet;
mod file;
mod force;
mod highlight;
mod icons;
mod image;
mod include;
mod inline;
mod layered;
mod map;
mod markdown;
mod math;
mod node_table;
pub mod pdf;
mod radial;
mod render;
pub mod review;
mod routing;
mod terminal;
mod text;
mod tileset;
mod timeline;
mod tree;
mod video;
mod visibility;
mod wireframe;

pub use build::{
    BuildError, BuildOptions, PageSubSite, RebuildOutcome, build, build_incremental,
    build_with_options, doc_entry_for_page, open_doc_for_edit, open_doc_for_edit_with_overlay,
    schema_registry, subsite_for_page, take_render_warnings,
};
pub use comments::{CommentRecord, CommentScope};
pub use markdown::{markdown, skill};
pub use pdf::{PageSize, PdfError, pdf};
pub use review::Handshake;

/// Highlight `source` as HTML `<span class="tok-…">` runs using the same
/// syntect grammar + classed output the rendered code blocks use — so an
/// in-browser editor (the `serve --edit` source editor) gets identical
/// token classes to the site's own `code` blocks, styled by the theme CSS
/// already on every page. Unknown languages fall back to plain text.
pub fn highlight_code(source: &str, language: &str) -> String {
    highlight::highlight_html(source, language, false)
}
