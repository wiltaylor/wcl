//! Intermediate representation for the PDF backend.
//!
//! [`collect`](super::collect) lowers wdoc blocks into this small,
//! paint-agnostic block/inline model; [`layout`](super::layout) measures and
//! paginates it; [`paint`](super::paint) draws it with krilla. The IR grows
//! per phase — at this stage it carries prose only (headings + paragraphs of
//! styled inline text).

/// A font role, mapped to one of the bundled Noto faces by
/// [`super::text::FontBook`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FontFamily {
    /// Body text (Noto Serif).
    Serif,
    /// Headings and UI chrome (Noto Sans).
    Sans,
    /// Inline / block code (Noto Sans Mono).
    Mono,
}

/// The styling of an inline text run.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextStyle {
    /// Which font family the run is set in.
    pub family: FontFamily,
    /// Whether the run is bold.
    pub bold: bool,
    /// Whether the run is italic.
    pub italic: bool,
}

impl TextStyle {
    /// The default body-text style: regular serif.
    pub(crate) fn body() -> Self {
        Self {
            family: FontFamily::Serif,
            bold: false,
            italic: false,
        }
    }

    /// The heading style: bold sans-serif. The size is applied per level by
    /// the layout pass, not stored here.
    pub(crate) fn heading() -> Self {
        Self {
            family: FontFamily::Sans,
            bold: true,
            italic: false,
        }
    }

    /// The monospace style for code.
    pub(crate) fn code() -> Self {
        Self {
            family: FontFamily::Mono,
            bold: false,
            italic: false,
        }
    }
}

/// A run of inline content within a block. Inline code is just `Text` carrying
/// the [`FontFamily::Mono`] style; icons and math join with the SVG phase.
#[derive(Clone, Debug)]
pub(crate) enum InlineRun {
    /// Styled text.
    Text {
        /// The run text.
        text: String,
        /// How it is set.
        style: TextStyle,
    },
    /// A hyperlink wrapping styled child runs.
    Link {
        /// The wrapped child runs.
        runs: Vec<InlineRun>,
        /// Link target.
        href: String,
    },
    /// An inline SVG object (icon or equation) carried as a standalone SVG
    /// string, embedded and overlaid in the text flow by the layout pass.
    Object {
        /// The standalone SVG string.
        svg: String,
    },
}

/// A syntax-highlighted token within a code line.
#[derive(Clone, Debug)]
pub(crate) struct CodeSpan {
    /// The token text.
    pub text: String,
    /// Colour the highlighter assigned, as RGB.
    pub color: (u8, u8, u8),
}

/// One flattened list item: a nesting `depth`, its marker (`•` / `1.` / `1.2.`),
/// and its inline content.
#[derive(Clone, Debug)]
pub(crate) struct ListLine {
    /// Nesting depth, zero at the top level.
    pub depth: u8,
    /// The rendered bullet or number.
    pub marker: String,
    /// The line's inline content.
    pub runs: Vec<InlineRun>,
}

/// One entry on the printed "Contents" page: a nesting `depth`, the chapter
/// `title`, the `page` it links to (`None` for a grouping heading), and the
/// resolved `number` text (empty for a grouping heading).
#[derive(Clone, Debug)]
pub(crate) struct TocLine {
    /// Nesting depth, zero at the top level.
    pub depth: u8,
    /// The chapter title.
    pub title: String,
    /// Page this entry links to; `None` for a grouping heading.
    pub page: Option<String>,
    /// Resolved section number; empty for a grouping heading.
    pub number: String,
}

/// A table cell: a list of inline runs.
pub(crate) type Cell = Vec<InlineRun>;
/// A table row: a list of cells.
pub(crate) type Row = Vec<Cell>;

/// A block-level flow node.
#[derive(Clone, Debug)]
pub(crate) enum BlockNode {
    /// A heading, `level` in `1..=6`.
    Heading {
        /// Heading level, `1..=6`.
        level: u8,
        /// The heading text.
        runs: Vec<InlineRun>,
    },
    /// A body paragraph.
    Paragraph {
        /// The paragraph text.
        runs: Vec<InlineRun>,
    },
    /// Embedded SVG content (a diagram, chart, timeline, or block equation),
    /// carried as the renderer's SVG string for the embed pass to parse.
    Svg {
        /// The SVG source for the embed pass to parse.
        svg: String,
    },
    /// A syntax-highlighted code block: one inner `Vec` per source line.
    Code {
        /// Highlighted tokens, one inner `Vec` per source line.
        lines: Vec<Vec<CodeSpan>>,
    },
    /// A bullet or numbered list, flattened to indented marked lines.
    List {
        /// The flattened, marked lines.
        lines: Vec<ListLine>,
    },
    /// A table: an optional header row plus body rows, each cell a run list.
    Table {
        /// The header row; empty when the table has none.
        header: Row,
        /// The body rows.
        rows: Vec<Row>,
    },
    /// A printed table-of-contents page: indented chapter titles with leader
    /// dots and right-aligned page numbers. Each entry that names a page is a
    /// clickable jump to it.
    Toc {
        /// The contents entries, in reading order.
        entries: Vec<TocLine>,
    },
    /// A callout (admonition): an accent colour, an optional glyph, a bold
    /// heading, and body text.
    Callout {
        /// The callout's accent colour, as RGB.
        accent: (u8, u8, u8),
        /// The kind's glyph as a standalone `<svg>`, already recoloured to
        /// `accent`. `None` for a callout with no kind and no `icon`.
        icon: Option<String>,
        /// The bold heading line.
        heading: Vec<InlineRun>,
        /// The callout body.
        body: Vec<InlineRun>,
    },
    /// A raster image: the encoded file bytes plus an optional display size.
    Image {
        /// The encoded image file.
        bytes: Vec<u8>,
        /// Display width, or `None` to use the intrinsic size.
        disp_w: Option<f32>,
        /// Display height, or `None` to use the intrinsic size.
        disp_h: Option<f32>,
    },
    /// A diagram whose `card` shapes carry native wdoc bodies. The SVG draws
    /// everything except the card content (cards are empty boxes); each
    /// [`CardSpec`] body is laid out and painted natively over its box.
    Diagram {
        /// The diagram SVG, with card boxes left empty.
        svg: String,
        /// The diagram SVG's `viewBox` `(min_x, min_y, width, height)`.
        viewbox: (f32, f32, f32, f32),
        /// Card bodies to paint over their boxes.
        cards: Vec<CardSpec>,
    },
}

/// A card inside a diagram: its box (in the diagram's viewBox coordinates) and
/// its body as collected PDF blocks.
#[derive(Clone, Debug)]
pub(crate) struct CardSpec {
    /// `(x, y, width, height)` in viewBox coordinates.
    pub rect: (f32, f32, f32, f32),
    /// The card's content, as PDF blocks.
    pub body: Vec<BlockNode>,
}
