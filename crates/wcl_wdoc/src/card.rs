//! The `card` shape: a diagram box whose body is arbitrary wdoc content.
//!
//! Like `map` / `tilemap`, a card is `@native` (see [`crate::native`])
//! because its body is HTML — produced by the block renderer + inline
//! engine ([`render_block`]) — not SVG primitives. The renderer wraps that
//! HTML in an SVG `<foreignObject>` so it's drawn in place, always visible,
//! and scales with the diagram.
//!
//! Two entry points share one builder ([`render_card_foreign`]):
//!   - [`render_card`] — the free-standing `card` shape, positioned by
//!     `x`/`y`/anchors like a `rect`;
//!   - the `timeline` renderer (`crate::timeline`), which positions event
//!     cards by the timeline's scale and calls the builder directly.

use std::fmt::Write as _;

use wcl_lang::Block;

use crate::render::{
    RenderCtx, escape_html, field_id, field_utf8, field_utf8_list, render_block, resolve_rect_box,
};

/// Render a free-standing `@block("card")` placed in a diagram /
/// container: resolve its box (anchor-aware, like `rect`) and draw its
/// `@children(WdocBlock)` body as a `<foreignObject>`.
pub(crate) fn render_card(
    block: &Block<'_>,
    ctx: RenderCtx<'_>,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let (x, y, w, h) = resolve_rect_box(block, parent_w, parent_h);
    let body: Vec<Block<'_>> = block.blocks().collect();
    render_card_foreign(
        field_utf8(block, "title").as_deref(),
        &body,
        &field_utf8_list(block, "class"),
        field_id(block, "id").as_deref(),
        (x, y, w, h),
        ctx,
    )
}

/// Shared builder: an SVG `<foreignObject>` holding a `wdoc-card` `<div>`
/// with an optional title and the rendered body blocks. The div carries
/// the XHTML namespace so the (HTML5) content parses correctly inside the
/// SVG. Returns `""` when there's nothing to draw.
pub(crate) fn render_card_foreign(
    title: Option<&str>,
    body: &[Block<'_>],
    extra_classes: &[String],
    id: Option<&str>,
    (x, y, w, h): (f64, f64, f64, f64),
    ctx: RenderCtx<'_>,
) -> String {
    let mut inner = String::new();
    if let Some(t) = title {
        let _ = write!(
            inner,
            "<div class=\"wdoc-card-title\">{}</div>",
            escape_html(t)
        );
    }
    for child in body {
        if let Some(html) = render_block(ctx.doc, child, ctx.patterns, ctx.base_dir) {
            inner.push_str(&html);
        }
    }
    if inner.is_empty() {
        return String::new();
    }

    let mut classes = String::from("wdoc-card");
    for c in extra_classes {
        classes.push(' ');
        classes.push_str(&escape_html(c));
    }
    let id_attr = id
        .map(|i| format!(" data-card-id=\"{}\"", escape_html(i)))
        .unwrap_or_default();
    format!(
        "<foreignObject x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\">\
         <div xmlns=\"http://www.w3.org/1999/xhtml\" class=\"{classes}\"{id_attr}>{inner}</div>\
         </foreignObject>"
    )
}
