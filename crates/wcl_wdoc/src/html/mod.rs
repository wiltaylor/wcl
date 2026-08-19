//! The HTML backend: the reading of a wdoc document that `wcl wdoc build`
//! and `wcl wdoc serve` emit as a static site.
//!
//! Sits on the backend-neutral core in [`crate::render`] (value accessors,
//! repeater / component expansion, `lower` dispatch) and on
//! [`crate::svg`] for the drawings it embeds, the same way
//! [`crate::markdown`] and [`crate::pdf`] do. Split into focused
//! submodules — kept flat (every item is `pub(crate)` and re-exported
//! here) so they read as one logical unit:
//!
//! - [`content`] — the HTML reading of the semantic content IR
//! - [`css`] — injected style constants + `class`-block lowering
//! - [`lower`] — the HTML fundamental dispatch
//! - [`page`] — page shell, templates, page-level blocks + HTML fundamentals
//! - [`postprocess`] — heading anchors, slugs, and footnote rewriting
//! - [`theme`] — a `theme` block → `--wdoc-*` custom-property CSS

mod content;
mod css;
mod lower;
mod page;
mod postprocess;
mod theme;

pub(crate) use content::*;
pub(crate) use css::*;
pub(crate) use lower::*;
pub(crate) use page::*;
pub(crate) use postprocess::*;
pub(crate) use theme::*;
