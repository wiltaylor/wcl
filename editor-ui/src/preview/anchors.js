// Shared DOM anchor contract for edit-mode preview builds: every editable
// block carries `data-wcl-span="start:end"` (byte offsets into the declaring
// file) plus `data-wcl-file`. Parsing and selector construction live here so
// the attribute format has one owner.

/** Parse a raw `start:end` span attribute value, or null when absent. */
export function parseSpanAttr(raw) {
  if (!raw) return null;
  const [start, end] = raw.split(':').map(Number);
  return { start, end };
}

/** The (file, span) anchor of an element, or null when unanchored. */
export function anchorSpanOf(el) {
  const span = parseSpanAttr(el.getAttribute('data-wcl-span'));
  const file = el.getAttribute('data-wcl-file');
  return span && file ? { file, span } : null;
}

/** CSS selector matching every element anchored at (file, span). */
export function spanSelector(file, span) {
  return `[data-wcl-file="${CSS.escape(file)}"][data-wcl-span="${span.start}:${span.end}"]`;
}
