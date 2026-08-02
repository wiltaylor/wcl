//! Block-kind names the render backends dispatch on.
//!
//! The HTML ([`crate::render`]), Markdown ([`crate::markdown`]) and PDF
//! ([`crate::pdf`]) backends each special-case a block vocabulary; their
//! dispatch matches use these constants so renaming a kind is a
//! compiler-checked change, not a grep. Kinds a single backend owns (page
//! chrome, wireframe widgets) stay as literals at their one site.
//!
//! **The three sets are not the same set, and never were.** That is what
//! the semantic content IR ([`crate::content`]) is retiring: a kind routed
//! through it has no entry here at all, because every backend reads it
//! from one declaration rather than special-casing the block. `callout`
//! was the first to go.

pub(crate) const COLUMN: &str = "column";
pub(crate) const FRAGMENT: &str = "fragment";
pub(crate) const REGION: &str = "region";
pub(crate) const EDIT_FIELD: &str = "edit_field";
pub(crate) const EDIT_OBJECT: &str = "edit_object";
pub(crate) const DIAGRAM: &str = "diagram";
pub(crate) const SEQUENCE_DIAGRAM: &str = "sequence_diagram";
pub(crate) const STATE_DIAGRAM: &str = "state_diagram";
pub(crate) const TERMINAL: &str = "terminal";
pub(crate) const LIST: &str = "list";
pub(crate) const TABLE: &str = "table";
pub(crate) const CODE: &str = "code";
pub(crate) const IMAGE: &str = "image";
pub(crate) const FILE: &str = "file";
pub(crate) const VIDEO: &str = "video";
pub(crate) const DEMO: &str = "demo";
pub(crate) const REPEATER: &str = "wdoc_repeater";
pub(crate) const INSTANCE: &str = "wdoc_instance";
pub(crate) const CONTENT: &str = "wdoc_content";
