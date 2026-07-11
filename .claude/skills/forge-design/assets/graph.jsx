/* Forge node graph — optional 4th copy-in asset (needs console.css's
   "Node graph" section). Controlled component: nodes/edges/selection come
   from props; interactions are reported through callbacks and the consumer
   updates its own store.

   <NodeGraph
     nodes={[{ id, x, y, title, w?, inputs: [{id, type, label?}], outputs: [...] }]}
     edges={[{ id, from: {node, port}, to: {node, port}, state?: 'active'|'broken' }]}
     selected={null | { kind: 'node'|'edge', id }}
     onNodeMove={(id, x, y) => ...}   // fires per pointermove during a drag
     onConnect={({ from, to }) => ...} // consumer appends the edge
     onSelect={(sel|null) => ...}      // null = background click
     onDelete={(sel) => ...}           // Delete/Backspace with a selection
   >
     {(node) => <optional body JSX per node>}
   </NodeGraph>

   Imports only solid-js — no coupling to ui.jsx. */

import { Show, For, createSignal, createMemo } from 'solid-js';

/* Port type → token colour (from the Forge port-colour table in tokens.md). */
export const PORT_COLORS = {
  trigger: 'var(--fg-0)',
  string: 'var(--success)',
  number: 'var(--info)',
  boolean: 'var(--danger)',
  object: 'var(--accent)',
  array: 'var(--warning)',
  any: 'var(--fg-3)',
};
export const portColor = (type) => PORT_COLORS[type] ?? PORT_COLORS.any;

const compatible = (a, b) => a === b || a === 'any' || b === 'any';

/* Geometry constants — port dot positions and edge anchors share these, so
   they coincide by construction (no DOM measurement during drag). */
const NODE_W = 180;
const HEAD_H = 33;
const ROW_H = 24;
const STUB = 16;   // min straight run out of / into a port
const BEND_R = 6;  // elbow corner radius

/* Input rows render first, then output rows — the output anchor offsets by
   the input count so dots and edge endpoints coincide. */
const inAnchor = (n, i) => ({ x: n.x, y: n.y + HEAD_H + ROW_H * i + ROW_H / 2 });
const outAnchor = (n, i) => ({
  x: n.x + (n.w ?? NODE_W),
  y: n.y + HEAD_H + ROW_H * ((n.inputs?.length ?? 0) + i) + ROW_H / 2,
});

/* ---------------- Elbow routing -------------------------------------------- */
/* Orthogonal (Manhattan) polyline: a exits rightward, b enters leftward. */
function elbowPoints(a, b) {
  if (b.x - a.x >= 2 * STUB) {
    if (a.y === b.y) return [a, b];
    const mx = (a.x + b.x) / 2;
    return [a, { x: mx, y: a.y }, { x: mx, y: b.y }, b];
  }
  const my = (a.y + b.y) / 2;  // backward: detour around via stubs
  return [
    a,
    { x: a.x + STUB, y: a.y },
    { x: a.x + STUB, y: my },
    { x: b.x - STUB, y: my },
    { x: b.x - STUB, y: b.y },
    b,
  ];
}

const dist = (p, q) => Math.abs(p.x - q.x) + Math.abs(p.y - q.y);  // axis-aligned
function towards(c, p, r) {
  const dx = Math.sign(p.x - c.x), dy = Math.sign(p.y - c.y);
  return { x: c.x + dx * r, y: c.y + dy * r };
}

function roundedPath(pts, r = BEND_R) {
  let d = `M ${pts[0].x} ${pts[0].y}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const p = pts[i - 1], c = pts[i], n = pts[i + 1];
    const rr = Math.min(r, dist(p, c) / 2, dist(c, n) / 2);
    const inPt = towards(c, p, rr), outPt = towards(c, n, rr);
    d += ` L ${inPt.x} ${inPt.y} Q ${c.x} ${c.y} ${outPt.x} ${outPt.y}`;
  }
  const last = pts[pts.length - 1];
  return `${d} L ${last.x} ${last.y}`;
}

export const edgePath = (a, b) => roundedPath(elbowPoints(a, b));

/* ---------------- NodeGraph ------------------------------------------------- */
export function NodeGraph(props) {
  let root;
  let drag = null;  // {id, dx, dy} — plain variable, not reactive (per-frame)
  const [pending, setPending] = createSignal(null);  // {from, type, x, y}

  const nodeById = createMemo(() => {
    const m = {};
    for (const n of props.nodes ?? []) m[n.id] = n;
    return m;
  });

  const toCanvas = (e) => {
    const r = root.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  const headDown = (node) => (e) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    const p = toCanvas(e);
    drag = { id: node.id, dx: p.x - node.x, dy: p.y - node.y };
    props.onSelect?.({ kind: 'node', id: node.id });
  };
  const rootMove = (e) => {
    if (drag) {
      const p = toCanvas(e);
      props.onNodeMove?.(drag.id, p.x - drag.dx, p.y - drag.dy);
    } else if (pending()) {
      const p = toCanvas(e);
      setPending((prev) => ({ ...prev, x: p.x, y: p.y }));
    }
  };
  const rootUp = () => {
    drag = null;
    setPending(null);
  };

  const portDown = (node, port) => (e) => {
    e.stopPropagation();
    e.preventDefault();
    const p = toCanvas(e);
    setPending({ from: { node: node.id, port: port.id }, type: port.type, x: p.x, y: p.y });
  };
  const portUp = (node, port) => (e) => {
    const p = pending();
    if (p && compatible(p.type, port.type) && p.from.node !== node.id) {
      e.stopPropagation();
      props.onConnect?.({ from: p.from, to: { node: node.id, port: port.id } });
    }
    setPending(null);
  };

  const edgeGeom = (edge) => {
    const from = nodeById()[edge.from.node];
    const to = nodeById()[edge.to.node];
    if (!from || !to) return null;
    const oi = (from.outputs ?? []).findIndex((pt) => pt.id === edge.from.port);
    const ii = (to.inputs ?? []).findIndex((pt) => pt.id === edge.to.port);
    if (oi < 0 || ii < 0) return null;
    return {
      d: edgePath(outAnchor(from, oi), inAnchor(to, ii)),
      color: portColor((from.outputs ?? [])[oi]?.type),
    };
  };

  const onKeyDown = (e) => {
    if ((e.key === 'Delete' || e.key === 'Backspace') && props.selected &&
        !/^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName)) {
      e.preventDefault();
      props.onDelete?.(props.selected);
    }
  };

  const isSel = (kind, id) => props.selected?.kind === kind && props.selected?.id === id;
  const pendingStart = () => {
    const p = pending();
    if (!p) return null;
    const n = nodeById()[p.from.node];
    if (!n) return null;
    const oi = (n.outputs ?? []).findIndex((pt) => pt.id === p.from.port);
    return oi < 0 ? null : outAnchor(n, oi);
  };

  return (
    <div class={`fgraph ${props.class ?? ''}`} style={props.style} ref={root} tabindex="0"
         onPointerMove={rootMove} onPointerUp={rootUp} onKeyDown={onKeyDown}
         onPointerDown={(e) => { if (e.target === root) props.onSelect?.(null); }}>
      <svg class="fgraph-svg">
        <For each={props.edges}>
          {(edge) => {
            const geom = createMemo(() => edgeGeom(edge));
            return (
              <Show when={geom()}>
                <g>
                  <path class="fgraph-edge-hit" d={geom().d}
                        onClick={() => props.onSelect?.({ kind: 'edge', id: edge.id })} />
                  <path class="fgraph-edge" d={geom().d}
                        style={{ stroke: edge.state || isSel('edge', edge.id) ? undefined : geom().color }}
                        classList={{
                          'is-active': edge.state === 'active',
                          'is-broken': edge.state === 'broken',
                          'is-selected': isSel('edge', edge.id),
                        }} />
                </g>
              </Show>
            );
          }}
        </For>
        <Show when={pending() && pendingStart()}>
          <path class="fgraph-pending" d={edgePath(pendingStart(), { x: pending().x, y: pending().y })} />
        </Show>
      </svg>
      <For each={props.nodes}>
        {(node) => (
          <div class="fgraph-node" classList={{ 'is-selected': isSel('node', node.id) }}
               style={{ left: `${node.x}px`, top: `${node.y}px`, width: node.w ? `${node.w}px` : undefined }}
               onPointerDown={(e) => e.stopPropagation()}>
            <div class="fgraph-node-head" onPointerDown={headDown(node)}>
              <span class="fgraph-node-title">{node.title}</span>
            </div>
            <For each={node.inputs ?? []}>
              {(port) => (
                <div class="fgraph-row">
                  <span class="fgraph-port fgraph-port-in" style={{ background: portColor(port.type) }}
                        onPointerDown={(e) => e.stopPropagation()} onPointerUp={portUp(node, port)} />
                  {port.label ?? port.id}
                </div>
              )}
            </For>
            <For each={node.outputs ?? []}>
              {(port) => (
                <div class="fgraph-row fgraph-row-out">
                  {port.label ?? port.id}
                  <span class="fgraph-port fgraph-port-out" style={{ background: portColor(port.type) }}
                        onPointerDown={portDown(node, port)} />
                </div>
              )}
            </For>
            <Show when={props.children}>
              <div class="fgraph-node-body">{props.children(node)}</div>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}

/* ---------------- Flowchart ------------------------------------------------- */
/* Display-oriented DAG: nodes without x/y are auto-laid-out left-to-right
   (longest-path layering + one barycenter pass). Edges reuse the .fgraph-edge
   state classes, so 'active' (marching ants) and 'broken' (flash) come free.
   Cycles tolerated: back-edges are ignored for layering but still drawn. */

const FLOW_W = 160, FLOW_H = 36, GAP_X = 56, GAP_Y = 20;

export function layoutFlow(nodes, edges) {
  const ids = nodes.map((n) => n.id);
  const out = new Map(ids.map((id) => [id, []]));
  for (const e of edges) if (out.has(e.from)) out.get(e.from).push(e);

  /* DFS back-edge detection (gray-stack). */
  const state = new Map();  // 0 unvisited, 1 in-stack, 2 done
  const backEdges = new Set();
  const visit = (id) => {
    state.set(id, 1);
    for (const e of out.get(id) ?? []) {
      const s = state.get(e.to) ?? 0;
      if (s === 1) backEdges.add(e);
      else if (s === 0 && out.has(e.to)) visit(e.to);
    }
    state.set(id, 2);
  };
  const indeg = new Map(ids.map((id) => [id, 0]));
  for (const e of edges) if (!backEdges.has(e) && indeg.has(e.to)) indeg.set(e.to, indeg.get(e.to) + 1);
  const roots = ids.filter((id) => edges.every((e) => e.to !== id));
  for (const r of roots.length ? roots : ids.slice(0, 1)) if (!state.get(r)) visit(r);
  for (const id of ids) if (!state.get(id)) visit(id);

  /* Longest-path layering over the back-edge-free DAG (relaxation passes). */
  const layer = new Map(ids.map((id) => [id, 0]));
  for (let pass = 0; pass < ids.length; pass++) {
    let changed = false;
    for (const e of edges) {
      if (backEdges.has(e) || !layer.has(e.from) || !layer.has(e.to)) continue;
      if (layer.get(e.to) < layer.get(e.from) + 1) {
        layer.set(e.to, layer.get(e.from) + 1);
        changed = true;
      }
    }
    if (!changed) break;
  }

  /* Group by layer (insertion order), one barycenter pass over predecessors. */
  const layers = [];
  for (const n of nodes) {
    const l = layer.get(n.id);
    (layers[l] ??= []).push(n.id);
  }
  for (let l = 1; l < layers.length; l++) {
    const prevIdx = new Map((layers[l - 1] ?? []).map((id, i) => [id, i]));
    const bary = (id) => {
      const preds = edges.filter((e) => !backEdges.has(e) && e.to === id && prevIdx.has(e.from));
      if (!preds.length) return Number.MAX_SAFE_INTEGER;  // keep insertion order at the end
      return preds.reduce((s, e) => s + prevIdx.get(e.from), 0) / preds.length;
    };
    layers[l] = (layers[l] ?? []).map((id, i) => ({ id, b: bary(id), i }))
      .sort((a, b) => a.b - b.b || a.i - b.i).map((x) => x.id);
  }

  /* Coordinates with per-layer vertical centering. */
  const tallest = Math.max(...layers.map((l) => l?.length ?? 0));
  const totalH = tallest * FLOW_H + (tallest - 1) * GAP_Y;
  const pos = new Map();
  layers.forEach((col, l) => {
    const colH = col.length * FLOW_H + (col.length - 1) * GAP_Y;
    const y0 = (totalH - colH) / 2;
    col.forEach((id, i) => pos.set(id, {
      x: 16 + l * (FLOW_W + GAP_X),
      y: 16 + y0 + i * (FLOW_H + GAP_Y),
    }));
  });
  return {
    pos, backEdges,
    width: 32 + layers.length * FLOW_W + (layers.length - 1) * GAP_X,
    height: 32 + totalH,
  };
}

export function Flowchart(props) {
  const layout = createMemo(() => layoutFlow(props.nodes ?? [], props.edges ?? []));
  const anchor = (id, side) => {
    const p = layout().pos.get(id);
    if (!p) return null;
    return { x: side === 'out' ? p.x + FLOW_W : p.x, y: p.y + FLOW_H / 2 };
  };
  return (
    <div class="fchart-scroll">
      <div class={`fflow ${props.class ?? ''}`}
           style={{ width: `${layout().width}px`, height: `${layout().height}px`, ...props.style }}>
        <svg class="fgraph-svg">
          <For each={props.edges}>
            {(e) => {
              const a = createMemo(() => anchor(e.from, 'out'));
              const b = createMemo(() => anchor(e.to, 'in'));
              return (
                <Show when={a() && b()}>
                  <path class="fgraph-edge" d={edgePath(a(), b())}
                        classList={{ 'is-active': e.state === 'active', 'is-broken': e.state === 'broken' }} />
                  <Show when={e.label}>
                    {(() => {
                      const lx = () => (a().x + b().x) / 2;
                      const ly = () => (a().y + b().y) / 2;
                      return (
                        <>
                          <rect class="fflow-edge-label-bg" x={lx() - e.label.length * 3.2 - 4} y={ly() - 8}
                                width={e.label.length * 6.4 + 8} height="16" rx="3" />
                          <text class="fflow-edge-label" x={lx()} y={ly()} text-anchor="middle" dominant-baseline="central">
                            {e.label}
                          </text>
                        </>
                      );
                    })()}
                  </Show>
                </Show>
              );
            }}
          </For>
        </svg>
        <For each={props.nodes}>
          {(n) => {
            const p = () => layout().pos.get(n.id);
            return (
              <Show when={p()}>
                <div class={`fflow-node${n.tone ? ` tone-${n.tone}` : ''}`}
                     classList={{ 'is-clickable': !!props.onNodeClick }}
                     style={{ left: `${p().x}px`, top: `${p().y}px`, width: `${FLOW_W}px` }}
                     onClick={() => props.onNodeClick?.(n.id)}>
                  {n.label}
                </div>
              </Show>
            );
          }}
        </For>
      </div>
    </div>
  );
}
