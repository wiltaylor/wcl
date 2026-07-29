/* The Systems canvas: a WAD's containment forest as nested boxes on an
   endless pan/zoom surface.

   Boxes lay themselves out from the model (preview/c4layout.js), so one
   grows and shrinks as its contents change. Dragging moves a box FREELY:
   it lands exactly where dropped, its ancestors grow to contain it, and its
   siblings are frozen first so they don't shuffle around the move. Those
   placements are session-only — nothing is written for them, and the
   re-pack button (or a reload) returns the box to the automatic layout.
   A drop on a box the schema accepts as a parent ALSO re-parents (one
   `set_field` write); one it forbids snaps back with the reason. Dragging
   from a box's port to another box writes a relation block; grabbing an
   edge near its arrowhead retargets or removes it. Every write commits
   straight to disk through the shared block-ops pipeline and refetches the
   model — the canvas has no cache of its own to keep in sync.

   Nothing here names a WAD kind: the drop rules, the property forms and the
   relation vocabulary all come from the schema metadata in the payload. */

import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import {
  ChevronsDownUp,
  ChevronsUpDown,
  LayoutGrid,
  Maximize2,
  Plus,
  RefreshCw,
} from 'lucide-solid';
import { Button, IconButton, Spinner, Tabs, ToggleGroup, toast } from '@forge/ui';

import {
  buildForest,
  edgePath,
  hitChain,
  hitTest,
  layoutForest,
  originOf,
  rollUp,
  selfPath,
} from '../../preview/c4layout';
import {
  busy,
  commitOpsQuiet,
  commitUnitCreateQuiet,
  designTab,
  palette,
  setDesignTab,
} from '../../state/design';
import { activeEntry } from '../../state/sites';
import {
  activePerspective,
  clearPositions,
  collapsed,
  fieldOptional,
  loadSystems,
  loading,
  model,
  nodeByKey,
  parentField,
  perspectives,
  pinPositions,
  positions,
  resetSystems,
  setNodePosition,
  selectPerspective,
  selectedNode,
  setSelectedNode,
  refEdges,
  subtreeKeys,
  toggleCollapsed,
  visibleKinds,
  wouldCycle,
} from '../../state/systems';
import AddNodeDialog from './AddNodeDialog';
import NodeDetailModal from './NodeDetailModal';
import NodePanel from './NodePanel';

/** Box tint per nesting depth — containment reads as depth, not as kind. */
const DEPTH_CLASS = ['is-d0', 'is-d1', 'is-d2', 'is-d3'];

export default function SystemsView() {
  const [viewBox, setViewBox] = createSignal(null);
  const [addFor, setAddFor] = createSignal(null); // { parentKey } | { parentKey: null }
  /** A drag in flight: { type, ... } — see onPointerDown. */
  const [drag, setDrag] = createSignal(null);
  const [dropTarget, setDropTarget] = createSignal(null); // node key | 'root' | null
  const [cursor, setCursor] = createSignal(null); // world coords
  /** The object open in the details modal: { file, span } | null. */
  const [detailFor, setDetailFor] = createSignal(null);
  let svg;
  let pointer = null;

  const forest = createMemo(() =>
    buildForest(model()?.nodes ?? [], {
      visibleKinds: visibleKinds() ?? null,
      collapsed: collapsed(),
    }),
  );
  const layout = createMemo(() => layoutForest(forest(), { positions: positions() }));

  /** Frame the whole model. Runs once when the first layout lands; after
      that the viewport is the user's until they press Fit. */
  const fit = () => {
    const l = layout();
    if (!l.boxes.size) return;
    const pad = 60;
    setViewBox({ x: -pad, y: -pad, w: l.w + pad * 2, h: l.h + pad * 2 });
  };
  createEffect(() => {
    if (!viewBox()) fit();
  });
  createEffect(() => {
    if (!model()) setViewBox(null);
  });

  // The model is per-document: picking another site in the topbar reloads
  // it in place, and a selection that isn't a WAD drops back to the canvas
  // rather than leaving another document's model on screen.
  let loadedEntry = null;
  createEffect(() => {
    if (palette() && !palette().wad) {
      setDesignTab('canvas');
      return;
    }
    const entry = activeEntry();
    if (!entry || entry === loadedEntry) return;
    loadedEntry = entry;
    resetSystems();
    setViewBox(null);
    loadSystems({ keep: false });
  });

  const box = (key) => layout().boxes.get(key);
  const nodeOf = (key) => forest().byKey.get(key) ?? nodeByKey(key);

  // ---- edges --------------------------------------------------------
  /** Relation edges, rolled up to whatever box is actually drawn. */
  const edges = createMemo(() => {
    const f = forest();
    const l = layout();
    return (model()?.edges ?? [])
      .map((e) => ({ ...e, fromKey: rollUp(e.from, f, l), toKey: rollUp(e.to, f, l) }))
      .filter((e) => e.fromKey && e.toKey);
  });
  /** Opt-in reference edges (`repo`, `built_by`, …) from the kind metadata. */
  const referenceEdges = createMemo(() => {
    const fields = refEdges();
    if (!fields.size) return [];
    const f = forest();
    const l = layout();
    const out = [];
    for (const n of model()?.nodes ?? []) {
      for (const [name, cell] of Object.entries(n.cells ?? {})) {
        if (!fields.has(name) || cell.state !== 'literal' || !cell.expr) continue;
        const fromKey = rollUp(n.id, f, l);
        const toKey = rollUp(cell.text, f, l);
        if (fromKey && toKey) out.push({ key: `${n.key}/${name}`, field: name, fromKey, toKey });
      }
    }
    return out;
  });

  const pathOf = (e) =>
    e.fromKey === e.toKey ? selfPath(box(e.fromKey)) : edgePath(box(e.fromKey), box(e.toKey));

  // ---- pointer ------------------------------------------------------
  /** The user-unit scale one screen pixel maps to. */
  const worldScale = () => {
    const r = svg.getBoundingClientRect();
    const vb = viewBox();
    // The viewBox is fitted with the default `xMidYMid meet`, so the smaller
    // of the two ratios wins and the rest is letterboxing.
    return Math.min(r.width / vb.w, r.height / vb.h) || 1;
  };

  /** Screen → world. Uses the SVG's own matrix, which already accounts for
      the uniform scale AND the letterbox offset a fitted viewBox leaves —
      deriving it from the element's width alone puts every hit test out by
      half the letterbox. */
  const toWorld = (e) => {
    const ctm = svg.getScreenCTM?.();
    if (ctm) {
      const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
      return { x: p.x, y: p.y };
    }
    const r = svg.getBoundingClientRect();
    const vb = viewBox();
    const s = worldScale();
    return {
      x: vb.x + (e.clientX - r.left - (r.width - vb.w * s) / 2) / s,
      y: vb.y + (e.clientY - r.top - (r.height - vb.h * s) / 2) / s,
    };
  };

  const onPointerDown = (e) => {
    if (!viewBox()) return;
    const portEl = e.target.closest('[data-port]');
    const edgeEl = e.target.closest('[data-edge]');
    const w = toWorld(e);
    if (portEl && !busy()) {
      setDrag({ type: 'link', from: portEl.getAttribute('data-port') });
      setCursor(w);
    } else if (edgeEl && !busy()) {
      const edge = edges().find((x) => x.key === edgeEl.getAttribute('data-edge'));
      if (!edge) return;
      // Only the arrow end detaches — the write side is the relation's
      // `destination`.
      const a = box(edge.fromKey);
      const b = box(edge.toKey);
      const near = (bx) => Math.hypot(w.x - (bx.x + bx.w / 2), w.y - (bx.y + bx.h / 2));
      if (near(b) > near(a)) return;
      setDrag({ type: 'retarget', edge });
      setCursor(w);
    } else {
      const key = hitTest(w, layout());
      if (key && !busy()) {
        const b = box(key);
        setDrag({
          type: 'node',
          key,
          moved: false,
          sx: e.clientX,
          sy: e.clientY,
          // Offset from the box's top-left, so the ghost tracks the grab.
          ox: b.x - w.x,
          oy: b.y - w.y,
          from: b,
        });
        setCursor(w);
      } else {
        setDrag({ type: 'pan', x: e.clientX, y: e.clientY, vb: { ...viewBox() } });
      }
    }
    pointer = e.pointerId;
    svg.setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e) => {
    const d = drag();
    if (!d) return;
    if (d.type === 'pan') {
      const perPixel = 1 / worldScale();
      setViewBox({
        ...d.vb,
        x: d.vb.x - (e.clientX - d.x) * perPixel,
        y: d.vb.y - (e.clientY - d.y) * perPixel,
      });
      return;
    }
    const w = toWorld(e);
    setCursor(w);
    if (d.type === 'node') {
      if (!d.moved && Math.hypot(e.clientX - d.sx, e.clientY - d.sy) < 4) return;
      if (!d.moved) setDrag({ ...d, moved: true });
      // The drop target is the innermost box that ISN'T the dragged node or
      // one of its own descendants.
      const own = subtreeKeys(d.key);
      const chain = hitChain(w, layout()).filter((k) => !own.has(k));
      setDropTarget(chain.length ? chain[chain.length - 1] : 'root');
    } else {
      const from = d.type === 'link' ? d.from : d.edge.fromKey;
      const chain = hitChain(w, layout()).filter((k) => k !== from);
      setDropTarget(chain.length ? chain[chain.length - 1] : null);
    }
  };

  const endDrag = () => {
    setDrag(null);
    setDropTarget(null);
    setCursor(null);
    if (pointer !== null && svg?.hasPointerCapture?.(pointer)) svg.releasePointerCapture(pointer);
    pointer = null;
  };

  const onPointerUp = () => {
    const d = drag();
    const target = dropTarget();
    const cursorAt = cursor();
    endDrag();
    if (!d) return;
    if (d.type === 'node') {
      if (!d.moved) {
        setSelectedNode(d.key);
        return;
      }
      drop(d, target, cursorAt);
    } else if (d.type === 'link') {
      if (target) createRelation(d.from, target);
    } else if (d.type === 'retarget') {
      retargetRelation(d.edge, target);
    }
  };

  const onKeyDown = (e) => {
    if (e.key !== 'Escape') return;
    if (drag()) endDrag();
    else if (selectedNode()) {
      // Pop outward through the containment chain, like the preview's
      // nested anchors.
      const parent = forest().parentOf.get(selectedNode());
      setSelectedNode(parent ?? null);
    }
  };
  onMount(() => window.addEventListener('keydown', onKeyDown));
  onCleanup(() => window.removeEventListener('keydown', onKeyDown));

  /** Double-clicking a box opens everything about it. */
  const onDoubleClick = (e) => {
    if (!viewBox()) return;
    const key = hitTest(toWorld(e), layout());
    const n = key && nodeOf(key);
    if (!n) return;
    setSelectedNode(key);
    setDetailFor({ file: n.file, span: n.span });
  };

  const onWheel = (e) => {
    e.preventDefault();
    const vb = viewBox();
    if (!vb) return;
    const factor = e.deltaY > 0 ? 1.15 : 1 / 1.15;
    // Zoom about the cursor's WORLD point (letterbox-aware, like toWorld).
    const { x: mx, y: my } = toWorld(e);
    setViewBox({
      x: mx - (mx - vb.x) * factor,
      y: my - (my - vb.y) * factor,
      w: vb.w * factor,
      h: vb.h * factor,
    });
  };

  // ---- placement ----------------------------------------------------
  /** The keys drawn inside `parentKey` (the roots when it is null). */
  const childrenKeys = (parentKey) =>
    (parentKey ? (forest().childrenOf.get(parentKey) ?? []) : forest().roots).map((n) => n.key);

  /** Freeze a family where it currently sits — see `pinPositions`. */
  const freeze = (parentKey) => {
    const o = originOf(layout(), parentKey);
    pinPositions(
      childrenKeys(parentKey)
        .filter((k) => layout().boxes.has(k))
        .map((k) => {
          const b = box(k);
          return [k, { x: b.x - o.x, y: b.y - o.y }];
        }),
    );
  };

  /**
   * A finished box drag. The box goes exactly where it was dropped and its
   * ancestors grow to contain it; a drop on a LEGAL new parent also
   * re-parents it, and one the schema forbids snaps back untouched.
   */
  const drop = async (d, target, at) => {
    const node = nodeOf(d.key);
    if (!node || !at) return;
    const current = forest().parentOf.get(d.key) ?? null;
    const targetKey = target === 'root' ? null : target;
    const moving = targetKey !== current;
    if (moving && !canDrop(d.key, target)) {
      // Snap back: nothing moves, and the schema reason is on the toast.
      const parent = targetKey ? nodeOf(targetKey) : null;
      toast(
        parent
          ? `A ${node.kind} cannot sit inside a ${parent.kind}`
          : `A ${node.kind} must name a ${node.parent?.kind ?? 'parent'} — drop it on one`,
        { duration: 4000 },
      );
      return;
    }
    // Freeze BOTH families where they sit, so neither the box's old
    // neighbours nor its new ones shuffle around the move, then place it.
    // Placing first means it never blinks back: the re-parent below
    // refetches the model, and every box position is derived from state.
    freeze(current);
    if (moving) freeze(targetKey);
    const origin = originOf(layout(), targetKey);
    setNodePosition(
      d.key,
      { x: at.x + d.ox - origin.x, y: at.y + d.oy - origin.y },
      childrenKeys(targetKey),
    );
    if (moving) await reparent(d.key, target);
  };

  /** Is a drop of `key` onto `target` ('root' = the background) allowed? */
  const canDrop = (key, target) => {
    const node = nodeOf(key);
    if (!node) return false;
    if (target === 'root' || target === null) {
      // Detaching only works when the schema lets the link go.
      return !node.parent || fieldOptional(node.kind, node.parent.field);
    }
    const parent = nodeOf(target);
    return (
      !!parent &&
      parent.key !== key &&
      !!parentField(node.kind, parent.kind) &&
      !wouldCycle(key, parent.key)
    );
  };

  // ---- writes -------------------------------------------------------
  /** Move `key` under `target` ('root' detaches). */
  const reparent = async (key, target) => {
    const node = nodeOf(key);
    if (!node || !target) return;
    if (target === 'root') {
      const field = node.parent?.field;
      if (!field) return;
      if (!fieldOptional(node.kind, field)) {
        toast(`A ${node.kind} must name a ${node.parent.kind} — drop it on one`, {
          duration: 5000,
        });
        return;
      }
      await write(node.file, [{ op: 'remove_field', span: node.span, field }], node.etag,
        `Detached ${node.title}`);
      return;
    }
    const parent = nodeOf(target);
    if (!parent || parent.key === key) return;
    if (node.parent?.id === parent.id) return; // already there
    const field = parentField(node.kind, parent.kind);
    if (!field) {
      toast(`A ${node.kind} cannot sit inside a ${parent.kind}`, { duration: 4000 });
      return;
    }
    if (wouldCycle(key, parent.key)) {
      toast('That would nest a node inside itself', { duration: 4000 });
      return;
    }
    const ops = [{ op: 'set_field', span: node.span, field, expr: parent.id }];
    // Moving between levels can leave the old parent field dangling (a
    // code_item that named a container now names a component); drop it.
    const stale = node.parent?.field;
    if (stale && stale !== field && fieldOptional(node.kind, stale)) {
      ops.push({ op: 'remove_field', span: node.span, field: stale });
    }
    await write(node.file, ops, node.etag, `Moved ${node.title} into ${parent.title}`);
  };

  /** The relation kind (the payload's edge kind) and its field names. */
  const edgeKind = () => model()?.kinds?.find((k) => k.edge);

  const createRelation = async (fromKey, toKey) => {
    const ek = edgeKind();
    const a = nodeOf(fromKey);
    const b = nodeOf(toKey);
    if (!ek || !a || !b) {
      toast('This document declares no relation type', { duration: 4000 });
      return;
    }
    const taken = new Set((model()?.ids ?? []).concat((model()?.edges ?? []).map((e) => e.id)));
    let id = `r_${a.id}_${b.id}`;
    for (let n = 2; taken.has(id); n += 1) id = `r_${a.id}_${b.id}_${n}`;
    const fields = {
      [ek.edge.source]: { ident: a.id },
      [ek.edge.destination]: { ident: b.id },
    };
    // A required `kind` field takes the vocabulary's first symbol; the
    // property panel edits it from there.
    for (const f of ek.fields ?? []) {
      if (f.optional !== false || f.inline_slot != null || f.name in fields) continue;
      if (f.symbols?.length) fields[f.name] = { sym: f.symbols[0] };
      else if (f.default == null) fields[f.name] = '';
    }
    const res = await commitUnitCreateQuiet({
      kind: ek.kind,
      type_name: ek.type_name,
      id,
      fields,
    });
    if (res.ok) {
      toast(`Linked ${a.title} → ${b.title}`, { duration: 3000 });
      await reload();
    }
  };

  const retargetRelation = async (edge, toKey) => {
    const ek = edgeKind();
    if (!ek) return;
    if (!toKey) {
      await write(edge.file, [{ op: 'delete', span: edge.span }], edge.etag, `Removed ${edge.id}`);
      return;
    }
    const b = nodeOf(toKey);
    if (!b || b.id === edge.to) return;
    await write(
      edge.file,
      [{ op: 'set_field', span: edge.span, field: ek.edge.destination, expr: b.id }],
      edge.etag,
      `Repointed ${edge.id} → ${b.title}`,
    );
  };

  const write = async (file, ops, etag, message) => {
    const res = await commitOpsQuiet(file, ops, etag ? { etag } : {});
    if (!res.ok) return res;
    if (message) toast(message, { duration: 3000 });
    await reload();
    return res;
  };

  const reload = () => loadSystems({ keep: true });

  // ---- add ----------------------------------------------------------
  /** Kinds that may be created under `parentKey` (null = at the top). */
  const addableUnder = (parentKey) => {
    const parent = parentKey ? nodeOf(parentKey) : null;
    const shown = visibleKinds() ?? new Set();
    return (model()?.kinds ?? []).filter((k) => {
      if (k.edge || !shown.has(k.kind)) return false;
      if (!parent) return true;
      return (k.parents ?? []).some((p) => p.kind === parent.kind);
    });
  };

  const collapseAll = () => {
    const keys = [...layout().boxes.keys()].filter((k) => (forest().childrenOf.get(k) ?? []).length);
    keys.forEach(toggleCollapsed);
  };

  /** Is the in-flight node drag legal on the current drop target? Drops on
      the box the node already lives in are plain moves, always fine. */
  const legalDrop = () => {
    const d = drag();
    const t = dropTarget();
    if (d?.type !== 'node' || !t) return false;
    const current = forest().parentOf.get(d.key) ?? null;
    return (t === 'root' ? null : t) === current || canDrop(d.key, t);
  };

  return (
    <div class="ed-sys">
      <div class="ed-design-note">
        <ToggleGroup
          options={[
            { value: 'canvas', label: 'Canvas' },
            { value: 'systems', label: 'Systems' },
          ]}
          value={designTab()}
          onChange={(t) => setDesignTab(t)}
        />
        {/* Perspectives keep each slice of the model to itself — the C4
            drill-down, the people who use it, where it runs. */}
        <Show when={perspectives().length > 1}>
          <Tabs
            tabs={perspectives().map((p) => ({ id: p.id, label: p.label }))}
            active={activePerspective()?.id}
            onChange={(id) => selectPerspective(id)}
          />
        </Show>
        <Show when={loading() || busy()}>
          <Spinner size={12} label="Loading the systems model" />
        </Show>
        <span class="spacer" />
        <Button size="sm" disabled={busy() || !model()} onClick={() => setAddFor({ parentKey: null })}>
          <Plus size={13} /> Add
        </Button>
        <IconButton
          icon={collapsed().size ? ChevronsUpDown : ChevronsDownUp}
          label={collapsed().size ? 'Expand all' : 'Collapse all'}
          onClick={collapseAll}
        />
        <IconButton icon={Maximize2} label="Fit the whole model" onClick={fit} />
        <IconButton
          icon={LayoutGrid}
          label="Re-pack the layout (drops hand-placed boxes)"
          onClick={clearPositions}
        />
        <IconButton icon={RefreshCw} label="Reload the model" onClick={() => reload()} />
      </div>
      <div class="ed-sys-body">
        <Show
          when={viewBox() && layout().boxes.size}
          fallback={
            <div class="ed-empty">
              {model() ? 'Nothing to draw — enable a kind in the panel' : 'Loading the systems model…'}
            </div>
          }
        >
          <svg
            ref={svg}
            class="ed-sys-svg"
            viewBox={`${viewBox().x} ${viewBox().y} ${viewBox().w} ${viewBox().h}`}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={endDrag}
            onDblClick={onDoubleClick}
            onWheel={onWheel}
          >
            <defs>
              <marker
                id="ed-sys-arrow"
                viewBox="0 0 10 10"
                refX="9"
                refY="5"
                markerWidth="7"
                markerHeight="7"
                orient="auto-start-reverse"
              >
                <path d="M 0 1 L 9 5 L 0 9 z" class="ed-sys-arrowhead" />
              </marker>
            </defs>

            {/* boxes, outermost first so children paint on top */}
            <For each={layout().order}>
              {(key) => {
                const b = () => box(key);
                const n = () => nodeOf(key);
                const kids = () => forest().childrenOf.get(key) ?? [];
                const hidden = () => forest().hiddenCount.get(key) ?? 0;
                return (
                  <Show when={n()}>
                    <g
                      class="ed-sys-node"
                      classList={{
                        [DEPTH_CLASS[Math.min(b().depth, DEPTH_CLASS.length - 1)]]: true,
                        'is-selected': selectedNode() === key,
                        'is-target': dropTarget() === key,
                        'is-dragging': drag()?.type === 'node' && drag().key === key && drag().moved,
                        'has-kids': kids().length > 0 || hidden() > 0,
                      }}
                      transform={`translate(${b().x}, ${b().y})`}
                    >
                      <rect width={b().w} height={b().h} rx="8" />
                      <text class="ed-sys-title" x="12" y={kids().length ? 20 : 24}>
                        {n().title}
                      </text>
                      <text class="ed-sys-kind" x="12" y={kids().length ? 31 : 39}>
                        {n().subtitle ? `${n().kind} · ${n().subtitle}` : n().kind}
                      </text>
                      <Show when={hidden() > 0}>
                        <g class="ed-sys-count">
                          <circle cx={b().w - 16} cy="16" r="10" />
                          <text x={b().w - 16} y="19.5">
                            {hidden()}
                          </text>
                          <title>{hidden()} nested — click the chevron to expand</title>
                        </g>
                      </Show>
                      <Show when={kids().length > 0 || hidden() > 0}>
                        <g
                          class="ed-sys-fold"
                          onPointerDown={(e) => {
                            e.stopPropagation();
                            toggleCollapsed(key);
                          }}
                        >
                          <rect x={b().w - 34} y="6" width="20" height="20" rx="4" />
                          <text x={b().w - 24} y="20">
                            {hidden() > 0 ? '+' : '–'}
                          </text>
                        </g>
                      </Show>
                      {/* out-port: drag to another box to write a relation */}
                      <circle
                        class="ed-sys-port"
                        cx={b().w}
                        cy={b().h / 2}
                        r="5"
                        data-port={key}
                      >
                        <title>drag onto another box to add a relation</title>
                      </circle>
                    </g>
                  </Show>
                );
              }}
            </For>

            {/* edges over the boxes so nested endpoints stay visible */}
            <For each={referenceEdges()}>
              {(e) => (
                <path class="ed-sys-refedge" d={pathOf(e)} marker-end="url(#ed-sys-arrow)">
                  <title>{e.field}</title>
                </path>
              )}
            </For>
            <For each={edges()}>
              {(e) => (
                <g
                  class="ed-sys-edge-g"
                  classList={{ 'is-hidden': drag()?.type === 'retarget' && drag().edge.key === e.key }}
                >
                  <path class="ed-sys-edge" d={pathOf(e)} marker-end="url(#ed-sys-arrow)" />
                  <path class="ed-sys-edge-hit" d={pathOf(e)} data-edge={e.key} />
                  <title>{e.label ?? e.rel_kind ?? e.id}</title>
                </g>
              )}
            </For>

            {/* drag feedback */}
            <Show when={drag()?.type === 'node' && drag().moved && cursor()}>
              <rect
                class="ed-sys-ghost"
                x={cursor().x + drag().ox}
                y={cursor().y + drag().oy}
                width={box(drag().key).w}
                height={box(drag().key).h}
                rx="8"
              />
            </Show>
            <Show when={dropTarget() && dropTarget() !== 'root' && drag()?.type === 'node'}>
              <rect
                class="ed-sys-droprect"
                classList={{ 'is-illegal': !legalDrop() }}
                x={box(dropTarget()).x}
                y={box(dropTarget()).y}
                width={box(dropTarget()).w}
                height={box(dropTarget()).h}
                rx="8"
              />
            </Show>
            <Show when={cursor() && (drag()?.type === 'link' || drag()?.type === 'retarget')}>
              <path
                class="ed-sys-pending"
                d={edgePath(
                  box(drag().type === 'link' ? drag().from : drag().edge.fromKey),
                  { x: cursor().x, y: cursor().y, w: 1, h: 1 },
                )}
              />
            </Show>
          </svg>
        </Show>

        <NodePanel
          onAddChild={(key) => setAddFor({ parentKey: key })}
          onReload={reload}
          onOpenDetail={(n) => setDetailFor({ file: n.file, span: n.span })}
        />
      </div>

      <Show when={detailFor()}>
        <NodeDetailModal
          anchor={detailFor()}
          onReanchor={setDetailFor}
          onClose={() => setDetailFor(null)}
        />
      </Show>

      <AddNodeDialog
        open={!!addFor()}
        parent={addFor()?.parentKey ? nodeOf(addFor().parentKey) : null}
        kinds={addableUnder(addFor()?.parentKey ?? null)}
        takenIds={model()?.ids ?? []}
        onClose={() => setAddFor(null)}
        onSubmit={async (unit) => {
          const res = await commitUnitCreateQuiet(unit);
          if (res.ok) await reload();
          return res;
        }}
      />
    </div>
  );
}
