//! The **wskill model**: a wskill folder's unit graph, as typed Rust.
//!
//! A wskill is a folder of WCL data about one topic — units (concepts, facts,
//! procedures, lessons, …), curated indexes that pin them into a reading
//! order, and `related` links between them — plus a registry (`wskill.wcl`)
//! declaring the projections that render it (a book, a Claude skill, a deck,
//! a course). [`Graph`] is all of that read into memory:
//!
//! ```no_run
//! let graph = wcl_wskill::Graph::open(std::path::Path::new("docs/wskills/wcl"))?;
//! for unit in graph.unindexed() {
//!     println!("{} is in no index", unit.id);
//! }
//! # Ok::<(), wcl_wskill::Error>(())
//! ```
//!
//! The crate sits between [`wcl_wdoc`] (which renders the projections) and
//! the `wcl` binary, because the model has more than one consumer and none of
//! them should have to run an editor to get at it: `wcl wskill graph` prints
//! it, the curator agent reads and audits it, and the browser editor's graph
//! view is a layout and a serialisation over it.
//!
//! Two things follow from the curator being a first-class consumer:
//!
//! - a [`Graph`] can be loaded **at a git revision** ([`Graph::open_at_rev`]),
//!   because auditing a change means diffing two of them;
//! - a [`Graph`] is **owned** — nothing borrows the document it was read
//!   from, so two revisions can be held at once.
//!
//! What is *not* here: layout (a graph view lays out what it draws), the
//! wire shapes of any HTTP endpoint, and rendering. The model says what the
//! wskill is.

mod load;
mod model;
mod registry;
#[cfg(test)]
mod testsupport;

pub use load::Error;
pub use model::{
    Anchor, ContentBlock, Course, CourseModule, Edge, EdgeKind, Graph, Index, NodeKey, Topic, Unit,
    View, Visibility, routes_to, structural_view_kind,
};
pub use registry::{Artifact, ROOT_MARKER, Registry};
