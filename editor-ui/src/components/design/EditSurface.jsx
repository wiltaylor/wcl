/* The reusable WYSIWYG editing surface: an edit-mode preview build in an
   iframe, the selection/session layer (preview/wysiwyg.js) inside it, and
   the floating block toolbar in the parent (Forge chrome can't render
   inside the plain wdoc build). Extracted from DesignCanvas so the graph
   view's content modal hosts the exact same editor over a different build.

   Click selects a block; click again enters a source-swap text session for
   prose kinds or opens the right structured editor (the popovers render in
   DesignView's BlockEditorModals, shared by every surface). Every commit
   runs the disk → targeted rebuild → re-anchor loop in state/design.js.

   Mounting it is a matter of passing what you have: a preview and a little
   configuration. Everything else — registering the rebuild hook, releasing
   the busy gate on the paths where no frame will load, saving and restoring
   the page the host was showing, and clearing the selection and any
   half-finished editing session — is scoped to this component's own mount
   and unmount, so a new host cannot forget it.

   Props:
   - preview      — the preview to mount (state/preview.js): `src()` is the
                    iframe URL (null = fallback slot), bumping `reloadSeq()`
                    with an unchanged src reloads in place (scroll survives),
                    `build({ changed })` is what the commit tail runs while
                    this surface is mounted, and `id` is the identity the
                    commit bus keys staleness on — so a surface is named by
                    the preview it shows rather than by a constant.
   - ref          — callback receiving the mount-scoped surface handle
                    ({ goto, doc, redecorate, merged, currentSite }),
                    and `null` on unmount. The handle goes inert when
                    released, so a host holding it cannot act on a surface
                    that is gone.
   - fallback     — JSX shown while the preview has no page
   - hideChrome   — hide the book template's sidebar/rail/pagenav in the
                    frame (the content modal's page-only previews)
   - gutter       — optional config for the per-block left gutter (drag
                    handle + profile button, placed on every frame load):
                    { merged, currentSite } — merged builds tint the button
                    from the visibility stamps and ghost blocks hidden in
                    the rendering view. The button pops up the visibility
                    editor; the handle re-orders via batched `move` ops.
   - site         — the site this build renders under (non-merged
                    surfaces); the visibility editor uses it to hide a
                    block in place when it excludes the current view.
   - onFrameLoad  — optional: hosts that mirror the frame's content (the
                    screen editor's structure tree) rebuild on it. */

import { Show, createEffect, createSignal, onCleanup } from 'solid-js';
import {
  ArrowDown,
  ArrowUp,
  Bold,
  Braces,
  Code,
  Eye,
  Hand,
  IndentDecrease,
  IndentIncrease,
  Italic,
  Link,
  Plus,
  RotateCcw,
  Scaling,
  Settings2,
  Shapes,
  Table,
  Trash2,
} from 'lucide-solid';
import { Badge, IconButton, Select, toast } from '@forge/ui';

import { api } from '../../api';
import { activeEntry, selected } from '../../state/sites';
import {
  busy,
  commitOps,
  commitOpsLocal,
  commitUnitField,
  editingSession,
  frameReady,
  loadPalette,
  palette,
  pendingReveal,
  pushSurfaceRebuild,
  selection,
  setEditingSession,
  setPendingReveal,
  setPopover,
  setSelection,
  showRefusal,
} from '../../state/design';
import { currentPage, setCurrentPage } from '../../state/preview';
import {
  adjacentSameFileSibling,
  anchorEls,
  anchorOf,
  closestOfKind,
  diagramOf,
  elBySpan,
  isManualLayout,
  pageInfo,
  prevSiblingOfKind,
  restampExcept,
  shapeChildren,
  shapeIdOf,
  spanOf,
} from '../../preview/anchors';
import { injectBareCss } from '../../preview/frame';
import { mappedSpan, moveDomBlock, patchAnchors } from '../../preview/localops';
import { cellNamed } from '../../preview/schemaform';
import { placeVisGutters } from '../../preview/visgutter';
import {
  beginTextSession,
  installDesign,
  markSelected,
  wclString,
  wrapSelection,
} from '../../preview/wysiwyg';
import {
  clientToUser,
  installShapeDrag,
  readTranslate,
  refreshShapeHandles,
} from '../../preview/diagram';
import { createPageScopes } from '../../preview/pagescope';
import { shapeOps } from '../../preview/shapeops';
import { createSurfaceHandle } from '../../preview/surfaceref';
import {
  cellCoords,
  delColAt,
  insertColAt,
  insertRowAt,
  parseTable,
  tableCommit,
} from '../../preview/table';
import ShapePanel from './ShapePanel';

/** The floating shape dock's footprint, for edge flipping/clamping. */
const DOCK_W = 332;
const DOCK_GAP = 10;

/* Who owns `currentPage` while surfaces nest — see preview/pagescope.js.
   One stack for every mounted surface; each one hands the page back to
   whatever was under it, so a host never has to remember to. */
const pageScopes = createPageScopes();

const PROSE = new Set(['p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'li']);
const HEADING_KINDS = ['p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6'];
const DIAGRAM_LAYOUTS = ['free', 'grid', 'layered', 'force', 'radial'];

export default function EditSurface(props) {
  let iframe;
  let wrapper;
  let lastSeq = 0;
  let lastHref = null;
  let teardown = null;
  let teardownDrag = null;
  let session = null; // active beginTextSession handle
  const [toolbarPos, setToolbarPos] = createSignal(null);
  const [dockPos, setDockPos] = createSignal(null); // shape properties dock
  const [sessionRegion, setSessionRegion] = createSignal(null); // {el, plain}

  const doc = () => iframe?.contentDocument;
  const src = () => props.preview?.src() ?? null;
  const reloadSeq = () => props.preview?.reloadSeq() ?? 0;

  // ------------------------------------------------------------------
  // Mount scope: the handle, the rebuild hook, and the obligations that
  // used to be the host's to remember.
  // ------------------------------------------------------------------

  // Everything a host may ask of this surface, in one reference whose
  // lifetime is the component's.
  const { handle, release } = createSurfaceHandle({
    goto: (url) => {
      if (iframe) iframe.src = url;
    },
    /** The preview document — hosts resolving anchors in the frame
        themselves (the screen editor's widget palette finds the wireframe
        diagram through it). */
    doc,
    /** Re-place the gutters after an in-place change (the visibility
        editor refreshes ghosts and button tints through it). */
    redecorate: () => decorate(),
    merged: () => props.gutter?.merged ?? false,
    currentSite: () => props.gutter?.currentSite ?? props.site,
  });
  props.ref?.(handle);

  const pageScope = pageScopes.push(currentPage());
  /** The page the frame is showing — kept in the shared state AND in this
      surface's own scope, so unmounting hands the right one back. */
  const notePage = (d) => {
    const page = pageInfo(d);
    setCurrentPage(page);
    pageScope.note(page);
  };

  onCleanup(() => {
    release();
    props.ref?.(null);
    setSelection(null);
    setEditingSession(null);
    const back = pageScope.release();
    if (back.restore) setCurrentPage(back.page);
  });

  // This surface owns the commit tail while it is mounted — through its
  // preview's own rebuild when it has one, else the main build the canvas
  // shows. The busy gate comes with it: a rebuild that leaves nothing to
  // reload (an unchanged href, or one refused because a build is already
  // running) releases the gate here rather than leaving the surface
  // disabled; the reloading paths release from the frame's load.
  onCleanup(
    pushSurfaceRebuild({
      // Read at commit time, not at mount: one surface can be handed a
      // different preview while it stays mounted (the content modal's tabs).
      get id() {
        return props.preview?.id ?? null;
      },
      run: async ({ changed }) => {
        const href0 = src();
        const seq0 = reloadSeq();
        const res = (await props.preview?.build({ changed })) ?? { ok: false };
        const reloading = !!src() && (src() !== href0 || reloadSeq() !== seq0);
        if (res?.ok && !reloading) frameReady();
        return res;
      },
    }),
  );

  // ------------------------------------------------------------------
  // Sessions
  // ------------------------------------------------------------------

  const endSession = () => {
    session = null;
    setSessionRegion(null);
    setEditingSession(null);
  };

  /** Source-swap prose session on one region of the selected block. */
  const proseSession = async (anchor, region, point) => {
    const res = await api.blockSource({ file: anchor.file, span: anchor.span });
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      return;
    }
    const slot =
      region.type === 'label'
        ? res.cells?.labels?.[region.slot]
        : cellNamed(res.cells, region.name);
    if (!slot || slot.state !== 'text') {
      // Interpolated / computed content: the fragment editor is the truth.
      setPopover({ type: 'fragment', anchor });
      return;
    }
    setEditingSession({ anchor, region });
    setSessionRegion({ el: region.regionEl, plain: false });
    const splitKinds = anchor.kind === 'p' || anchor.kind === 'li';
    session = beginTextSession(doc(), region.regionEl, slot.text ?? '', {
      caretAt: point,
      onCancel: endSession,
      onCommit: (text) => {
        endSession();
        const op =
          region.type === 'label'
            ? { op: 'set_label', span: anchor.span, slot: region.slot, text }
            : { op: 'set_field', span: anchor.span, field: region.name, text };
        commitOps(anchor.file, [op], { etag: res.etag, reveal: 'edited' });
      },
      onEnter: splitKinds
        ? (before, after) => {
            endSession();
            commitOps(
              anchor.file,
              [
                { op: 'set_label', span: anchor.span, slot: 0, text: before },
                {
                  op: 'insert_after',
                  span: anchor.span,
                  source: `${anchor.kind} ${wclString(after)}`,
                },
              ],
              { etag: res.etag, reveal: 'inserted', edit: true },
            );
          }
        : undefined,
    });
  };

  /** Inline table-cell session: click a cell in the selected table and
      edit its raw text in place; row/column operations ride the floating
      toolbar. Clicks outside any cell (border/edges) open the grid modal. */
  const tableSession = async (anchor, targetEl, point) => {
    const cell = targetEl?.closest?.('td, th');
    if (!cell || !anchor.el.contains(cell)) return setPopover({ type: 'table', anchor });
    const res = await api.blockSource({ file: anchor.file, span: anchor.span });
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      return;
    }
    const model = parseTable(res);
    const at = model && cellCoords(cell, model.headerRows);
    // Computed tables (or unmapped cells) fall back to the modal path,
    // which explains why there is no grid.
    if (!model || !at) return setPopover({ type: 'table', anchor });
    const commitGrid = (grid) => {
      const w = tableCommit(model, grid, anchor.span);
      const ops = w.ops ?? [{ op: 'replace_source', span: anchor.span, source: w.source }];
      commitOps(anchor.file, ops, { etag: res.etag, reveal: 'edited' });
    };
    setEditingSession({ anchor, table: { model, at, commitGrid } });
    setSessionRegion({ el: cell, plain: false });
    const commitCell = (text) => {
      endSession();
      if (text === (model.grid[at.row]?.[at.col] ?? '')) return;
      const g = model.grid.map((r) => [...r]);
      g[at.row][at.col] = text;
      commitGrid(g);
    };
    session = beginTextSession(doc(), cell, model.grid[at.row]?.[at.col] ?? '', {
      caretAt: point,
      onCancel: endSession,
      onCommit: commitCell,
      // Cells are single-line: plain Enter commits (no split semantics).
      onEnter: (before, after) => commitCell(before + after),
    });
  };

  /** A structural table edit from the toolbar mid-session: keep the cell's
      current text, transform the grid, commit the lot atomically. */
  const tableStructural = (transform) => {
    const t = editingSession()?.table;
    if (!t) return;
    const g = t.model.grid.map((r) => [...r]);
    if (session) g[t.at.row][t.at.col] = session.textNow();
    session?.finish(true);
    endSession();
    // A refused transform (last row/column) still commits the cell text.
    const next = transform(g, t.at, Math.max(...g.map((r) => r.length), 1));
    t.commitGrid(next ?? g);
  };

  /** Inline session on an edit_field binding: read the object's field,
      edit in place, write through /api/unit/field. */
  const fieldSession = async (binding, point) => {
    const loc = await api.locateObject({
      entry: activeEntry(),
      page_file: currentPage()?.file ?? undefined,
      kind: binding.kind,
      target: binding.target ?? undefined,
      files: [],
    });
    if (!loc.ok) {
      toast(loc.error, { tone: 'danger', duration: 6000 });
      return;
    }
    const src = await api.blockSource({ file: loc.file, span: loc.span });
    const slot = src.ok ? cellNamed(src.cells, binding.field) : null;
    if (!slot || slot.state !== 'text') {
      toast(`${binding.field} is computed — edit it in the source`, {
        tone: 'danger',
        duration: 5000,
      });
      return;
    }
    setEditingSession({ binding });
    setSessionRegion({ el: binding.el, plain: binding.plain });
    session = beginTextSession(doc(), binding.el, slot.text ?? '', {
      caretAt: point,
      onCancel: endSession,
      onCommit: (text) => {
        endSession();
        commitUnitField(binding, text);
      },
    });
  };

  // ------------------------------------------------------------------
  // Diagram shapes: the DOM/request half of the five gestures
  // ------------------------------------------------------------------
  // The ops themselves come from preview/shapeops.js — given a block's
  // source, its kind definition and the gesture it returns either the ops
  // or a refusal with a reason. What stays here is the fetch, the commit,
  // the message and everything that has to read the document.

  const kindDefOf = (kind) => palette()?.diagram_kinds?.find((k) => k.kind === kind) ?? null;
  /** Does this shape kind nest children? (the schema-derived palette flag) */
  const acceptsChildren = (kind) => kindDefOf(kind)?.accepts_children === true;

  const shapeMove = async (el, delta) => {
    const d = doc();
    const a = d && anchorOf(d, el);
    if (!a) return;
    const src = await api.blockSource({ file: a.file, span: a.span });
    if (!src.ok) return toast(src.error, { tone: 'danger', duration: 5000 });
    const res = shapeOps({
      gesture: 'move',
      kind: a.kind,
      kindDef: kindDefOf(a.kind),
      span: a.span,
      source: src,
      delta,
    });
    if (!res.ok) return showRefusal(res);
    commitOps(a.file, res.ops, { etag: src.etag, reveal: 'edited' });
  };

  const shapeResize = async (el, delta) => {
    const d = doc();
    const a = d && anchorOf(d, el);
    if (!a) return;
    const src = await api.blockSource({ file: a.file, span: a.span });
    if (!src.ok) return toast(src.error, { tone: 'danger', duration: 5000 });
    const res = shapeOps({
      gesture: 'resize',
      kind: a.kind,
      kindDef: kindDefOf(a.kind),
      span: a.span,
      source: src,
      delta,
      // The anchor's stashed measurement: dimensions the source leaves to
      // the renderer start from what is on screen.
      box: a.box ?? null,
    });
    if (!res.ok) return showRefusal(res);
    commitOps(a.file, res.ops, { etag: src.etag, reveal: 'edited' });
  };

  /** Port dragged from one shape onto another: wire them with an `a -> b`
      connection statement. The op addresses the DIAGRAM (connections are its
      items), not either shape, so resolve the enclosing svg's anchor. */
  const shapeConnect = async (fromEl, toEl) => {
    const d = doc();
    if (!d) return;
    const from = shapeIdOf(fromEl);
    const to = shapeIdOf(toEl);
    const owner = anchorOf(d, diagramOf(fromEl)) || null;
    // Missing ids, a self-connection and a generated diagram (one span
    // shared across every instance) are the builder's to refuse.
    const built = shapeOps({ gesture: 'connect', from, to, owner });
    if (!built.ok) return showRefusal(built);
    const src = await api.blockSource({ file: owner.file, span: owner.span });
    if (!src.ok) return toast(src.error, { tone: 'danger', duration: 5000 });
    const res = await commitOps(owner.file, built.ops, { etag: src.etag, reveal: 'edited' });
    if (res?.ok) toast(`Connected ${from} → ${to}`, { duration: 3000 });
  };

  /** A move released somewhere structural: re-home the shape's source —
      after a leaf sibling, into a container widget, or out onto the
      diagram (with x/y at the drop point when the target places shapes by
      coordinate). One atomic batch: insert the canonical slice at the
      target, delete the original. */
  const shapeRelocate = async (el, target, point) => {
    const d = doc();
    const a = d && anchorOf(d, el);
    const t = d && anchorOf(d, target.el);
    if (!a || !t) return;
    const src = await api.blockSource({ file: a.file, span: a.span });
    if (!src.ok) return toast(src.error, { tone: 'danger', duration: 5000 });
    // Only meaningful on a manual-layout diagram; the builder decides,
    // off the anchor's own layout (which reads through a viewport wrapper).
    const at = target.mode === 'diagram' ? clientToUser(target.el, point.x, point.y) : null;
    const res = shapeOps({
      gesture: 'relocate',
      slice: src.source,
      mode: target.mode,
      source: a,
      target: { ...t, acceptsChildren: acceptsChildren(t.kind) },
      at,
      slot: target.slot ?? null,
    });
    if (!res.ok) return showRefusal(res);
    // The move is legal but the solver will place it: say so, rather than
    // letting the shape land somewhere the author didn't drop it.
    if (at && !res.positional) {
      toast('This diagram lays its shapes out — the drop position was ignored', { duration: 4000 });
    }
    commitOps(a.file, res.ops, { etag: src.etag, reveal: 'inserted' });
  };

  /** Materialize every top-level child's solver position into explicit
      x/y fields and switch the diagram to :free — one atomic batch. */
  const convertToManual = () => {
    const a = selection();
    const d = doc();
    if (!a || a.kind !== 'diagram' || !d) return;
    const children = [];
    // Top-level children only: a container's nested shapes keep their
    // container-local layout.
    for (const g of shapeChildren(a.el)) {
      const sa = anchorOf(d, g);
      children.push({
        kindDef: sa && kindDefOf(sa.kind),
        span: sa?.span,
        at: readTranslate(g),
      });
    }
    const res = shapeOps({ gesture: 'convert', span: a.span, children });
    if (!res.ok) return showRefusal(res);
    if (res.skipped) {
      toast(`${res.skipped} shape(s) without x/y fields kept their defaults`, { duration: 4000 });
    }
    commitOps(a.file, res.ops, { reveal: 'edited' });
  };

  const setDiagramLayout = (mode) => {
    const a = selection();
    if (!a) return;
    commitOps(
      a.file,
      [{ op: 'set_field', span: a.span, field: 'layout', expr: `:${mode}` }],
      { reveal: 'edited' },
    );
  };

  /** Route a second click on the selected block to the right editor. */
  const startEditFor = (anchor, targetEl, point) => {
    // Shape properties live in the sidebar (already open on selection);
    // a second click on the diagram background opens its source.
    if (anchor.shape) return;
    if (anchor.kind === 'diagram') return setPopover({ type: 'fragment', anchor });
    if (anchor.kind === 'callout') {
      const body = targetEl?.closest?.('.callout-body');
      const title = targetEl?.closest?.('.callout-title, .callout-heading');
      if (body) return proseSession(anchor, { type: 'field', name: 'body', regionEl: body }, point);
      if (title) return proseSession(anchor, { type: 'label', slot: 0, regionEl: title }, point);
      return setPopover({ type: 'fragment', anchor });
    }
    if (PROSE.has(anchor.kind)) {
      return proseSession(anchor, { type: 'label', slot: 0, regionEl: anchor.el }, point);
    }
    if (anchor.kind === 'code') return setPopover({ type: 'code', anchor });
    if (anchor.kind === 'table') return tableSession(anchor, targetEl, point);
    if (anchor.kind === 'image') return setPopover({ type: 'image', anchor });
    if (palette()?.components?.some((c) => c.name === anchor.kind)) {
      return setPopover({ type: 'component', anchor });
    }
    return setPopover({ type: 'fragment', anchor });
  };

  // ------------------------------------------------------------------
  // Frame lifecycle
  // ------------------------------------------------------------------

  // Gutter placement, re-runnable after in-place commits (the sibling
  // lists and stamps change without a frame reload).
  const decorate = () => {
    const d = doc();
    if (!d) return;
    placeVisGutters(d, {
      merged: props.gutter?.merged ?? false,
      currentSite: props.gutter?.currentSite,
      onProfile: (a) => openVisibility(a),
      onReorder: reorderLocal,
      enabled: () => !busy(),
    });
  };

  /** Open the visibility editor on `anchor`, telling it how to apply what
      it decides. The editor returns the ops it wants; the surface applies
      them, so DOM mutation stays on the side that owns the DOM. */
  const openVisibility = (anchor) =>
    setPopover({ type: 'visibility', anchor, apply: applyVisibility });

  /** Commit the visibility editor's ops IN PLACE on this surface: restamp
      (merged builds) or remove (hidden in the shown view), no rebuild and
      no iframe reload. Un-hiding a block absent from a non-merged DOM
      can't be shown without a render, so that one case reports `false` and
      falls back to the full loop. */
  const applyVisibility = ({ anchor, ops, except, wasExcept, etag }) => {
    const onApplied = (res) => {
      const d = doc();
      if (!d) return true;
      const merged = props.gutter?.merged ?? false;
      const site = props.gutter?.currentSite ?? props.site;
      const els = anchorEls(d, anchor.file, anchor.span);
      const hidesHere = !merged && site && except.includes(site);
      const unhidesHere = !merged && site && wasExcept.includes(site) && !except.includes(site);
      if (unhidesHere && els.length === 0) return false;
      patchAnchors(d, anchor.file, res.span_map ?? []);
      for (const el of els) {
        if (hidesHere) el.remove();
        else restampExcept(el, except);
      }
      if (hidesHere) {
        const sel = selection();
        if (sel && sel.file === anchor.file && sel.span.start === anchor.span.start) {
          setSelection(null);
        }
      }
      decorate();
      return true;
    };
    return commitOpsLocal(anchor.file, ops, {
      etag,
      surface: props.preview?.id ?? null,
      onApplied,
    });
  };

  const spanOfEl = (e) => spanOf(e) ?? { start: 0, end: 0 };

  /** Gutter drag → in-place move + local commit; anchors patched from the
      response's span_map, so no rebuild or reload. The `move_to` op is
      span-addressed on both ends and resolves at the common-ancestor
      level, so template blocks (an `edit_field`-wrapped title, a
      `related_section`) move correctly past invisible AST siblings like
      the `project` body splice. Shared anchors (repeater output rendering
      one source block in several places) fall back to the full loop —
      moving one instance in place would lie. */
  const reorderLocal = ({ file, span, el, sameFile, dropIdx }) => {
    const d = doc();
    const target = sameFile[dropIdx]
      ? { before: spanOfEl(sameFile[dropIdx]) }
      : { after: spanOfEl(sameFile[sameFile.length - 1]) };
    const ops = [{ op: 'move_to', span, ...target }];
    if (!d || anchorEls(d, file, span).length > 1) {
      commitOps(file, ops, { reveal: 'edited' });
      return;
    }
    // Optimistic move to the drop slot: before the slot's block, or after
    // the last same-file block (never past unrelated trailing content).
    const ref = sameFile[dropIdx] ?? sameFile[sameFile.length - 1].nextElementSibling;
    const revert = moveDomBlock(el, ref);
    decorate();
    commitOpsLocal(file, ops, {
      surface: props.preview?.id ?? null,
      onApplied(res) {
        patchAnchors(d, file, res.span_map ?? []);
        notePage(d);
        decorate();
      },
    }).then((res) => {
      if (!res.ok) {
        revert();
        decorate();
      }
    });
  };

  const onFrameLoad = () => {
    const d = doc();
    if (!d) return;
    if (props.hideChrome) injectBareCss(d);
    decorate();
    teardown?.();
    teardownDrag?.();
    session = null;
    setSessionRegion(null);
    setEditingSession(null);
    setSelection(null);
    notePage(d);
    loadPalette();
    const select = (anchor) => {
      setSelection(anchor);
      markSelected(d, anchor?.el ?? null, anchor?.shared);
      refreshShapeHandles(d, anchor?.shape ? anchor.el : null);
    };
    teardown = installDesign(d, {
      enabled: () => !busy(),
      onSelect: select,
      onEditIntent: (anchor, targetEl, ev) =>
        startEditFor(anchor, targetEl, ev ? { x: ev.clientX, y: ev.clientY } : null),
      onFieldIntent: (binding, ev) =>
        fieldSession(binding, ev ? { x: ev.clientX, y: ev.clientY } : null),
    });
    teardownDrag = installShapeDrag(d, {
      enabled: () => !busy(),
      selectedShape: () => (selection()?.shape ? selection().el : null),
      onMove: shapeMove,
      onResize: shapeResize,
      onConnect: shapeConnect,
      acceptsChildren,
      onRelocate: shapeRelocate,
    });
    // Re-anchor after a commit rebuild.
    const reveal = pendingReveal();
    if (reveal) {
      setPendingReveal(null);
      const el = elBySpan(d, reveal.file, reveal.span);
      if (el) {
        const anchor = anchorOf(d, el);
        select(anchor);
        el.scrollIntoView?.({ block: 'nearest' });
        if (reveal.edit && anchor) startEditFor(anchor, el, null);
      }
    }
    // Hosts that mirror the frame's content (the screen editor's structure
    // tree) rebuild here — anchors are fresh.
    props.onFrameLoad?.();
    frameReady();
  };

  // Rebuild → reload in place (scroll survives); new href → src swap.
  createEffect(() => {
    const seq = reloadSeq();
    const href = src();
    if (seq === lastSeq) return;
    lastSeq = seq;
    if (href === lastHref) iframe?.contentWindow?.location.reload();
    lastHref = href;
  });

  // ------------------------------------------------------------------
  // Toolbar positioning (parent overlay tracking an iframe-content rect)
  // ------------------------------------------------------------------

  let raf = 0;
  const trackToolbar = () => {
    const target = sessionRegion()?.el ?? selection()?.el;
    if (!target || !wrapper || !iframe) {
      setToolbarPos(null);
      return;
    }
    const r = target.getBoundingClientRect();
    const ir = iframe.getBoundingClientRect();
    const wr = wrapper.getBoundingClientRect();
    // The iframe viewport maps 1:1 onto the iframe element's box.
    const top = ir.top - wr.top + r.top;
    setToolbarPos({
      left: Math.max(4, ir.left - wr.left + r.left),
      top: Math.max(4, top - 36),
      below: top - 36 < 4,
      bottom: ir.top - wr.top + r.bottom,
    });
    // The shape properties dock rides the selected shape's right edge,
    // flipping to the left when it would overflow the surface (so it works
    // in narrow hosts like the graph view's content modal too).
    if (selection()?.shape) {
      const rightOf = ir.left - wr.left + r.right + DOCK_GAP;
      const leftOf = ir.left - wr.left + r.left - DOCK_W - DOCK_GAP;
      setDockPos({
        left: rightOf + DOCK_W <= wr.width ? rightOf : Math.max(4, leftOf),
        top: Math.min(Math.max(4, top), Math.max(4, wr.height - 260)),
      });
    } else {
      setDockPos(null);
    }
  };
  createEffect(() => {
    void selection();
    void sessionRegion();
    void busy();
    cancelAnimationFrame(raf);
    const loop = () => {
      trackToolbar();
      raf = requestAnimationFrame(loop);
    };
    if (selection() || sessionRegion()) loop();
    else {
      setToolbarPos(null);
      setDockPos(null);
    }
    onCleanup(() => cancelAnimationFrame(raf));
  });
  onCleanup(() => {
    cancelAnimationFrame(raf);
    teardown?.();
    teardownDrag?.();
  });

  // ------------------------------------------------------------------
  // Toolbar actions
  // ------------------------------------------------------------------

  const structural = (ops, opts = {}) => {
    const a = selection();
    if (!a) return;
    commitOps(a.file, ops, opts);
  };
  /** Toolbar move: swap with the adjacent same-file sibling in place
      (local commit, anchors patched, selection kept — `move_to` relative
      to the DOM neighbour, correct across invisible AST siblings); shapes,
      shared anchors and edge positions fall back to the full loop. */
  const moveSel = (dir) => {
    const a = selection();
    if (!a) return;
    const d = doc();
    const sib =
      d && !a.shape && !a.shared ? adjacentSameFileSibling(d, a.el, dir) : null;
    if (!sib) {
      structural([{ op: 'move', span: a.span, dir }], { reveal: 'edited' });
      return;
    }
    const ops = [
      dir === 'up'
        ? { op: 'move_to', span: a.span, before: spanOfEl(sib) }
        : { op: 'move_to', span: a.span, after: spanOfEl(sib) },
    ];
    const revert = dir === 'up' ? moveDomBlock(a.el, sib) : moveDomBlock(sib, a.el);
    decorate();
    commitOpsLocal(a.file, ops, {
      surface: props.preview?.id ?? null,
      onApplied(res) {
        patchAnchors(d, a.file, res.span_map ?? []);
        setSelection({ ...a, span: mappedSpan(res.span_map ?? [], a.span) ?? a.span });
        notePage(d);
        decorate();
      },
    }).then((res) => {
      if (!res.ok) {
        revert();
        decorate();
      }
    });
  };
  const deleteSel = () => structural([{ op: 'delete', span: selection().span }], { reveal: null });
  const changeKind = (kind) =>
    structural([{ op: 'set_kind', span: selection().span, kind }], { reveal: 'edited' });
  /** Drop the shape's explicit position — under a solver layout the solver
      takes over; under :free it returns to the kind's defaults. */
  const resetPosition = () =>
    structural(
      [
        { op: 'remove_field', span: selection().span, field: 'x' },
        { op: 'remove_field', span: selection().span, field: 'y' },
      ],
      { reveal: 'edited' },
    );
  const resetSize = () =>
    structural(
      [
        { op: 'remove_field', span: selection().span, field: 'width' },
        { op: 'remove_field', span: selection().span, field: 'height' },
      ],
      { reveal: 'edited' },
    );

  /** li indent/outdent: re-home the item's source under the previous
      sibling / after the parent item, in one atomic batch. */
  const reindent = async (indent) => {
    const a = selection();
    const d = doc();
    if (!a || a.kind !== 'li' || !d) return;
    const src = await api.blockSource({ file: a.file, span: a.span });
    if (!src.ok) return toast(src.error, { tone: 'danger', duration: 5000 });
    let ops = null;
    if (indent) {
      const prev = prevSiblingOfKind(a.el, 'li');
      const prevAnchor = prev && anchorOf(d, prev);
      if (!prevAnchor) return toast('No previous item to nest under', { duration: 3000 });
      ops = [
        { op: 'insert_child', span: prevAnchor.span, index: 9999, source: src.source },
        { op: 'delete', span: a.span },
      ];
    } else {
      const parentLi = closestOfKind(a.el.parentElement, 'li');
      const parentAnchor = parentLi && anchorOf(d, parentLi);
      if (!parentAnchor) return toast('Already at the top level', { duration: 3000 });
      ops = [
        { op: 'insert_after', span: parentAnchor.span, source: src.source },
        { op: 'delete', span: a.span },
      ];
    }
    commitOps(a.file, ops, { etag: src.etag, reveal: 'inserted' });
  };

  const marker = (open, close) => {
    const region = sessionRegion();
    const d = doc();
    if (!region || !d) return;
    wrapSelection(d, region.el, open, close ?? open);
    region.el.focus();
  };

  const isComponent = () => palette()?.components?.some((c) => c.name === selection()?.kind);

  return (
    <div class="ed-design-canvas" ref={wrapper}>
      <Show
        when={src()}
        fallback={props.fallback ?? <div class="ed-empty">Building the design canvas…</div>}
      >
        <iframe ref={iframe} src={src()} title="design canvas" onLoad={onFrameLoad} />
      </Show>

      <Show when={toolbarPos() && !busy()}>
        <div
          class="ed-design-toolbar"
          style={{
            left: `${toolbarPos().left}px`,
            top: `${toolbarPos().below ? toolbarPos().bottom + 6 : toolbarPos().top}px`,
          }}
        >
          <Show
            when={editingSession()}
            fallback={
              <>
                <span class="ed-design-kind">{selection()?.kind}</span>
                <Show when={selection()?.shared}>
                  <Badge tone="warning">edits affect all instances</Badge>
                </Show>
                <Show when={HEADING_KINDS.includes(selection()?.kind)}>
                  <Select
                    options={HEADING_KINDS.map((k) => ({ value: k, label: k }))}
                    value={selection()?.kind}
                    onChange={(k) => k !== selection()?.kind && changeKind(k)}
                  />
                </Show>
                <Show when={selection()?.kind === 'diagram'}>
                  <IconButton
                    icon={Shapes}
                    label="Add shape"
                    onClick={() => setPopover({ type: 'add-shape', anchor: selection() })}
                  />
                  <Select
                    options={DIAGRAM_LAYOUTS.map((k) => ({ value: k, label: `layout: ${k}` }))}
                    value={
                      DIAGRAM_LAYOUTS.includes(selection()?.layout) ? selection().layout : 'free'
                    }
                    onChange={(k) => k !== (selection()?.layout ?? 'free') && setDiagramLayout(k)}
                  />
                  <Show when={!isManualLayout(selection()?.layout)}>
                    <IconButton
                      icon={Hand}
                      label="Convert to manual layout (keep positions)"
                      onClick={convertToManual}
                    />
                  </Show>
                </Show>
                <Show when={selection()?.shape}>
                  <IconButton
                    icon={RotateCcw}
                    label="Reset position (let the layout place it)"
                    onClick={resetPosition}
                  />
                  <IconButton icon={Scaling} label="Reset size" onClick={resetSize} />
                </Show>
                <Show when={selection()?.kind === 'table'}>
                  <IconButton
                    icon={Table}
                    label="Open grid editor"
                    onClick={() => setPopover({ type: 'table', anchor: selection() })}
                  />
                </Show>
                <IconButton icon={ArrowUp} label="Move up" onClick={() => moveSel('up')} />
                <IconButton icon={ArrowDown} label="Move down" onClick={() => moveSel('down')} />
                <Show when={selection()?.kind === 'li'}>
                  <IconButton icon={IndentIncrease} label="Nest item" onClick={() => reindent(true)} />
                  <IconButton icon={IndentDecrease} label="Unnest item" onClick={() => reindent(false)} />
                </Show>
                <Show when={!selection()?.shape}>
                  <IconButton
                    icon={Plus}
                    label="Insert below"
                    onClick={() => setPopover({ type: 'insert', anchor: selection() })}
                  />
                </Show>
                <Show when={isComponent()}>
                  <IconButton
                    icon={Settings2}
                    label="Properties"
                    onClick={() => setPopover({ type: 'component', anchor: selection() })}
                  />
                </Show>
                <Show when={selected()?.wskill}>
                  <IconButton
                    icon={Eye}
                    label="Views (visibility)"
                    onClick={() => openVisibility(selection())}
                  />
                </Show>
                <IconButton
                  icon={Braces}
                  label="Edit as source"
                  onClick={() => setPopover({ type: 'fragment', anchor: selection() })}
                />
                <IconButton icon={Trash2} label="Delete block" onClick={deleteSel} />
              </>
            }
          >
            <span class="ed-design-kind">
              {editingSession()?.table ? 'cell — Enter saves, Esc cancels' : 'editing — Ctrl+Enter saves, Esc cancels'}
            </span>
            <Show when={!sessionRegion()?.plain}>
              <IconButton icon={Bold} label="Bold" onClick={() => marker('**')} />
              <IconButton icon={Italic} label="Italic" onClick={() => marker('_')} />
              <IconButton icon={Code} label="Inline code" onClick={() => marker('`')} />
              <IconButton icon={Link} label="Link" onClick={() => marker('[', '](page)')} />
            </Show>
            <Show when={editingSession()?.table}>
              <span class="ed-tbl-sep" />
              <button
                class="ed-tbl-op"
                title="Insert row above"
                onClick={() => tableStructural((g, at, cols) => insertRowAt(g, at.row - 1, cols))}
              >
                +row ↑
              </button>
              <button
                class="ed-tbl-op"
                title="Insert row below"
                onClick={() => tableStructural((g, at, cols) => insertRowAt(g, at.row, cols))}
              >
                +row ↓
              </button>
              <button
                class="ed-tbl-op"
                title="Delete this row"
                onClick={() =>
                  tableStructural((g, at) => (g.length > 1 ? g.filter((_, i) => i !== at.row) : null))
                }
              >
                −row
              </button>
              <button
                class="ed-tbl-op"
                title="Insert column left"
                onClick={() => tableStructural((g, at) => insertColAt(g, at.col - 1))}
              >
                +col ←
              </button>
              <button
                class="ed-tbl-op"
                title="Insert column right"
                onClick={() => tableStructural((g, at) => insertColAt(g, at.col))}
              >
                +col →
              </button>
              <button
                class="ed-tbl-op"
                title="Delete this column"
                onClick={() => tableStructural((g, at) => delColAt(g, at.col))}
              >
                −col
              </button>
            </Show>
          </Show>
        </div>
      </Show>

      <Show when={dockPos() && selection()?.shape && !busy()}>
        <div
          class="ed-shape-dock"
          style={{ left: `${dockPos().left}px`, top: `${dockPos().top}px` }}
        >
          <ShapePanel />
        </div>
      </Show>
    </div>
  );
}
