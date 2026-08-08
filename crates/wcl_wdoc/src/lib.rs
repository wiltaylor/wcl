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
pub mod content;
mod demo;
mod dopesheet;
mod file;
mod force;
pub mod git;
mod highlight;
mod icons;
mod image;
mod inline;
mod layered;
mod map;
mod markdown;
mod math;
mod native;
mod node_table;
mod page_metadata;
pub mod pdf;
mod radial;
mod render;
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
    BuildError, BuildOptions, RebuildOutcome, build, build_incremental, build_with_options,
    schema_registry, take_render_warnings, wdoc_environment,
};
pub use markdown::markdown;
pub use pdf::{PageSize, PdfError, pdf};
