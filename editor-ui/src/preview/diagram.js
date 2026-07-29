/* Diagram-shape interaction layer for the Design-mode iframe: drag-to-move
   on ANY shape (no pre-selection needed — the 3px threshold keeps clicks
   and the drill-down selection intact) plus corner resize handles on the
   selected one. A move's release resolves WHERE it landed:

   - a top-level shape dropped on its own diagram (or over a sibling leaf)
     under a manual layout → the classic positional move: `onMove(el,
     {dx, dy})` in user units (the caller commits x/y);
   - anything else with a valid target (into a container widget, after a
     stacked sibling, out to the diagram) → `onRelocate(el, target,
     clientPoint)` — the caller re-homes the block structurally;
   - no valid outcome → the ghost snaps back, nothing commits.

   During a move the dragged element goes `pointer-events: none` (the
   connect-preview trick) so hit-testing sees what's BENEATH the ghost.

   Pointer handling is capture-phase and stops propagation, so the bundled
   pan-zoom player (bubble-phase listeners on the <svg>) never pans while a
   shape or handle is being dragged. */

import { markDropTarget, resolveWidgetDrop } from './widgetdnd';

const CSS_ID = 'wcl-diagram-css';
const FRAME_CSS = `
.wcl-wys-handle {
  fill: #3b82f6; stroke: #ffffff; stroke-width: 1;
  vector-effect: non-scaling-stroke; pointer-events: all;
}
.wcl-wys-handle[data-wcl-handle="nw"], .wcl-wys-handle[data-wcl-handle="se"] { cursor: nwse-resize; }
.wcl-wys-handle[data-wcl-handle="ne"], .wcl-wys-handle[data-wcl-handle="sw"] { cursor: nesw-resize; }
.wcl-wys-dragging, .wcl-wys-dragging * { cursor: grabbing !important; }
.wcl-wys-port {
  fill: #3b82f6; stroke: #ffffff; stroke-width: 1;
  vector-effect: non-scaling-stroke; pointer-events: all; cursor: crosshair;
}
.wcl-wys-port:hover { fill: #2563eb; r: 6; }
.wcl-wys-pending {
  stroke: #3b82f6; stroke-width: 2; stroke-dasharray: 4 3;
  vector-effect: non-scaling-stroke; pointer-events: none;
}
.wcl-wys-drop > * { outline: 2px solid #3b82f6; outline-offset: 2px; }
svg.wcl-wys-drop { outline: 2px dashed #3b82f6; outline-offset: -2px; }
svg.wcl-wys-drop > * { outline: none; }
.wcl-wys-drop-cell {
  fill: rgba(59, 130, 246, 0.18) !important;
  stroke: #3b82f6 !important; stroke-dasharray: none !important; opacity: 1 !important;
  outline: none !important;
}
`;

const SVG_NS = 'http://www.w3.org/2000/svg';
/** Layout modes whose shapes position by their own x/y (drag allowed). */
const MANUAL_LAYOUTS = ['free', 'none'];
/** Client-pixel movement below this is a click, not a drag. */
const DRAG_THRESHOLD = 3;

/** Inject the interaction stylesheet (idempotent). Exported for the widget
    drop layer, which shares the `.wcl-wys-drop` target highlight. */
export function injectCss(doc) {
  if (!doc?.head || doc.getElementById(CSS_ID)) return;
  const style = doc.createElement('style');
  style.id = CSS_ID;
  style.textContent = FRAME_CSS;
  doc.head.appendChild(style);
}

/** Parse a `translate(x y)` / `translate(x, y)` transform attribute. */
export function readTranslate(el) {
  const m = /translate\(\s*(-?[\d.eE+]+)[\s,]+(-?[\d.eE+]+)\s*\)/.exec(
    el?.getAttribute?.('transform') ?? '',
  );
  return m ? { x: Number(m[1]), y: Number(m[2]) } : { x: 0, y: 0 };
}

/** Map a client point into the svg's user coordinate space (accounts for
    the fitted viewBox and any pan/zoom). Falls back to a viewBox/rect scale
    when getScreenCTM is unavailable (non-rendering test DOMs). */
export function clientToUser(svg, x, y) {
  if (typeof svg.getScreenCTM === 'function') {
    const ctm = svg.getScreenCTM();
    if (ctm) {
      const inv = ctm.inverse();
      return { x: inv.a * x + inv.c * y + inv.e, y: inv.b * x + inv.d * y + inv.f };
    }
  }
  const rect = svg.getBoundingClientRect?.();
  const vb = (svg.getAttribute('viewBox') ?? '').split(/[\s,]+/).map(Number);
  if (rect?.width && rect?.height && vb.length === 4) {
    return {
      x: vb[0] + (x - rect.left) * (vb[2] / rect.width),
      y: vb[1] + (y - rect.top) * (vb[3] / rect.height),
    };
  }
  return { x, y };
}

/** Whether `shapeEl`'s diagram honors per-shape x/y (manual layout). */
export function isDraggable(shapeEl) {
  const layout = shapeEl?.closest?.('svg[data-wcl-layout]')?.getAttribute('data-wcl-layout');
  return MANUAL_LAYOUTS.includes(layout ?? '');
}

/** The geometry delta of a corner resize: pointer user-delta (du, dv) →
    { dx, dy, dw, dh } to apply to the shape's x/y/width/height. Pure. */
export function resizeDelta(corner, du, dv) {
  switch (corner) {
    case 'se':
      return { dx: 0, dy: 0, dw: du, dh: dv };
    case 'ne':
      return { dx: 0, dy: dv, dw: du, dh: -dv };
    case 'sw':
      return { dx: du, dy: 0, dw: -du, dh: dv };
    case 'nw':
      return { dx: du, dy: dv, dw: -du, dh: -dv };
    default:
      return { dx: 0, dy: 0, dw: 0, dh: 0 };
  }
}

/** The shape's content bbox: the measurement `markSelected` stashed before
    injecting selection chrome, else a live getBBox (which would include any
    injected chrome — hence the stash). */
function shapeBox(el) {
  if (el.__wclShapeBox) return el.__wclShapeBox;
  if (typeof el.getBBox !== 'function') return null;
  try {
    return el.getBBox();
  } catch {
    return null;
  }
}

/** Create / refresh the corner resize handles inside the selected shape's
    <g> (they ride its transform, including the live drag preview). Pass
    `el = null` to remove all handles. Under a solver layout only the `se`
    handle shows — a top/left resize shifts x/y, which the solver ignores. */
export function refreshShapeHandles(doc, el) {
  for (const g of doc.querySelectorAll('.wcl-wys-handles')) g.remove();
  if (!el || !el.hasAttribute?.('data-wcl-shape')) return;
  const box = shapeBox(el);
  if (!box) return;
  injectCss(doc);
  const corners = isDraggable(el)
    ? [
        ['nw', box.x, box.y],
        ['ne', box.x + box.width, box.y],
        ['sw', box.x, box.y + box.height],
        ['se', box.x + box.width, box.y + box.height],
      ]
    : [['se', box.x + box.width, box.y + box.height]];
  const group = doc.createElementNS(SVG_NS, 'g');
  group.setAttribute('class', 'wcl-wys-handles');
  const size = 7;
  for (const [corner, cx, cy] of corners) {
    const r = doc.createElementNS(SVG_NS, 'rect');
    r.setAttribute('class', 'wcl-wys-handle');
    r.setAttribute('data-wcl-handle', corner);
    r.setAttribute('x', cx - size / 2);
    r.setAttribute('y', cy - size / 2);
    r.setAttribute('width', size);
    r.setAttribute('height', size);
    group.appendChild(r);
  }
  // The out-port: drag it onto another shape to wire `a -> b`. Only shapes
  // that HAVE an id can be an edge endpoint (a connection names ids).
  if (el.getAttribute('data-wcl-shape-id')) {
    const port = doc.createElementNS(SVG_NS, 'circle');
    port.setAttribute('class', 'wcl-wys-port');
    port.setAttribute('data-wcl-port', '');
    port.setAttribute('cx', box.x + box.width);
    port.setAttribute('cy', box.y + box.height / 2);
    port.setAttribute('r', 5);
    const t = doc.createElementNS(SVG_NS, 'title');
    t.textContent = 'drag to another shape to connect';
    port.appendChild(t);
    group.appendChild(port);
  }
  el.appendChild(group);
}

/** The shape under a client point, excluding `exclude`. The diagram is
    server-rendered HTML with no client-side geometry model (unlike the unit
    graph), so hit-testing goes through the document. */
export function shapeAt(doc, x, y, exclude) {
  const hit = doc.elementFromPoint?.(x, y);
  const shape = hit?.closest?.('[data-wcl-shape][data-wcl-shape-id]');
  return shape && shape !== exclude ? shape : null;
}

/** Install shape drag + resize on the iframe document. `handlers`:
    - enabled()            — gate (false while committing/rebuilding)
    - selectedShape()      — the currently selected shape element (or null)
    - onMove(el, d)        — positional release: d = { dx, dy } user units
    - onResize(el, d)      — handle released: d = { dx, dy, dw, dh }
    - onConnect(from, to)  — port dropped on another shape
    - acceptsChildren(kind) — may a shape of `kind` nest children?
    - onRelocate(el, target, point) — structural release: `target` from
      resolveWidgetDrop, `point` iframe client coords
    Returns a teardown. Idempotent per document. */
export function installShapeDrag(doc, handlers) {
  if (!doc || doc.__wclShapeDragWired) return () => {};
  doc.__wclShapeDragWired = true;
  injectCss(doc);

  /* One gesture at a time: null | { kind: 'move'|'resize'|'connect', el,
     svg, corner?, base (translate), origTransform, startClient, moved,
     lastUser } */
  let gesture = null;
  let justDragged = false;

  const down = (e) => {
    if (!handlers.enabled() || e.button !== 0) return;
    const selected = handlers.selectedShape?.();
    const port = e.target.closest?.('.wcl-wys-port');
    const handle = e.target.closest?.('.wcl-wys-handle');
    const shape = e.target.closest?.('[data-wcl-shape]');
    if (port && selected?.contains(port)) {
      gesture = {
        kind: 'connect',
        el: selected,
        svg: selected.closest('svg'),
        startClient: { x: e.clientX, y: e.clientY },
        moved: false,
      };
    } else if (handle && selected?.contains(handle)) {
      gesture = {
        kind: 'resize',
        el: selected,
        svg: selected.closest('svg'),
        corner: handle.getAttribute('data-wcl-handle'),
        startClient: { x: e.clientX, y: e.clientY },
        moved: false,
      };
    } else if (shape) {
      // ANY shape drags directly — the nearest anchor, the same one a
      // click would select. Whether the release is positional, structural,
      // or a snap-back is decided at `up`.
      gesture = {
        kind: 'move',
        el: shape,
        svg: shape.closest('svg'),
        base: readTranslate(shape),
        origTransform: shape.getAttribute('transform'),
        startClient: { x: e.clientX, y: e.clientY },
        moved: false,
      };
    } else {
      return;
    }
    if (!gesture.svg) {
      gesture = null;
      return;
    }
    // Keep the pan-zoom player (bubble-phase svg listeners) from panning,
    // and stop the browser from starting a text selection.
    e.stopPropagation();
    e.preventDefault();
  };

  const userDelta = (g, e) => {
    const a = clientToUser(g.svg, g.startClient.x, g.startClient.y);
    const b = clientToUser(g.svg, e.clientX, e.clientY);
    return { du: b.x - a.x, dv: b.y - a.y };
  };

  const accepts = (kind) => handlers.acceptsChildren?.(kind) ?? false;
  /** The would-be drop target under the cursor, excluding the dragged
      subtree (its pointer-events are off during the drag, so the hit
      already sees beneath it — the exclude is belt and braces). */
  const dropTarget = (g, e) =>
    resolveWidgetDrop(doc.elementFromPoint?.(e.clientX, e.clientY), accepts, g.el);
  /** Is a resolved target the classic positional case (commit x/y) rather
      than a structural re-home? Top-level shape, its own manual-layout
      diagram — a drop on the background or over another TOP-LEVEL leaf both
      count (free-position shapes overlap freely). A NESTED target leaf is
      an ordering intent: insert after it, inside its container. */
  const isPositional = (g, target) => {
    if (!target) return false;
    const nested = g.el.parentElement?.closest?.('[data-wcl-shape]');
    if (nested || !isDraggable(g.el)) return false;
    if (target.mode === 'diagram') return target.el === g.el.closest('svg[data-wcl-layout]');
    return (
      target.mode === 'after' &&
      target.el.closest('svg') === g.svg &&
      !target.el.parentElement?.closest?.('[data-wcl-shape]')
    );
  };

  const markDrop = (el, cellEl = null) => markDropTarget(doc, el, cellEl);

  const move = (e) => {
    if (!gesture) return;
    const dx = e.clientX - gesture.startClient.x;
    const dy = e.clientY - gesture.startClient.y;
    if (!gesture.moved && Math.abs(dx) < DRAG_THRESHOLD && Math.abs(dy) < DRAG_THRESHOLD) return;
    if (!gesture.moved) {
      gesture.moved = true;
      doc.documentElement.classList.add('wcl-wys-dragging');
      if (gesture.kind === 'move') gesture.el.style.pointerEvents = 'none';
    }
    e.stopPropagation();
    e.preventDefault();
    const { du, dv } = userDelta(gesture, e);
    gesture.lastUser = { du, dv };
    if (gesture.kind === 'connect') {
      previewConnect(doc, gesture, e);
    } else if (gesture.kind === 'move') {
      // Live preview: the wrapper <g> position is purely visual and is
      // discarded when the commit rebuilds the page (or snapped back).
      gesture.el.setAttribute(
        'transform',
        `translate(${gesture.base.x + du} ${gesture.base.y + dv})`,
      );
      // Highlight where a release would re-home the widget; positional
      // moves get no highlight (nothing changes structurally). A resolved
      // layout slot (grid cell / row gap) lights up individually.
      const target = dropTarget(gesture, e);
      const structural = target && !isPositional(gesture, target);
      markDrop(structural ? target.el : null, structural ? (target.cellEl ?? null) : null);
    } else {
      previewResize(gesture, du, dv);
    }
  };

  /** Put the dragged element back exactly as it was rendered. */
  const snapBack = (g) => {
    if (g.origTransform == null) g.el.removeAttribute('transform');
    else g.el.setAttribute('transform', g.origTransform);
  };

  const up = (e) => {
    if (!gesture) return;
    const g = gesture;
    gesture = null;
    doc.documentElement.classList.remove('wcl-wys-dragging');
    markDrop(null);
    if (!g.moved) return;
    e.stopPropagation();
    e.preventDefault();
    // The compatibility click after a real drag must not re-drill/deselect.
    justDragged = true;
    if (g.kind === 'connect') {
      clearConnectPreview(doc);
      const target = shapeAt(doc, e.clientX, e.clientY, g.el);
      if (target) handlers.onConnect?.(g.el, target);
      return;
    }
    const { du, dv } = g.lastUser ?? userDelta(g, e);
    if (g.kind === 'resize') {
      handlers.onResize?.(g.el, resizeDelta(g.corner, du, dv));
      return;
    }
    // Resolve the drop while the ghost is STILL hit-transparent — restoring
    // pointer-events first would make elementFromPoint hit the dragged
    // widget itself and every drop degrade to a positional move.
    const target = dropTarget(g, e);
    g.el.style.pointerEvents = '';
    if (isPositional(g, target)) {
      handlers.onMove?.(g.el, { dx: du, dy: dv });
      return;
    }
    // Structural (or refused): the ghost snaps back either way — a commit
    // re-renders the page, and a refusal must leave the DOM untouched.
    snapBack(g);
    if (target && handlers.onRelocate) {
      handlers.onRelocate(g.el, target, { x: e.clientX, y: e.clientY });
    }
  };

  const suppressClick = (e) => {
    if (!justDragged) return;
    justDragged = false;
    e.stopPropagation();
    e.preventDefault();
  };

  doc.addEventListener('pointerdown', down, true);
  doc.addEventListener('pointermove', move, true);
  doc.addEventListener('pointerup', up, true);
  doc.addEventListener('click', suppressClick, true);
  return () => {
    doc.removeEventListener('pointerdown', down, true);
    doc.removeEventListener('pointermove', move, true);
    doc.removeEventListener('pointerup', up, true);
    doc.removeEventListener('click', suppressClick, true);
    delete doc.__wclShapeDragWired;
  };
}

/** Live connect preview: a dashed line from the port to the cursor, plus a
    ring on the shape under it. Injected imperatively — the diagram is
    server-rendered markup, not a component we can re-render. */
function previewConnect(doc, g, e) {
  const box = shapeBox(g.el);
  if (!box) return;
  const base = readTranslate(g.el);
  const from = { x: base.x + box.x + box.width, y: base.y + box.y + box.height / 2 };
  const to = clientToUser(g.svg, e.clientX, e.clientY);
  let line = g.svg.querySelector('.wcl-wys-pending');
  if (!line) {
    line = doc.createElementNS(SVG_NS, 'line');
    line.setAttribute('class', 'wcl-wys-pending');
    g.svg.appendChild(line);
  }
  line.setAttribute('x1', from.x);
  line.setAttribute('y1', from.y);
  line.setAttribute('x2', to.x);
  line.setAttribute('y2', to.y);

  for (const el of doc.querySelectorAll('.wcl-wys-drop')) el.classList.remove('wcl-wys-drop');
  // The line follows the cursor, so it would hit-test as the topmost element.
  line.style.pointerEvents = 'none';
  shapeAt(doc, e.clientX, e.clientY, g.el)?.classList.add('wcl-wys-drop');
}

function clearConnectPreview(doc) {
  for (const el of doc.querySelectorAll('.wcl-wys-pending')) el.remove();
  for (const el of doc.querySelectorAll('.wcl-wys-drop')) el.classList.remove('wcl-wys-drop');
}

/** Live resize preview: stretch the selection box + reposition the handles
    to the prospective bbox (the shape itself re-renders on commit). */
function previewResize(g, du, dv) {
  const box = shapeBox(g.el);
  if (!box) return;
  const d = resizeDelta(g.corner, du, dv);
  const pad = 3;
  const sel = g.el.querySelector?.(':scope > .wcl-wys-sel-box') ?? g.el.querySelector?.('.wcl-wys-sel-box');
  const x = box.x + d.dx;
  const y = box.y + d.dy;
  const w = Math.max(box.width + d.dw, 8);
  const h = Math.max(box.height + d.dh, 8);
  if (sel) {
    sel.setAttribute('x', x - pad);
    sel.setAttribute('y', y - pad);
    sel.setAttribute('width', w + 2 * pad);
    sel.setAttribute('height', h + 2 * pad);
  }
  const size = 7;
  const at = { nw: [x, y], ne: [x + w, y], sw: [x, y + h], se: [x + w, y + h] };
  for (const handle of g.el.querySelectorAll('.wcl-wys-handle')) {
    const [cx, cy] = at[handle.getAttribute('data-wcl-handle')] ?? [];
    if (cx === undefined) continue;
    handle.setAttribute('x', cx - size / 2);
    handle.setAttribute('y', cy - size / 2);
  }
}
