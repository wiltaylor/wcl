/* Unit tests for the table editor's pure grid operations: ragged-row
   padding, positional row/column insertion, reorder moves (with edge
   no-ops), and the last-column delete guard. */

import { describe, expect, it } from 'vitest';

import {
  cellCoords,
  delColAt,
  insertColAt,
  insertRowAt,
  moveCol,
  moveRow,
  padGrid,
  parseTable,
  rowExpr,
  rowsExpr,
  tableCommit,
} from './table';

const grid = () => [
  ['h1', 'h2', 'h3'],
  ['a1', 'a2', 'a3'],
  ['b1', 'b2', 'b3'],
];

describe('padGrid', () => {
  it('pads ragged rows to the column count', () => {
    const out = padGrid([['a'], ['b', 'c']], 3);
    expect(out).toEqual([
      ['a', '', ''],
      ['b', 'c', ''],
    ]);
  });

  it('leaves full rows alone', () => {
    const g = grid();
    expect(padGrid(g, 3)).toEqual(g);
  });
});

describe('insertRowAt', () => {
  it('inserts an empty row below the given index', () => {
    const out = insertRowAt(grid(), 0, 3);
    expect(out).toHaveLength(4);
    expect(out[1]).toEqual(['', '', '']);
    expect(out[2]).toEqual(['a1', 'a2', 'a3']);
  });

  it('inserts at the top with r = -1 and appends past the end', () => {
    expect(insertRowAt(grid(), -1, 3)[0]).toEqual(['', '', '']);
    const out = insertRowAt(grid(), 2, 3);
    expect(out[3]).toEqual(['', '', '']);
  });
});

describe('moveRow', () => {
  it('swaps a row with its neighbour', () => {
    const down = moveRow(grid(), 1, 1);
    expect(down[1][0]).toBe('b1');
    expect(down[2][0]).toBe('a1');
    const up = moveRow(grid(), 1, -1);
    expect(up[0][0]).toBe('a1');
    expect(up[1][0]).toBe('h1');
  });

  it('is a no-op at the edges', () => {
    const g = grid();
    expect(moveRow(g, 0, -1)).toBe(g);
    expect(moveRow(g, 2, 1)).toBe(g);
  });
});

describe('insertColAt', () => {
  it('inserts an empty column right of the given index', () => {
    const out = insertColAt(grid(), 0);
    expect(out[0]).toEqual(['h1', '', 'h2', 'h3']);
    expect(out[1]).toEqual(['a1', '', 'a2', 'a3']);
  });

  it('inserts leftmost with c = -1', () => {
    const out = insertColAt(grid(), -1);
    expect(out[0]).toEqual(['', 'h1', 'h2', 'h3']);
  });
});

describe('delColAt', () => {
  it('deletes the column from every row', () => {
    const out = delColAt(grid(), 1);
    expect(out[0]).toEqual(['h1', 'h3']);
    expect(out[2]).toEqual(['b1', 'b3']);
  });

  it('refuses to drop the last column', () => {
    const g = [['only'], ['x']];
    expect(delColAt(g, 0)).toBe(g);
  });
});

describe('parseTable / tableCommit', () => {
  const listSrc = {
    source: 'table {…}',
    fields: {
      header: { state: 'list', items: ['A', 'B'] },
      rows: { state: 'rows', rows: [['1', '2'], ['3', '4']] },
    },
  };
  const pipeSrc = {
    source: 'table {\n    rows:\n      | "A" | "B" |\n      | "1" | "2" |\n  }',
    fields: {},
  };
  const span = { start: 5, end: 9 };

  it('models list tables with the header as row 0', () => {
    const m = parseTable(listSrc);
    expect(m.mode).toBe('lists');
    expect(m.headerRows).toBe(1);
    expect(m.grid).toEqual([['A', 'B'], ['1', '2'], ['3', '4']]);
    const w = tableCommit(m, [['A!', 'B'], ['1', '2']], span);
    expect(w.ops).toEqual([
      { op: 'set_field', span, field: 'header', expr: '["A!", "B"]' },
      { op: 'set_field', span, field: 'rows', expr: '[["1", "2"]]' },
    ]);
  });

  it('models pipe tables and rewrites the row lines', () => {
    const m = parseTable(pipeSrc);
    expect(m.mode).toBe('pipes');
    expect(m.grid).toEqual([['A', 'B'], ['1', '2']]);
    const w = tableCommit(m, [['A', 'B'], ['1', 'x']], span);
    expect(w.source).toContain('| "1" | "x" |');
    expect(w.source.startsWith('table {')).toBe(true);
  });

  it('returns null for computed tables', () => {
    expect(parseTable({ source: 't', fields: { rows: { state: 'computed' } } })).toBeNull();
    expect(parseTable({ source: 'table {\n}', fields: {} })).toBeNull();
  });
});

describe('cellCoords', () => {
  it('maps thead/tbody cells to grid coordinates', () => {
    document.body.innerHTML =
      '<table><thead><tr><th id="h1">A</th><th>B</th></tr></thead>' +
      '<tbody><tr><td>1</td><td id="c12">2</td></tr><tr><td id="c21">3</td><td>4</td></tr></tbody></table>';
    expect(cellCoords(document.getElementById('h1'), 1)).toMatchObject({ row: 0, col: 0 });
    expect(cellCoords(document.getElementById('c12'), 1)).toMatchObject({ row: 1, col: 1 });
    expect(cellCoords(document.getElementById('c21'), 1)).toMatchObject({ row: 2, col: 0 });
    // Header-less model: tbody rows map straight through.
    expect(cellCoords(document.getElementById('c12'), 0)).toMatchObject({ row: 0, col: 1 });
    expect(cellCoords(document.body, 1)).toBeNull();
  });
});

describe('rowExpr / rowsExpr', () => {
  it('builds WCL list literals with string escaping', () => {
    expect(rowExpr(['a', 'b "q"'])).toBe('["a", "b \\"q\\""]');
    expect(rowsExpr([['a'], ['b', 'c']])).toBe('[["a"], ["b", "c"]]');
    expect(rowsExpr([])).toBe('[]');
  });

  it('round-trips markdown-ish cell text', () => {
    expect(rowExpr(['`code`', 'a\nb'])).toBe('["`code`", "a\\nb"]');
  });
});

describe('moveCol', () => {
  it('swaps a column with its neighbour in every row', () => {
    const right = moveCol(grid(), 0, 1);
    expect(right[0]).toEqual(['h2', 'h1', 'h3']);
    expect(right[1]).toEqual(['a2', 'a1', 'a3']);
    const left = moveCol(grid(), 2, -1);
    expect(left[0]).toEqual(['h1', 'h3', 'h2']);
  });

  it('is a no-op at the edges', () => {
    const g = grid();
    expect(moveCol(g, 0, -1)).toBe(g);
    expect(moveCol(g, 2, 1)).toBe(g);
  });
});
