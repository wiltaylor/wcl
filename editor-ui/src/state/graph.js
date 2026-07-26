/* Shared graph-mode state: the /api/graph payload and the index-panel
   coordination between GraphView (owns the fetch + simulation) and
   IndexPanel (the left bar on the graph tab). Module-level signals so the
   two components stay decoupled — GraphView registers its reload function
   here, the panel registers its drop-target element. */

import { createSignal } from 'solid-js';

const [graphData, setGraphData] = createSignal(null);
/** The index selected in the panel (node key like "index:overview"). */
const [selectedIndex, setSelectedIndex] = createSignal(null);
/** The index auto-expanded by graph focus (node key) — panel expansion
    only, distinct from selection so revealing where a focused unit lives
    never scopes/fades the graph. */
const [revealedIndex, setRevealedIndex] = createSignal(null);
/** Set while a graph node is being dragged: { key, type, id } — the panel
    uses it to show its drop affordance. */
const [nodeDragging, setNodeDragging] = createSignal(null);
/** The focused graph node's key — shared so the index panel can offer
    per-section pin buttons for it. */
const [focusedNode, setFocusedNode] = createSignal(null);
/** The node whose content modal is open (key), if any. Shared because both
    the canvas (a node click) and the index panel (a row double-click) open
    it, while GraphView owns the rendering. */
const [contentFor, setContentFor] = createSignal(null);

export {
  graphData,
  setGraphData,
  selectedIndex,
  setSelectedIndex,
  revealedIndex,
  setRevealedIndex,
  nodeDragging,
  setNodeDragging,
  focusedNode,
  setFocusedNode,
  contentFor,
  setContentFor,
};

/** The selected index's node from the current payload. */
export function selectedIndexNode() {
  const key = selectedIndex();
  return key ? (graphData()?.nodes.find((n) => n.key === key) ?? null) : null;
}

/** Per-unit index-membership counts from a graph payload: total pin edges
    plus a per-site breakdown where an index hidden in a view doesn't count
    toward it — the node corner badges and the content modal's per-view
    "not indexed" indicator. Returns { [nodeKey]: { total, sites } }.
    Sub-index pins ride their top-level index's edges, so they inherit its
    per-view visibility (accepted approximation).

    A node's `organized` sites (the training syllabus for lessons/modules,
    the deck for its presentation unit — navigation built FROM the unit
    data, never index-pinned) count as membership in those sites, so
    structurally-placed units don't read as orphans. */
export function pinCounts(data) {
  if (!data) return {};
  const byKey = Object.fromEntries(data.nodes.map((n) => [n.key, n]));
  const out = {};
  for (const e of data.edges) {
    if (e.kind !== 'pin') continue;
    const idx = byKey[e.from];
    const rec = (out[e.to] ??= { total: 0, sites: {} });
    rec.total += 1;
    for (const s of data.sites) {
      if (idx?.views?.[s] !== false) rec.sites[s] = (rec.sites[s] ?? 0) + 1;
    }
  }
  for (const n of data.nodes) {
    for (const s of n.organized ?? []) {
      const rec = (out[n.key] ??= { total: 0, sites: {} });
      rec.total += 1;
      rec.sites[s] = (rec.sites[s] ?? 0) + 1;
    }
  }
  return out;
}

/** Every pinned unit id across an index node and its nested sub-indexes
    (the `children` tree in the graph payload). */
export function subtreePinnedIds(indexNode) {
  const out = new Set();
  const walk = (level) => {
    for (const id of level?.pinned ?? []) out.add(id);
    for (const c of level?.children ?? []) walk(c);
  };
  walk(indexNode);
  return out;
}

/** Every index level whose own `related` list pins the unit, as
    `{ topKey, indexId }` — `topKey` is the top-level index node's key (the
    panel accordion entry), `indexId` the owning level (a sub-index id for
    nested pins). Order follows the payload's node/children order. */
export function indexHitsForUnit(data, unitId) {
  const hits = [];
  for (const n of data?.nodes ?? []) {
    if (n.type !== 'index') continue;
    const walk = (level) => {
      if ((level.pinned ?? []).includes(unitId)) {
        hits.push({ topKey: n.key, indexId: level.id });
      }
      for (const c of level.children ?? []) walk(c);
    };
    walk(n);
  }
  return hits;
}

/** The viewBox origin that brings a node fully into view, or null when it
    already is. Pans only — never zooms — so the reader's scale is preserved.
    `vb` is {x, y, w, h}, `box` the node's {w, h}, `p` its top-left. */
export function panToInclude(vb, box, p, margin = 40) {
  let { x, y } = vb;
  if (p.x - margin < x) x = p.x - margin;
  else if (p.x + box.w + margin > x + vb.w) x = p.x + box.w + margin - vb.w;
  if (p.y - margin < y) y = p.y - margin;
  else if (p.y + box.h + margin > y + vb.h) y = p.y + box.h + margin - vb.h;
  return x === vb.x && y === vb.y ? null : { ...vb, x, y };
}

/** The focused node when it is a unit (pinnable into sections), else null. */
export function focusedUnitNode() {
  const key = focusedNode();
  const n = key ? (graphData()?.nodes.find((n) => n.key === key) ?? null) : null;
  return n?.type === 'unit' ? n : null;
}

let reloadFn = null;
export const setGraphReload = (fn) => {
  reloadFn = fn;
};
export const reloadGraph = (opts) => reloadFn?.(opts);

let panelEl = null;
export const setIndexPanelEl = (el) => {
  panelEl = el;
};
/** Is a client point inside the index panel? (GraphView's drop test.) */
export const indexPanelAt = (x, y) => {
  if (!panelEl || !panelEl.isConnected) return false;
  const r = panelEl.getBoundingClientRect();
  return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
};
