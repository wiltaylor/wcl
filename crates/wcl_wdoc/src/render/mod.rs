//! The rendering core every wdoc backend shares.
//!
//! Backend-neutral on purpose: reading values off blocks, expanding
//! repeaters and components, calling a block's `lower` function and
//! classifying what it returns, resolving a colour theme to role colours,
//! and collecting the diagnostics a render pass accumulates. The backends
//! that consume it are siblings — [`crate::html`], [`crate::svg`],
//! [`crate::markdown`], [`crate::pdf`] and [`crate::blocks::terminal`].
//!
//! Split into focused submodules — kept flat (every item is `pub(crate)`
//! and re-exported here) so they read as one logical unit:
//!
//! - [`accessors`] — field / map / value readers + HTML escaping
//! - [`expand`] — `wdoc_repeater` / `wdoc_component` body expansion, shared
//!   by the HTML and SVG (diagram) paths
//! - [`lower`] — `lower`-function dispatch, the lowered-node classification
//!   every backend walks, and the render-pass diagnostic sinks
//! - [`theme`] — a `theme` block → concrete role colours

mod accessors;
mod expand;
mod lower;
mod theme;

pub(crate) use accessors::*;
pub(crate) use expand::*;
pub(crate) use lower::*;
pub(crate) use theme::*;

#[cfg(test)]
mod tests {
    use super::{escape_html, kind_for_variant, kind_to_typename};

    #[test]
    fn escapes_html_specials() {
        assert_eq!(
            escape_html("<a href=\"x\">hi & 'bye'</a>"),
            "&lt;a href=&quot;x&quot;&gt;hi &amp; &#39;bye&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn kind_to_typename_capitalises() {
        assert_eq!(kind_to_typename("process"), "Process");
        assert_eq!(kind_to_typename("decision"), "Decision");
        assert_eq!(kind_to_typename("h1"), "H1");
    }

    #[test]
    fn kind_for_variant_lowercases() {
        assert_eq!(kind_for_variant("Process"), "process");
        assert_eq!(kind_for_variant("Paragraph"), "paragraph");
        assert_eq!(kind_for_variant("Rect"), "rect");
    }
}
