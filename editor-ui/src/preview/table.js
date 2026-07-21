/* Pipe-table parsing/serialization for the Design-mode table editor.
   Cells are WCL expressions; plain double-quoted strings unquote for
   editing and requote on save, anything else stays raw expr text. */

import { wclString } from './wysiwyg';

/** Split one pipe row into raw cell texts (respecting quoted strings). */
export function splitPipeRow(line) {
  const cells = [];
  let cur = '';
  let inStr = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (inStr) {
      cur += c;
      if (c === '\\') {
        cur += line[++i] ?? '';
      } else if (c === '"') inStr = false;
    } else if (c === '"') {
      cur += c;
      inStr = true;
    } else if (c === '|') {
      cells.push(cur.trim());
      cur = '';
    } else cur += c;
  }
  cells.push(cur.trim());
  // `| a | b |` → leading/trailing empties from the outer pipes.
  if (cells[0] === '') cells.shift();
  if (cells[cells.length - 1] === '') cells.pop();
  return cells;
}

/** A raw cell expr → display text (unquote plain strings; WCL string
    escapes are a JSON subset). Non-string exprs stay raw. */
export function cellDisplay(raw) {
  if (!/^"(?:[^"\\]|\\.)*"$/.test(raw)) return raw;
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

/** Display text → raw cell expr (always a quoted string on save). */
export function cellRaw(text) {
  return wclString(text);
}

/** A WCL list-literal expression for one row of display texts. */
export function rowExpr(row) {
  return `[${row.map((c) => cellRaw(c)).join(', ')}]`;
}

/** A WCL list-of-lists expression for a grid of display texts — the write
    form for `rows = [[…], …]` list-literal tables. */
export function rowsExpr(grid) {
  return `[${grid.map(rowExpr).join(', ')}]`;
}

// --- Table source model (shared by inline cell editing + the grid modal) ---

/** Parse a table's /api/block/source payload into an editable grid model:
    `{ mode: 'lists'|'pipes', grid, headerRows: 0|1, lines?, rowIdx? }`.
    `lists` = all-literal `header`/`rows` fields (grid row 0 is the header
    when present); `pipes` = literal pipe rows (row 0 is the header).
    Returns null for computed tables — no grid representation. */
export function parseTable(src) {
  const rowsField = src.fields?.rows;
  const headerItems = src.fields?.header?.state === 'list' ? src.fields.header.items : null;
  if (rowsField) {
    if (rowsField.state !== 'rows') return null;
    const grid = [...(headerItems ? [headerItems] : []), ...rowsField.rows].map((r) => [...r]);
    if (!grid.length) return null;
    const cols = Math.max(...grid.map((r) => r.length), 1);
    return { mode: 'lists', grid: padGrid(grid, cols), headerRows: headerItems ? 1 : 0 };
  }
  const lines = src.source.split('\n');
  const rowIdx = [];
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim().startsWith('|')) rowIdx.push(i);
  }
  if (!rowIdx.length) return null;
  const grid = rowIdx.map((i) => splitPipeRow(lines[i].trim()).map(cellDisplay));
  const cols = Math.max(...grid.map((r) => r.length), 1);
  return { mode: 'pipes', grid: padGrid(grid, cols), headerRows: 1, lines, rowIdx };
}

/** The write for an edited grid: `{ ops }` (set_field header/rows exprs)
    for list tables, `{ source }` (whole-block rewrite keeping surrounding
    lines + indent) for pipe tables. `span` is the table block's span. */
export function tableCommit(model, grid, span) {
  if (model.mode === 'lists') {
    const ops = [];
    const body = model.headerRows ? grid.slice(1) : grid;
    if (model.headerRows) {
      ops.push({ op: 'set_field', span, field: 'header', expr: rowExpr(grid[0] ?? []) });
    }
    ops.push({ op: 'set_field', span, field: 'rows', expr: rowsExpr(body) });
    return { ops };
  }
  const indent = (model.lines[model.rowIdx[0]].match(/^\s*/) ?? [''])[0] || '    ';
  const rendered = grid.map((row) => `${indent}| ${row.map((c) => cellRaw(c)).join(' | ')} |`);
  const source = [
    ...model.lines.slice(0, model.rowIdx[0]),
    ...rendered,
    ...model.lines.slice(model.rowIdx[model.rowIdx.length - 1] + 1),
  ].join('\n');
  return { source };
}

/** Map a click target inside a rendered table to grid coordinates
    (`{ row, col, cellEl }`), accounting for the thead row when the model
    carries a header. Returns null off any cell. */
export function cellCoords(target, headerRows) {
  const cell = target?.closest?.('td, th');
  if (!cell) return null;
  const tr = cell.parentElement;
  const inHead = !!cell.closest('thead');
  if (inHead && !headerRows) return null;
  const bodyIndex = [...(tr?.parentElement?.children ?? [])].indexOf(tr);
  if (bodyIndex < 0) return null;
  const row = inHead ? 0 : bodyIndex + headerRows;
  const col = [...tr.children].indexOf(cell);
  return { row, col, cellEl: cell };
}

// --- Pure grid operations for the table editor -----------------------------
// All return new arrays (rows are re-created, untouched rows are shared).

/** Pad ragged rows to `cols` cells so column ops are index-safe. */
export function padGrid(grid, cols) {
  return grid.map((row) => (row.length >= cols ? row : [...row, ...Array(cols - row.length).fill('')]));
}

/** Insert an empty row after index `r` (r = -1 inserts at the top). */
export function insertRowAt(grid, r, cols) {
  const next = [...grid];
  next.splice(r + 1, 0, Array(cols).fill(''));
  return next;
}

/** Swap row `r` with its neighbour (`dir` = -1 up / +1 down); edge moves are no-ops. */
export function moveRow(grid, r, dir) {
  const t = r + dir;
  if (t < 0 || t >= grid.length) return grid;
  const next = [...grid];
  [next[r], next[t]] = [next[t], next[r]];
  return next;
}

/** Insert an empty column after index `c` (c = -1 inserts leftmost). */
export function insertColAt(grid, c) {
  return grid.map((row) => {
    const next = [...row];
    next.splice(c + 1, 0, '');
    return next;
  });
}

/** Delete column `c`; refuses to drop the last column. */
export function delColAt(grid, c) {
  if ((grid[0]?.length ?? 0) <= 1) return grid;
  return grid.map((row) => row.filter((_, ci) => ci !== c));
}

/** Swap column `c` with its neighbour (`dir` = -1 left / +1 right); edge moves are no-ops. */
export function moveCol(grid, c, dir) {
  const t = c + dir;
  if (t < 0 || t >= (grid[0]?.length ?? 0)) return grid;
  return grid.map((row) => {
    const next = [...row];
    [next[c], next[t]] = [next[t], next[c]];
    return next;
  });
}
