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
    pub family: FontFamily,
    pub bold: bool,
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
    Text { text: String, style: TextStyle },
    /// A hyperlink wrapping styled child runs.
    Link { runs: Vec<InlineRun>, href: String },
}

/// A syntax-highlighted token within a code line.
#[derive(Clone, Debug)]
pub(crate) struct CodeSpan {
    pub text: String,
    pub color: (u8, u8, u8),
}

/// One flattened list item: a nesting `depth`, its marker (`•` / `1.` / `1.2.`),
/// and its inline content.
#[derive(Clone, Debug)]
pub(crate) struct ListLine {
    pub depth: u8,
    pub marker: String,
    pub runs: Vec<InlineRun>,
}

/// A table cell: a list of inline runs.
pub(crate) type Cell = Vec<InlineRun>;
/// A table row: a list of cells.
pub(crate) type Row = Vec<Cell>;

/// A block-level flow node.
#[derive(Clone, Debug)]
pub(crate) enum BlockNode {
    /// A heading, `level` in `1..=6`.
    Heading { level: u8, runs: Vec<InlineRun> },
    /// A body paragraph.
    Paragraph { runs: Vec<InlineRun> },
    /// Embedded SVG content (a diagram, chart, timeline, or block equation),
    /// carried as the renderer's SVG string for the embed pass to parse.
    Svg { svg: String },
    /// A syntax-highlighted code block: one inner `Vec` per source line.
    Code { lines: Vec<Vec<CodeSpan>> },
    /// A bullet or numbered list, flattened to indented marked lines.
    List { lines: Vec<ListLine> },
    /// A table: an optional header row plus body rows, each cell a run list.
    Table { header: Row, rows: Vec<Row> },
}
