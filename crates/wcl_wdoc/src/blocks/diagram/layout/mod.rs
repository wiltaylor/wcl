//! Where a diagram's children end up, and how the edges get between them.
//!
//! A `diagram` (or `container`) picks a solver with its `layout` symbol:
//! [`layered`] ranks shapes topologically against the `@connections(Edge)`
//! graph, [`force`] relaxes them as charged particles on springs, and
//! [`radial`] rings them around a hub. Each takes the children plus the
//! edge graph and returns one `(tx, ty)` offset per child — they assign
//! position and nothing else, so a shape's own `width` / `height` still
//! decides its size. [`routing`] runs afterwards, threading orthogonal
//! edge paths around the boxes the solver placed.
//!
//! All four are pure and deterministic, which is load-bearing rather than
//! tidy: the collect pass and the render pass each recompute the layout
//! independently, so identical inputs must give byte-identical results or
//! the routed edges would drift away from the drawn shapes.
//!
//! Driven by [`crate::svg::diagram`], which is the only caller that picks
//! between them.

pub(crate) mod force;
pub(crate) mod layered;
pub(crate) mod radial;
pub(crate) mod routing;
