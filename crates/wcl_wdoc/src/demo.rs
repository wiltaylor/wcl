//! The `demo` block: an example source listing plus a live preview of the
//! same children, rendered under both the light and the dark palette.
//!
//! `@native` (see [`crate::native`]) because it must reach the children's
//! source text ([`Block::to_source`]) and re-render the same children into
//! palette-scoped wrappers — neither is expressible in WCL.
//! The scoped `.wdoc-theme-light` / `.wdoc-theme-dark` classes are emitted by
//! [`crate::render::theme`]; CSS custom properties inherit, so each wrapper
//! re-themes its own subtree regardless of the reader's global theme toggle.
//!
//! The children are rendered to HTML *twice*, once per pane, with the build's
//! UI theme mode flipped each time — SVG content bakes the resolved palette in
//! Rust (for PDF parity), so one render cannot suit both panes. The block
//! renderer's registry writes (images / icons / videos) are keyed by source,
//! so the second pass is idempotent.

use std::fmt::Write as _;
use std::path::Path;

use wcl_lang::{Block, Document};

use crate::highlight;
use crate::inline::InlinePatterns;
use crate::render::{append_attr, escape_html, field_bool, field_id, field_utf8, render_block};

/// The formatted WCL source of a demo block's children — the "example".
/// Each child is pretty-printed via [`Block::to_source`] and joined with a
/// blank line, so the listing matches what the author wrote (normalised by
/// the formatter). Shared by the HTML and Markdown backends.
pub(crate) fn demo_source(block: &Block<'_>) -> String {
    block
        .blocks()
        .map(|b| b.to_source())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Render a `demo` block to HTML: the example source in a highlighted `code`
/// block, then the children rendered live inside a light-themed and a
/// dark-themed preview pane, side by side. With `diagram = true` the panes
/// also centre + scale their contents (so a compact diagram doesn't sit in a
/// wide, half-empty pane).
pub(crate) fn render_html(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    let diagram = field_bool(block, "diagram").unwrap_or(false);

    let mut out = String::from("<div class=\"wdoc-demo\"");
    append_attr(&mut out, "id", field_id(block, "id").as_deref());
    out.push('>');

    // Optional caption.
    if let Some(title) = field_utf8(block, "title") {
        write!(
            out,
            "<p class=\"wdoc-demo-label\">{}</p>",
            escape_html(&title)
        )
        .expect("write to String");
    }

    // Preview — the children rendered live, shown under each palette. HTML
    // content re-themes for free via the scoped `--wdoc-*` vars on each pane,
    // but SVG content (wireframe / diagrams) *bakes* the resolved UI palette in
    // Rust (no `currentColor`, for PDF parity), so one render can't adapt to
    // both panes. Render the children twice — once per mode — flipping the UI
    // theme so each pane's baked colours match its palette. Registry writes
    // (images / icons) are keyed by source, so the second pass is idempotent.
    let saved_ui = patterns.ui_theme();
    let render_mode = |mode: &str| -> String {
        let mut ui = saved_ui.clone();
        ui.mode = mode.to_string();
        patterns.set_ui_theme(ui);
        block
            .blocks()
            .filter_map(|b| render_block(doc, &b, patterns, base_dir))
            .collect()
    };
    let children_light = render_mode("light");
    let children_dark = render_mode("dark");
    patterns.set_ui_theme(saved_ui);

    let row_class = if diagram {
        "wdoc-preview-row wdoc-preview-diagram"
    } else {
        "wdoc-preview-row"
    };
    out.push_str("<p class=\"wdoc-demo-label\">Preview</p>");
    write!(out, "<div class=\"{row_class}\">").expect("write to String");
    write!(
        out,
        "<div class=\"wdoc-body wdoc-theme-light wdoc-preview\">{children_light}</div>\
         <div class=\"wdoc-body wdoc-theme-dark wdoc-preview\">{children_dark}</div>"
    )
    .expect("write to String");
    out.push_str("</div>");

    // Example — the children's formatted source, highlighted like `code`
    // (mirrors the WCL `code` lower: tokens inside `<pre><code language-…>`),
    // shown just below the preview.
    let source = demo_source(block);
    if !source.is_empty() {
        let body = highlight::highlight_html(&source, "wcl", false);
        out.push_str("<p class=\"wdoc-demo-label\">Example</p>");
        write!(
            out,
            "<pre class=\"code-block\"><code class=\"language-wcl\">{body}</code></pre>"
        )
        .expect("write to String");
    }

    out.push_str("</div>");
    out
}
