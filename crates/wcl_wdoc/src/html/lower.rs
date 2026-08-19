//! HTML lowering dispatch: invoke a block's `lower` function and render the
//! fundamental HTML variants it returns.
//!
//! The shared half — calling the lowering, classifying what came back, and
//! recursing into custom variants — lives in [`crate::render::lower`]; this
//! module is the HTML reading of the result, the sibling of
//! [`crate::svg::lower`].

use wcl_lang::{Block, Document, Value, VariantPayload};

use crate::inline::InlinePatterns;
use crate::render::*;

use super::*;

/// Custom HTML-block lowering (h1..h6, text, code, callout, wireframe
/// widgets, and friends).
pub(crate) fn lower_html_block(
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    patterns: &InlinePatterns,
) -> String {
    // Route through `lower_block` so HTML shares the other backends'
    // error handling: a `lower` body that errors records a fatal eval
    // diagnostic (it used to be silently swallowed here), and a missing
    // lowering records a warning. A block lowering to the content IR
    // renders through the shared reading of it (`render_content`).
    let Some(items) = lower_block(doc, block, kind) else {
        return String::new();
    };
    items
        .iter()
        .map(|item| match item {
            Lowered::Content(node) => render_content(doc, node, patterns),
            Lowered::Html(value) => render_html_variant(doc, value, 0, patterns),
        })
        .collect()
}

/// If `value` is an `Html::Head`, render its `children` (the
/// head fragment) and return it; otherwise `None`. Lets `render_template`
/// hoist a template's top-level head fundamentals into the page `<head>`.
pub(crate) fn head_fundamental_html_with_blocks(
    doc: &Document,
    value: &Value,
    patterns: &InlinePatterns,
    block_renderer: Option<&BlockRenderer<'_>>,
) -> Option<String> {
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return None;
    };
    if kind_for_variant(variant) != "head" {
        return None;
    }
    let VariantPayload::Record(map) = payload else {
        return Some(String::new());
    };
    let head = match map.get("children") {
        Some(Value::List(items)) => items
            .iter()
            .map(|v| render_html_variant_with_blocks(doc, v, 0, patterns, block_renderer))
            .collect(),
        _ => String::new(),
    };
    Some(head)
}

/// Render one lowered content variant as HTML.
pub(crate) fn render_html_variant(
    doc: &Document,
    value: &Value,
    depth: usize,
    patterns: &InlinePatterns,
) -> String {
    render_html_variant_with_blocks(doc, value, depth, patterns, None)
}

/// Render a lowered variant whose payload carries child blocks the
/// caller supplies.
pub(crate) fn render_html_variant_with_blocks(
    doc: &Document,
    value: &Value,
    depth: usize,
    patterns: &InlinePatterns,
    block_renderer: Option<&BlockRenderer<'_>>,
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
        "blocks" => match (map.get("blocks"), block_renderer) {
            (Some(Value::List(handles)), Some(render)) => {
                let slot = map.get("slot").and_then(|value| match value {
                    Value::Symbol(name)
                    | Value::Identifier(name)
                    | Value::Utf8(name)
                    | Value::Ascii(name) => Some(name.as_str()),
                    _ => None,
                });
                let owner = map.get("owner").and_then(|value| match value {
                    Value::Identifier(name) | Value::Utf8(name) | Value::Ascii(name) => {
                        Some(name.as_str())
                    }
                    _ => None,
                });
                let fallback = if handles.is_empty() {
                    match map.get("fallback") {
                        Some(Value::List(items)) => items
                            .iter()
                            .map(|value| {
                                render_html_variant_with_blocks(
                                    doc,
                                    value,
                                    depth + 1,
                                    patterns,
                                    block_renderer,
                                )
                            })
                            .collect(),
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };
                render(handles, slot, owner, &fallback)
            }
            _ => String::new(),
        },
        "paragraph" => render_paragraph_payload(doc, map, patterns),
        "table" => render_table_payload(map),
        "element" => render_element_payload(doc, map, depth, patterns, block_renderer),
        "raw" => render_raw_payload(map),
        "style" => map
            .get("name")
            .and_then(|value| match value {
                Value::Symbol(name)
                | Value::Identifier(name)
                | Value::Utf8(name)
                | Value::Ascii(name) => Some(name.as_str()),
                _ => None,
            })
            .and_then(|name| patterns.style(name))
            .map(|css| format!("<style>{css}</style>"))
            .unwrap_or_default(),
        // A `Head` reached in body context renders to nothing — its
        // children are hoisted into `<head>` only when the fundamental is
        // returned at a template's top level (see `head_fundamental_html`).
        "head" => String::new(),
        // An icon, resolved against the registry so it records sprite
        // usage. Emitted by the stdlib `callout` lowering (and available
        // to any user HTML lowering).
        "icon" => render_icon_fundamental(map, patterns.icons()),
        // Inline prose run through the inline-pattern engine (bold /
        // italic / link / icon). The Rust regex engine stays a leaf; the
        // `<p>`/`<span>` wrappers around it live in WCL (the `text` lower).
        "inline" => render_inline_fundamental(doc, map, patterns),
        // Syntax-highlighted code body (syntect). Like `inline`, the
        // engine is a leaf; the `<pre><code>` wrapper is the `code` lower.
        "highlighted" => render_highlighted_fundamental(map),
        // LaTeX → self-contained SVG via RaTeX. Like `highlighted`, the
        // SVG is a Rust leaf; the centring `<div>` wrapper is in math.rs.
        "math" => crate::math::render_math_fundamental(map),
        // A custom variant: expand it through its kind's own `lower`. What
        // that produces may be content — a user block is free to lower to
        // the semantic IR through a chain of its own variants.
        other => lower_recurse(doc, map, other, depth, |v, d| match recursed_content(v) {
            Some(node) => render_content(doc, &node, patterns),
            None => render_html_variant_with_blocks(doc, v, d, patterns, block_renderer),
        }),
    }
}
