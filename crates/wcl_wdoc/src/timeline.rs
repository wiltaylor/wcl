//! The `timeline` renderer.
//!
//! A timeline lowers to SVG fundamentals in WCL (axis, phases, ticks,
//! markers, plain-text item labels — see `lib/timeline.wcl`). Its event
//! `card` children, however, carry rich wdoc content that must render as
//! HTML `<foreignObject>`s, which WCL can't express. So the timeline is
//! special-cased here (like `map`) to thread the [`RenderCtx`] through:
//! we run the WCL `lower`, render every ordinary fundamental as usual,
//! and intercept the `SvgFundamental::Card { x, y, width, height, index }`
//! markers — each `index` names the timeline's Nth `card` child, whose
//! body we draw with [`crate::card::render_card_foreign`] at the
//! WCL-computed box.

use wcl_lang::{Block, Value, VariantPayload};

use crate::card::render_card_foreign;
use crate::render::{
    RenderCtx, field_id, field_utf8, field_utf8_list, kind_for_variant, lower_to_values, map_f64,
    render_svg_variant,
};

/// Render a `@block("timeline")`: its WCL-lowered SVG chrome plus a
/// `<foreignObject>` card for each event child. Falls back to plain
/// fundamental rendering when there are no cards (byte-identical to the
/// generic `lower_svg_block` path).
pub(crate) fn render_timeline(
    block: &Block<'_>,
    ctx: RenderCtx<'_>,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let Some(items) = lower_to_values(ctx.doc, block, "timeline") else {
        return String::new();
    };
    // The event cards, in source order — the same order the WCL `lower`
    // saw them (via `block_to_record`), so a `Card`'s `index` lines up.
    let cards: Vec<Block<'_>> = block.blocks().filter(|b| b.kind() == "card").collect();

    let mut out = String::new();
    for value in &items {
        if let Some(card) = card_foreign(value, &cards, ctx) {
            out.push_str(&card);
        } else {
            out.push_str(&render_svg_variant(ctx.doc, value, parent_w, parent_h, 0));
        }
    }
    out
}

/// If `value` is a `SvgFundamental::Card`, resolve its referenced child
/// block and render it as a `<foreignObject>`; otherwise `None` (the
/// caller renders it as an ordinary fundamental).
fn card_foreign(value: &Value, cards: &[Block<'_>], ctx: RenderCtx<'_>) -> Option<String> {
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return None;
    };
    if kind_for_variant(variant) != "card" {
        return None;
    }
    let VariantPayload::Record(map) = payload else {
        return None;
    };
    let x = map_f64(map, "x").unwrap_or(0.0);
    let y = map_f64(map, "y").unwrap_or(0.0);
    let w = map_f64(map, "width").unwrap_or(0.0);
    let h = map_f64(map, "height").unwrap_or(0.0);
    let index = map_f64(map, "index").unwrap_or(0.0) as usize;
    let card = cards.get(index)?;
    let body: Vec<Block<'_>> = card.blocks().collect();
    Some(render_card_foreign(
        field_utf8(card, "title").as_deref(),
        &body,
        &field_utf8_list(card, "class"),
        field_id(card, "id").as_deref(),
        (x, y, w, h),
        ctx,
    ))
}
