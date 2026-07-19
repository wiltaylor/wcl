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
