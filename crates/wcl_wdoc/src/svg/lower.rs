//! SVG lowering dispatch: invoke a block's `lower` function and render the
//! fundamental shape variants it returns.
//!
//! The shared half — calling the lowering, classifying what came back, and
//! recursing into custom variants — lives in [`crate::render::lower`]; this
//! module is the SVG reading of the result, the sibling of
//! [`crate::html::lower`].

use wcl_lang::{Block, Document, Value, VariantPayload};

use crate::blocks::diagram::shapes::*;
use crate::inline::InlinePatterns;
use crate::render::*;

/// Custom diagram-shape lowering. Resolves the block's `lower`
/// function, calls it with a record built from the block's fields,
/// and renders each returned variant. `links` is the inline-pattern
/// resolver used by `Link` fundamentals; `None` renders their children
/// unwrapped.
pub(crate) fn lower_svg_block(
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    parent_w: f64,
    parent_h: f64,
    links: Option<&InlinePatterns>,
) -> String {
    let Some(items) = lower_to_values(doc, block, kind) else {
        return String::new();
    };
    items
        .iter()
        .map(|v| render_svg_variant(doc, v, parent_w, parent_h, 0, links))
        .collect()
}

// `_parent_w` / `_parent_h` are threaded through so future variant
// kinds can pick them up; today's fundamentals carry pre-resolved
// geometry in the payload itself.
/// Render one lowered content variant as SVG.
pub(crate) fn render_svg_variant(
    doc: &Document,
    value: &Value,
    _parent_w: f64,
    _parent_h: f64,
    depth: usize,
    links: Option<&InlinePatterns>,
) -> String {
    if depth > MAX_LOWER_DEPTH {
        return depth_marker();
    }
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return String::new();
    };
    let kind = kind_for_variant(variant);
    let VariantPayload::Record(map) = payload else {
        return String::new();
    };
    match kind.as_str() {
        "rect" => render_rect_payload(map),
        "circle" => render_circle_payload(map),
        "line" => render_line_payload(map),
        "label" => render_label_payload(map),
        "polygon" => render_polygon_payload(map),
        "polyline" => render_polyline_payload(map),
        // A recursive wrapper giving lowered sub-shapes a clickable
        // in-site link. Without a resolver the children pass through
        // unwrapped (PDF embedding, contexts with no page registry).
        "link" => {
            let children = match map.get("children") {
                Some(Value::List(items)) => items.as_slice(),
                _ => &[],
            };
            let inner: String = children
                .iter()
                .map(|v| render_svg_variant(doc, v, _parent_w, _parent_h, depth + 1, links))
                .collect();
            match (map_utf8(map, "href"), links) {
                (Some(href), Some(patterns)) => format!(
                    "<a href=\"{}\">{inner}</a>",
                    escape_html(&patterns.resolve_href(&href))
                ),
                _ => inner,
            }
        }
        // Custom variant — look up its type's `lower` and recurse with the
        // variant's record payload as the new arg.
        other => lower_recurse(doc, map, other, depth, |v, d| {
            render_svg_variant(doc, v, _parent_w, _parent_h, d, links)
        }),
    }
}
