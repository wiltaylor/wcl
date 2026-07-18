/* Byte-span → CodeMirror reveal support for the edit_object jump. wdoc
   spans are UTF-8 byte offsets into the declaring file; CodeMirror
   positions are UTF-16 code units, so revealing a span converts against
   the buffer text. A small registry (fed by a ViewPlugin smuggled through
   CodeEditor's `language` prop, like the LSP extensions) maps open paths
   to live EditorViews; a reveal for a not-yet-mounted buffer parks in
   `pending` until its view appears. */

import { EditorView, ViewPlugin } from '@codemirror/view';

import { buffer } from './buffers';

const views = new Map();
const pending = new Map();

/** UTF-16 offset in `text` of the UTF-8 byte offset `byteOff`. */
export function byteToUtf16(text, byteOff) {
  let bytes = 0;
  let units = 0;
  for (const ch of text) {
    if (bytes >= byteOff) break;
    const cp = ch.codePointAt(0);
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    units += ch.length;
  }
  return units;
}

/** A CodeMirror extension that registers `path`'s EditorView while mounted. */
export function registerView(path) {
  return ViewPlugin.define((view) => {
    views.set(path, view);
    const p = pending.get(path);
    if (p) {
      pending.delete(path);
      // The plugin constructor runs before the view is measured; scroll
      // after a tick so the target lands where asked.
      setTimeout(() => reveal(view, path, p.start, p.end), 0);
    }
    return {
      destroy() {
        if (views.get(path) === view) views.delete(path);
      },
    };
  });
}

/** Select + scroll to a byte span of `path`, now or when its view mounts. */
export function revealSpan(path, start, end) {
  const view = views.get(path);
  if (view) reveal(view, path, start, end);
  else pending.set(path, { start, end });
}

function reveal(view, path, startByte, endByte) {
  const text = buffer(path)?.text ?? view.state.doc.toString();
  const max = view.state.doc.length;
  const from = Math.min(byteToUtf16(text, startByte), max);
  const to = Math.min(byteToUtf16(text, endByte), max);
  view.dispatch({
    selection: { anchor: from, head: to },
    effects: EditorView.scrollIntoView(from, { y: 'center' }),
  });
  view.focus();
}
