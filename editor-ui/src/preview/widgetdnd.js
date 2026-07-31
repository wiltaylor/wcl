/* Widget drag-and-drop support for the wireframe editor: the SHARED drop
   resolution (palette inserts, canvas moves and tree moves all use it), the
   target-highlight manager, the structural-move op builder, and the DOM
   walk that turns the rendered frame into the structure tree.

   Drop semantics, resolved from the point (or row) under the cursor:
   - a LEAF widget      → after it (ordering inside a stacked container)
   - a CONTAINER widget → append inside it
   - the diagram <svg>  → into the diagram (the caller decides coordinates)
   - anything else      → null (cancelled)

   The gestures themselves are POINTER-based (surfaces/pointerDrag.js) —
   native HTML5 drag from the parent document into the iframe proved
   unreliable in real browsers and impossible to drive from tests. */

import {
  SEL,
  closestMatching,
  diagramOf,
  kindOf,
  shapeChildren,
  shapeIdOf,
  slotOf,
  slotZoneIn,
} from './anchors';
import { injectCss } from './diagram';

/**
 * Resolve a drop at `hitEl` (the caller's `doc.elementFromPoint` result —
 * passed in so tests need no layout engine). `acceptsChildren(kind)` says
 * whether a widget kind nests children (the palette's schema-derived
 * `accepts_children`). `exclude` is the element being MOVED, when the drag
 * is a move: anchors on or inside it are skipped (a widget can't be
 * dropped into itself), resolution continuing outward.
 *
 * An `inside` resolution also reads the layout-container guide zone under
 * the cursor (the renderer's edit-mode `data-wf-slot` cells / gap strips):
 * `slot` is the insertion index the drop targets — a grid cell, a row /
 * column gap — and `cellEl` the zone element (for the highlight). Both are
 * null on a plain container hit (append).
 *
 * @returns {{ mode: 'after'|'inside'|'diagram', el: Element,
 *             slot?: number|null, cellEl?: Element|null } | null}
 */
export function resolveWidgetDrop(hitEl, acceptsChildren, exclude = null) {
  if (!hitEl) return null;
  // The same nearest-anchor walk selection uses, skipping the subtree being
  // moved instead of template chrome — so what can be dropped onto matches
  // what can be selected.
  const el = closestMatching(
    hitEl,
    SEL.shape,
    (c) => !!exclude && (c === exclude || exclude.contains(c)),
  );
  if (el) {
    if (!acceptsChildren(kindOf(el) ?? '')) return { mode: 'after', el };
    const cell = slotZoneIn(hitEl, el);
    return { mode: 'inside', el, slot: cell ? slotOf(cell) : null, cellEl: cell };
  }
  const svg = diagramOf(hitEl);
  if (svg && (!exclude || !exclude.contains(svg))) return { mode: 'diagram', el: svg };
  return null;
}

/**
 * Highlight `el` as the live drop target with the diagram layer's
 * `.wcl-wys-drop` class, clearing any previous mark. `null` clears all.
 * `cellEl` (a resolved `data-wf-slot` zone) additionally gets the
 * `.wcl-wys-drop-cell` fill so the targeted grid cell / gap lights up.
 */
export function markDropTarget(doc, el, cellEl = null) {
  if (!doc) return;
  injectCss(doc);
  for (const m of doc.querySelectorAll('.wcl-wys-drop')) {
    if (m !== el) m.classList.remove('wcl-wys-drop');
  }
  el?.classList?.add('wcl-wys-drop');
  for (const m of doc.querySelectorAll('.wcl-wys-drop-cell')) {
    if (m !== cellEl) m.classList.remove('wcl-wys-drop-cell');
  }
  cellEl?.classList?.add('wcl-wys-drop-cell');
}

/**
 * Rewrite a widget's canonical source slice to sit at `{x, y}` (user
 * units): existing `x = …` / `y = …` field lines are replaced, missing
 * ones inserted after the opening brace (added when the block printed
 * braceless). Canonical formatting makes the line surgery safe — the
 * commit reparses and reformats anyway.
 */
export function placeSliceAt(source, { x, y }) {
  const fmt = (v) => {
    const r = Math.round(v * 10) / 10;
    return Number.isInteger(r) ? `${r}.0` : String(r);
  };
  let out = String(source ?? '');
  if (!/\{/.test(out)) out = `${out.trimEnd()} {\n}`;
  for (const [name, v] of [
    ['y', y],
    ['x', x],
  ]) {
    const re = new RegExp(`^(\\s*)${name} = .*$`, 'm');
    if (re.test(out)) out = out.replace(re, `$1${name} = ${fmt(v)}`);
    else out = out.replace('{', `{\n  ${name} = ${fmt(v)}`);
  }
  return out;
}

/**
 * The ops of one structural widget move: insert the widget's canonical
 * `slice` at the resolved target (after a leaf / inside a container or the
 * diagram), delete the original at `sourceSpan` — one atomic batch on one
 * parse. `at` (user units) rewrites the slice's position for manual-layout
 * diagram drops; `slot` (a resolved `data-wf-slot` index) inserts at that
 * position instead of appending — the batch mutates one pre-edit AST, so
 * insert-at-index composes with the span-addressed delete even for a move
 * within the same container. Shared by the canvas gesture and the tree rows.
 */
export function relocateOps({ slice, mode, targetSpan, sourceSpan, at = null, slot = null }) {
  const source = at ? placeSliceAt(slice, at) : slice;
  const insert =
    mode === 'after'
      ? { op: 'insert_after', span: targetSpan, source }
      : { op: 'insert_child', span: targetSpan, index: slot ?? 9999, source };
  return [insert, { op: 'delete', span: sourceSpan }];
}

/**
 * The rendered frame's widget structure, walked from the anchored shapes:
 * one root per diagram (`kind: 'diagram'`), children its DIRECT-descendant
 * shape anchors (nested anchors recurse).
 * Labels prefer the shape id, then the widget's first rendered text line,
 * then the bare kind. Everything keeps its live element so callers can
 * select/scroll/drag without another lookup.
 */
export function widgetTreeFrom(doc) {
  if (!doc) return [];
  const nodeFor = (el) => ({
    kind: kindOf(el) ?? 'widget',
    el,
    shapeId: shapeIdOf(el),
    label: labelFor(el),
    children: shapeChildren(el).map(nodeFor),
  });
  return [...doc.querySelectorAll(SEL.diagram)].map((svg) => ({
    kind: 'diagram',
    el: svg,
    shapeId: null,
    label: 'wireframe',
    children: shapeChildren(svg).map(nodeFor),
  }));
}

/** A display label for a widget anchor: its own first rendered text line
    (not a descendant widget's — and not the layout-guide kind tag), trimmed
    and capped. */
function labelFor(el) {
  for (const t of el.querySelectorAll('text')) {
    if (t.closest(SEL.shape) !== el) continue;
    if (t.closest(SEL.guide)) continue;
    const s = t.textContent?.trim();
    if (s) return s.length > 24 ? `${s.slice(0, 24)}…` : s;
  }
  return null;
}
