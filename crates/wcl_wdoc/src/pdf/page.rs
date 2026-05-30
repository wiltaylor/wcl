//! Page chrome: the running header (document title) with a hairline rule, and
//! the footer page number. Drawn into the page margins, outside the content
//! box the layout pass fills.

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Rect};
use krilla::paint::Fill;
use krilla::surface::Surface;

use super::Geometry;
use super::paint::{CHROME_SIZE, draw_line, line_width};
use super::text::ShapedLine;

/// Muted colour for header/footer chrome and the hairline rule.
const CHROME_COLOR: (u8, u8, u8) = (140, 140, 150);

/// Draw the running header (title + rule) and the footer page number.
pub(crate) fn draw_chrome(
    surface: &mut Surface,
    geom: &Geometry,
    header: &ShapedLine,
    footer: &ShapedLine,
) {
    // Header title, left-aligned, sitting in the top margin above the content.
    let header_baseline = geom.margin_top - 28.0;
    draw_line(
        surface,
        header,
        geom.content_left(),
        header_baseline,
        CHROME_SIZE,
        CHROME_COLOR,
    );

    // Hairline rule under the header.
    draw_rule(
        surface,
        geom.content_left(),
        geom.margin_top - 18.0,
        geom.content_width(),
        0.6,
        CHROME_COLOR,
    );

    // Footer page number, centred in the bottom margin.
    let footer_baseline = geom.height - geom.margin_bottom + 36.0;
    let footer_x =
        geom.content_left() + (geom.content_width() - line_width(footer, CHROME_SIZE)) / 2.0;
    draw_line(
        surface,
        footer,
        footer_x,
        footer_baseline,
        CHROME_SIZE,
        CHROME_COLOR,
    );
}

fn draw_rule(surface: &mut Surface, x: f32, y: f32, w: f32, thickness: f32, color: (u8, u8, u8)) {
    let Some(rect) = Rect::from_xywh(x, y, w, thickness) else {
        return;
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(rect);
    let Some(path) = pb.finish() else {
        return;
    };
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(color.0, color.1, color.2).into(),
        ..Fill::default()
    }));
    surface.draw_path(&path);
}
