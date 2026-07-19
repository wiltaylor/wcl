/* An incremental, pinnable port of the wcl_wdoc diagram force solver
   (crates/wcl_wdoc/src/force.rs) for the Design-mode unit graph: the same
   Coulomb repulsion / Hooke springs / centroid gravity / collision sweeps,
   restructured from a batch 300-iteration pass into a per-frame `tick()`
   so the graph re-simulates live while a node is dragged. Positions are
   seeded from the server layout, so the spiral init, normalization, and
   determinism quantization are deliberately absent. Plain module — no
   Solid imports — so it runs under bare vitest. */

const EPS = 0.01;
const COLLIDE_SWEEPS_PER_TICK = 2;

// Hand-tuned in anger (2026-07-19): long links + weak springs + strong
// repulsion + a wide dead zone give a spacious, calm graph that only
// reacts locally to drags.
export const DEFAULT_PARAMS = {
  repulsion: 30000,
  linkDistance: 300,
  // No centering pull by default: gravity re-centers the WHOLE graph every
  // tick, which reads as "everything drags back to the middle" during a
  // node drag. The slider can bring it back for compacting a layout.
  gravity: 0,
  spring: 0.01,
  // Force dead zone: a node whose net displacement for a tick is below
  // this many pixels stays frozen. This keeps forces local — only nodes
  // actually disturbed (by the drag, or a changed edge) move, instead of
  // the whole graph re-relaxing on every touch.
  deadZone: 5,
  // Hard minimum gap between node bounding circles: the collision sweeps
  // guarantee no pair ever sits closer than this, springs or not.
  minGap: 150,
};

export function createSimulation(params = {}) {
  // Live-tunable: the graph's force-settings panel updates these
  // mid-simulation via setParams.
  let repulsion = params.repulsion ?? DEFAULT_PARAMS.repulsion;
  let linkDistance = params.linkDistance ?? DEFAULT_PARAMS.linkDistance;
  let gravity = params.gravity ?? DEFAULT_PARAMS.gravity;
  let spring = params.spring ?? DEFAULT_PARAMS.spring;
  let deadZone = params.deadZone ?? DEFAULT_PARAMS.deadZone;
  let minGap = params.minGap ?? DEFAULT_PARAMS.minGap;
  // Cooling rate per tick: a reheat settles in ~1.5s at 60fps. While a
  // node is pinned the temperature never drops below the floor, so the
  // rest of the graph keeps responding to the drag — but the floor is
  // LOW: it caps per-tick steps, and a hot floor makes bystander nodes
  // overshoot and fly around instead of easing out of the way.
  const coolRate = () => linkDistance / 90;
  const pinFloor = () => Math.max(8, linkDistance * 0.1);

  const setParams = (next = {}) => {
    if (next.repulsion !== undefined) repulsion = next.repulsion;
    if (next.linkDistance !== undefined) linkDistance = next.linkDistance;
    if (next.gravity !== undefined) gravity = next.gravity;
    if (next.spring !== undefined) spring = next.spring;
    if (next.deadZone !== undefined) deadZone = next.deadZone;
    if (next.minGap !== undefined) minGap = next.minGap;
  };

  /** key → { x, y, w, h, r } — x/y are CENTERS internally. */
  let nodes = new Map();
  /** [keyA, keyB] pairs, deduped of self-loops and unknown endpoints. */
  let links = [];
  const pinned = new Set();
  let temperature = 0;

  const setGraph = (nextNodes, nextEdges) => {
    const fresh = new Map();
    for (const n of nextNodes) {
      const prev = nodes.get(n.key);
      fresh.set(n.key, {
        // Server/DTO coords are top-left; keep centers internally. An
        // already-simulated node keeps its position — only new keys take
        // the seed passed in.
        x: prev ? prev.x : n.x + n.w / 2,
        y: prev ? prev.y : n.y + n.h / 2,
        w: n.w,
        h: n.h,
        r: Math.hypot(n.w, n.h) / 2,
      });
    }
    nodes = fresh;
    links = [];
    for (const e of nextEdges) {
      if (e.from !== e.to && nodes.has(e.from) && nodes.has(e.to)) {
        links.push([e.from, e.to]);
      }
    }
    for (const key of pinned) if (!nodes.has(key)) pinned.delete(key);
  };

  const pin = (key, cx, cy) => {
    const n = nodes.get(key);
    if (!n) return;
    pinned.add(key);
    n.x = cx;
    n.y = cy;
  };

  const release = (key) => pinned.delete(key);

  const reheat = (t = linkDistance) => {
    temperature = Math.max(temperature, t);
  };

  const tick = () => {
    if (pinned.size > 0) temperature = Math.max(temperature, pinFloor());
    if (temperature <= 0 || nodes.size === 0) return false;

    const keys = [...nodes.keys()];
    const disp = new Map(keys.map((k) => [k, { x: 0, y: 0 }]));

    // Coulomb repulsion between every distinct pair, over the gap net of
    // both radii so overlapping boxes push apart hard.
    for (let i = 0; i < keys.length; i++) {
      const a = nodes.get(keys[i]);
      const da = disp.get(keys[i]);
      for (let j = i + 1; j < keys.length; j++) {
        const b = nodes.get(keys[j]);
        const db = disp.get(keys[j]);
        const dx = a.x - b.x;
        const dy = a.y - b.y;
        const dist = Math.max(Math.hypot(dx, dy), EPS);
        const gap = Math.max(dist - a.r - b.r, EPS);
        const force = repulsion / (gap * gap);
        const ux = dx / dist;
        const uy = dy / dist;
        da.x += ux * force;
        da.y += uy * force;
        db.x -= ux * force;
        db.y -= uy * force;
      }
    }

    // Hooke spring along each edge toward its ideal length.
    for (const [ka, kb] of links) {
      const a = nodes.get(ka);
      const b = nodes.get(kb);
      const dx = a.x - b.x;
      const dy = a.y - b.y;
      const dist = Math.max(Math.hypot(dx, dy), EPS);
      const ideal = linkDistance + a.r + b.r;
      const force = (dist - ideal) * spring;
      const ux = dx / dist;
      const uy = dy / dist;
      const da = disp.get(ka);
      const db = disp.get(kb);
      da.x -= ux * force;
      da.y -= uy * force;
      db.x += ux * force;
      db.y += uy * force;
    }

    // Centering gravity toward the current centroid.
    if (gravity !== 0) {
      let cx = 0;
      let cy = 0;
      for (const n of nodes.values()) {
        cx += n.x;
        cy += n.y;
      }
      cx /= nodes.size;
      cy /= nodes.size;
      for (const k of keys) {
        const n = nodes.get(k);
        const d = disp.get(k);
        d.x += (cx - n.x) * gravity;
        d.y += (cy - n.y) * gravity;
      }
    }

    // Apply, clamped to the temperature; pinned nodes never move, and a
    // node inside the force dead zone stays frozen — settled parts of the
    // graph don't creep while something else is dragged.
    let anyMoved = false;
    for (const k of keys) {
      if (pinned.has(k)) continue;
      const n = nodes.get(k);
      const d = disp.get(k);
      const mag = Math.hypot(d.x, d.y);
      const eff = Math.min(mag, temperature);
      if (eff > Math.max(deadZone, EPS)) {
        const scale = eff / mag;
        n.x += d.x * scale;
        n.y += d.y * scale;
        anyMoved = true;
      }
    }
    temperature = Math.max(temperature - coolRate(), 0);

    // A few collision sweeps per tick keep boxes disjoint without the
    // batch solver's 200-sweep tail. A pinned node takes none of the
    // push — its partner absorbs the full correction.
    for (let sweep = 0; sweep < COLLIDE_SWEEPS_PER_TICK; sweep++) {
      let moved = false;
      for (let i = 0; i < keys.length; i++) {
        const a = nodes.get(keys[i]);
        for (let j = i + 1; j < keys.length; j++) {
          const b = nodes.get(keys[j]);
          const dx = a.x - b.x;
          const dy = a.y - b.y;
          const dist = Math.max(Math.hypot(dx, dy), EPS);
          const minDist = a.r + b.r + minGap;
          if (dist >= minDist) continue;
          const aPinned = pinned.has(keys[i]);
          const bPinned = pinned.has(keys[j]);
          if (aPinned && bPinned) continue;
          const ux = dx / dist;
          const uy = dy / dist;
          const push = minDist - dist;
          if (aPinned) {
            b.x -= ux * push;
            b.y -= uy * push;
          } else if (bPinned) {
            a.x += ux * push;
            a.y += uy * push;
          } else {
            a.x += (ux * push) / 2;
            a.y += (uy * push) / 2;
            b.x -= (ux * push) / 2;
            b.y -= (uy * push) / 2;
          }
          moved = true;
          anyMoved = true;
        }
      }
      if (!moved) break;
    }

    // Stop as soon as nothing moves: temperature alone (cooling tail)
    // isn't a reason to keep ticking a settled graph.
    return pinned.size > 0 || (temperature > 0 && anyMoved);
  };

  /** Current top-left positions for rendering: Map key → {x, y}. */
  const positions = () => {
    const out = new Map();
    for (const [k, n] of nodes) out.set(k, { x: n.x - n.w / 2, y: n.y - n.h / 2 });
    return out;
  };

  return { setGraph, setParams, pin, movePin: pin, release, reheat, tick, positions };
}
