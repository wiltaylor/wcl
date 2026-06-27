//! HTML/SVG rendering for wdoc documents.
//!
//! Split into focused submodules — kept flat (every item is `pub(crate)`
//! and re-exported here) so they read as one logical unit and external
//! `crate::render::*` paths are unchanged:
//!
//! - [`accessors`] — field / map / value readers + HTML escaping
//! - [`css`] — injected style constants + `class`-block lowering
//! - [`expand`] — `wdoc_repeater` / `wdoc_component` body expansion, shared
//!   by the HTML and SVG (diagram) paths
//! - [`html`] — page shell, templates, page-level blocks + HTML fundamentals
//! - [`lower`] — `lower`-function dispatch + recursive variant rendering
//! - [`svg`] — diagram layout, edge routing, and shape geometry
//! - [`theme`] — `ColourTheme` → `--wdoc-*` custom-property CSS

mod accessors;
mod css;
mod expand;
mod headings;
mod html;
mod lower;
mod svg;
mod theme;

pub(crate) use accessors::*;
pub(crate) use css::*;
pub(crate) use expand::*;
pub(crate) use html::*;
pub(crate) use lower::*;
pub(crate) use svg::*;
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
