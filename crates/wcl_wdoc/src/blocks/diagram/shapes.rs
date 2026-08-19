//! The fundamental diagram shapes: `rect`, `circle`, `line`, `label` and
//! `polygon`.
//!
//! Each is rendered twice over, which is why both readings live together:
//! the block-side renderers resolve anchors against the parent box before
//! drawing, while the variant-payload renderers draw geometry a lowering
//! already resolved (see [`crate::svg::lower`]). They agree by sharing the
//! `emit_*` writers in [`crate::svg::primitives`].
//!
//! `container` and `boundary` are fundamentals too, but their rendering
//! *is* the recursive layout walk — they stay in [`crate::svg::diagram`]
//! with the pass they drive.

use std::collections::BTreeMap;

use wcl_lang::{Block, Value};

use crate::render::*;
use crate::svg::*;

use super::text;

/// Render a `rect` shape.
pub(crate) fn render_rect(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let (x, y, w, h) = resolve_rect_box(block, parent_w, parent_h);
    let p = shape_paint(block);
    emit_rect(
        &p.class,
        x,
        y,
        w,
        h,
        p.fill.as_deref(),
        p.stroke.as_deref(),
        p.id.as_deref(),
    )
}

/// Render a `circle` shape.
pub(crate) fn render_circle(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let (cx, cy, r) = resolve_circle(block, parent_w, parent_h);
    let p = shape_paint(block);
    emit_circle(
        &p.class,
        cx,
        cy,
        r,
        p.fill.as_deref(),
        p.stroke.as_deref(),
        p.id.as_deref(),
    )
}

/// Render a `line` shape.
pub(crate) fn render_line(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let x1 = field_f64(block, "x1").unwrap_or(0.0);
    let y1 = field_f64(block, "y1").unwrap_or(0.0);
    let x2 = field_f64(block, "x2").unwrap_or(0.0);
    let y2 = field_f64(block, "y2").unwrap_or(0.0);
    let (ox, oy) = resolve_point_anchor(block, parent_w, parent_h);
    let p = shape_paint(block);
    emit_line(
        &p.class,
        x1 + ox,
        y1 + oy,
        x2 + ox,
        y2 + oy,
        p.stroke.as_deref(),
        p.id.as_deref(),
    )
}

/// Render a `label` shape.
pub(crate) fn render_label(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let raw = label_string(block).unwrap_or_default();
    let own_x = field_f64(block, "x").unwrap_or(0.0);
    let own_y = field_f64(block, "y").unwrap_or(0.0);
    let (x, y) = resolve_point_anchored(block, parent_w, parent_h, own_x, own_y);
    let (content, font_size) = fit_label(
        &raw,
        field_f64(block, "font_size"),
        field_f64(block, "fit_width"),
        field_f64(block, "fit_height"),
    );
    let p = shape_paint(block);
    emit_text(
        &content,
        x,
        y,
        font_size,
        &p.class,
        p.fill.as_deref(),
        p.id.as_deref(),
    )
}

/// Render a `polygon` shape.
pub(crate) fn render_polygon(block: &Block<'_>, parent_w: f64, parent_h: f64) -> String {
    let points = field_utf8(block, "points").unwrap_or_default();
    let (ox, oy) = resolve_point_anchor(block, parent_w, parent_h);
    let p = shape_paint(block);
    emit_polygon(
        &p.class,
        &points,
        ox,
        oy,
        p.fill.as_deref(),
        p.stroke.as_deref(),
        p.id.as_deref(),
    )
}

// ── Fundamental renderers (variant-payload side) ──────────────────
//
// Variant payloads carry pre-resolved geometry. Anchors are not
// honored here — lowering functions are expected to emit final
// coordinates.

/// Render a rect from an already-lowered payload.
pub(crate) fn render_rect_payload(map: &BTreeMap<String, Value>) -> String {
    let p = shape_paint(map);
    emit_rect_rounded(
        &p.class,
        map_f64(map, "x").unwrap_or(0.0),
        map_f64(map, "y").unwrap_or(0.0),
        map_f64(map, "width").unwrap_or(0.0),
        map_f64(map, "height").unwrap_or(0.0),
        map_f64(map, "rx"),
        p.fill.as_deref(),
        p.stroke.as_deref(),
        p.id.as_deref(),
    )
}

/// Render a circle from an already-lowered payload.
pub(crate) fn render_circle_payload(map: &BTreeMap<String, Value>) -> String {
    let p = shape_paint(map);
    emit_circle(
        &p.class,
        map_f64(map, "cx").unwrap_or(0.0),
        map_f64(map, "cy").unwrap_or(0.0),
        map_f64(map, "r").unwrap_or(0.0),
        p.fill.as_deref(),
        p.stroke.as_deref(),
        p.id.as_deref(),
    )
}

/// Render a line from an already-lowered payload.
pub(crate) fn render_line_payload(map: &BTreeMap<String, Value>) -> String {
    let p = shape_paint(map);
    emit_line_dashed(
        &p.class,
        map_f64(map, "x1").unwrap_or(0.0),
        map_f64(map, "y1").unwrap_or(0.0),
        map_f64(map, "x2").unwrap_or(0.0),
        map_f64(map, "y2").unwrap_or(0.0),
        p.stroke.as_deref(),
        map_utf8(map, "dash").as_deref(),
        p.id.as_deref(),
    )
}

/// Open multi-segment path (`points` as in `<polyline>`). Always
/// `fill="none"` — an open path is a stroke, not a region; closed
/// regions are `Polygon`'s job. `dash` is an inline `stroke-dasharray`
/// (inline, not a theme class, so it survives the PDF backend's SVG
/// embedding).
pub(crate) fn render_polyline_payload(map: &BTreeMap<String, Value>) -> String {
    let p = shape_paint(map);
    let mut out = format!(
        "<polyline{} points=\"{}\" fill=\"none\"",
        p.class,
        escape_html(&map_utf8(map, "points").unwrap_or_default())
    );
    append_attr(&mut out, "stroke", p.stroke.as_deref());
    append_attr(
        &mut out,
        "stroke-dasharray",
        map_utf8(map, "dash").as_deref(),
    );
    append_attr(&mut out, "id", p.id.as_deref());
    out.push_str(" />");
    out
}

/// Render a label from an already-lowered payload.
pub(crate) fn render_label_payload(map: &BTreeMap<String, Value>) -> String {
    let raw = map_utf8(map, "content").unwrap_or_default();
    let x = map_f64(map, "x").unwrap_or(0.0);
    let y = map_f64(map, "y").unwrap_or(0.0);
    let (content, font_size) = fit_label(
        &raw,
        map_f64(map, "font_size"),
        map_f64(map, "fit_width"),
        map_f64(map, "fit_height"),
    );
    let p = shape_paint(map);
    emit_text(
        &content,
        x,
        y,
        font_size,
        &p.class,
        p.fill.as_deref(),
        p.id.as_deref(),
    )
}

/// Render a polygon from an already-lowered payload.
pub(crate) fn render_polygon_payload(map: &BTreeMap<String, Value>) -> String {
    let p = shape_paint(map);
    emit_polygon(
        &p.class,
        &map_utf8(map, "points").unwrap_or_default(),
        0.0,
        0.0,
        p.fill.as_deref(),
        p.stroke.as_deref(),
        p.id.as_deref(),
    )
}

/// Resolve a label's rendered text + font size. An explicit `font_size` wins
/// (the text is still wrapped to a `fit_width` box if one is given); otherwise,
/// when the label auto-fits a box, word-wrap it to that box and shrink to fit
/// (so a long label stacks into lines inside the box rather than overflowing);
/// otherwise the default size with the text unchanged.
fn fit_label(
    content: &str,
    font_size: Option<f64>,
    fit_w: Option<f64>,
    fit_h: Option<f64>,
) -> (String, f64) {
    if let Some(fs) = font_size {
        let text = match fit_w {
            Some(w) => text::wrap(content, w - text::H_PAD, fs),
            None => content.to_string(),
        };
        return (text, fs);
    }
    match fit_w {
        Some(w) => {
            let inner_h = fit_h.map(|h| h - text::V_PAD).unwrap_or(1.0e9);
            text::wrap_to_box(content, w - text::H_PAD, inner_h)
        }
        None => (content.to_string(), text::DEFAULT_FONT_SIZE),
    }
}
