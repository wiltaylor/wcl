/* Shared graph-mode state: the /api/graph payload and the index-panel
   coordination between GraphView (owns the fetch + simulation) and
   IndexPanel (the left bar on the graph tab). Module-level signals so the
   two components stay decoupled — GraphView registers its reload function
   here, the panel registers its drop-target element. */

import { createSignal } from 'solid-js';

const [graphData, setGraphData] = createSignal(null);
/** The index selected in the panel (node key like "index:overview"). */
const [selectedIndex, setSelectedIndex] = createSignal(null);
/** Set while a graph node is being dragged: { key, type, id } — the panel
    uses it to show its drop affordance. */
const [nodeDragging, setNodeDragging] = createSignal(null);

export {
  graphData,
  setGraphData,
  selectedIndex,
  setSelectedIndex,
  nodeDragging,
  setNodeDragging,
};

/** The selected index's node from the current payload. */
export function selectedIndexNode() {
  const key = selectedIndex();
  return key ? (graphData()?.nodes.find((n) => n.key === key) ?? null) : null;
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
