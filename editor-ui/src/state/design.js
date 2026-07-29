/* Design-mode state: the mode toggle, the nav/palette models, the canvas
   selection, and the commit → targeted-rebuild → re-anchor loop every
   WYSIWYG operation runs through.

   Design mode is disk-only: entering it saves dirty buffers, every op
   commits straight to disk through the validating pipeline, and open
   (clean) buffers of touched files are refreshed from the response — so
   spans and etags stay single-sourced. After any commit all held spans are
   stale (the canonical printer reformats the whole file); either the
   rebuild refreshes the anchors before the next edit (the default loop),
   or an in-place commit (`commitOpsLocal`) patches the live DOM's anchors
   from the response's `span_map` without reloading the iframe. */

import { createSignal } from 'solid-js';

import { api } from '../api';
import { applyDiskUpdate, buffers, buffer, openFile, saveBuffer } from './buffers';
import { reloadGraph } from './graph';
import { activeEntry, activeSite, rebuild, selected } from './sites';
import { treeData } from './tree';
import { revealSpan } from './views';
import { toast } from '@forge/ui';

const [mode, setMode] = createSignal('code'); // 'code' | 'design'
const [navModel, setNavModel] = createSignal(null);
const [palette, setPalette] = createSignal(null);
/** Selected canvas block: { el, file, span, kind, shared } | null. */
const [selection, setSelection] = createSignal(null);
/** Commit → rebuild in flight: all canvas affordances disabled. */
const [busy, setBusy] = createSignal(false);
/** An active text session exists (the toolbar shows marker buttons). */
const [editingSession, setEditingSession] = createSignal(null);
/** The page currently shown in the canvas: { name, file } | null. */
const [currentPage, setCurrentPage] = createSignal(null);
/** Re-anchor instruction applied on the next frame load:
    { file, span, edit?: {kind, region} } */
const [pendingReveal, setPendingReveal] = createSignal(null);
/** Open structured editor: { type: 'fragment'|'code'|'table'|'image'|
    'component'|'insert', anchor, ... } | null (DesignView renders it). */
const [popover, setPopover] = createSignal(null);
/** Canvas navigation request: a page name the iframe should open. */
const [gotoPage, setGotoPage] = createSignal(null);
/** Which design surface shows: the WYSIWYG canvas, the wskill unit graph,
    or a WAD's systems model. */
const [designTab, setDesignTab] = createSignal('canvas'); // 'canvas' | 'graph' | 'systems'
/** Graph-mode commits skip the iframe rebuild; the canvas rebuilds on
    return when this is set. */
const [canvasStale, setCanvasStale] = createSignal(false);

export {
  mode,
  navModel,
  palette,
  selection,
  setSelection,
  busy,
  setBusy,
  editingSession,
  setEditingSession,
  currentPage,
  setCurrentPage,
  pendingReveal,
  setPendingReveal,
  popover,
  setPopover,
  gotoPage,
  setGotoPage,
  designTab,
  setDesignTab,
  canvasStale,
  setCanvasStale,
};

/** The shared scaffolding of every commit: the entry + busy gate, the
    dirty-open-buffer guard, the busy flag around the API call, the danger
    toast on failure, and (opt-in) the clean-buffer refresh from the
    response. The variant tails — in-place patch, quiet stale-marking, or
    the full rebuild loop — stay with the callers; `release` drops the busy
    gate here on success (the quiet paths; the rebuild paths release via
    frameReady, or afterCommit on a failed rebuild). */
async function runCommit(run, { guardFile = null, sync = false, release = false } = {}) {
  const entry = activeEntry();
  if (!entry || busy()) return { ok: false, error: 'busy' };
  if (guardFile && !dirtyGuard(guardFile)) return { ok: false, error: 'dirty buffer' };
  setBusy(true);
  const res = await run(entry);
  if (!res.ok) {
    setBusy(false);
    toast(res.error, { tone: 'danger', duration: 8000 });
    return res;
  }
  if (sync) syncBuffer(res.file, res.file_text, res.etag);
  if (release) setBusy(false);
  return res;
}

/** The `/api/block/ops` call every commitOps variant makes. */
const blockOpsCall = (file, ops, opts) => (entry) =>
  api.blockOps({
    entry,
    page_file: currentPage()?.file ?? undefined,
    file,
    ...(opts.etag ? { etag: opts.etag } : {}),
    ops,
  });

/** The `/api/unit/create` call both commitUnitCreate variants make. */
const unitCreateCall = (unit, pin) => (entry) =>
  api.unitCreate({
    entry,
    page_file: currentPage()?.file ?? undefined,
    unit,
    ...(pin ? { pin } : {}),
  });

/** A commit without the canvas rebuild loop — the graph view's write path
    (it refetches its own data; the canvas rebuilds when shown again). */
export async function commitOpsQuiet(file, ops, opts = {}) {
  const res = await runCommit(blockOpsCall(file, ops, opts), {
    guardFile: file,
    sync: true,
    release: true,
  });
  if (res.ok) setCanvasStale(true);
  return res;
}

/** Listeners for in-place commits (`commitOpsLocal`) — the content modal
    subscribes to mark its cached per-view builds stale without rebuilding
    the surface the commit just patched. */
const localCommitListeners = new Set();
export function onLocalCommit(fn) {
  localCommitListeners.add(fn);
  return () => localCommitListeners.delete(fn);
}

/** A commit applied IN PLACE — no preview rebuild, no iframe reload. The
    caller's `onApplied(res)` mirrors the change in the live DOM (move the
    element, restamp visibility) and patches the stale anchors from the
    response's `span_map`; returning `false` means the change can't be
    represented in place (e.g. un-hiding a block absent from this view's
    DOM) and the normal rebuild tail runs instead. Everything else — the
    graph payload, the canvas behind a modal, the other view tabs — is
    marked stale and refreshes lazily. */
export async function commitOpsLocal(file, ops, opts = {}) {
  const res = await runCommit(blockOpsCall(file, ops, opts), { guardFile: file, sync: true });
  if (!res.ok) return res;
  const applied = opts.onApplied ? opts.onApplied(res) : true;
  if (applied === false) {
    // Not representable in place — the full loop (busy released by the
    // rebuilt frame's load, or by afterCommit on failure).
    await afterCommit({ changed: [res.file] });
    return res;
  }
  // The graph payload's spans shifted with the reformat.
  reloadGraph({ keepPositions: true });
  // The canvas is stale unless it IS the patched surface (no modal open,
  // canvas tab showing) — setting the flag there would trigger an
  // immediate rebuild of the surface we just patched.
  if (surfaceRebuild || designTab() !== 'canvas') setCanvasStale(true);
  for (const fn of localCommitListeners) fn({ file: res.file });
  setBusy(false); // no frame load coming — release directly
  return res;
}

/** A nav op without the canvas rebuild loop — the graph-mode index panel's
    write path (pin/unpin/reorder). The nav model refreshes so the canvas
    NavPanel stays in sync; the caller refetches the graph itself. */
export async function commitNavOpQuiet(payload) {
  const res = await runCommit((entry) => api.navOp({ entry, site: activeSite(), ...payload }), {
    guardFile: payload.file,
    release: true,
  });
  if (!res.ok) return res;
  setCanvasStale(true);
  loadNav();
  return res;
}

/** Unit creation without the canvas rebuild loop — the graph view's write
    path (its caller refetches the graph; the canvas rebuilds when shown
    again). Same payload as [`commitUnitCreate`]. */
export async function commitUnitCreateQuiet(unit, pin) {
  const res = await runCommit(unitCreateCall(unit, pin), { release: true });
  if (!res.ok) return res;
  setCanvasStale(true);
  loadNav();
  return res;
}

export async function loadNav() {
  const entry = activeEntry();
  if (!entry) return { ok: false, error: 'no site selected' };
  const res = await api.nav(entry, activeSite());
  if (res.ok) setNavModel(res);
  return res;
}

export async function loadPalette() {
  const entry = activeEntry();
  if (!entry) return { ok: false, error: 'no site selected' };
  const res = await api.palette(entry, activeSite(), currentPage()?.file ?? null);
  if (res.ok) setPalette(res);
  return res;
}

/** Disk-only modes (Design / Data) start by flushing dirty buffers. */
async function saveAllDirty(context) {
  for (const path of Object.keys(buffers.buffers)) {
    if (buffers.buffers[path]?.dirty) {
      const res = await saveBuffer(path);
      if (!res.ok) {
        toast(`Save ${path} before ${context}: ${res.error}`, { tone: 'danger', duration: 6000 });
        return false;
      }
    }
  }
  return true;
}

/** Enter Design mode: save every dirty buffer (disk-only policy), then load
    the nav + palette models. The canvas builds on first mount. */
export async function enterDesign() {
  if (!(await saveAllDirty('Design mode'))) return false;
  setMode('design');
  loadNav();
  loadPalette();
  return true;
}

/** Enter Data mode: same disk-only policy; the DataView loads its own
    type/row models. */
export async function enterData() {
  if (!(await saveAllDirty('Data mode'))) return false;
  setSelection(null);
  setEditingSession(null);
  setMode('data');
  return true;
}

export function exitDesign() {
  setSelection(null);
  setEditingSession(null);
  setMode('code');
}

/** "Open code": jump from the canvas to the current page's declaring
    source — switch to Code mode, open the file in a tab, and select the
    page block's span. */
export async function openPageSource() {
  const page = currentPage();
  if (!page?.file) {
    toast('No page loaded in the canvas yet', { duration: 3000 });
    return;
  }
  // data-wcl-page-file is absolute; the buffer store keys repo-relative.
  const root = treeData()?.root;
  let rel = page.file;
  if (root && rel.startsWith(root)) rel = rel.slice(root.length).replace(/^[/\\]/, '');
  exitDesign();
  const res = await openFile(rel);
  if (!res.ok) {
    toast(res.error, { tone: 'danger', duration: 6000 });
    return;
  }
  if (page.span) revealSpan(rel, page.span.start, page.span.end);
}

/** Guard: WYSIWYG ops write to disk, so a dirty open buffer of the same
    file would silently lose its changes. */
function dirtyGuard(file) {
  if (buffer(file)?.dirty) {
    toast(`${file} has unsaved editor changes — save or close its tab first`, {
      tone: 'danger',
      duration: 6000,
    });
    return false;
  }
  return true;
}

/** Refresh an open (clean) buffer of a committed file from the response. */
function syncBuffer(file, fileText, etag) {
  const b = buffer(file);
  if (b && !b.dirty && fileText != null) applyDiskUpdate(file, fileText, etag);
}

/** Run a batch of block ops against one file, then the commit loop:
    targeted rebuild of the current page, anchors refreshed on reload,
    `reveal` (or the op's edited/inserted span) re-selected.

    opts: { etag?, reveal?: 'edited'|'inserted'|null, edit?: bool (start a
    text session on the revealed block), refreshNav?: bool } */
export async function commitOps(file, ops, opts = {}) {
  const res = await runCommit(blockOpsCall(file, ops, opts), { guardFile: file, sync: true });
  if (!res.ok) return res;
  const role = opts.reveal ?? 'edited';
  const hit = role && res.spans?.find((sp) => sp.role === role);
  if (hit) {
    // Key the reveal by the caller's `file` (the anchor's data-wcl-file
    // string), not the server's repo-relative `res.file` — the rebuilt
    // page stamps anchors with the path the document was parsed from, and
    // elBySpan must match that exact attribute value.
    setPendingReveal({ file, span: hit.span, edit: opts.edit ?? false });
  }
  await afterCommit({ changed: [res.file], refreshNav: opts.refreshNav });
  return res;
}

/** Write one field of a located data object (the edit_field bindings). */
export async function commitUnitField(binding, value) {
  const res = await runCommit(
    (entry) =>
      api.unitField({
        entry,
        page_file: currentPage()?.file ?? undefined,
        kind: binding.kind,
        target: binding.target ?? undefined,
        field: binding.field,
        value,
      }),
    { sync: true },
  );
  if (!res.ok) return res;
  await afterCommit({ changed: [res.file], refreshNav: true });
  return res;
}

/** A structural nav edit, then a rebuild (page-set changes take the full
    path automatically) and a nav-model reload. */
export async function commitNavOp(payload) {
  const res = await runCommit((entry) => api.navOp({ entry, site: activeSite(), ...payload }), {
    guardFile: payload.file,
  });
  if (!res.ok) return res;
  await afterCommit({ changed: payload.file ? [payload.file] : [], refreshNav: true });
  return res;
}

/** Create a unit (with optional pin), then rebuild + refresh models. */
export async function commitUnitCreate(unit, pin) {
  const res = await runCommit(unitCreateCall(unit, pin));
  if (!res.ok) return res;
  await afterCommit({ changed: res.file ? [res.file] : [], refreshNav: true });
  return res;
}

/** When set, the commit tail rebuilds through this hook instead of the main
    preview — the graph content modal registers its own targeted view build
    while it is open, so edits made inside it refresh its iframe (and not the
    hidden canvas). Receives { changed } and must resolve to { ok }. */
let surfaceRebuild = null;
export function setSurfaceRebuild(fn) {
  surfaceRebuild = fn;
}

/** The shared tail of every commit: rebuild (targeted when possible — the
    server falls back to full on structural change), refresh models, release
    the busy gate once the iframe has reloaded (onFrameReady). */
async function afterCommit({ changed, refreshNav }) {
  setSelection(null);
  setEditingSession(null);
  const page = currentPage()?.name;
  const res = surfaceRebuild
    ? await surfaceRebuild({ changed: changed ?? [] })
    : await rebuild({
        ...(page ? { pages: [page] } : {}),
        changed: changed ?? [],
      });
  if (!res.ok) {
    setBusy(false);
    toast('Rebuild failed — see the preview pane', { tone: 'danger', duration: 6000 });
  }
  if (refreshNav) loadNav();
  // busy is released by onFrameReady (the owning EditSurface) once anchors
  // are fresh; a failed rebuild released it above.
}

/** Called by the canvas when the iframe (re)loaded and anchors are fresh. */
export function frameReady() {
  setBusy(false);
}
