//! The block kinds that are only legal inside a `diagram`.
//!
//! Each extends `SvgBlock` in the stdlib, so the diagram's `@children`
//! slot accepts it and nothing else does. They are dispatched from
//! [`crate::svg::shapes`] alongside the SVG fundamentals (`rect`,
//! `circle`, `line`, …) — those stay with the backend that emits them,
//! because they *are* the emitter; these are blocks that happen to draw
//! themselves as SVG.
//!
//! What each needs Rust for: [`card`] measures a body of arbitrary wdoc
//! content and wraps it in a `<foreignObject>`; [`map`] and [`tileset`]
//! window into a user-supplied image; [`dopesheet`] and [`timeline`]
//! resolve a time scale to geometry; [`node_table`] and [`tree`] read a
//! child subtree the lowering record never sees; [`wireframe`] bakes
//! resolved theme colours into a self-contained panel.
//!
//! The layout algorithms that *place* these (`force`, `layered`,
//! `radial`, `routing`) are not blocks and live at the crate root, where
//! [`crate::svg::diagram`] drives them.

pub(crate) mod card;
pub(crate) mod dopesheet;
pub(crate) mod map;
pub(crate) mod node_table;
pub(crate) mod tileset;
pub(crate) mod timeline;
pub(crate) mod tree;
pub(crate) mod wireframe;
