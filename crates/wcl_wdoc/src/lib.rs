//! WCL-driven static site / document generator.
//!
//! Embeds a bundled `wdoc.wcl` standard library (page + h1..h6 + p +
//! diagram + … blocks) that a user document opts into with
//! `import <wdoc.wcl>`, renders pages declared in user `.wcl` files to
//! HTML, and ships a tiny dev server. The `wcl` CLI wires these entry
//! points behind `wcl wdoc build` and `wcl wdoc serve`.

pub mod build;
mod card;
mod dopesheet;
mod force;
mod highlight;
mod icons;
mod image;
mod inline;
mod layered;
mod map;
mod math;
pub mod pdf;
mod render;
mod routing;
pub mod serve;
mod terminal;
mod text;
mod tileset;
mod timeline;
mod video;
mod wireframe;

pub use build::{BuildError, build, schema_registry};
pub use pdf::{PageSize, PdfError, pdf};
pub use serve::serve;
