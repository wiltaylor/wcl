//! Dopesheet support: the placeable `dopesheet` diagram block, an
//! animated window over a sprite sheet.
//!
//! A `dopesheet` names a sheet image plus its frame geometry (frame
//! size, the offset to the first frame, the origin-to-origin stride) and
//! a frame range + speed. Like `tilemap`, it crops a window out of an
//! external image — but instead of a static grid it emits a single
//! nested `<svg>` whose `viewBox` the bundled `dopesheet-player.js`
//! advances frame-by-frame at the authored fps. The sheet image is the
//! same kind of asset the `image` block handles, so the renderer reuses
//! the [`ImageRegistry`](crate::image::ImageRegistry) to copy it into
//! `_wdoc/` and read its pixel dimensions; frames therefore resolve when
//! the output is *served*, not via `file://`.

use std::fmt::Write as _;

use wcl_lang::Block;

use crate::image::ImageRegistry;
use crate::render::{
    escape_html, field_bool, field_f64, field_i64, field_id, field_utf8_list, label_string,
};

/// Resolved frame geometry + play range for one dopesheet.
struct Geom {
    /// Frames per row in the sprite sheet.
    columns: i64,
    /// X of the first frame within the sheet.
    offset_x: i64,
    /// Y of the first frame within the sheet.
    offset_y: i64,
    /// Horizontal distance between frames.
    stride_x: i64,
    /// Vertical distance between rows.
    stride_y: i64,
    /// First frame index of the range.
    from: i64,
    /// Last frame index of the range, inclusive.
    to: i64,
}

impl Geom {
    /// Read the geometry off the block, defaulting stride to the frame
    /// size, columns to as many whole frames as fit across the sheet, and
    /// the play range to the whole sheet. `img_w` / `img_h` are the sheet
    /// pixel dimensions (0 when unknown — an external / unreadable
    /// source, in which case an explicit `columns` is needed for a
    /// multi-frame animation).
    fn read(block: &Block<'_>, fw: i64, fh: i64, img_w: i64, img_h: i64) -> Geom {
        let offset_x = field_i64(block, "offset_x").unwrap_or(0).max(0);
        let offset_y = field_i64(block, "offset_y").unwrap_or(0).max(0);
        let stride_x = field_i64(block, "stride_x")
            .filter(|v| *v > 0)
            .unwrap_or(fw);
        let stride_y = field_i64(block, "stride_y")
            .filter(|v| *v > 0)
            .unwrap_or(fh);
        let columns = match field_i64(block, "columns") {
            Some(c) if c > 0 => c,
            _ => fit_count(img_w, offset_x, stride_x),
        };
        let rows = fit_count(img_h, offset_y, stride_y);
        let total = (columns * rows).max(1);
        let from = field_i64(block, "from").unwrap_or(0).clamp(0, total - 1);
        let to = match field_i64(block, "to") {
            Some(t) => t.clamp(from, total - 1),
            None => total - 1,
        };
        Geom {
            columns,
            offset_x,
            offset_y,
            stride_x,
            stride_y,
            from,
            to,
        }
    }
}

/// How many whole frames of `stride` fit along an axis of `extent`
/// pixels after `offset`. At least one (so a sheet whose dimensions are
/// unknown still yields a single frame).
fn fit_count(extent: i64, offset: i64, stride: i64) -> i64 {
    if stride <= 0 {
        return 1;
    }
    ((extent - offset) / stride).max(1)
}

/// The (column, row) of a flat frame index within the sheet grid.
fn frame_cell(idx: i64, columns: i64) -> (i64, i64) {
    let cols = columns.max(1);
    (idx % cols, idx / cols)
}

/// Render a `@block("dopesheet")` to a `<g class="wdoc-dopesheet">`
/// carrying the frame geometry as `data-dope-*` attributes plus a nested
/// `<svg>` windowing into the sheet at the first frame. The bundled
/// player reads the attributes and animates the inner `viewBox`. A miss
/// (no source, no frame size) renders nothing — best-effort, like a
/// failed shape lowering.
pub(crate) fn render_dopesheet(
    block: &Block<'_>,
    images: &ImageRegistry,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let Some(source) = label_string(block) else {
        return String::new();
    };
    if source.is_empty() {
        return String::new();
    }
    let fw = field_i64(block, "frame_width").unwrap_or(0);
    let fh = field_i64(block, "frame_height").unwrap_or(0);
    if fw <= 0 || fh <= 0 {
        return String::new();
    }

    // Register records the source for copying into `_wdoc/` and reads the
    // sheet's pixel dimensions (needed to size the inner `<image>` and to
    // default `columns` / `rows`).
    let entry = images.register(&source);
    let (img_w, img_h) = entry.dims.map_or((0, 0), |(w, h)| (w as i64, h as i64));
    let g = Geom::read(block, fw, fh, img_w, img_h);

    let scale = field_f64(block, "scale").unwrap_or(1.0);
    let dw = fw as f64 * scale;
    let dh = fh as f64 * scale;
    let (x, y) = crate::tileset::place(block, parent_w, parent_h, dw, dh);

    // Initial window: the `from` frame.
    let (c0, r0) = frame_cell(g.from, g.columns);
    let vx = g.offset_x + c0 * g.stride_x;
    let vy = g.offset_y + r0 * g.stride_y;

    let loop_ = field_bool(block, "loop").unwrap_or(true);
    let autoplay = field_bool(block, "autoplay").unwrap_or(true);
    let controls = field_bool(block, "controls").unwrap_or(true);
    let fps = field_f64(block, "fps").unwrap_or(12.0);

    let mut classes = vec!["wdoc-dopesheet".to_string()];
    if field_bool(block, "smooth") == Some(true) {
        classes.push("smooth".to_string());
    }
    classes.extend(field_utf8_list(block, "class"));
    let joined = classes
        .iter()
        .map(|s| escape_html(s))
        .collect::<Vec<_>>()
        .join(" ");
    let href = escape_html(&entry.url);

    let mut out = format!("<g class=\"{joined}\"");
    if let Some(id) = field_id(block, "id") {
        let _ = write!(out, " id=\"{}\"", escape_html(&id));
    }
    let _ = write!(
        out,
        " data-dope-cols=\"{cols}\" data-dope-fw=\"{fw}\" data-dope-fh=\"{fh}\" \
         data-dope-ox=\"{ox}\" data-dope-oy=\"{oy}\" data-dope-sx=\"{sx}\" data-dope-sy=\"{sy}\" \
         data-dope-from=\"{from}\" data-dope-to=\"{to}\" data-dope-fps=\"{fps}\" \
         data-dope-loop=\"{lp}\" data-dope-autoplay=\"{ap}\">",
        cols = g.columns,
        ox = g.offset_x,
        oy = g.offset_y,
        sx = g.stride_x,
        sy = g.stride_y,
        from = g.from,
        to = g.to,
        lp = i32::from(loop_),
        ap = i32::from(autoplay),
    );
    let _ = write!(
        out,
        "<svg class=\"dope-frame\" x=\"{x}\" y=\"{y}\" width=\"{dw}\" height=\"{dh}\" \
         viewBox=\"{vx} {vy} {fw} {fh}\" preserveAspectRatio=\"none\">\
         <image href=\"{href}\" x=\"0\" y=\"0\" width=\"{img_w}\" height=\"{img_h}\" \
         preserveAspectRatio=\"none\"/></svg>",
    );
    // Centred play/pause overlay glyph (hidden while playing by the
    // player). Sized off the smaller display dimension so it fits inside.
    if controls {
        let cx = x + dw / 2.0;
        let cy = y + dh / 2.0;
        let fsz = (dw.min(dh) * 0.5).clamp(9.0, 48.0);
        let _ = write!(
            out,
            "<text class=\"dope-btn\" x=\"{cx:.1}\" y=\"{cy:.1}\" text-anchor=\"middle\" \
             dominant-baseline=\"central\" font-size=\"{fsz:.1}\">▶</text>",
        );
    }
    out.push_str("</g>");
    out
}

/// The absolute-in-parent bounding box `(x, y, w, h)` of a dopesheet, for
/// the collect pass (edge routing + viewBox fit). A single frame's
/// display size — `frame_width`/`frame_height` × `scale` — positioned via
/// the shared anchor helper (mirrors `render_dopesheet`).
pub(crate) fn dopesheet_bbox(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
    let fw = field_i64(block, "frame_width").unwrap_or(0).max(0) as f64;
    let fh = field_i64(block, "frame_height").unwrap_or(0).max(0) as f64;
    let scale = field_f64(block, "scale").unwrap_or(1.0);
    let w = fw * scale;
    let h = fh * scale;
    let (x, y) = crate::tileset::place(block, parent_w, parent_h, w, h);
    (x, y, w, h)
}

/// Whether a block subtree contains a `dopesheet` (drives the per-site
/// player-asset write + per-page script injection).
pub(crate) fn uses_dopesheet(block: &Block<'_>) -> bool {
    crate::render::block_tree_any(block, &|b| b.kind() == "dopesheet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_whole_frames_along_an_axis() {
        // 72px sheet, 12px frames, no offset → 6 frames.
        assert_eq!(fit_count(72, 0, 12), 6);
        // With a 4px offset only 5 whole frames remain past it.
        assert_eq!(fit_count(72, 4, 12), 5);
        // A non-dividing stride truncates to whole frames.
        assert_eq!(fit_count(50, 0, 12), 4);
        // Unknown dimensions / bad stride still yield one frame.
        assert_eq!(fit_count(0, 0, 12), 1);
        assert_eq!(fit_count(72, 0, 0), 1);
    }

    #[test]
    fn flat_index_maps_to_grid_cell() {
        // Single-row strip: column advances, row stays 0.
        assert_eq!(frame_cell(0, 6), (0, 0));
        assert_eq!(frame_cell(5, 6), (5, 0));
        // Wraps onto the next row past the last column.
        assert_eq!(frame_cell(6, 6), (0, 1));
        assert_eq!(frame_cell(13, 5), (3, 2));
        // Zero columns is treated as one (no divide-by-zero).
        assert_eq!(frame_cell(3, 0), (0, 3));
    }
}
