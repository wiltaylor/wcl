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
use super::ir::{BlockNode, Cell, CodeSpan, ListLine, Row, TextStyle};
use super::svg_embed::SvgEmbedder;
use super::text::{FontBook, InlineObject, ShapedGlyph};

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

/// One physical page's painted content. Rects paint first (backgrounds), then
/// images and SVGs, then glyphs on top.
#[derive(Default)]
pub(crate) struct LaidOutPage {
    pub glyphs: Vec<PlacedGlyph>,
    pub links: Vec<LinkBox>,
    pub svgs: Vec<PlacedSvg>,
    pub rects: Vec<RectFill>,
    pub images: Vec<PlacedImage>,
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
            if let BlockNode::Code { lines } = block {
                place_code(
                    lines,
                    book,
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
            if let BlockNode::List { lines } = block {
                place_list(
                    lines,
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
                continue;
            }
            if let BlockNode::Table { header, rows } = block {
                place_table(
                    header,
                    rows,
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
                BlockNode::Svg { .. }
                | BlockNode::Code { .. }
                | BlockNode::List { .. }
                | BlockNode::Table { .. }
                | BlockNode::Callout { .. }
                | BlockNode::Image { .. } => {
                    unreachable!("handled above")
                }
            };

            if !at_page_top {
                cy += space_before;
            }

            let shaped = book.shape_paragraph(runs, content_w, size, line_height, embedder);
            let mut placed = vec![false; shaped.objects.len()];
            for line in &shaped.lines {
                // Break before a line that would overflow — unless the page is
                // empty (a single oversized line overflows rather than loops).
                if cy + line.height > content_h && !at_page_top {
                    pages.push(LaidOutPage::default());
                    cy = 0.0;
                }
                let baseline = top + cy + line.ascent;
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
                cy += line.height;
                at_page_top = false;
            }

            cy += space_after;
        }
    }

    (pages, section_starts)
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
        while i < lines.len() && *cy + lh + pad <= content_h {
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
