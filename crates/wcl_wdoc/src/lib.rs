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
pub mod content;
mod demo;
mod dopesheet;
mod file;
mod force;
mod highlight;
mod icons;
mod image;
mod include;
mod inline;
mod kinds;
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
mod sidecar;
pub mod sites;
mod terminal;
mod text;
mod tileset;
mod timeline;
pub mod training;
mod tree;
mod video;
mod visibility;
mod wireframe;

pub use build::{
    BuildError, BuildOptions, PAGES_MANIFEST_HREF, PageSubSite, RebuildOutcome, build,
    build_incremental, build_with_options, doc_entry_for_page, open_doc_for_edit,
    open_doc_for_edit_with_overlay, pages_in_file, schema_registry, subsite_for_page,
    take_render_warnings, wdoc_environment,
};
pub use comments::{CommentRecord, CommentScope};
pub use force::layout_graph;
pub use markdown::{markdown, skill};
pub use pdf::{PageSize, PdfError, pdf};
pub use review::Handshake;
pub use sites::{EntryIncludeInfo, EntrySiteInfo, entry_site_info};

/// Highlight `source` as HTML `<span class="tok-…">` runs using the same
/// syntect grammar + classed output the rendered code blocks use — so an
/// in-browser editor gets identical token classes to the site's own `code`
/// blocks, styled by the theme CSS already on every page. Unknown languages
/// fall back to plain text.
pub fn highlight_code(source: &str, language: &str) -> String {
    highlight::highlight_html(source, language, false)
}
