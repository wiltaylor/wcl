//! WCL-driven static site / document generator.
//!
//! Embeds a bundled `wdoc.wcl` standard library (page + h1..h6 + p +
//! diagram + … blocks) that a user document opts into with
//! `import <wdoc.wcl>`, and renders pages declared in user `.wcl` files to
//! HTML / PDF / Markdown. The `wcl` CLI wires these entry points behind
//! `wcl wdoc build` / `pdf` / `markdown` (and drives its own dev server for
//! `wcl wdoc serve`).
//!
//! # Map
//!
//! `render` is the backend-neutral core: reading values off blocks,
//! expanding repeaters and components, calling a block's `lower` function
//! and classifying what it returns, resolving a colour theme to role
//! colours, and collecting a pass's diagnostics. The backends sit on top of
//! it, one per output, each a walker over the same lowered nodes —
//! `html` (the static site), `svg` (the drawings HTML and PDF embed),
//! `markdown`, [`pdf`](mod@pdf), and `terminal`. The widget modules between them
//! (`card`, `map`, `tree`, `timeline`, `wireframe`, …) render one block
//! kind apiece and are consumed by whichever backends can show it.
//!
//! [`build`](mod@build) drives the HTML backend end to end and holds the entry points
//! the CLI calls.

/// Site building: the HTML backend and the build entry points.
pub mod build;
mod card;
pub mod content;
mod css_lint;
mod demo;
mod dopesheet;
mod file;
mod force;
pub mod git;
mod highlight;
mod html;
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
mod svg;
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
