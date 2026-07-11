/* Open-buffer store: one entry per open file, keyed by repo-relative path.
   Tracks the live text, the last-saved text, the save etag, and per-file
   LSP diagnostics (as @forge/code CodeAnnotation[]). */

import { createSignal } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { api } from '../api';
import { lsp, isWcl, docUri } from '../lsp/client';

const [store, setStore] = createStore({ buffers: {}, order: [] });
const [active, setActive] = createSignal(null);
/** Save conflict awaiting a user decision: { path } | null. */
const [conflict, setConflict] = createSignal(null);

export { store as buffers, active, conflict };

export function buffer(path) {
  return store.buffers[path];
}

/* Files never opened as text buffers — viewed via /api/raw instead (svg is
   deliberately absent: it edits as text and previews as an image). */
const BINARY_EXT = /\.(png|jpe?g|gif|webp|bmp|ico|woff2?|ttf|otf|pdf|zip|gz|tar|mp[34]|webm|wasm)$/i;

export function isBinaryPath(path) {
  return BINARY_EXT.test(path);
}

export async function openFile(path) {
  if (store.buffers[path]) {
    setActive(path);
    return { ok: true };
  }
  let entry;
  if (isBinaryPath(path)) {
    entry = { path, binary: true, dirty: false, annotations: [] };
  } else {
    const res = await api.readFile(path);
    if (!res.ok) return res;
    entry = {
      path,
      text: res.text,
      savedText: res.text,
      etag: res.etag,
      dirty: false,
      annotations: [],
    };
  }
  setStore(
    produce((s) => {
      s.buffers[path] = entry;
      s.order.push(path);
    }),
  );
  setActive(path);
  if (isWcl(path)) lsp.didOpen(docUri(path), entry.text);
  return { ok: true };
}

export function editBuffer(path, text) {
  const b = store.buffers[path];
  if (!b || b.text === text) return;
  setStore('buffers', path, { text, dirty: text !== b.savedText });
  if (isWcl(path)) lsp.scheduleChange(docUri(path), text);
}

export function setAnnotations(path, annotations) {
  const b = store.buffers[path];
  if (!b) return;
  // Identical diagnostics must not touch the store: a new array reference
  // makes CodeEditor re-dispatch setDiagnostics, which closes any open
  // lint tooltip. Key order is fixed (toAnnotations), so stringify works.
  if (JSON.stringify(b.annotations) === JSON.stringify(annotations)) return;
  setStore('buffers', path, 'annotations', annotations);
}

/** Save one buffer. `overwrite` skips the etag check (conflict resolution). */
export async function saveBuffer(path, { overwrite = false } = {}) {
  const b = store.buffers[path];
  if (!b) return { ok: false, error: 'no such buffer' };
  if (b.binary) return { ok: true };
  const res = await api.saveFile(path, b.text, overwrite ? undefined : b.etag);
  if (res.ok) {
    setStore('buffers', path, {
      savedText: b.text,
      dirty: false,
      etag: res.etag ?? b.etag,
    });
    setConflict(null);
  } else if (res.status === 409) {
    setConflict({ path });
  }
  return res;
}

/** Conflict resolution: drop the buffer's changes and reload from disk. */
export async function reloadFromDisk(path) {
  const res = await api.readFile(path);
  if (res.ok) {
    setStore('buffers', path, {
      text: res.text,
      savedText: res.text,
      etag: res.etag,
      dirty: false,
    });
    if (isWcl(path)) lsp.scheduleChange(docUri(path), res.text);
  }
  setConflict(null);
  return res;
}

export function dismissConflict() {
  setConflict(null);
}

export function closeBuffer(path) {
  if (!store.buffers[path]) return;
  if (isWcl(path)) lsp.didClose(docUri(path));
  setStore(
    produce((s) => {
      delete s.buffers[path];
      s.order = s.order.filter((p) => p !== path);
    }),
  );
  if (active() === path) setActive(store.order[store.order.length - 1] ?? null);
}

/** Every dirty buffer as {path, text} — the preview overlay payload. */
export function dirtyFiles() {
  return store.order
    .filter((p) => store.buffers[p]?.dirty)
    .map((p) => ({ path: p, text: store.buffers[p].text }));
}
