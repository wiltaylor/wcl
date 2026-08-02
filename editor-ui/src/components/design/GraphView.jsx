/* The interactive unit graph: every wskill unit (and index) as a
   force-laid-out SVG node, edges for `related` links and index pins, and
   per-view visibility at a glance — chips on each node, and an expandable
   side panel per unit listing its content blocks with per-view toggles
   (writes = the same `set_visibility` op the canvas toolbar uses, minus
   the iframe rebuild; the canvas refreshes when shown again). Pan by
   dragging the background, zoom with the wheel. The server's deterministic
   layout seeds the positions; from there a client-side force simulation
   (forceSim.js, the same solver ported) takes over — dragging a node pins
   it to the cursor while the rest of the graph re-simulates live, and the
   Reload button re-derives the layout from scratch. Edges anchor at ports:
   drag from a node's out-port onto another node to write a `related` /
   pin entry (`related_add`), grab an edge near its arrowhead and drop it
   on empty space (`related_remove`) or another node (remove + add, one
   batch) — every write commits immediately and refetches, keeping the
   current positions. */

import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import {
  FileCode2,
  FilePlus,
  Layers,
  ListFilter,
  RefreshCw,
  SlidersHorizontal,
  Sparkles,
} from 'lucide-solid';
import {
  Button,
  IconButton,
  Popover,
  Slider,
  Spinner,
  ToggleGroup,
  toast,
} from '@forge/ui';

import { api } from '../../api';
import { openFile } from '../../state/buffers';
import { revealSpan } from '../../state/views';
import { activeEntry, activeView, selectView, selected } from '../../state/sites';
import { targetHasPage } from '../../state/preview';
import {
  busy,
  commitNavOpQuiet,
  commitOpsQuiet,
  commitUnitCreateQuiet,
  curatorRunning,
  designTab,
  designTabOptions,
  exitDesign,
  loadNav,
  loadPalette,
  runCurator,
  setDesignTab,
  setGotoPage,
  setPopover,
} from '../../state/design';
import {
  graphData,
  setGraphData,
  selectedIndex,
  setSelectedIndex,
  selectedIndexNode,
  setNodeDragging,
  setGraphReload,
  indexPanelAt,
  panToInclude,
  focusedNode,
  setFocusedNode,
  contentFor,
  setContentFor,
  pinCounts as pinCountsOf,
  subtreePinnedIds,
} from '../../state/graph';
import AddUnitDialog from './AddUnitDialog';
import ContentModal from './ContentModal';
import UnitSearch from './UnitSearch';
import { DEFAULT_PARAMS, createSimulation } from './forceSim';

const KIND_COLORS = {
  concept: '#3b82f6',
  entity: '#10b981',
  fact: '#f59e0b',
  procedure: '#8b5cf6',
  research: '#ec4899',
  index: '#6b7280',
};

// Tuned force settings persist per-browser so a good set of values sticks
// across sessions (positions themselves stay ephemeral). The key is
// versioned: bump it whenever DEFAULT_PARAMS change, or stale saves win
// the merge and nobody sees the new defaults. v3 = the spacious tuning.
const PARAMS_KEY = 'wcl.graph.force.v3';
const loadSimParams = () => {
  try {
    return { ...DEFAULT_PARAMS, ...JSON.parse(localStorage.getItem(PARAMS_KEY) ?? '{}') };
  } catch {
    return { ...DEFAULT_PARAMS };
  }
};

export default function GraphView() {
  const data = graphData;
  const setData = setGraphData;
  const [loading, setLoading] = createSignal(false);
  // Focus is shared state (state/graph.js) so the index panel can offer
  // per-section pin buttons for the focused node.
  const focus = focusedNode;
  const setFocus = setFocusedNode;
  const [addOpen, setAddOpen] = createSignal(false);
  const [viewBox, setViewBox] = createSignal(null); // {x, y, w, h}
  // Client-owned node positions (top-left, world coords) — seeded from the
  // server layout, then driven by the simulation.
  const [positions, setPositions] = createStore({});
  // A pending connect/rewire drag: { from } (node key) — drives the
  // dashed preview edge and the drop-target ring.
  const [linking, setLinking] = createSignal(null);
  const [cursor, setCursor] = createSignal(null); // world coords
  const [dropTarget, setDropTarget] = createSignal(null); // node key
  // The edge being rewired stays hidden until its commit lands (or the
  // drag cancels): index into data().edges.
  const [hiddenEdge, setHiddenEdge] = createSignal(null);
  // Filters fade (never remove) non-matching nodes: `viewFilter` = the
  // views a node must be visible in; `indexFilter` = null | 'in' | 'out'
  // (whether a unit is pinned by any index).
  const [viewFilter, setViewFilter] = createSignal(new Set());
  const [indexFilter, setIndexFilter] = createSignal(null);

  const sites = () =>
    (selected()?.views ?? []).map((v) => v.site).filter(Boolean);
  const siteKinds = () =>
    (selected()?.views ?? []).filter((v) => v.site).map((v) => `${v.site}=${v.kind}`);

  // ---- simulation loop ---------------------------------------------
  const [simParams, setSimParams] = createSignal(loadSimParams());
  let sim = createSimulation(simParams());
  let raf = null;
  const applyPositions = () =>
    setPositions(
      produce((p) => {
        const live = sim.positions();
        for (const k of Object.keys(p)) if (!live.has(k)) delete p[k];
        for (const [k, v] of live) p[k] = v;
      }),
    );
  const step = () => {
    const alive = sim.tick();
    applyPositions();
    raf = alive ? requestAnimationFrame(step) : null;
  };
  const startLoop = () => {
    if (!raf) raf = requestAnimationFrame(step);
  };
  onCleanup(() => {
    if (raf) cancelAnimationFrame(raf);
  });

  const load = async ({ keepPositions = false } = {}) => {
    const entry = activeEntry();
    if (!entry) return;
    setLoading(true);
    const res = await api.graph(entry, sites(), siteKinds());
    setLoading(false);
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      return;
    }
    // Order matters: setData re-renders the <Show> children synchronously,
    // and the svg's viewBox attribute reads viewBox() — it must exist first.
    // Units only — index nodes never render or simulate (see visNodes).
    const simNodes = res.nodes.filter((n) => n.type !== 'index');
    const simEdges = res.edges.filter((e) => e.kind !== 'pin');
    if (!viewBox()) {
      const w = Math.max(600, ...simNodes.map((n) => n.x + n.w)) + 60;
      const h = Math.max(400, ...simNodes.map((n) => n.y + n.h)) + 60;
      setViewBox({ x: -30, y: -30, w, h });
    }
    // A plain load resets to the server's deterministic layout; a
    // keep-positions reload (after any commit — spans shift on every
    // reformat) retains the simulated positions and gently absorbs the
    // changed springs.
    if (!keepPositions) {
      // Fresh layout: seed from the server offsets, then settle the sim
      // SILENTLY to its own equilibrium before showing anything — the
      // server solver freezes mid-relaxation, and without this the first
      // drag would visibly re-relax the whole graph instead of only
      // disturbing the neighborhood.
      sim = createSimulation(simParams());
      sim.setGraph(simNodes, simEdges);
      sim.reheat();
      for (let i = 0; i < 600 && sim.tick(); i++);
    } else {
      sim.setGraph(simNodes, simEdges);
    }
    applyPositions();
    setData(res);
    setHiddenEdge(null);
    if (selectedIndex() && !res.nodes.some((n) => n.key === selectedIndex())) {
      setSelectedIndex(null);
    }
    if (keepPositions) {
      sim.reheat(40);
      startLoop();
    }
  };
  onMount(() => setGraphReload(load));

  // The graph is per-wskill: picking another one in the topbar must reload
  // it in place (it used to need a Design-mode round trip). Keyed on the
  // wskill, not on `activeEntry()`, so switching VIEW tabs — every view has
  // its own entry over the same data — keeps the layout and the selection.
  let loadedKey = null;
  createEffect(() => {
    // Only a wskill has a unit graph; picking a plain site drops back to
    // the canvas rather than leaving another document's graph on screen.
    if (selected() && !selected().wskill) {
      setDesignTab('canvas');
      return;
    }
    const key = selected()?.registry ?? selected()?.root ?? activeEntry();
    if (!key || key === loadedKey) return;
    loadedKey = key;
    // Nothing from the previous wskill survives: its ids, pins and
    // positions all belong to another document.
    setSelectedIndex(null);
    setFocus(null);
    setContentFor(null);
    setData(null);
    setViewBox(null);
    load();
  });

  /** Apply a force-setting change live: the sim reheats so the graph
      visibly reorganizes under the new values. */
  const tweak = (patch) => {
    const next = { ...simParams(), ...patch };
    setSimParams(next);
    try {
      localStorage.setItem(PARAMS_KEY, JSON.stringify(next));
    } catch {
      /* private mode — settings just don't persist */
    }
    sim.setParams(next);
    sim.reheat();
    startLoop();
  };

  const node = (key) => data()?.nodes.find((n) => n.key === key);
  const pos = (n) => positions[n.key] ?? { x: n.x, y: n.y };

  // Index nodes are TOC machinery (managed in the index panel), not pages
  // to edit — the graph renders and simulates UNITS only, so pin edges
  // (index → unit) drop too. The payload keeps both: the index panel, the
  // pin-count badges and the delete cleanup all still read them.
  const visNodes = createMemo(() => (data()?.nodes ?? []).filter((n) => n.type !== 'index'));
  const visEdges = createMemo(() => (data()?.edges ?? []).filter((e) => e.kind !== 'pin'));

  // ---- filters -----------------------------------------------------
  // "Indexed" = pinned by an index OR organized structurally (a lesson in
  // the training syllabus, a presentation in its deck) — those units are
  // placed by construction, never orphans.
  const pinnedKeys = createMemo(
    () =>
      new Set([
        ...(data()?.edges ?? []).filter((e) => e.kind === 'pin').map((e) => e.to),
        ...(data()?.nodes ?? []).filter((n) => (n.organized ?? []).length).map((n) => n.key),
      ]),
  );
  // Per-unit index-membership counts (state/graph.js pinCounts, shared
  // with the content modal's per-view badge) — the node corner badges.
  const pinCounts = createMemo(() => pinCountsOf(data()));
  const countOf = (n) => pinCounts()[n.key] ?? { total: 0, sites: {} };
  const countTitle = (n) => {
    const c = countOf(n);
    const per = (data()?.sites ?? []).map((s) => `${s}: ${c.sites[s] ?? 0}`).join(' · ');
    const org = (n.organized ?? []).length
      ? ` · organized by ${n.organized.join('/')}'s own navigation`
      : '';
    return `in ${c.total} ${c.total === 1 ? 'index' : 'indexes'}${per ? ` — ${per}` : ''}${org}`;
  };
  const toggleSiteFilter = (site) => {
    const next = new Set(viewFilter());
    if (next.has(site)) next.delete(site);
    else next.add(site);
    setViewFilter(next);
  };
  /** True when the node fails any active filter (rendered faded). Index
      nodes are the navigation structure — the index filter skips them. */
  const faded = (n) => {
    // A selected index (the left panel) scopes the graph to it + members —
    // subtree-wide, so sub-index pins count as membership.
    const idx = selectedIndexNode();
    if (idx && n.key !== idx.key && !subtreePinnedIds(idx).has(n.id)) return true;
    for (const site of viewFilter()) {
      if (n.views?.[site] === false) return true;
    }
    const ix = indexFilter();
    if (ix && n.type === 'unit') {
      const pinned = pinnedKeys().has(n.key);
      if (ix === 'in' ? !pinned : pinned) return true;
    }
    return false;
  };
  const edgeFaded = (e) => {
    const a = node(e.from);
    const b = node(e.to);
    return (a && faded(a)) || (b && faded(b));
  };
  const outPort = (n) => {
    const p = pos(n);
    return { x: p.x + n.w, y: p.y + n.h / 2 };
  };
  const inPort = (n) => {
    const p = pos(n);
    return { x: p.x, y: p.y + n.h / 2 };
  };
  /** Cubic Bézier with horizontal tangents from an out-port to a point. */
  const curve = (o, i) => {
    const t = Math.min(Math.max(Math.abs(i.x - o.x) / 2, 40), 140);
    return `M ${o.x} ${o.y} C ${o.x + t} ${o.y}, ${i.x - t} ${i.y}, ${i.x} ${i.y}`;
  };
  const edgePath = (e) => {
    const a = node(e.from);
    const b = node(e.to);
    return a && b ? curve(outPort(a), inPort(b)) : '';
  };

  // ---- pointer interaction -----------------------------------------
  // One dispatcher owns the svg's pointer events; hit-test priority is
  // port > edge handle > node > pan.
  let svg;
  let drag = null;

  const toWorld = (e) => {
    const r = svg.getBoundingClientRect();
    const vb = viewBox();
    return {
      x: vb.x + ((e.clientX - r.left) / r.width) * vb.w,
      y: vb.y + ((e.clientY - r.top) / r.height) * vb.h,
    };
  };

  /** The node under a world point, excluding the link source. */
  const hitNode = (w, srcKey) => {
    for (const n of data().nodes) {
      if (n.key === srcKey) continue;
      const p = pos(n);
      if (w.x >= p.x && w.x <= p.x + n.w && w.y >= p.y && w.y <= p.y + n.h) {
        return n.key;
      }
    }
    return null;
  };

  const editable = (n) => {
    if (n?.related_editable === false) {
      toast('The related list is computed — edit the source instead', {
        duration: 4000,
      });
      return false;
    }
    return true;
  };

  /** The index level owning a pin edge's `related` list: the index node
      itself for top-level pins, else the matching entry in its nested
      `children` tree (sub-index pins carry the owning id on the edge). */
  const pinLevel = (n, indexId) => {
    if (!n || n.id === indexId) return n;
    const walk = (levels) => {
      for (const c of levels ?? []) {
        if (c.id === indexId) return c;
        const hit = walk(c.children);
        if (hit) return hit;
      }
      return null;
    };
    return walk(n.children) ?? n;
  };

  const onPointerDown = (e) => {
    const portEl = e.target.closest('[data-port]');
    const edgeEl = e.target.closest('[data-edge-handle]');
    const nodeEl = e.target.closest('[data-node]');
    if (portEl && !busy()) {
      const from = portEl.getAttribute('data-port');
      if (!editable(node(from))) return;
      drag = { type: 'connect', from };
      setLinking({ from });
      setCursor(toWorld(e));
    } else if (edgeEl && !busy()) {
      const index = Number(edgeEl.getAttribute('data-edge-handle'));
      const edge = visEdges()[index];
      const a = node(edge.from);
      const b = node(edge.to);
      if (!a || !b) return;
      // Only the target end detaches (the write side is the from-block);
      // a grab nearer the source end is ignored.
      const w = toWorld(e);
      const ip = inPort(b);
      const op = outPort(a);
      if (Math.hypot(w.x - ip.x, w.y - ip.y) > Math.hypot(w.x - op.x, w.y - op.y)) {
        return;
      }
      if (!editable(edge.kind === 'pin' ? pinLevel(a, edge.index_id) : a)) return;
      drag = { type: 'rewire', edge, index };
      setLinking({ from: edge.from });
      setHiddenEdge(index);
      setCursor(w);
      setDropTarget(edge.to);
    } else if (nodeEl) {
      const key = nodeEl.getAttribute('data-node');
      const n = node(key);
      const w = toWorld(e);
      const p = pos(n);
      drag = {
        type: 'node',
        key,
        moved: false,
        sx: e.clientX,
        sy: e.clientY,
        // Grab offset from the node center, so the node doesn't jump.
        ox: p.x + n.w / 2 - w.x,
        oy: p.y + n.h / 2 - w.y,
        // Where the node started — restored when the drag ends on the
        // index panel (the drop pins it, it doesn't move it).
        startPos: { ...p },
      };
    } else {
      drag = { type: 'pan', x: e.clientX, y: e.clientY, vb: { ...viewBox() } };
    }
    svg.setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e) => {
    if (!drag) return;
    if (drag.type === 'pan') {
      const scale = viewBox().w / svg.clientWidth;
      setViewBox({
        ...drag.vb,
        x: drag.vb.x - (e.clientX - drag.x) * scale,
        y: drag.vb.y - (e.clientY - drag.y) * scale,
      });
    } else if (drag.type === 'node') {
      if (!drag.moved) {
        if (Math.hypot(e.clientX - drag.sx, e.clientY - drag.sy) < 4) return;
        // No reheat: the pin's temperature floor is enough — a full
        // reheat would let the first ticks take huge steps and fling
        // the neighborhood around.
        drag.moved = true;
        setNodeDragging({ key: drag.key });
      }
      const w = toWorld(e);
      sim.pin(drag.key, w.x + drag.ox, w.y + drag.oy);
      startLoop();
    } else {
      const w = toWorld(e);
      setCursor(w);
      setDropTarget(hitNode(w, drag.from ?? drag.edge.from));
    }
  };

  const clearLink = () => {
    setLinking(null);
    setCursor(null);
    setDropTarget(null);
  };

  /** A node drag released over the index panel: snap the node back and
      pin it into the selected index. */
  const dropOnPanel = (d) => {
    const idx = selectedIndexNode();
    const n = node(d.key);
    if (n && d.startPos) {
      sim.pin(d.key, d.startPos.x + n.w / 2, d.startPos.y + n.h / 2);
      sim.release(d.key);
      applyPositions();
    }
    if (!idx || !n) return;
    if (n.type !== 'unit') {
      toast('Only units can be pinned into an index', { duration: 3000 });
      return;
    }
    if ((idx.pinned ?? []).includes(n.id)) {
      toast(`${n.id} is already in “${idx.title}”`, { duration: 3000 });
      return;
    }
    commitNavOpQuiet({ op: 'pin_unit', index_id: idx.id, unit_id: n.id }).then((res) => {
      if (res.ok) {
        toast(`Pinned ${n.id} into “${idx.title}”`, { duration: 3000 });
        load({ keepPositions: true });
      }
    });
  };

  const onPointerUp = (e) => {
    const d = drag;
    drag = null;
    if (!d) return;
    if (d.type === 'node') {
      setNodeDragging(null);
      if (!d.moved) {
        // Clicking a node opens its editor. Focus follows so the index panel
        // still knows which unit is in play (its per-section pin buttons read
        // it) once the modal closes.
        setFocus(d.key);
        setContentFor(d.key);
        return;
      }
      sim.release(d.key);
      if (indexPanelAt(e.clientX, e.clientY)) dropOnPanel(d);
    } else if (d.type === 'connect') {
      const to = dropTarget();
      clearLink();
      if (to) connectEdge(d.from, to);
    } else if (d.type === 'rewire') {
      const to = dropTarget();
      clearLink();
      if (to === d.edge.to) setHiddenEdge(null); // dropped back — no-op
      else rewireEdge(d.edge, to);
    }
  };

  const cancelDrag = () => {
    if (drag?.type === 'node' && drag.moved) sim.release(drag.key);
    drag = null;
    setNodeDragging(null);
    setHiddenEdge(null);
    clearLink();
  };
  const onKeyDown = (e) => {
    if (e.key === 'Escape') cancelDrag();
  };
  onMount(() => window.addEventListener('keydown', onKeyDown));
  onCleanup(() => window.removeEventListener('keydown', onKeyDown));

  // Keep the focused node on screen. Focus can arrive from somewhere the
  // node isn't visible — clicking a row in the index panel — so pan (never
  // zoom) the viewBox until it is. A node focused by clicking it on the
  // canvas is already inside the box, so this is a no-op there.
  createEffect(() => {
    const key = focus();
    if (!key) return;
    const n = node(key);
    const vb = viewBox();
    if (!n || !vb || n.type === 'index') return;
    const next = panToInclude(vb, n, pos(n));
    if (next) setViewBox(next);
  });

  const onWheel = (e) => {
    e.preventDefault();
    const vb = viewBox();
    const factor = e.deltaY > 0 ? 1.15 : 1 / 1.15;
    const mx = vb.x + (e.offsetX / svg.clientWidth) * vb.w;
    const my = vb.y + (e.offsetY / svg.clientHeight) * vb.h;
    setViewBox({
      x: mx - (mx - vb.x) * factor,
      y: my - (my - vb.y) * factor,
      w: vb.w * factor,
      h: vb.h * factor,
    });
  };

  // ---- edge writes -------------------------------------------------
  // Both target the from-side block; an index source yields a pin edge.
  const connectEdge = async (fromKey, toKey) => {
    const a = node(fromKey);
    const b = node(toKey);
    const why = window.prompt(`Why does ${a.title ?? a.id} relate to ${b.title ?? b.id}?`);
    if (!why?.trim()) return;
    const res = await commitOpsQuiet(a.file, [
      { op: 'related_add', span: a.span, id: b.id, why: why.trim() },
    ]);
    if (res.ok) {
      toast(`Linked ${a.id} → ${b.id}`, { duration: 3000 });
      await load({ keepPositions: true });
    }
  };

  const rewireEdge = async (edge, toKey) => {
    const a = node(edge.from);
    const oldB = node(edge.to);
    // Pin edges write the owning index level's `related` list — a nested
    // sub-index for attributed pins — via the id-addressed nav ops (the
    // from-node's span would target the top-level list only).
    if (edge.kind === 'pin') {
      const owner = edge.index_id ?? a.id;
      const res = await commitNavOpQuiet({ op: 'unpin_unit', index_id: owner, unit_id: oldB.id });
      if (!res.ok) {
        setHiddenEdge(null);
        return;
      }
      let msg = `Unpinned ${oldB.id} from ${owner}`;
      if (toKey) {
        const b = node(toKey);
        const r2 = await commitNavOpQuiet({ op: 'pin_unit', index_id: owner, unit_id: b.id });
        if (r2.ok) msg = `Moved pin ${oldB.id} → ${b.id} in ${owner}`;
      }
      toast(msg, { duration: 3000 });
      await load({ keepPositions: true });
      return;
    }
    const ops = [{ op: 'related_remove', span: a.span, id: oldB.id }];
    let msg = `Removed ${a.id} → ${oldB.id}`;
    if (toKey) {
      const b = node(toKey);
      const why = window.prompt(`Why does ${a.title ?? a.id} relate to ${b.title ?? b.id}?`);
      if (!why?.trim()) {
        setHiddenEdge(null);
        return;
      }
      ops.push({ op: 'related_add', span: a.span, id: b.id, why: why.trim() });
      msg = `Moved ${a.id} → ${b.id}`;
    }
    const res = await commitOpsQuiet(a.file, ops);
    if (res.ok) {
      toast(msg, { duration: 3000 });
      await load({ keepPositions: true });
    } else {
      setHiddenEdge(null);
    }
  };

  const focused = () => (focus() ? node(focus()) : null);

  const viewHasPage = (view, page) =>
    targetHasPage({ entry: view.entry, site: view.site, page });

  const [openingPage, setOpeningPage] = createSignal(false);
  /** Open the unit's page in the canvas. The page may exist only in
      another view's site (a lesson is a training page; an :ai-only unit
      has no HTML page at all), so probe the active view first, then the
      rest, switching the canvas view when the page lives elsewhere. */
  const openInCanvas = async (n) => {
    if (openingPage()) return;
    const prefix = n.kind === 'procedure' ? 'process' : n.kind;
    const page = `${prefix}_${n.id}`;
    setOpeningPage(true);
    try {
      const views = (selected()?.views ?? []).filter((v) => !v.skill);
      const act = activeView();
      const ordered = [act, ...views.filter((v) => v.id !== act?.id)].filter(Boolean);
      for (const v of ordered) {
        if (!(await viewHasPage(v, page))) continue;
        if (v.id !== act?.id) {
          // Retargets the main preview; the canvas builds the new view and
          // the pending goto performs itself once its href lands.
          selectView(v.id);
          setDesignTab('canvas');
          loadNav();
          loadPalette();
        } else {
          setDesignTab('canvas');
        }
        setGotoPage(page);
        return;
      }
      toast(`No view builds a page for “${n.title}” — preview it via Content & visibility`, {
        duration: 5000,
      });
    } finally {
      setOpeningPage(false);
    }
  };

  const openCode = async (n) => {
    exitDesign();
    const res = await openFile(n.file);
    if (res.ok) revealSpan(n.file, n.span.start, n.span.end);
    else toast(res.error, { tone: 'danger', duration: 6000 });
  };

  // ---- unit deletion (the modal's Delete button) ----
  // The confirmation lives in the modal; the work stays here, where the whole
  // graph payload is, so it can strip every reference to the unit.
  const [deleting, setDeleting] = createSignal(false);

  /** Delete a unit and clean up every reference the graph knows about:
      `related` entries on other units (span-addressed removes, batched per
      file — the unit's own file folds its removes into the delete batch so
      all spans stay pre-reformat fresh), then index pins (id-addressed nav
      ops, immune to the reformats). `related` ids are plain identifiers,
      so the transient dangling states between commits are harmless.
      Computed `related` expressions can't be edited — those are skipped
      with a note. */
  const deleteUnit = async (n) => {
    setDeleting(true);
    try {
      const nodes = data()?.nodes ?? [];
      const edges = data()?.edges ?? [];
      const byKey = (k) => nodes.find((x) => x.key === k);
      const opsByFile = new Map();
      const skipped = [];
      for (const e of edges) {
        if (e.kind !== 'related' || e.to !== n.key) continue;
        const src = byKey(e.from);
        if (!src) continue;
        if (src.related_editable === false) {
          skipped.push(src.title);
          continue;
        }
        const list = opsByFile.get(src.file) ?? [];
        list.push({ op: 'related_remove', span: src.span, id: n.id });
        opsByFile.set(src.file, list);
      }
      const ownOps = opsByFile.get(n.file) ?? [];
      opsByFile.delete(n.file);
      for (const [file, ops] of opsByFile) {
        const res = await commitOpsQuiet(file, ops);
        if (!res.ok) return;
      }
      const res = await commitOpsQuiet(n.file, [...ownOps, { op: 'delete', span: n.span }]);
      if (!res.ok) return;
      const unpinned = new Set();
      for (const e of edges) {
        if (e.kind !== 'pin' || e.to !== n.key) continue;
        const owner = e.index_id ?? byKey(e.from)?.id;
        if (!owner || unpinned.has(owner)) continue;
        unpinned.add(owner);
        await commitNavOpQuiet({ op: 'unpin_unit', index_id: owner, unit_id: n.id });
      }
      if (skipped.length) {
        toast(`Computed related lists left untouched: ${skipped.join(', ')}`, {
          duration: 6000,
        });
      }
      toast(`Deleted ${n.title}`, { tone: 'success', duration: 3000 });
      setFocus(null);
      load({ keepPositions: true });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div class="ed-graph">
      <div class="ed-design-note">
        <ToggleGroup
          options={designTabOptions()}
          value={designTab()}
          onChange={(t) => setDesignTab(t)}
        />
        <span class="ed-design-page">unit graph</span>
        {/* Search pans the graph to the hit (revealNode → focus → the pan
            effect above), so no host wiring beyond the default. */}
        <UnitSearch class="ed-unitsearch-inline" placeholder="Find a unit…" />
        <Show when={data()}>
          <div class="ed-graph-filters">
            <ListFilter size={13} class="ed-graph-filtericon" />
            <For each={data().sites}>
              {(site) => (
                <button
                  type="button"
                  class="ed-graph-filterchip"
                  classList={{ 'is-on': viewFilter().has(site) }}
                  title={`Fade nodes not in the ${site} view`}
                  onClick={() => toggleSiteFilter(site)}
                >
                  {site}
                </button>
              )}
            </For>
            <span class="ed-graph-filtersep" />
            <button
              type="button"
              class="ed-graph-filterchip"
              classList={{ 'is-on': indexFilter() === 'in' }}
              title="Fade units not pinned in any index"
              onClick={() => setIndexFilter(indexFilter() === 'in' ? null : 'in')}
            >
              indexed
            </button>
            <button
              type="button"
              class="ed-graph-filterchip"
              classList={{ 'is-on': indexFilter() === 'out' }}
              title="Fade units already pinned in an index"
              onClick={() => setIndexFilter(indexFilter() === 'out' ? null : 'out')}
            >
              unindexed
            </button>
          </div>
        </Show>
        <Show when={loading() || busy()}>
          <Spinner size={12} label={curatorRunning() ? 'Curator running' : 'Updating graph'} />
        </Show>
        <span class="spacer" />
        <Button
          size="sm"
          disabled={busy() || !data()}
          onClick={() => runCurator({ scope: 'whole_graph' })}
        >
          <Sparkles size={13} /> Curate graph
        </Button>
        <Button size="sm" disabled={busy() || !data()} onClick={() => setAddOpen(true)}>
          <FilePlus size={13} /> Add unit
        </Button>
        <Show when={selected()?.wskill}>
          <Button size="sm" onClick={() => setPopover({ type: 'profiles' })}>
            <Layers size={13} /> Profiles
          </Button>
        </Show>
        <Popover label="Forces" icon={SlidersHorizontal} size="sm" align="end" width={280}>
          <div class="ed-graph-forces">
            <Slider
              label="Link distance"
              min={30}
              max={600}
              step={10}
              value={simParams().linkDistance}
              showValue
              onChange={(v) => tweak({ linkDistance: v })}
            />
            <Slider
              label="Spring strength"
              min={0}
              max={0.3}
              step={0.01}
              value={simParams().spring}
              showValue
              onChange={(v) => tweak({ spring: v })}
            />
            <Slider
              label="Repulsion"
              min={0}
              max={100000}
              step={1000}
              value={simParams().repulsion}
              showValue
              onChange={(v) => tweak({ repulsion: v })}
            />
            <Slider
              label="Gravity"
              min={0}
              max={0.2}
              step={0.005}
              value={simParams().gravity}
              showValue
              onChange={(v) => tweak({ gravity: v })}
            />
            <Slider
              label="Dead zone"
              min={0}
              max={15}
              step={0.5}
              value={simParams().deadZone}
              showValue
              onChange={(v) => tweak({ deadZone: v })}
            />
            <Slider
              label="Min distance"
              min={0}
              max={300}
              step={10}
              value={simParams().minGap}
              showValue
              onChange={(v) => tweak({ minGap: v })}
            />
            <Button size="sm" variant="ghost" onClick={() => tweak({ ...DEFAULT_PARAMS })}>
              Reset defaults
            </Button>
          </div>
        </Popover>
        <IconButton icon={RefreshCw} label="Reset layout" onClick={() => load()} />
      </div>
      <div class="ed-graph-body">
        <Show when={data() && viewBox()} fallback={<div class="ed-empty">Loading the unit graph…</div>}>
          <svg
            ref={svg}
            class="ed-graph-svg"
            viewBox={`${viewBox().x} ${viewBox().y} ${viewBox().w} ${viewBox().h}`}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onWheel={onWheel}
          >
            <defs>
              <marker
                id="ed-arrow"
                viewBox="0 0 10 10"
                refX="9"
                refY="5"
                markerWidth="7"
                markerHeight="7"
                orient="auto-start-reverse"
              >
                <path d="M 0 1 L 9 5 L 0 9 z" class="ed-graph-arrow" />
              </marker>
              <marker
                id="ed-arrow-pin"
                viewBox="0 0 10 10"
                refX="9"
                refY="5"
                markerWidth="7"
                markerHeight="7"
                orient="auto-start-reverse"
              >
                <path d="M 0 1 L 9 5 L 0 9 z" class="ed-graph-arrow-pin" />
              </marker>
            </defs>
            {/* edges under nodes */}
            <For each={visEdges()}>
              {(e, i) => (
                <g classList={{ 'is-hidden': hiddenEdge() === i(), 'is-faded': edgeFaded(e) }}>
                  <path
                    d={edgePath(e)}
                    class={e.kind === 'pin' ? 'ed-graph-edge-pin' : 'ed-graph-edge'}
                    marker-end={e.kind === 'pin' ? 'url(#ed-arrow-pin)' : 'url(#ed-arrow)'}
                  />
                  <path d={edgePath(e)} class="ed-graph-edge-hit" data-edge-handle={i()} />
                </g>
              )}
            </For>
            <For each={visNodes()}>
              {(n) => (
                <g
                  data-node={n.key}
                  transform={`translate(${pos(n).x}, ${pos(n).y})`}
                  class="ed-graph-node"
                  classList={{
                    'is-focus': focus() === n.key,
                    'is-target': dropTarget() === n.key,
                    'is-faded': faded(n),
                  }}
                >
                  <rect
                    width={n.w}
                    height={n.h}
                    rx="8"
                    style={{ stroke: KIND_COLORS[n.kind] ?? '#94a3b8' }}
                  />
                  <text x={n.w / 2} y="19" class="ed-graph-title">
                    {n.title}
                  </text>
                  <text x={n.w / 2} y="32" class="ed-graph-kind">
                    {n.kind}
                  </text>
                  {/* per-view chips */}
                  <For each={data().sites}>
                    {(site, i) => (
                      <circle
                        cx={10 + i() * 12}
                        cy={n.h - 9}
                        r="4"
                        class="ed-graph-chip"
                        classList={{
                          'is-on': n.views?.[site] !== false,
                          'is-custom': n.visibility?.custom,
                        }}
                      >
                        <title>
                          {site}: {n.views?.[site] === false ? 'hidden' : 'visible'}
                        </title>
                      </circle>
                    )}
                  </For>
                  {/* index-membership count badge (units only) */}
                  <Show when={n.type === 'unit'}>
                    <g class="ed-graph-count" classList={{ 'is-zero': countOf(n).total === 0 }}>
                      <circle cx={n.w - 2} cy={2} r="8" />
                      <text x={n.w - 2} y="5.5">
                        {countOf(n).total}
                      </text>
                      <title>{countTitle(n)}</title>
                    </g>
                  </Show>
                  {/* ports: drag out of the right port to link */}
                  <circle cx="0" cy={n.h / 2} r="3.5" class="ed-graph-port-in" />
                  <circle
                    cx={n.w}
                    cy={n.h / 2}
                    r="5"
                    class="ed-graph-port"
                    classList={{ 'is-disabled': n.related_editable === false }}
                    data-port={n.key}
                  >
                    <title>
                      {n.related_editable === false
                        ? 'related is computed — edit the source'
                        : 'drag to another unit to link'}
                    </title>
                  </circle>
                </g>
              )}
            </For>
            {/* pending connect/rewire preview */}
            <Show when={linking() && cursor()}>
              <path
                class="ed-graph-pending"
                d={curve(outPort(node(linking().from)), cursor())}
              />
            </Show>
          </svg>
        </Show>

        {/* Keyed: the modal builds its per-tab previews and probes for the
            merged one when it mounts, so moving it to another unit (its own
            search box does exactly that) has to be a remount, not a prop
            change onto stale previews. */}
        <Show when={contentFor()} keyed>
          {(key) => (
            <ContentModal
              nodeKey={key}
              onClose={() => setContentFor(null)}
              onOpenPage={(n) => openInCanvas(n)}
              onOpenCode={(n) => openCode(n)}
              opening={openingPage()}
              onDelete={async (n) => {
                await deleteUnit(n);
                setContentFor(null);
              }}
              deleting={deleting()}
            />
          )}
        </Show>

        <AddUnitDialog
          open={addOpen()}
          onClose={() => setAddOpen(false)}
          indexes={(data()?.nodes ?? []).filter((n) => n.type === 'index')}
          onSubmit={async (unit, pin) => {
            const res = await commitUnitCreateQuiet(unit, pin);
            if (res.ok) load({ keepPositions: true });
            return res;
          }}
        />
      </div>
    </div>
  );
}
