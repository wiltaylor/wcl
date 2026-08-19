//! Paint pass: turn laid-out pages into PDF bytes with krilla.
//!
//! Builds one krilla page per [`LaidOutPage`], draws the running header and
//! footer ([`page`](super::page)), then draws each placed glyph. Glyphs are
//! drawn one cluster at a time — simple and correct (selectable, extractable
//! text); run-coalescing for smaller output is a later optimisation.

use std::collections::HashMap;

use krilla::Document;
use krilla::action::{Action, LinkAction};
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::color::rgb;
use krilla::destination::XyzDestination;
use krilla::error::KrillaResult;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla_svg::{SurfaceExt, SvgSettings};

use super::Geometry;
use super::ir::{FontFamily, TextStyle};
use super::layout::LaidOutPage;
use super::page;
use super::text::{FontBook, ShapedLine};

/// The chrome (header/footer) text style and size.
pub(crate) const CHROME_SIZE: f32 = 9.0;
/// Text style for page chrome — running heads and folios — set in
/// sans to distinguish it from body text.
pub(crate) const CHROME_STYLE: TextStyle = TextStyle {
    family: FontFamily::Sans,
    bold: false,
    italic: false,
};

/// Render all pages to PDF bytes. With `title_page`, an unnumbered cover page
/// (the title, centred) precedes the numbered content pages. An `outline`
/// (built from the site's `toc`) becomes the PDF's reader-sidebar bookmarks.
pub(crate) fn paint(
    pages: &[LaidOutPage],
    book: &mut FontBook,
    geom: &Geometry,
    title: &str,
    title_page: bool,
    dests: &HashMap<String, usize>,
    outline: Option<krilla::outline::Outline>,
) -> KrillaResult<Vec<u8>> {
    let total = pages.len();
    let mut doc = Document::new();
    if let Some(outline) = outline {
        doc.set_outline(outline);
    }

    if title_page {
        let line = book.shape_label(title, TextStyle::heading(), 30.0);
        let mut kpage = doc.start_page_with(
            PageSettings::from_wh(geom.width, geom.height).expect("valid page dimensions"),
        );
        let mut surface = kpage.surface();
        let x = geom.content_left() + (geom.content_width() - line_width(&line, 30.0)) / 2.0;
        let y = geom.height * 0.42;
        draw_line(&mut surface, &line, x, y, 30.0, (24, 24, 27));
        surface.finish();
        kpage.finish();
    }

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
        paint_content(&mut surface, page_content);
        surface.finish();

        // Link annotations live on the page, added once the surface is done.
        for link in &page_content.links {
            let Some(rect) = Rect::from_xywh(link.x, link.y, link.w, link.h) else {
                continue;
            };
            if let Some(target) = link_target(&link.href, dests) {
                kpage.add_annotation(Annotation::from(LinkAnnotation::new(rect, target)));
            }
        }

        kpage.finish();
    }

    doc.finish()
}

/// Paint one laid-out page's drawable content (everything but the chrome and
/// link annotations): rect fills, raster images, embedded SVG, glyphs, then card
/// overlays. Card overlays recurse under a `translate · scale` transform clipped
/// to the card box, so a card body's content lands scaled inside its box.
fn paint_content(surface: &mut Surface, page: &LaidOutPage) {
    for r in &page.rects {
        fill_rect(surface, r.x, r.y, r.w, r.h, r.color);
    }
    for img in &page.images {
        if let Some(size) = Size::from_wh(img.w, img.h) {
            surface.push_transform(&Transform::from_translate(img.x, img.y));
            surface.draw_image(img.image.clone(), size);
            surface.pop();
        }
    }
    for placed in &page.svgs {
        if let Some(size) = Size::from_wh(placed.w, placed.h) {
            surface.push_transform(&Transform::from_translate(placed.x, placed.y));
            surface.draw_svg(&placed.tree, size, SvgSettings::default());
            surface.pop();
        }
    }
    for g in &page.glyphs {
        draw_glyph(
            surface, &g.font, g.glyph_id, g.x, g.y, g.size, g.color, &g.cluster,
        );
    }
    for ov in &page.card_overlays {
        surface.push_transform(&Transform::from_translate(ov.x, ov.y));
        surface.push_transform(&Transform::from_scale(ov.scale, ov.scale));
        // Clip to the card box (card-local coords) so the body can't spill out.
        let clip = Rect::from_xywh(0.0, 0.0, ov.w, ov.h).and_then(|r| {
            let mut pb = PathBuilder::new();
            pb.push_rect(r);
            pb.finish()
        });
        if let Some(path) = &clip {
            surface.push_clip_path(path, &FillRule::NonZero);
        }
        paint_content(surface, &ov.content);
        if clip.is_some() {
            surface.pop();
        }
        surface.pop();
        surface.pop();
    }
}

/// Resolve a link href to a krilla [`Target`]: external URLs become URI
/// actions; internal `<page>.html` hrefs become destinations to that page (when
/// it is in this document); anything else is left unlinked.
fn link_target(href: &str, dests: &HashMap<String, usize>) -> Option<Target> {
    if href.contains("://") || href.starts_with("mailto:") || href.starts_with("tel:") {
        return Some(Target::Action(Action::Link(LinkAction::new(
            href.to_string(),
        ))));
    }
    let path = href.split('#').next().unwrap_or(href);
    let name = path.strip_suffix(".html")?;
    let &page_index = dests.get(name)?;
    Some(Target::Destination(
        XyzDestination::new(page_index, Point::from_xy(0.0, 0.0)).into(),
    ))
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

/// Fill an axis-aligned rectangle with a solid colour.
pub(crate) fn fill_rect(
    surface: &mut Surface,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: (u8, u8, u8),
) {
    let Some(rect) = Rect::from_xywh(x, y, w, h) else {
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
