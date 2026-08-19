//! The styled character grid: [`Style`] (ANSI attribute bits), [`Cell`]
//! (a glyph + fg/bg + style), and [`Grid`] (the `cols × rows` cell store).

use super::*;

#[derive(Clone, Copy, Default)]
/// One cell's text attributes, as SGR set them.
pub(super) struct Style {
    /// Bold.
    pub(super) bold: bool,
    /// Dim / faint.
    pub(super) dim: bool,
    /// Italic.
    pub(super) italic: bool,
    /// Underline.
    pub(super) underline: bool,
    /// Strikethrough.
    pub(super) strike: bool,
    /// Blink.
    pub(super) blink: bool,
    /// Swap foreground and background when painting.
    pub(super) inverse: bool,
    /// Hide the glyph, painting it as background.
    pub(super) conceal: bool,
}

#[derive(Clone, Copy)]
/// One character cell: its glyph, colours and style.
pub(super) struct Cell {
    /// The character in this cell.
    pub(super) ch: char,
    /// Foreground colour.
    pub(super) fg: Color,
    /// Background colour.
    pub(super) bg: Color,
    /// Text attributes.
    pub(super) style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            style: Style::default(),
        }
    }
}

/// A terminal screen: a fixed-size cell buffer plus the cursor.
pub(super) struct Grid {
    /// Width in cells.
    pub(super) cols: usize,
    /// Height in cells.
    pub(super) rows: usize,
    /// Cells in row-major order, `rows * cols` of them.
    pub(super) cells: Vec<Cell>,
    /// Cursor `(row, col)`, or `None` when hidden.
    pub(super) cursor: Option<(usize, usize)>,
}

impl Grid {
    /// A blank grid of the given size.
    pub(super) fn new(cols: usize, rows: usize) -> Self {
        Grid {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
            cursor: None,
        }
    }

    /// Write one cell, ignoring out-of-range coordinates.
    pub(super) fn set(&mut self, row: usize, col: usize, cell: Cell) {
        if row < self.rows && col < self.cols {
            self.cells[row * self.cols + col] = cell;
        }
    }

    /// Borrow one row of cells.
    pub(super) fn row(&self, r: usize) -> &[Cell] {
        &self.cells[r * self.cols..(r + 1) * self.cols]
    }
}
