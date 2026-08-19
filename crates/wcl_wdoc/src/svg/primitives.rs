//! The SVG emitter toolkit: the `emit_*` element writers every shape is
//! ultimately built from, and the box / anchor geometry resolvers that
//! turn a block's declared position into one.
//!
//! Nothing here knows about a block *kind* — the fundamental shapes that
//! call these (`rect`, `circle`, `line`, `label`, `polygon`) are blocks
//! and live in [`crate::blocks::diagram::shapes`]. The exception is
//! `container` / `boundary`, whose rendering is the recursive layout walk
//! itself and stays in [`super::diagram`].

use std::fmt::Write as _;

use wcl_lang::Block;

use crate::blocks::diagram::text;

use super::*;

/// Pick the font size for a label: explicit `font_size` overrides
/// everything; otherwise auto-fit to the optional `fit_width` /
/// `fit_height` (with padding applied); otherwise the default.
pub(crate) fn resolve_label_font_size(
    content: &str,
    font_size: Option<f64>,
    fit_w: Option<f64>,
    fit_h: Option<f64>,
) -> f64 {
    if let Some(fs) = font_size {
        return fs;
    }
    match (fit_w, fit_h) {
        (Some(w), Some(h)) => text::fit_font_size(content, w - text::H_PAD, h - text::V_PAD),
        _ => text::DEFAULT_FONT_SIZE,
    }
}

/// Emit a centred multi-line `<text>` element. Each line goes in
/// its own `<tspan x="cx" dy="...">`, the first tspan shifted up
/// by `(lines-1)/2 * 1.2em` so the whole block straddles `(cx, cy)`
/// vertically. `text-anchor="middle"` + `dominant-baseline="middle"`
/// handle horizontal + vertical alignment for each line.
pub(crate) fn emit_text(
    content: &str,
    cx: f64,
    cy: f64,
    font_size: f64,
    class_attr: &str,
    fill: Option<&str>,
    id: Option<&str>,
) -> String {
    let metrics = text::measure(content);
    let mut out = format!(
        "<text{class_attr} x=\"{cx}\" y=\"{cy}\" font-size=\"{font_size}\" \
         text-anchor=\"middle\" dominant-baseline=\"middle\""
    );
    append_attr(&mut out, "fill", fill);
    append_attr(&mut out, "id", id);
    out.push('>');
    let n = metrics.lines.len();
    let first_dy = if n <= 1 {
        0.0
    } else {
        -((n as f64 - 1.0) / 2.0) * text::LINE_HEIGHT
    };
    for (i, line) in metrics.lines.iter().enumerate() {
        let dy = if i == 0 { first_dy } else { text::LINE_HEIGHT };
        write!(
            out,
            "<tspan x=\"{cx}\" dy=\"{dy}em\">{}</tspan>",
            escape_html(line)
        )
        .expect("write to String");
    }
    out.push_str("</text>");
    out
}

// ── Shared SVG emitters ───────────────────────────────────────────
//
// The single production site for each fundamental's SVG string. The
// block-side renderers (which resolve anchors against parent dims) and
// the variant-payload renderers (which carry pre-resolved geometry)
// both read their own source — `field_*` vs `map_*` — then hand the
// resolved primitives here, so the element markup lives in one place.

#[allow(clippy::too_many_arguments)] // cohesive <rect> attributes
/// Emit an SVG `<rect>` with the given geometry and paint.
pub(crate) fn emit_rect(
    cls: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: Option<&str>,
    stroke: Option<&str>,
    id: Option<&str>,
) -> String {
    emit_rect_rounded(cls, x, y, w, h, None, fill, stroke, id)
}

/// [`emit_rect`] with an optional corner radius (`rx`) — the rounded
/// box used by statechart states.
#[allow(clippy::too_many_arguments)] // cohesive <rect> attributes
pub(crate) fn emit_rect_rounded(
    cls: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rx: Option<f64>,
    fill: Option<&str>,
    stroke: Option<&str>,
    id: Option<&str>,
) -> String {
    let mut out = format!("<rect{cls} x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"");
    if let Some(rx) = rx {
        write!(out, " rx=\"{rx}\"").expect("write to String");
    }
    append_attr(&mut out, "fill", fill);
    append_attr(&mut out, "stroke", stroke);
    append_attr(&mut out, "id", id);
    out.push_str(" />");
    out
}

/// Emit an SVG `<circle>`.
pub(crate) fn emit_circle(
    cls: &str,
    cx: f64,
    cy: f64,
    r: f64,
    fill: Option<&str>,
    stroke: Option<&str>,
    id: Option<&str>,
) -> String {
    let mut out = format!("<circle{cls} cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"");
    append_attr(&mut out, "fill", fill);
    append_attr(&mut out, "stroke", stroke);
    append_attr(&mut out, "id", id);
    out.push_str(" />");
    out
}

/// Emit an SVG `<line>`.
pub(crate) fn emit_line(
    cls: &str,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: Option<&str>,
    id: Option<&str>,
) -> String {
    emit_line_dashed(cls, x1, y1, x2, y2, stroke, None, id)
}

/// [`emit_line`] with an optional inline `stroke-dasharray` (inline,
/// not a theme class, so it survives the PDF backend's SVG embedding).
#[allow(clippy::too_many_arguments)] // cohesive <line> attributes
pub(crate) fn emit_line_dashed(
    cls: &str,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: Option<&str>,
    dash: Option<&str>,
    id: Option<&str>,
) -> String {
    let mut out = format!("<line{cls} x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"");
    append_attr(&mut out, "stroke", stroke);
    append_attr(&mut out, "stroke-dasharray", dash);
    append_attr(&mut out, "id", id);
    out.push_str(" />");
    out
}

/// `points` is escaped here. A non-zero `(ox, oy)` becomes a
/// `transform="translate(ox oy)"` (the block-side anchor offset);
/// pre-resolved payloads pass `(0.0, 0.0)` and so emit no transform.
pub(crate) fn emit_polygon(
    cls: &str,
    points: &str,
    ox: f64,
    oy: f64,
    fill: Option<&str>,
    stroke: Option<&str>,
    id: Option<&str>,
) -> String {
    let mut out = format!("<polygon{cls} points=\"{}\"", escape_html(points));
    if ox != 0.0 || oy != 0.0 {
        write!(out, " transform=\"translate({ox} {oy})\"").expect("write to String");
    }
    append_attr(&mut out, "fill", fill);
    append_attr(&mut out, "stroke", stroke);
    append_attr(&mut out, "id", id);
    out.push_str(" />");
    out
}

// ── Box / anchor geometry resolvers ───────────────────────────────

/// Resolve a rect's box, honouring percentage sizes against the
/// parent and any declared anchor.
pub(crate) fn resolve_rect_box(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
    let mut x = field_f64(block, "x").unwrap_or(0.0);
    let mut y = field_f64(block, "y").unwrap_or(0.0);
    let mut w = field_f64(block, "width").unwrap_or(0.0);
    let mut h = field_f64(block, "height").unwrap_or(0.0);
    apply_axis_anchor(
        &mut x,
        &mut w,
        field_f64(block, "anchor_left"),
        field_f64(block, "anchor_right"),
        parent_w,
    );
    apply_axis_anchor(
        &mut y,
        &mut h,
        field_f64(block, "anchor_top"),
        field_f64(block, "anchor_bottom"),
        parent_h,
    );
    (x, y, w, h)
}

/// Resolve a container's box, which sizes to its children when no
/// explicit size is declared.
pub(crate) fn resolve_container_box(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
    let mut x = 0.0;
    let mut y = 0.0;
    // Auto-fit to content. When the container uses :layered or :grid
    // layout, its natural size is the bbox of its laid-out children
    // plus 2*padding on each axis; we honor a declared width/height
    // as a minimum but never as a ceiling. This is what lets
    // `stroke = "..."` chrome hug the contents (with the requested
    // inset, if any) instead of painting a fixed-size frame with
    // empty space inside. :none layout keeps the old behaviour
    // (parent fallback) since children there carry their own coords.
    let padding = container_padding(block);
    let (content_w, content_h) = content_size(block);
    let outer_w = if content_w > 0.0 {
        content_w + 2.0 * padding
    } else {
        0.0
    };
    let outer_h = if content_h > 0.0 {
        content_h + 2.0 * padding
    } else {
        0.0
    };
    let decl_w = field_f64(block, "width");
    let decl_h = field_f64(block, "height");
    let mut w = match decl_w {
        Some(d) => d.max(outer_w),
        None if outer_w > 0.0 => outer_w,
        None => parent_w,
    };
    let mut h = match decl_h {
        Some(d) => d.max(outer_h),
        None if outer_h > 0.0 => outer_h,
        None => parent_h,
    };
    apply_axis_anchor(
        &mut x,
        &mut w,
        field_f64(block, "anchor_left"),
        field_f64(block, "anchor_right"),
        parent_w,
    );
    apply_axis_anchor(
        &mut y,
        &mut h,
        field_f64(block, "anchor_top"),
        field_f64(block, "anchor_bottom"),
        parent_h,
    );
    (x, y, w, h)
}

/// Shift one axis so the declared anchor lands where it should.
pub(crate) fn apply_axis_anchor(
    pos: &mut f64,
    size: &mut f64,
    near: Option<f64>,
    far: Option<f64>,
    parent: f64,
) {
    match (near, far) {
        (Some(n), Some(f)) => {
            *pos = n;
            *size = parent - n - f;
        }
        (Some(n), None) => {
            *pos = n;
        }
        (None, Some(f)) => {
            *pos = parent - f - *size;
        }
        (None, None) => {}
    }
}

/// Resolve a circle's centre and radius.
pub(crate) fn resolve_circle(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64, f64) {
    // An unpositioned circle centers in its parent box, so it sits in
    // the middle of a layout cell (and fills it when its diameter was
    // used to size the cell). Explicit cx/cy still win.
    let cx = field_f64(block, "cx").unwrap_or(parent_w / 2.0);
    let cy = field_f64(block, "cy").unwrap_or(parent_h / 2.0);
    let r = field_f64(block, "r").unwrap_or(0.0);
    let al = field_f64(block, "anchor_left");
    let ar = field_f64(block, "anchor_right");
    let at = field_f64(block, "anchor_top");
    let ab = field_f64(block, "anchor_bottom");
    if al.is_none() && ar.is_none() && at.is_none() && ab.is_none() {
        return (cx, cy, r);
    }
    let mut bx = cx - r;
    let mut bw = 2.0 * r;
    let mut by = cy - r;
    let mut bh = 2.0 * r;
    apply_axis_anchor(&mut bx, &mut bw, al, ar, parent_w);
    apply_axis_anchor(&mut by, &mut bh, at, ab, parent_h);
    let new_r = (bw.min(bh) / 2.0).max(0.0);
    (bx + bw / 2.0, by + bh / 2.0, new_r)
}

/// Resolve a point-shaped element's position.
pub(crate) fn resolve_point_anchor(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64) {
    let dx = match (
        field_f64(block, "anchor_left"),
        field_f64(block, "anchor_right"),
    ) {
        (Some(l), _) => l,
        (None, Some(r)) => parent_w - r,
        _ => 0.0,
    };
    let dy = match (
        field_f64(block, "anchor_top"),
        field_f64(block, "anchor_bottom"),
    ) {
        (Some(t), _) => t,
        (None, Some(b)) => parent_h - b,
        _ => 0.0,
    };
    (dx, dy)
}

/// Resolve a point, applying the anchor on both axes.
pub(crate) fn resolve_point_anchored(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
    own_x: f64,
    own_y: f64,
) -> (f64, f64) {
    let x = match (
        field_f64(block, "anchor_left"),
        field_f64(block, "anchor_right"),
    ) {
        (Some(l), _) => l,
        (None, Some(r)) => parent_w - r,
        _ => own_x,
    };
    let y = match (
        field_f64(block, "anchor_top"),
        field_f64(block, "anchor_bottom"),
    ) {
        (Some(t), _) => t,
        (None, Some(b)) => parent_h - b,
        _ => own_y,
    };
    (x, y)
}
