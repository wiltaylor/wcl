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
//! `markdown` and [`pdf`](mod@pdf).
//!
//! `blocks` holds one module per block kind that needs Rust to render at
//! all, split by where the kind is legal: `blocks::diagram` for the ones
//! only a `diagram` accepts, `blocks` itself for the ones that reach
//! ordinary page content. Each is consumed by whichever backends can show
//! its kind, and each brings its own supporting machinery with it —
//! `blocks::diagram::layout` (the placement solvers and edge router),
//! `blocks::diagram::text` (the metrics that size a shape to its label),
//! `blocks::highlight` (syntect, behind the `code` block).
//!
//! What is left at the root is what belongs to no single kind: `content`
//! (the semantic IR every backend walks), `inline` (the prose pattern
//! engine), `native` (which kinds are rendered in Rust, and by which
//! backends), `visibility`, `css_lint`, `page_metadata` and `git`.
//!
//! [`build`](mod@build) drives the HTML backend end to end and holds the entry points
//! the CLI calls.

mod blocks;
/// Site building: the HTML backend and the build entry points.
pub mod build;
pub mod content;
mod css_lint;
pub mod git;
mod html;
mod inline;
mod markdown;
mod native;
mod page_metadata;
pub mod pdf;
mod render;
mod svg;
mod visibility;

pub use build::{
    BuildError, BuildOptions, RebuildOutcome, build, build_incremental, build_with_options,
    schema_registry, take_render_warnings, wdoc_environment,
};
pub use markdown::markdown;
pub use pdf::{PageSize, PdfError, pdf};
