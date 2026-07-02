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
use super::ir::{BlockNode, CardSpec, Cell, CodeSpan, ListLine, Row, TextStyle, TocLine};
use super::svg_embed::SvgEmbedder;
use super::text::{FontBook, InlineObject, ShapedGlyph, ShapedLine};

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

/// A filled rectangle (a code-block background) in absolute page coordinates.
pub(crate) struct RectFill {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: (u8, u8, u8),
}

/// A raster image placed in absolute page coordinates, sized to `(w, h)`.
pub(crate) struct PlacedImage {
    pub image: krilla::image::Image,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A card body laid out in card-local coordinates, to be painted (scaled +
/// translated, clipped) over its box inside a diagram.
pub(crate) struct CardOverlay {
    pub content: LaidOutPage,
    /// Card box top-left in absolute page coordinates.
    pub x: f32,
    pub y: f32,
    /// Diagram scale (viewBox units → page points).
    pub scale: f32,
    /// Card box size in viewBox units (the clip box, pre-scale).
    pub w: f32,
    pub h: f32,
}

/// One physical page's painted content. Rects paint first (backgrounds), then
/// images and SVGs, then glyphs, then card overlays on top.
#[derive(Default)]
pub(crate) struct LaidOutPage {
    pub glyphs: Vec<PlacedGlyph>,
    pub links: Vec<LinkBox>,
    pub svgs: Vec<PlacedSvg>,
    pub rects: Vec<RectFill>,
    pub images: Vec<PlacedImage>,
    pub card_overlays: Vec<CardOverlay>,
}

const SPACE_AROUND_SVG: f32 = 8.0;
const CODE_SIZE: f32 = 9.5;
const CODE_LINE_HEIGHT: f32 = 1.5;
const CODE_PAD: f32 = 9.0;
const CODE_BG: (u8, u8, u8) = (244, 244, 245);

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

/// Lay out every section into paginated pages. Returns the pages and, for each
/// section, the index of the physical page it starts on (for internal links).
pub(crate) fn layout(
    sections: &[Vec<BlockNode>],
    book: &mut FontBook,
    embedder: &SvgEmbedder,
    geom: &Geometry,
) -> (Vec<LaidOutPage>, Vec<usize>) {
    let content_w = geom.content_width();
    let content_h = geom.content_height();
    let left = geom.content_left();
    let top = geom.content_top();

    let mut pages: Vec<LaidOutPage> = vec![LaidOutPage::default()];
    let mut section_starts: Vec<usize> = Vec::with_capacity(sections.len());
    let mut cy = 0.0_f32;
    let mut at_page_top = true;

    for (si, section) in sections.iter().enumerate() {
        if si > 0 {
            pages.push(LaidOutPage::default());
            cy = 0.0;
            at_page_top = true;
        }
        section_starts.push(pages.len() - 1);

        place_blocks(
            section,
            book,
            embedder,
            &mut pages,
            &mut cy,
            &mut at_page_top,
            left,
            top,
            content_w,
            content_h,
        );
    }

    (pages, section_starts)
}

/// Place a run of blocks into `pages`, flowing + paginating from `cy`. Shared by
/// the top-level section loop and the card sub-layout (which calls it with a
/// card-local geometry and an unbounded height so it never paginates).
#[allow(clippy::too_many_arguments)]
fn place_blocks(
    blocks: &[BlockNode],
    book: &mut FontBook,
    embedder: &SvgEmbedder,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    for block in blocks {
        if let BlockNode::Svg { svg } = block {
            place_svg(
                svg,
                embedder,
                pages,
                cy,
                at_page_top,
                left,
                top,
                content_w,
                content_h,
            );
            continue;
        }
        if let BlockNode::Diagram {
            svg,
            viewbox,
            cards,
        } = block
        {
            place_diagram(
                svg,
                *viewbox,
                cards,
                book,
                embedder,
                pages,
                cy,
                at_page_top,
                left,
                top,
                content_w,
                content_h,
            );
            continue;
        }
        if let BlockNode::Code { lines } = block {
            place_code(
                lines,
                book,
                pages,
                cy,
                at_page_top,
                left,
                top,
                content_w,
                content_h,
            );
            continue;
        }
        if let BlockNode::Image {
            bytes,
            disp_w,
            disp_h,
        } = block
        {
            place_image(
                bytes,
                *disp_w,
                *disp_h,
                pages,
                cy,
                at_page_top,
                left,
                top,
                content_w,
                content_h,
            );
            continue;
        }
        if let BlockNode::List { lines } = block {
            place_list(
                lines,
                book,
                embedder,
                pages,
                cy,
                at_page_top,
                left,
                top,
                content_w,
                content_h,
            );
            continue;
        }
        if let BlockNode::Table { header, rows } = block {
            place_table(
                header,
                rows,
                book,
                embedder,
                pages,
                cy,
                at_page_top,
                left,
                top,
                content_w,
                content_h,
            );
            continue;
        }
        if let BlockNode::Toc { entries } = block {
            place_toc(
                entries,
                book,
                pages,
                cy,
                at_page_top,
                left,
                top,
                content_w,
                content_h,
            );
            continue;
        }
        if let BlockNode::Callout {
            accent,
            heading,
            body,
        } = block
        {
            place_callout(
                *accent,
                heading,
                body,
                book,
                embedder,
                pages,
                cy,
                at_page_top,
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
            BlockNode::Svg { .. }
            | BlockNode::Diagram { .. }
            | BlockNode::Code { .. }
            | BlockNode::List { .. }
            | BlockNode::Table { .. }
            | BlockNode::Toc { .. }
            | BlockNode::Callout { .. }
            | BlockNode::Image { .. } => unreachable!("handled above"),
        };

        if !*at_page_top {
            *cy += space_before;
        }

        let shaped = book.shape_paragraph(runs, content_w, size, line_height, embedder);
        let mut placed = vec![false; shaped.objects.len()];
        for line in &shaped.lines {
            // Break before a line that would overflow — unless the page is empty
            // (a single oversized line overflows rather than loops).
            if *cy + line.height > content_h && !*at_page_top {
                pages.push(LaidOutPage::default());
                *cy = 0.0;
            }
            let baseline = top + *cy + line.ascent;
            let page = pages.last_mut().expect("at least one page");
            place_line(
                page,
                &line.glyphs,
                &shaped.hrefs,
                &shaped.objects,
                &mut placed,
                left,
                baseline,
                size,
            );
            *cy += line.height;
            *at_page_top = false;
        }

        *cy += space_after;
    }
}

/// Place a diagram and overlay its cards: embed the SVG (which draws the card
/// boxes), map each card's viewBox rect to the SVG's PDF placement, and lay the
/// card body out natively in card-local coordinates for the paint pass to scale
/// + clip into the box.
#[allow(clippy::too_many_arguments)]
fn place_diagram(
    svg: &str,
    viewbox: (f32, f32, f32, f32),
    cards: &[CardSpec],
    book: &mut FontBook,
    embedder: &SvgEmbedder,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    let Some((px, py, scale)) = place_svg(
        svg,
        embedder,
        pages,
        cy,
        at_page_top,
        left,
        top,
        content_w,
        content_h,
    ) else {
        return;
    };
    const CARD_PAD: f32 = 6.0;
    let (min_x, min_y, _, _) = viewbox;
    for card in cards {
        let (cx, cyv, cw, ch) = card.rect;
        let card_x = px + (cx - min_x) * scale;
        let card_y = py + (cyv - min_y) * scale;
        // Lay the body out at full size into a card-local box; paint scales it.
        let mut sub = vec![LaidOutPage::default()];
        let mut scy = 0.0_f32;
        let mut stop = true;
        place_blocks(
            &card.body,
            book,
            embedder,
            &mut sub,
            &mut scy,
            &mut stop,
            CARD_PAD,
            CARD_PAD,
            (cw - 2.0 * CARD_PAD).max(1.0),
            f32::MAX,
        );
        let content = sub.into_iter().next().unwrap_or_default();
        pages
            .last_mut()
            .expect("at least one page")
            .card_overlays
            .push(CardOverlay {
                content,
                x: card_x,
                y: card_y,
                scale,
                w: cw,
                h: ch,
            });
    }
}

/// Embed, scale-to-fit, paginate, and centre one SVG block. Returns the
/// placement `(page_x, page_y, scale)` (top-left + viewBox→points scale) so a
/// caller can map the SVG's internal coordinates onto the page (card overlays).
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
) -> Option<(f32, f32, f32)> {
    let (tree, (tw, th)) = embedder.embed(svg)?;
    if tw <= 0.0 || th <= 0.0 {
        return None;
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
    let svg_x = left + (content_w - dw) / 2.0;
    let svg_y = top + *cy;
    let page = pages.last_mut().expect("at least one page");
    page.svgs.push(PlacedSvg {
        tree,
        x: svg_x,
        y: svg_y,
        w: dw,
        h: dh,
    });
    *cy += dh + SPACE_AROUND_SVG;
    *at_page_top = false;
    Some((svg_x, svg_y, scale))
}

/// Decode raster bytes into a krilla image by sniffing the magic bytes.
fn build_image(bytes: &[u8]) -> Option<krilla::image::Image> {
    use krilla::image::Image;
    let data = bytes.to_vec().into();
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Image::from_png(data, false).ok()
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Image::from_jpeg(data, false).ok()
    } else if bytes.starts_with(b"GIF8") {
        Image::from_gif(data, false).ok()
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Image::from_webp(data, false).ok()
    } else {
        None
    }
}

/// Embed, size, paginate, and centre a raster image. Natural size is the pixel
/// size at 96 dpi unless an explicit display width/height is given.
#[allow(clippy::too_many_arguments)]
fn place_image(
    bytes: &[u8],
    disp_w: Option<f32>,
    disp_h: Option<f32>,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    let Some(image) = build_image(bytes) else {
        return;
    };
    let (pw, ph) = image.size();
    if pw == 0 || ph == 0 {
        return;
    }
    let aspect = ph as f32 / pw as f32;
    // px → pt at 96 dpi.
    let natural_w = pw as f32 * 0.75;
    let mut w = disp_w.unwrap_or(natural_w).min(content_w);
    let mut h = disp_h.unwrap_or(w * aspect);
    if h > content_h {
        let s = content_h / h;
        w *= s;
        h *= s;
    }

    if !*at_page_top {
        *cy += SPACE_AROUND_SVG;
    }
    if *cy + h > content_h && !*at_page_top {
        pages.push(LaidOutPage::default());
        *cy = 0.0;
    }
    let page = pages.last_mut().expect("at least one page");
    page.images.push(PlacedImage {
        image,
        x: left + (content_w - w) / 2.0,
        y: top + *cy,
        w,
        h,
    });
    *cy += h + SPACE_AROUND_SVG;
    *at_page_top = false;
}

/// Place a code block: an inset background box with syntax-coloured monospace
/// text, splitting across pages at line boundaries (a fresh box per page).
#[allow(clippy::too_many_arguments)]
fn place_code(
    lines: &[Vec<CodeSpan>],
    book: &mut FontBook,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    let size = CODE_SIZE;
    let lh = size * CODE_LINE_HEIGHT;
    let pad = CODE_PAD;

    if !*at_page_top {
        *cy += SPACE_AROUND_SVG;
    }

    let mut i = 0;
    while i < lines.len() {
        if *cy + pad + lh > content_h && !*at_page_top {
            pages.push(LaidOutPage::default());
            *cy = 0.0;
            *at_page_top = true;
        }
        let seg_page = pages.len() - 1;
        let seg_top = top + *cy;
        *cy += pad;
        // Always take at least one line at the top of a page — a single line
        // taller than the page overflows rather than loops (mirrors the
        // paragraph break rule; without this, a content box shorter than one
        // padded code line would push blank pages forever).
        while i < lines.len() && (*at_page_top || *cy + lh + pad <= content_h) {
            let page = pages.last_mut().expect("at least one page");
            draw_code_line(page, book, &lines[i], left + pad, top + *cy, size);
            *cy += lh;
            *at_page_top = false;
            i += 1;
        }
        *cy += pad;
        let seg_h = (top + *cy) - seg_top;
        pages[seg_page].rects.push(RectFill {
            x: left,
            y: seg_top,
            w: content_w,
            h: seg_h,
            color: CODE_BG,
        });
        if i < lines.len() {
            pages.push(LaidOutPage::default());
            *cy = 0.0;
            *at_page_top = true;
        }
    }
    *cy += SPACE_AROUND_SVG;
}

/// Draw one code line's coloured spans left-to-right in monospace.
fn draw_code_line(
    page: &mut LaidOutPage,
    book: &mut FontBook,
    spans: &[CodeSpan],
    x0: f32,
    line_top: f32,
    size: f32,
) {
    let baseline = line_top + size * 0.82;
    let mut pen = x0;
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        let shaped = book.shape_label(&span.text, TextStyle::code(), size);
        for g in &shaped.glyphs {
            page.glyphs.push(PlacedGlyph {
                font: g.font.clone(),
                glyph_id: g.glyph_id,
                x: pen + g.x,
                y: baseline + g.dy,
                size,
                color: span.color,
                cluster: g.cluster.clone(),
            });
        }
        pen += shaped.width;
    }
}

/// Place a flattened list: each line indented by its depth, with a marker on
/// the first wrapped line and inline content (links included) following.
#[allow(clippy::too_many_arguments)]
fn place_list(
    lines: &[ListLine],
    book: &mut FontBook,
    embedder: &SvgEmbedder,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    const INDENT: f32 = 16.0;
    const MARKER_GAP: f32 = 18.0;
    const ITEM_SPACE: f32 = 2.0;

    if !*at_page_top {
        *cy += SPACE_AROUND_SVG;
    }

    for line in lines {
        let indent = f32::from(line.depth) * INDENT;
        let marker = book.shape_label(&line.marker, TextStyle::body(), BODY_SIZE);
        // Reserve at least MARKER_GAP, but more when the marker (e.g. "2.1.")
        // is wider, so the text never collides with it.
        let gap = (marker.width + 5.0).max(MARKER_GAP);
        let text_x = left + indent + gap;
        let text_w = (content_w - indent - gap).max(40.0);
        let shaped =
            book.shape_paragraph(&line.runs, text_w, BODY_SIZE, BODY_LINE_HEIGHT, embedder);
        let mut placed = vec![false; shaped.objects.len()];

        for (li, wl) in shaped.lines.iter().enumerate() {
            if *cy + wl.height > content_h && !*at_page_top {
                pages.push(LaidOutPage::default());
                *cy = 0.0;
            }
            let baseline = top + *cy + wl.ascent;
            let page = pages.last_mut().expect("at least one page");
            if li == 0 {
                for g in &marker.glyphs {
                    page.glyphs.push(PlacedGlyph {
                        font: g.font.clone(),
                        glyph_id: g.glyph_id,
                        x: left + indent + g.x,
                        y: baseline + g.dy,
                        size: BODY_SIZE,
                        color: TEXT_COLOR,
                        cluster: g.cluster.clone(),
                    });
                }
            }
            place_line(
                page,
                &wl.glyphs,
                &shaped.hrefs,
                &shaped.objects,
                &mut placed,
                text_x,
                baseline,
                BODY_SIZE,
            );
            *cy += wl.height;
            *at_page_top = false;
        }
        *cy += ITEM_SPACE;
    }
    *cy += SPACE_AROUND_SVG;
}

/// Place a callout: a tinted box with a coloured left border, a bold
/// accent-coloured heading, and body text. Rendered as a single unit (taller
/// than a page overflows rather than splitting).
#[allow(clippy::too_many_arguments)]
fn place_callout(
    accent: (u8, u8, u8),
    heading: &[crate::pdf::ir::InlineRun],
    body: &[crate::pdf::ir::InlineRun],
    book: &mut FontBook,
    embedder: &SvgEmbedder,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    const BORDER_W: f32 = 4.0;
    const PAD: f32 = 8.0;
    const GAP: f32 = 3.0;
    const BG: (u8, u8, u8) = (247, 247, 248);

    if !*at_page_top {
        *cy += SPACE_AROUND_SVG;
    }
    let inner_x = left + BORDER_W + PAD;
    let inner_w = (content_w - BORDER_W - 2.0 * PAD).max(40.0);
    let head = book.shape_paragraph(heading, inner_w, BODY_SIZE, BODY_LINE_HEIGHT, embedder);
    let bod = book.shape_paragraph(body, inner_w, BODY_SIZE, BODY_LINE_HEIGHT, embedder);
    let mut placed = vec![false; bod.objects.len()];
    let head_h: f32 = head.lines.iter().map(|l| l.height).sum();
    let bod_h: f32 = bod.lines.iter().map(|l| l.height).sum();
    let gap = if head_h > 0.0 && bod_h > 0.0 {
        GAP
    } else {
        0.0
    };
    let box_h = PAD + head_h + gap + bod_h + PAD;

    if *cy + box_h > content_h && !*at_page_top {
        pages.push(LaidOutPage::default());
        *cy = 0.0;
    }
    let box_top = top + *cy;
    let page = pages.last_mut().expect("at least one page");
    page.rects.push(RectFill {
        x: left,
        y: box_top,
        w: content_w,
        h: box_h,
        color: BG,
    });
    page.rects.push(RectFill {
        x: left,
        y: box_top,
        w: BORDER_W,
        h: box_h,
        color: accent,
    });

    let mut yy = PAD;
    for wl in &head.lines {
        let baseline = box_top + yy + wl.ascent;
        for g in &wl.glyphs {
            page.glyphs.push(PlacedGlyph {
                font: g.font.clone(),
                glyph_id: g.glyph_id,
                x: inner_x + g.x,
                y: baseline + g.dy,
                size: BODY_SIZE,
                color: accent,
                cluster: g.cluster.clone(),
            });
        }
        yy += wl.height;
    }
    yy += gap;
    for wl in &bod.lines {
        let baseline = box_top + yy + wl.ascent;
        place_line(
            page,
            &wl.glyphs,
            &bod.hrefs,
            &bod.objects,
            &mut placed,
            inner_x,
            baseline,
            BODY_SIZE,
        );
        yy += wl.height;
    }

    *cy += box_h + SPACE_AROUND_SVG;
    *at_page_top = false;
}

const TABLE_SIZE: f32 = 10.0;
const TABLE_LINE_HEIGHT: f32 = 1.3;
const TABLE_PAD: f32 = 5.0;
const TABLE_BORDER: (u8, u8, u8) = (205, 205, 210);
const TABLE_HEADER_BG: (u8, u8, u8) = (238, 238, 241);

/// Place a table: equal-width columns, an optional shaded header row, and a
/// hairline rule under each row. Rows paginate (a row that won't fit starts a
/// new page; the header is not repeated).
#[allow(clippy::too_many_arguments)]
fn place_table(
    header: &[Cell],
    rows: &[Row],
    book: &mut FontBook,
    embedder: &SvgEmbedder,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    let cols = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1);
    let col_w = content_w / cols as f32;

    if !*at_page_top {
        *cy += SPACE_AROUND_SVG;
    }
    if !header.is_empty() {
        draw_table_row(
            header,
            true,
            cols,
            col_w,
            book,
            embedder,
            pages,
            cy,
            at_page_top,
            left,
            top,
            content_w,
            content_h,
        );
    }
    for row in rows {
        draw_table_row(
            row,
            false,
            cols,
            col_w,
            book,
            embedder,
            pages,
            cy,
            at_page_top,
            left,
            top,
            content_w,
            content_h,
        );
    }
    *cy += SPACE_AROUND_SVG;
}

#[allow(clippy::too_many_arguments)]
fn draw_table_row(
    cells: &[Cell],
    is_header: bool,
    cols: usize,
    col_w: f32,
    book: &mut FontBook,
    embedder: &SvgEmbedder,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    let empty: Cell = Vec::new();
    let mut paras = Vec::with_capacity(cols);
    let mut tallest = 0.0_f32;
    for c in 0..cols {
        let runs = cells.get(c).unwrap_or(&empty);
        let para = book.shape_paragraph(
            runs,
            col_w - 2.0 * TABLE_PAD,
            TABLE_SIZE,
            TABLE_LINE_HEIGHT,
            embedder,
        );
        let h: f32 = para.lines.iter().map(|l| l.height).sum();
        tallest = tallest.max(h);
        paras.push(para);
    }
    let row_h = tallest + 2.0 * TABLE_PAD;

    if *cy + row_h > content_h && !*at_page_top {
        pages.push(LaidOutPage::default());
        *cy = 0.0;
    }
    let row_top = top + *cy;
    let page = pages.last_mut().expect("at least one page");
    if is_header {
        page.rects.push(RectFill {
            x: left,
            y: row_top,
            w: content_w,
            h: row_h,
            color: TABLE_HEADER_BG,
        });
    }
    for (c, para) in paras.iter().enumerate() {
        let cx = left + c as f32 * col_w + TABLE_PAD;
        let mut yy = TABLE_PAD;
        let mut placed = vec![false; para.objects.len()];
        for wl in &para.lines {
            let baseline = row_top + yy + wl.ascent;
            place_line(
                page,
                &wl.glyphs,
                &para.hrefs,
                &para.objects,
                &mut placed,
                cx,
                baseline,
                TABLE_SIZE,
            );
            yy += wl.height;
        }
    }
    page.rects.push(RectFill {
        x: left,
        y: row_top + row_h - 0.6,
        w: content_w,
        h: 0.6,
        color: TABLE_BORDER,
    });
    *cy += row_h;
    *at_page_top = false;
}

/// Muted leader-dot colour for the contents page.
const TOC_DOT_COLOR: (u8, u8, u8) = (140, 140, 150);

/// Place a printed table-of-contents: one line per entry — an indented title,
/// right-aligned page number, and leader dots filling the gap — with the whole
/// row a clickable jump to the named page. Entries are single-line; the list
/// paginates per row.
#[allow(clippy::too_many_arguments)]
fn place_toc(
    entries: &[TocLine],
    book: &mut FontBook,
    pages: &mut Vec<LaidOutPage>,
    cy: &mut f32,
    at_page_top: &mut bool,
    left: f32,
    top: f32,
    content_w: f32,
    content_h: f32,
) {
    const INDENT: f32 = 16.0;
    const GAP: f32 = 4.0;
    const ENTRY_SPACE: f32 = 4.0;
    let lh = BODY_SIZE * BODY_LINE_HEIGHT;
    let dot_w = book
        .shape_label(".", TextStyle::body(), BODY_SIZE)
        .width
        .max(1.0);

    if !*at_page_top {
        *cy += SPACE_AROUND_SVG;
    }

    for entry in entries {
        if *cy + lh > content_h && !*at_page_top {
            pages.push(LaidOutPage::default());
            *cy = 0.0;
        }
        let indent = f32::from(entry.depth) * INDENT;
        // Chapters (depth 0) are bold; nested entries regular — echoes the
        // weight the HTML book sidebar gives top-level chapters.
        let title_style = TextStyle {
            bold: entry.depth == 0,
            ..TextStyle::body()
        };
        let title = book.shape_label(&entry.title, title_style, BODY_SIZE);
        let number = book.shape_label(&entry.number, TextStyle::body(), BODY_SIZE);

        let baseline = top + *cy + title.ascent;
        let title_x = left + indent;
        let number_x = left + content_w - number.width;

        // Shape the leader dots (right-aligned against the number) before
        // borrowing the page, so the page borrow doesn't overlap `book`.
        let dots = if entry.number.is_empty() {
            None
        } else {
            let dots_start = title_x + title.width + GAP;
            let dots_end = number_x - GAP;
            let count = if dots_end > dots_start {
                ((dots_end - dots_start) / dot_w).floor() as i32
            } else {
                0
            };
            (count > 0).then(|| {
                let shaped =
                    book.shape_label(&".".repeat(count as usize), TextStyle::body(), BODY_SIZE);
                let x = dots_end - shaped.width;
                (shaped, x)
            })
        };

        let page = pages.last_mut().expect("at least one page");
        let push_run = |page: &mut LaidOutPage, line: &ShapedLine, x0: f32, color: (u8, u8, u8)| {
            for g in &line.glyphs {
                page.glyphs.push(PlacedGlyph {
                    font: g.font.clone(),
                    glyph_id: g.glyph_id,
                    x: x0 + g.x,
                    y: baseline + g.dy,
                    size: BODY_SIZE,
                    color,
                    cluster: g.cluster.clone(),
                });
            }
        };
        push_run(page, &title, title_x, TEXT_COLOR);
        if !entry.number.is_empty() {
            push_run(page, &number, number_x, TEXT_COLOR);
            if let Some((shaped, x)) = &dots {
                push_run(page, shaped, *x, TOC_DOT_COLOR);
            }
        }
        // The whole row jumps to the page (internal `<page>.html` → destination).
        if let Some(name) = &entry.page {
            page.links.push(LinkBox {
                x: title_x,
                y: baseline - title.ascent,
                w: content_w - indent,
                h: lh,
                href: format!("{name}.html"),
            });
        }
        *cy += lh + ENTRY_SPACE;
        *at_page_top = false;
    }
    *cy += SPACE_AROUND_SVG;
}

/// Place one line's glyphs, colour links, overlay inline objects, and
/// accumulate link rectangles by grouping runs of glyphs that share a link
/// index. `placed` (one bool per object) dedupes objects across wrapped lines.
#[allow(clippy::too_many_arguments)]
fn place_line(
    page: &mut LaidOutPage,
    glyphs: &[ShapedGlyph],
    hrefs: &[String],
    objects: &[InlineObject],
    placed: &mut [bool],
    left: f32,
    baseline: f32,
    size: f32,
) {
    // Group of consecutive same-link glyphs: (link_idx, min_x, max_x).
    let mut group: Option<(usize, f32, f32)> = None;
    let mut line_links: Vec<LinkBox> = Vec::new();

    for g in glyphs {
        let gx = left + g.x;
        // An inline object: overlay its SVG once (at the first placeholder
        // glyph), and skip drawing the placeholder space.
        if let Some(oi) = g.obj {
            if let (Some(slot), Some(obj)) = (placed.get_mut(oi), objects.get(oi))
                && !*slot
            {
                *slot = true;
                page.svgs.push(PlacedSvg {
                    tree: obj.tree.clone(),
                    x: gx,
                    y: baseline - obj.h + size * 0.12,
                    w: obj.w,
                    h: obj.h,
                });
            }
            continue;
        }
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
    // Keep every link box; the paint pass turns external hrefs into URI actions
    // and internal `<page>.html` hrefs into destinations (dropping the rest).
    out.push(LinkBox {
        x: min_x,
        y: baseline - size * 0.82,
        w: (max_x - min_x) + size * 0.6,
        h: size * 1.05,
        href: href.clone(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::IconRegistry;
    use crate::image::ImageRegistry;
    use crate::pdf::ir::InlineRun;
    use crate::pdf::palette::Palette;
    use crate::tileset::TilesetRegistry;

    /// Build a shaper + an embedder over empty registries (mirrors
    /// `svg_embed::tests::with_embedder`) so tests can drive the solver with
    /// hand-built IR and no real document.
    fn with_ctx(f: impl FnOnce(&mut FontBook, &SvgEmbedder)) {
        let doc = wcl_lang::Document::open("", "test.wcl").expect("empty doc parses");
        let icons = IconRegistry::load(&doc);
        let images = ImageRegistry::new(None);
        let Ok(tilesets) = TilesetRegistry::load(&doc, None) else {
            panic!("empty doc declares no tilesets");
        };
        let palette = Palette::default();
        let embedder = SvgEmbedder::new(
            &palette,
            "",
            &icons,
            &images,
            &tilesets,
            "#ffffff".to_string(),
            "#cccccc".to_string(),
        );
        let mut book = FontBook::new();
        f(&mut book, &embedder);
    }

    /// A geometry whose content box is exactly `w` × `h` — production geometry
    /// only comes from `PageSize`, so tests shrink the box to force pagination
    /// without shaping thousands of lines.
    fn geo(w: f32, h: f32) -> Geometry {
        Geometry {
            width: w + 20.0,
            height: h + 20.0,
            margin_x: 10.0,
            margin_top: 10.0,
            margin_bottom: 10.0,
        }
    }

    fn text_run(text: &str) -> InlineRun {
        InlineRun::Text {
            text: text.to_string(),
            style: TextStyle::body(),
        }
    }

    fn para(text: &str) -> BlockNode {
        BlockNode::Paragraph {
            runs: vec![text_run(text)],
        }
    }

    fn code_lines(n: usize) -> BlockNode {
        BlockNode::Code {
            lines: (0..n)
                .map(|i| {
                    vec![CodeSpan {
                        text: format!("line {i}"),
                        color: (0, 0, 0),
                    }]
                })
                .collect(),
        }
    }

    /// Breaks happen *before* placement, so no baseline may land outside the
    /// content box (an at-page-top oversized item is the documented exception —
    /// callers of this helper only feed content that fits line by line).
    fn assert_glyphs_within(pages: &[LaidOutPage], geo: &Geometry) {
        let top = geo.content_top() - 0.01;
        let bottom = geo.content_top() + geo.content_height() + 0.01;
        for (pi, page) in pages.iter().enumerate() {
            for glyph in &page.glyphs {
                assert!(
                    glyph.y >= top && glyph.y <= bottom,
                    "glyph at y {} escapes the content box on page {pi}",
                    glyph.y
                );
            }
        }
    }

    #[test]
    fn no_sections_still_yield_one_page() {
        with_ctx(|book, embedder| {
            let geo = geo(200.0, 200.0);
            // The paint pass indexes `pages.last_mut()` unconditionally, so
            // even an empty layout must produce one (blank) page.
            let (pages, starts) = layout(&[], book, embedder, &geo);
            assert_eq!(pages.len(), 1);
            assert!(starts.is_empty());
            // A section with no blocks is a legal blank page, not a crash.
            let (pages, starts) = layout(&[vec![]], book, embedder, &geo);
            assert_eq!(pages.len(), 1);
            assert_eq!(starts, vec![0]);
        });
    }

    #[test]
    fn each_section_starts_on_a_fresh_page() {
        with_ctx(|book, embedder| {
            let geo = geo(300.0, 400.0);
            let sections = [vec![para("one")], vec![para("two")], vec![para("three")]];
            let (pages, starts) = layout(&sections, book, embedder, &geo);
            // One source `page` block = one fresh physical page, and the
            // recorded starts are what internal links jump to.
            assert_eq!(pages.len(), 3);
            assert_eq!(starts, vec![0, 1, 2]);
            assert!(pages.iter().all(|p| !p.glyphs.is_empty()));
        });
    }

    #[test]
    fn long_paragraph_paginates_inside_the_content_box() {
        with_ctx(|book, embedder| {
            let geo = geo(200.0, 100.0);
            let text = "lorem ipsum dolor ".repeat(200);
            let (pages, starts) = layout(&[vec![para(&text)]], book, embedder, &geo);
            assert_eq!(starts, vec![0]);
            assert!(pages.len() > 1, "content must spill past one page");
            assert_glyphs_within(&pages, &geo);
            // A break never strands a blank page behind it.
            assert!(pages.iter().all(|p| !p.glyphs.is_empty()));
        });
    }

    #[test]
    fn overwide_word_and_empty_runs_do_not_panic() {
        with_ctx(|book, embedder| {
            let geo = geo(40.0, 120.0);
            let blocks = vec![
                // One unbreakable "word" much wider than the column: policy is
                // overflow (or forced glyph wrap), never a hang or a panic.
                para(&"x".repeat(300)),
                para(""),
                BlockNode::Paragraph { runs: vec![] },
                para("   "),
            ];
            let (pages, _) = layout(&[blocks], book, embedder, &geo);
            assert!(!pages.is_empty());
            let glyphs: usize = pages.iter().map(|p| p.glyphs.len()).sum();
            assert!(glyphs >= 300, "the oversized word still gets painted");
        });
    }

    #[test]
    fn degenerate_negative_geometry_terminates() {
        // A content box with negative width and height: every line overflows,
        // so each takes its own page — page count stays proportional to the
        // content, never an unbounded break loop. (cosmic-text clamps negative
        // wrap widths to zero, so shaping is safe too.)
        with_ctx(|book, embedder| {
            let geo = Geometry {
                width: 5.0,
                height: 5.0,
                margin_x: 10.0,
                margin_top: 10.0,
                margin_bottom: 10.0,
            };
            let blocks = vec![
                para("a few words of prose"),
                code_lines(3),
                BlockNode::List {
                    lines: vec![ListLine {
                        depth: 0,
                        marker: "•".to_string(),
                        runs: vec![text_run("item")],
                    }],
                },
            ];
            let (pages, _) = layout(&[blocks], book, embedder, &geo);
            assert!(pages.len() < 64, "unbounded page growth: {}", pages.len());
        });
    }

    #[test]
    fn oversized_code_line_at_page_top_overflows_rather_than_loops() {
        // A content box shorter than one padded code line: without the
        // at-page-top escape in `place_code` the segment loop would place
        // nothing and push blank pages forever.
        with_ctx(|book, embedder| {
            let geo = geo(200.0, 10.0);
            let (pages, _) = layout(&[vec![code_lines(3)]], book, embedder, &geo);
            assert_eq!(pages.len(), 3, "exactly one (overflowing) line per page");
            for page in &pages {
                assert!(!page.glyphs.is_empty(), "every page carries its line");
                assert!(page.rects.iter().any(|r| r.color == CODE_BG));
            }
        });
    }

    #[test]
    fn code_block_splits_with_a_background_box_per_page() {
        with_ctx(|book, embedder| {
            // Fits two padded code lines per page; five lines force splits.
            let geo = geo(300.0, 50.0);
            let (pages, _) = layout(&[vec![code_lines(5)]], book, embedder, &geo);
            assert!(
                pages.len() >= 2 && pages.len() <= 5,
                "expected a multi-page split, got {} pages",
                pages.len()
            );
            let bottom = geo.content_top() + geo.content_height() + 0.01;
            for page in &pages {
                // A fresh box per page segment, and the box never runs off
                // the bottom of its page.
                let boxes: Vec<_> = page.rects.iter().filter(|r| r.color == CODE_BG).collect();
                assert_eq!(boxes.len(), 1, "one background box per segment");
                assert!(boxes[0].y + boxes[0].h <= bottom);
                assert!(!page.glyphs.is_empty());
            }
        });
    }

    #[test]
    fn svg_taller_than_the_page_scales_to_fit() {
        with_ctx(|book, embedder| {
            let geo = geo(200.0, 100.0);
            let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" \
                       width=\"1000\" height=\"4000\" viewBox=\"0 0 1000 4000\"></svg>";
            let blocks = vec![
                BlockNode::Svg { svg: svg.into() },
                // A render string with no `<svg>` (e.g. a math-error marker)
                // is skipped benignly, not a fatal embed error.
                BlockNode::Svg {
                    svg: "no markup here".into(),
                },
            ];
            let (pages, _) = layout(&[blocks], book, embedder, &geo);
            assert_eq!(pages.len(), 1);
            assert_eq!(pages[0].svgs.len(), 1);
            let placed = &pages[0].svgs[0];
            // Width-fit would leave 800pt of height, so the height clamp wins:
            // scale = 100/4000, preserving aspect and centring horizontally.
            assert!((placed.h - 100.0).abs() < 0.01, "h {}", placed.h);
            assert!((placed.w - 25.0).abs() < 0.01, "w {}", placed.w);
            assert!((placed.x - 97.5).abs() < 0.01, "x {}", placed.x);
            assert!(crate::pdf::svg_embed::take_embed_error().is_none());
        });
    }

    #[test]
    fn callout_taller_than_the_page_overflows_in_one_piece() {
        with_ctx(|book, embedder| {
            let geo = geo(150.0, 40.0);
            let body = "callout body text that wraps across many short lines ".repeat(10);
            let blocks = vec![BlockNode::Callout {
                accent: (200, 60, 60),
                heading: vec![text_run("Note")],
                body: vec![text_run(&body)],
            }];
            let (pages, _) = layout(&[blocks], book, embedder, &geo);
            // Callouts render as one unit: taller than the page means the box
            // overflows — never a split box, never a break loop.
            assert_eq!(pages.len(), 1);
            let tint = pages[0].rects.first().expect("tint box painted");
            assert!(tint.h > geo.content_height(), "box really is oversized");
        });
    }

    #[test]
    fn toc_rows_link_and_fill_with_leader_dots() {
        with_ctx(|book, embedder| {
            let geo = geo(300.0, 400.0);
            let entries = vec![
                TocLine {
                    depth: 0,
                    title: "Introduction".to_string(),
                    page: Some("intro".to_string()),
                    number: "3".to_string(),
                },
                TocLine {
                    depth: 0,
                    title: "Grouping".to_string(),
                    page: None,
                    number: String::new(),
                },
            ];
            let (pages, _) = layout(&[vec![BlockNode::Toc { entries }]], book, embedder, &geo);
            let page = &pages[0];
            // Only the entry that names a page becomes a clickable row; the
            // internal `<page>.html` form is what paint resolves to a dest.
            assert_eq!(page.links.len(), 1);
            assert_eq!(page.links[0].href, "intro.html");
            // The title→number gap fills with muted leader dots.
            assert!(
                page.glyphs
                    .iter()
                    .any(|g| g.color == TOC_DOT_COLOR && g.cluster == ".")
            );
        });
    }

    #[test]
    fn flow_is_top_down_and_inline_links_get_boxes() {
        with_ctx(|book, embedder| {
            let geo = geo(400.0, 600.0);
            let blocks = vec![
                BlockNode::Heading {
                    level: 1,
                    runs: vec![text_run("Title")],
                },
                para("first paragraph"),
                BlockNode::Paragraph {
                    runs: vec![InlineRun::Link {
                        runs: vec![text_run("a link")],
                        href: "https://example.com".to_string(),
                    }],
                },
            ];
            let (pages, _) = layout(&[blocks], book, embedder, &geo);
            assert_eq!(pages.len(), 1);
            let page = &pages[0];
            // Glyphs are pushed in layout order, so spacing accumulation must
            // move baselines strictly down the page — never back up.
            let mut last = f32::MIN;
            for glyph in &page.glyphs {
                assert!(
                    glyph.y >= last - 0.01,
                    "baseline moved up: {} after {last}",
                    glyph.y
                );
                last = last.max(glyph.y);
            }
            let link = page
                .links
                .iter()
                .find(|l| l.href == "https://example.com")
                .expect("link box built");
            assert!(link.w > 0.0 && link.h > 0.0);
            assert!(page.glyphs.iter().any(|g| g.color == LINK_COLOR));
        });
    }

    #[test]
    fn table_rows_paginate_and_keep_rules_inside_the_page() {
        with_ctx(|book, embedder| {
            let geo = geo(300.0, 60.0);
            let cell = |t: &str| vec![text_run(t)];
            let header = vec![cell("Name"), cell("Value")];
            let rows: Vec<Row> = (0..6)
                .map(|i| vec![cell(&format!("row {i}")), cell("v")])
                .collect();
            let (pages, _) = layout(
                &[vec![BlockNode::Table { header, rows }]],
                book,
                embedder,
                &geo,
            );
            assert!(pages.len() > 1, "seven rows cannot fit a 60pt box");
            // The shaded header paints once — rows that break to a new page
            // do not repeat it.
            let header_boxes = pages
                .iter()
                .flat_map(|p| &p.rects)
                .filter(|r| r.color == TABLE_HEADER_BG)
                .count();
            assert_eq!(header_boxes, 1);
            let bottom = geo.content_top() + geo.content_height() + 0.01;
            for page in &pages {
                for r in &page.rects {
                    assert!(r.y + r.h <= bottom, "rule at {}+{} escapes", r.y, r.h);
                }
                assert!(!page.glyphs.is_empty(), "no blank pages from row breaks");
            }
        });
    }
}
