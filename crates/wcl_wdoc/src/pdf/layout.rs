//! Block-flow layout and pagination.
//!
//! Shapes each [`BlockNode`](super::ir::BlockNode) with the [`FontBook`], then
//! greedily fills the page content box line by line, breaking to a new page
//! when a line would overflow. Each section (one source `page` block) starts on
//! a fresh physical page. Output is a list of [`LaidOutPage`]s holding glyphs in
//! absolute page coordinates plus the rectangles of any external hyperlinks,
//! ready for the paint pass.

use usvg::Tree;

use super::Geometry;
use super::ir::BlockNode;
use super::svg_embed::SvgEmbedder;
use super::text::{FontBook, ShapedGlyph};

const BODY_SIZE: f32 = 11.0;
const BODY_LINE_HEIGHT: f32 = 1.45;
const HEADING_LINE_HEIGHT: f32 = 1.25;
const SPACE_AFTER_PARAGRAPH: f32 = 8.0;
const SPACE_BEFORE_HEADING: f32 = 12.0;
const SPACE_AFTER_HEADING: f32 = 5.0;

/// Near-black body/heading text colour for this phase (palette theming arrives
/// with the SVG/colour work).
const TEXT_COLOR: (u8, u8, u8) = (24, 24, 27);
/// Hyperlink colour.
const LINK_COLOR: (u8, u8, u8) = (37, 99, 235);

/// A glyph placed in absolute page coordinates (origin top-left, y down).
pub(crate) struct PlacedGlyph {
    pub font: krilla::text::Font,
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: (u8, u8, u8),
    pub cluster: String,
}

/// A clickable hyperlink rectangle in absolute page coordinates.
pub(crate) struct LinkBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub href: String,
}

/// An embedded SVG placed in absolute page coordinates, sized to `(w, h)`.
pub(crate) struct PlacedSvg {
    pub tree: Tree,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// One physical page's painted content.
#[derive(Default)]
pub(crate) struct LaidOutPage {
    pub glyphs: Vec<PlacedGlyph>,
    pub links: Vec<LinkBox>,
    pub svgs: Vec<PlacedSvg>,
}

const SPACE_AROUND_SVG: f32 = 8.0;

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 22.0,
        2 => 18.0,
        3 => 15.0,
        4 => 13.0,
        5 => 12.0,
        _ => 11.0,
    }
}

/// Lay out every section into paginated pages.
pub(crate) fn layout(
    sections: &[Vec<BlockNode>],
    book: &mut FontBook,
    embedder: &SvgEmbedder,
    geom: &Geometry,
) -> Vec<LaidOutPage> {
    let content_w = geom.content_width();
    let content_h = geom.content_height();
    let left = geom.content_left();
    let top = geom.content_top();

    let mut pages: Vec<LaidOutPage> = vec![LaidOutPage::default()];
    let mut cy = 0.0_f32;
    let mut at_page_top = true;

    for (si, section) in sections.iter().enumerate() {
        if si > 0 {
            pages.push(LaidOutPage::default());
            cy = 0.0;
            at_page_top = true;
        }

        for block in section {
            if let BlockNode::Svg { svg } = block {
                place_svg(
                    svg,
                    embedder,
                    &mut pages,
                    &mut cy,
                    &mut at_page_top,
                    left,
                    top,
                    content_w,
                    content_h,
                );
                continue;
            }

            let (runs, size, line_height, space_before, space_after) = match block {
                BlockNode::Heading { level, runs } => (
                    runs,
                    heading_size(*level),
                    HEADING_LINE_HEIGHT,
                    SPACE_BEFORE_HEADING,
                    SPACE_AFTER_HEADING,
                ),
                BlockNode::Paragraph { runs } => (
                    runs,
                    BODY_SIZE,
                    BODY_LINE_HEIGHT,
                    0.0,
                    SPACE_AFTER_PARAGRAPH,
                ),
                BlockNode::Svg { .. } => unreachable!("handled above"),
            };

            if !at_page_top {
                cy += space_before;
            }

            let shaped = book.shape_paragraph(runs, content_w, size, line_height);
            for line in &shaped.lines {
                // Break before a line that would overflow — unless the page is
                // empty (a single oversized line overflows rather than loops).
                if cy + line.height > content_h && !at_page_top {
                    pages.push(LaidOutPage::default());
                    cy = 0.0;
                }
                let baseline = top + cy + line.ascent;
                let page = pages.last_mut().expect("at least one page");
                place_line(page, &line.glyphs, &shaped.hrefs, left, baseline, size);
                cy += line.height;
                at_page_top = false;
            }

            cy += space_after;
        }
    }

    pages
}

/// Embed, scale-to-fit, paginate, and centre one SVG block.
#[allow(clippy::too_many_arguments)]
fn place_svg(
    svg: &str,
    embedder: &SvgEmbedder,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    let Some((tree, (tw, th))) = embedder.embed(svg) else {
        return;
    };
    if tw <= 0.0 || th <= 0.0 {
        return;
    }
    // Fit the content width (never upscale), then a full page if still too tall.
    let mut scale = (content_w / tw).min(1.0);
    if th * scale > content_h {
        scale = content_h / th;
    }
    let dw = tw * scale;
    let dh = th * scale;

    if !*at_page_top {
        *cy += SPACE_AROUND_SVG;
    }
    if *cy + dh > content_h && !*at_page_top {
        pages.push(LaidOutPage::default());
        *cy = 0.0;
    }
    let page = pages.last_mut().expect("at least one page");
    page.svgs.push(PlacedSvg {
        tree,
        x: left + (content_w - dw) / 2.0,
        y: top + *cy,
        w: dw,
        h: dh,
    });
    *cy += dh + SPACE_AROUND_SVG;
    *at_page_top = false;
}

/// Place one line's glyphs, colour links, and accumulate link rectangles by
/// grouping runs of glyphs that share a link index.
fn place_line(
    page: &mut LaidOutPage,
    glyphs: &[ShapedGlyph],
    hrefs: &[String],
    left: f32,
    baseline: f32,
    size: f32,
) {
    // Group of consecutive same-link glyphs: (link_idx, min_x, max_x).
    let mut group: Option<(usize, f32, f32)> = None;
    let mut line_links: Vec<LinkBox> = Vec::new();

    for g in glyphs {
        let gx = left + g.x;
        let color = if g.link.is_some() {
            LINK_COLOR
        } else {
            TEXT_COLOR
        };
        page.glyphs.push(PlacedGlyph {
            font: g.font.clone(),
            glyph_id: g.glyph_id,
            x: gx,
            y: baseline + g.dy,
            size,
            color,
            cluster: g.cluster.clone(),
        });

        match g.link {
            Some(idx) => match &mut group {
                Some((gi, _min, max)) if *gi == idx => *max = gx,
                _ => {
                    flush_link(group.take(), hrefs, baseline, size, &mut line_links);
                    group = Some((idx, gx, gx));
                }
            },
            None => flush_link(group.take(), hrefs, baseline, size, &mut line_links),
        }
    }
    flush_link(group.take(), hrefs, baseline, size, &mut line_links);
    page.links.extend(line_links);
}

fn flush_link(
    group: Option<(usize, f32, f32)>,
    hrefs: &[String],
    baseline: f32,
    size: f32,
    out: &mut Vec<LinkBox>,
) {
    let Some((idx, min_x, max_x)) = group else {
        return;
    };
    let Some(href) = hrefs.get(idx) else {
        return;
    };
    // Only external links get an annotation now; internal page links need PDF
    // destinations, which arrive with book assembly.
    if !is_clickable(href) {
        return;
    }
    out.push(LinkBox {
        x: min_x,
        y: baseline - size * 0.82,
        w: (max_x - min_x) + size * 0.6,
        h: size * 1.05,
        href: href.clone(),
    });
}

fn is_clickable(href: &str) -> bool {
    href.contains("://") || href.starts_with("mailto:") || href.starts_with("tel:")
}
