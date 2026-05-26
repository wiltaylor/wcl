//! WCL-driven static site / document generator.
//!
//! Embeds a bundled `wdoc.wcl` schema (page + h1..h6 + p blocks),
//! renders pages declared in user `.wcl` files to HTML, and ships a
//! tiny dev server. The `wcl` CLI wires these entry points behind
//! `wcl wdoc build` and `wcl wdoc serve`.

pub mod build;
mod highlight;
mod icons;
mod image;
mod inline;
mod layered;
mod render;
mod routing;
pub mod serve;
mod terminal;
mod text;
mod tileset;

pub use build::{BuildError, build};
pub use serve::serve;
