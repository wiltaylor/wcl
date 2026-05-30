//! The styled character grid: [`Style`] (ANSI attribute bits), [`Cell`]
//! (a glyph + fg/bg + style), and [`Grid`] (the `cols × rows` cell store).

use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct Style {
    pub(super) bold: bool,
    pub(super) dim: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strike: bool,
    pub(super) blink: bool,
    pub(super) inverse: bool,
    pub(super) conceal: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Cell {
    pub(super) ch: char,
    pub(super) fg: Color,
    pub(super) bg: Color,
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

pub(super) struct Grid {
    pub(super) cols: usize,
    pub(super) rows: usize,
    pub(super) cells: Vec<Cell>,
    pub(super) cursor: Option<(usize, usize)>,
}

impl Grid {
    pub(super) fn new(cols: usize, rows: usize) -> Self {
        Grid {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
            cursor: None,
        }
    }

    pub(super) fn set(&mut self, row: usize, col: usize, cell: Cell) {
        if row < self.rows && col < self.cols {
            self.cells[row * self.cols + col] = cell;
        }
    }

    pub(super) fn row(&self, r: usize) -> &[Cell] {
        &self.cells[r * self.cols..(r + 1) * self.cols]
    }
}
