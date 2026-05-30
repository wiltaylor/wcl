//! Paint pass: turn laid-out pages into PDF bytes with krilla.
//!
//! Builds one krilla page per [`LaidOutPage`], draws the running header and
//! footer ([`page`](super::page)), then draws each placed glyph. Glyphs are
//! drawn one cluster at a time — simple and correct (selectable, extractable
//! text); run-coalescing for smaller output is a later optimisation.

use krilla::Document;
use krilla::action::{Action, LinkAction};
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::color::rgb;
use krilla::error::KrillaResult;
use krilla::geom::{Point, Rect};
use krilla::page::PageSettings;
use krilla::paint::Fill;
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};

use super::Geometry;
use super::ir::{FontFamily, TextStyle};
use super::layout::LaidOutPage;
use super::page;
use super::text::{FontBook, ShapedLine};

/// The chrome (header/footer) text style and size.
pub(crate) const CHROME_SIZE: f32 = 9.0;
pub(crate) const CHROME_STYLE: TextStyle = TextStyle {
    family: FontFamily::Sans,
    bold: false,
    italic: false,
};

/// Render all pages to PDF bytes.
pub(crate) fn paint(
    pages: &[LaidOutPage],
    book: &mut FontBook,
    geom: &Geometry,
    title: &str,
) -> KrillaResult<Vec<u8>> {
    let total = pages.len();
    let mut doc = Document::new();

    for (i, page_content) in pages.iter().enumerate() {
        // Shape the chrome labels first — this borrows `book`, which must be
        // released before the surface borrows begin.
        let header = book.shape_label(title, CHROME_STYLE, CHROME_SIZE);
        let footer_line =
            book.shape_label(&format!("{} / {}", i + 1, total), CHROME_STYLE, CHROME_SIZE);

        let mut kpage = doc.start_page_with(
            PageSettings::from_wh(geom.width, geom.height).expect("valid page dimensions"),
        );
        let mut surface = kpage.surface();

        page::draw_chrome(&mut surface, geom, &header, &footer_line);

        for g in &page_content.glyphs {
            draw_glyph(
                &mut surface,
                &g.font,
                g.glyph_id,
                g.x,
                g.y,
                g.size,
                g.color,
                &g.cluster,
            );
        }

        surface.finish();

        // Link annotations live on the page, added once the surface is done.
        for link in &page_content.links {
            if let Some(rect) = Rect::from_xywh(link.x, link.y, link.w, link.h) {
                let target = Target::Action(Action::Link(LinkAction::new(link.href.clone())));
                kpage.add_annotation(Annotation::from(LinkAnnotation::new(rect, target)));
            }
        }

        kpage.finish();
    }

    doc.finish()
}

/// Draw a single glyph at an absolute baseline position.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_glyph(
    surface: &mut Surface,
    font: &Font,
    glyph_id: u16,
    x: f32,
    y: f32,
    size: f32,
    color: (u8, u8, u8),
    text: &str,
) {
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(color.0, color.1, color.2).into(),
        ..Fill::default()
    }));
    // Positioned absolutely, so the glyph carries no advance/offset of its own.
    let glyph = KrillaGlyph::new(
        GlyphId::new(u32::from(glyph_id)),
        0.0,
        0.0,
        0.0,
        0.0,
        0..text.len(),
        None,
    );
    surface.draw_glyphs(
        Point::from_xy(x, y),
        &[glyph],
        font.clone(),
        text,
        size,
        false,
    );
}

/// Draw a pre-shaped single line at `(x0, baseline)`.
pub(crate) fn draw_line(
    surface: &mut Surface,
    line: &ShapedLine,
    x0: f32,
    baseline: f32,
    size: f32,
    color: (u8, u8, u8),
) {
    for g in &line.glyphs {
        draw_glyph(
            surface,
            &g.font,
            g.glyph_id,
            x0 + g.x,
            baseline + g.dy,
            size,
            color,
            &g.cluster,
        );
    }
}

/// Approximate the painted width of a shaped line (for centring chrome).
pub(crate) fn line_width(line: &ShapedLine, size: f32) -> f32 {
    line.glyphs.last().map_or(0.0, |g| g.x) + size * 0.5
}
