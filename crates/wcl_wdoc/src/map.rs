//! The `map` block: a zoomable, pinned game-guide map placed inside a
//! `diagram`.
//!
//! Like `tilemap` / `image`, a map is `@native` (see [`crate::native`]) —
//! the tile crops, the icon `<use>` markers, and the HTML cards overlaid on
//! the SVG aren't expressible in WCL. The map reuses existing machinery
//! rather than adding new fundamentals:
//!
//! - tile / image copying → the threaded [`ImageRegistry`] (every tile is
//!   just an image; `register()` returns a `_wdoc/…` URL + records it);
//! - pins → the icon `<use>` sprite system ([`IconRegistry::resolve_shape`]);
//! - card content → `render_block` over the pin's `@children(WdocBlock)`,
//!   pushed into the diagram's overlay sink so it lands as absolutely
//!   positioned HTML *outside* the `<svg>` (inside the viewport wrapper).
//!
//! A diagram containing a map is rendered interactive (pan + zoom); the
//! bundled `wdoc-map.js` selects the sharpest layer for the current zoom
//! and opens a pin's card as a popup anchored to the marker.

use std::fmt::Write as _;

use wcl_lang::Block;

use crate::icons::ShapeOverride;
use crate::render::{
    RenderCtx, escape_html, field_bool, field_f64, field_i64, field_id, field_utf8,
    field_utf8_list, label_string, render_block,
};

/// Default marker size, in map units, when a pin sets no `size`.
const DEFAULT_PIN_SIZE: f64 = 24.0;
/// Default tile pixel size for tiled layers when neither the layer nor the
/// map sets `tile_size`.
const DEFAULT_TILE: i64 = 256;

/// Render a `@block("map")` to a `<g class="wdoc-map">` of layer images +
/// pin markers, pushing each pin's card HTML into `ctx.overlays`. A map
/// with no `source` and no `layer`s renders just its pins.
pub(crate) fn render_map(
    block: &Block<'_>,
    ctx: RenderCtx<'_>,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let width = field_f64(block, "width").unwrap_or(0.0);
    let height = field_f64(block, "height").unwrap_or(0.0);
    let (ox, oy) = crate::tileset::place(block, parent_w, parent_h, width, height);
    let default_tile = field_i64(block, "tile_size").unwrap_or(DEFAULT_TILE).max(1);

    // Layers: explicit `layer` children, or a single implicit layer from
    // the map's own `source` (the common single-image case).
    let layer_blocks: Vec<Block<'_>> = block.blocks().filter(|b| b.kind() == "layer").collect();
    let mut body = String::new();
    if layer_blocks.is_empty() {
        if let Some(src) = field_utf8(block, "source") {
            body.push_str(&render_single_image(ctx, &src, ox, oy, width, height));
        }
    } else {
        for layer in &layer_blocks {
            body.push_str(&render_layer(
                layer,
                ctx,
                ox,
                oy,
                width,
                height,
                default_tile,
            ));
        }
    }

    // Pins: a clickable marker (icon `<use>` + transparent hit rect) plus a
    // hidden card pushed to the overlay sink.
    for pin in block.blocks().filter(|b| b.kind() == "pin") {
        body.push_str(&render_pin(&pin, ctx, ox, oy));
        if let Some(card) = render_pin_card(&pin, ctx) {
            ctx.overlays.borrow_mut().push(card);
        }
    }

    let mut classes = vec!["wdoc-map".to_string()];
    // `smooth` defaults to true (browser smoothing); an explicit
    // `smooth = false` opts into nearest-neighbour (pixel-art maps).
    if field_bool(block, "smooth") == Some(false) {
        classes.push("pixelated".to_string());
    }
    classes.extend(field_utf8_list(block, "class"));
    let joined = join_classes(&classes);
    let mut out =
        format!("<g class=\"{joined}\" data-map-width=\"{width}\" data-map-height=\"{height}\"");
    if let Some(id) = field_id(block, "id") {
        let _ = write!(out, " id=\"{}\"", escape_html(&id));
    }
    let _ = write!(out, ">{body}</g>");
    out
}

/// The absolute-in-parent bounding box `(x, y, w, h)` of a map, for the
/// collect pass (edges + viewBox fit). The box is the map's coordinate
/// space (`width` × `height`), positioned via the shared `place` helper.
pub(crate) fn map_bbox(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64, f64, f64) {
    let width = field_f64(block, "width").unwrap_or(0.0);
    let height = field_f64(block, "height").unwrap_or(0.0);
    let (x, y) = crate::tileset::place(block, parent_w, parent_h, width, height);
    (x, y, width, height)
}

/// One whole-map `<image>` wrapped in a `wdoc-map-layer` group. Its
/// `data-native-width` is the image's natural pixel width (for the JS
/// layer picker), falling back to the map's coordinate width.
fn render_single_image(
    ctx: RenderCtx<'_>,
    source: &str,
    ox: f64,
    oy: f64,
    width: f64,
    height: f64,
) -> String {
    let entry = ctx.images.register(source);
    let native = entry.dims.map_or(width, |(w, _)| w as f64);
    format!(
        "<g class=\"wdoc-map-layer\" data-native-width=\"{native}\">\
         <image href=\"{href}\" x=\"{ox}\" y=\"{oy}\" width=\"{width}\" height=\"{height}\" \
         preserveAspectRatio=\"none\" /></g>",
        href = escape_html(&entry.url),
    )
}

/// Render one `layer` child. A `cols`/`rows` of 1 (the default) is a
/// single whole-map image; otherwise the layer is a grid of tiles cropped
/// from a folder, each filename built from `pattern` (default
/// `{x}_{y}.png`, 0-based). Destination edges snap to the integer grid so
/// adjacent tiles share an exact edge (mirrors `render_tilemap`).
fn render_layer(
    layer: &Block<'_>,
    ctx: RenderCtx<'_>,
    ox: f64,
    oy: f64,
    width: f64,
    height: f64,
    default_tile: i64,
) -> String {
    let source = field_utf8(layer, "source").unwrap_or_default();
    let cols = field_i64(layer, "cols").unwrap_or(1).max(1);
    let rows = field_i64(layer, "rows").unwrap_or(1).max(1);

    if cols == 1 && rows == 1 {
        return render_single_image(ctx, &source, ox, oy, width, height);
    }

    let tile = field_i64(layer, "tile_size").unwrap_or(default_tile).max(1);
    let pattern = field_utf8(layer, "pattern").unwrap_or_else(|| "{x}_{y}.png".to_string());
    let folder = source.trim_end_matches('/');
    let (cw, ch) = (cols as f64, rows as f64);
    let edge_x = |c: i64| (ox + c as f64 * width / cw).round();
    let edge_y = |r: i64| (oy + r as f64 * height / ch).round();

    let mut inner = String::new();
    for y in 0..rows {
        for x in 0..cols {
            let file = pattern
                .replace("{x}", &x.to_string())
                .replace("{y}", &y.to_string());
            let src = format!("{folder}/{file}");
            let entry = ctx.images.register(&src);
            let dx = edge_x(x);
            let dy = edge_y(y);
            let w = edge_x(x + 1) - dx;
            let h = edge_y(y + 1) - dy;
            let _ = write!(
                inner,
                "<image href=\"{href}\" x=\"{dx}\" y=\"{dy}\" width=\"{w}\" height=\"{h}\" \
                 preserveAspectRatio=\"none\" />",
                href = escape_html(&entry.url),
            );
        }
    }
    let native = (cols * tile) as f64;
    format!("<g class=\"wdoc-map-layer\" data-native-width=\"{native}\">{inner}</g>")
}

/// A pin marker: the resolved icon `<use>` plus a transparent hit `<rect>`
/// (so thin stroke icons stay easy to click), wrapped in a
/// `wdoc-map-pin` group carrying `data-map-pin="{id}"`. The icon's bottom
/// centre sits at the pin's (x, y); styling reuses the icon override path
/// (`color` + `class`, exactly like a diagram `icon` block).
fn render_pin(pin: &Block<'_>, ctx: RenderCtx<'_>, ox: f64, oy: f64) -> String {
    // The pin id is its inline label (`pin "boss" { … }`).
    let id = label_string(pin).unwrap_or_default();
    let px = ox + field_f64(pin, "x").unwrap_or(0.0);
    let py = oy + field_f64(pin, "y").unwrap_or(0.0);
    let size = field_f64(pin, "size").unwrap_or(DEFAULT_PIN_SIZE);
    let name = field_utf8(pin, "icon").unwrap_or_else(|| "lucide.map-pin".to_string());
    let set = field_id(pin, "set");
    // Anchor the marker by its bottom-centre, like a real map pin.
    let ix = px - size / 2.0;
    let iy = py - size;
    let over = ShapeOverride {
        color: field_utf8(pin, "color"),
        classes: field_utf8_list(pin, "class"),
        ..ShapeOverride::default()
    };
    let icon = ctx
        .icons
        .resolve_shape(&name, set.as_deref(), (ix, iy, size, size), &over)
        .unwrap_or_default();
    format!(
        "<g class=\"wdoc-map-pin\" data-map-pin=\"{id}\">\
         <rect x=\"{ix}\" y=\"{iy}\" width=\"{size}\" height=\"{size}\" fill=\"transparent\" />\
         {icon}</g>",
        id = escape_html(&id),
    )
}

/// Render a pin's card to a hidden `wdoc-map-card` `<div>` (its content is
/// the pin's child `WdocBlock`s, rendered through the normal page path).
/// `None` when the pin has neither a `title` nor any content.
fn render_pin_card(pin: &Block<'_>, ctx: RenderCtx<'_>) -> Option<String> {
    let id = label_string(pin)?;
    let title = field_utf8(pin, "title");
    let children: Vec<Block<'_>> = pin.blocks().collect();
    if title.is_none() && children.is_empty() {
        return None;
    }
    let mut body = String::new();
    if let Some(t) = &title {
        let _ = write!(
            body,
            "<div class=\"wdoc-map-card-title\">{}</div>",
            escape_html(t)
        );
    }
    for child in &children {
        if let Some(html) = render_block(ctx.doc, child, ctx.patterns, ctx.base_dir) {
            body.push_str(&html);
        }
    }
    let mut classes = vec!["wdoc-map-card".to_string()];
    classes.extend(field_utf8_list(pin, "card_class"));
    let joined = join_classes(&classes);
    Some(format!(
        "<div class=\"{joined}\" data-map-card=\"{id}\" hidden>{body}\
         <button type=\"button\" class=\"wdoc-map-card-close\" aria-label=\"Close\">\u{2715}</button>\
         </div>",
        id = escape_html(&id),
    ))
}

fn join_classes(classes: &[String]) -> String {
    classes
        .iter()
        .map(|s| escape_html(s))
        .collect::<Vec<_>>()
        .join(" ")
}
