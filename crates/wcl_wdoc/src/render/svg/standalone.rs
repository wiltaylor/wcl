//! Standalone page-level SVG blocks (`sequence_diagram` /
//! `state_diagram`): a `WdocBlock` whose geometry lives in a WCL
//! `lower_svg` function returning `list<Svg>`. The renderer
//! calls the lowering, fits a viewBox over the returned fundamentals'
//! bounding boxes, and emits a self-contained `<svg>` whose height
//! follows the content's aspect ratio (the block declares only
//! `width`). All three backends dispatch here: HTML embeds the string,
//! PDF rasterises it through the shared SVG embedder, Markdown writes
//! it to a standalone `.svg` file.

use std::fmt::Write as _;

use wcl_lang::{Block, Document, Value, VariantPayload};

use crate::inline::InlinePatterns;
use crate::text;

use super::*;

pub(crate) fn render_lowered_svg_block(
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    patterns: &InlinePatterns,
) -> String {
    let Some(items) = lower_to_values_named(doc, block, kind, "lower_svg") else {
        return String::new();
    };
    fit_lowered_svg(
        doc,
        &items,
        SvgFrame {
            width: field_f64(block, "width"),
            height: None,
            class_attr: &class_attr(block),
            id: field_id(block, "id").as_deref(),
            desc: field_utf8(block, "desc").as_deref(),
        },
        patterns,
    )
}

/// The width a fitted `<svg>` takes when nothing declares one — the same
/// default `sequence_diagram` / `state_diagram` carry, so a page-level
/// drawing is the width of a diagram beside it.
pub(crate) const DEFAULT_SVG_WIDTH: f64 = 640.0;

/// The frame a fitted `<svg>` is drawn in: the declared width, an optional
/// explicit height (absent ⇒ the content's aspect ratio decides), and the
/// attributes the element carries.
pub(crate) struct SvgFrame<'a> {
    /// Absent ⇒ [`DEFAULT_SVG_WIDTH`].
    pub width: Option<f64>,
    pub height: Option<f64>,
    /// A pre-built ` class="…"` attribute; empty for none.
    pub class_attr: &'a str,
    pub id: Option<&'a str>,
    /// The accessible name (`role="img"` + `aria-label` + `<title>`).
    pub desc: Option<&'a str>,
}

/// Fit a self-contained `<svg>` around already-lowered shape fundamentals.
///
/// Shared by [`render_lowered_svg_block`] (which reads the geometry off a
/// block's `lower_svg`) and the content IR's `Drawing` node (which carries
/// the shape vocabulary directly), so a page-level drawing is fitted the
/// same way whichever side produced it. `height`, when given, overrides the
/// aspect-derived one.
pub(crate) fn fit_lowered_svg(
    doc: &Document,
    items: &[Value],
    frame: SvgFrame<'_>,
    patterns: &InlinePatterns,
) -> String {
    let SvgFrame {
        width,
        height,
        class_attr: cls,
        id,
        desc,
    } = frame;
    let width = width.unwrap_or(DEFAULT_SVG_WIDTH);
    let mut bboxes: Vec<(f64, f64, f64, f64)> = Vec::new();
    for v in items {
        collect_fundamental_bbox(v, &mut bboxes);
    }
    let body: String = items
        .iter()
        .map(|v| render_svg_variant(doc, v, 0.0, 0.0, 0, Some(patterns)))
        .collect();
    // Fit the viewBox to the content (same 10px pad as diagrams) and
    // derive the rendered height from its aspect ratio, so the block
    // never clips or letterboxes regardless of how tall the lowering's
    // content turned out.
    // An empty diagram (no participants / states yet) renders a short
    // strip rather than a full-width square of blank space.
    let (vb, vbw, vbh) = match content_viewbox(&bboxes) {
        Some(v) => v,
        None => (format!("0 0 {width} 40"), width, 40.0),
    };
    let height = height.unwrap_or(if vbw > 0.0 { width * vbh / vbw } else { width });
    let mut out = format!("<svg{cls}");
    append_attr(&mut out, "id", id);
    // Accessibility, mirroring `diagram`: `desc` becomes the accessible
    // name (`role="img"` + `aria-label`) plus a `<title>` first child.
    if let Some(d) = desc {
        write!(out, " role=\"img\" aria-label=\"{}\"", escape_html(d)).expect("write to String");
    }
    let title = desc
        .map(|d| format!("<title>{}</title>", escape_html(d)))
        .unwrap_or_default();
    write!(
        out,
        " xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height:.0}\" \
         viewBox=\"{vb}\">{title}{body}</svg>"
    )
    .expect("write to String");
    out
}

/// The padded union of the fundamentals' bboxes as a viewBox string
/// plus its dimensions. `None` when nothing carried geometry.
fn content_viewbox(bboxes: &[(f64, f64, f64, f64)]) -> Option<(String, f64, f64)> {
    const PAD: f64 = 10.0;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &(x, y, w, h) in bboxes {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    }
    if !min_x.is_finite() {
        return None;
    }
    let bx = min_x - PAD;
    let by = min_y - PAD;
    let bw = (max_x - min_x).max(1.0) + 2.0 * PAD;
    let bh = (max_y - min_y).max(1.0) + 2.0 * PAD;
    Some((format!("{bx} {by} {bw} {bh}"), bw, bh))
}

/// Accumulate the bounding box of one lowered fundamental. Labels are
/// estimated from their text metrics (centred at `x`/`y`); the 10px
/// viewBox pad absorbs the estimate's slack. `Link` recurses into its
/// children; unknown custom variants contribute nothing (their own
/// lowerings' output is not visible here — diagrams that need exact
/// fitting should emit fundamentals directly).
fn collect_fundamental_bbox(value: &Value, out: &mut Vec<(f64, f64, f64, f64)>) {
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return;
    };
    let VariantPayload::Record(map) = payload else {
        return;
    };
    match kind_for_variant(variant).as_str() {
        "rect" => out.push((
            map_f64(map, "x").unwrap_or(0.0),
            map_f64(map, "y").unwrap_or(0.0),
            map_f64(map, "width").unwrap_or(0.0),
            map_f64(map, "height").unwrap_or(0.0),
        )),
        "circle" => {
            let cx = map_f64(map, "cx").unwrap_or(0.0);
            let cy = map_f64(map, "cy").unwrap_or(0.0);
            let r = map_f64(map, "r").unwrap_or(0.0);
            out.push((cx - r, cy - r, 2.0 * r, 2.0 * r));
        }
        "line" => {
            let x1 = map_f64(map, "x1").unwrap_or(0.0);
            let y1 = map_f64(map, "y1").unwrap_or(0.0);
            let x2 = map_f64(map, "x2").unwrap_or(0.0);
            let y2 = map_f64(map, "y2").unwrap_or(0.0);
            out.push((x1.min(x2), y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs()));
        }
        "polygon" | "polyline" => {
            if let Some(b) = points_bbox(&map_utf8(map, "points").unwrap_or_default()) {
                out.push(b);
            }
        }
        "label" => {
            let content = map_utf8(map, "content").unwrap_or_default();
            let x = map_f64(map, "x").unwrap_or(0.0);
            let y = map_f64(map, "y").unwrap_or(0.0);
            let font_size = resolve_label_font_size(
                &content,
                map_f64(map, "font_size"),
                map_f64(map, "fit_width"),
                map_f64(map, "fit_height"),
            );
            let metrics = text::measure(&content);
            let longest = metrics
                .lines
                .iter()
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0) as f64;
            let w = longest * font_size * text::CHAR_RATIO;
            let h = metrics.lines.len() as f64 * font_size * text::LINE_HEIGHT;
            out.push((x - w / 2.0, y - h / 2.0, w, h));
        }
        "link" => {
            if let Some(Value::List(children)) = map.get("children") {
                for c in children.iter() {
                    collect_fundamental_bbox(c, out);
                }
            }
        }
        _ => {}
    }
}

/// Bbox of a `<polygon>`/`<polyline>`-style `points` string
/// (`"x,y x,y …"`). `None` when no parseable pair exists.
fn points_bbox(points: &str) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for pair in points.split_whitespace() {
        let mut it = pair.split(',');
        let (Some(xs), Some(ys)) = (it.next(), it.next()) else {
            continue;
        };
        let (Ok(x), Ok(y)) = (xs.trim().parse::<f64>(), ys.trim().parse::<f64>()) else {
            continue;
        };
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if !min_x.is_finite() {
        return None;
    }
    Some((min_x, min_y, max_x - min_x, max_y - min_y))
}
