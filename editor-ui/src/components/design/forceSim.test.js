/* Unit tests for the incremental force simulation behind the Design-mode
   unit graph: spring/repulsion behaviour, pinning, cooling, position
   preservation across graph swaps, and collision resolution. */

import { describe, expect, it } from 'vitest';

import { createSimulation } from './forceSim';

// The physics assertions below are calibrated to THIS tuning — pinned
// explicitly so they stay valid when the user-facing DEFAULT_PARAMS are
// retuned.
const TUNE = { linkDistance: 110, spring: 0.1, repulsion: 9000, gravity: 0, deadZone: 0.8, minGap: 8 };

const box = (key, x, y, w = 100, h = 48) => ({ key, x, y, w, h });

const run = (sim, ticks) => {
  for (let i = 0; i < ticks; i++) sim.tick();
};

const centerDist = (sim, a, b) => {
  const pos = sim.positions();
  const pa = pos.get(a);
  const pb = pos.get(b);
  return Math.hypot(pa.x - pb.x, pa.y - pb.y);
};

describe('createSimulation', () => {
  it('pulls connected nodes toward the spring length; unconnected stay apart', () => {
    // Gravity off so only the spring can close a large starting gap.
    const nodes = [box('a', 0, 0), box('b', 800, 0)];
    const linked = createSimulation(TUNE);
    linked.setGraph(nodes, [{ from: 'a', to: 'b' }]);
    linked.reheat();
    run(linked, 300);
    const lone = createSimulation(TUNE);
    lone.setGraph(nodes, []);
    lone.reheat();
    run(lone, 300);
    // Spring ideal = linkDistance + both radii ≈ 221 for 100×48 boxes.
    expect(centerDist(linked, 'a', 'b')).toBeLessThan(400);
    expect(centerDist(lone, 'a', 'b')).toBeGreaterThan(700);
  });

  it('keeps a pinned node exactly in place while neighbors move', () => {
    const sim = createSimulation(TUNE);
    sim.setGraph([box('a', 0, 0), box('b', 60, 0)], [{ from: 'a', to: 'b' }]);
    sim.pin('a', 200, 300);
    const before = sim.positions().get('b');
    run(sim, 50);
    const pos = sim.positions();
    // Pinned center (200, 300) → top-left (150, 276) for a 100×48 box.
    expect(pos.get('a')).toEqual({ x: 150, y: 276 });
    const after = pos.get('b');
    expect(after.x === before.x && after.y === before.y).toBe(false);
  });

  it('cools to a stop when settled and ignores a reheat at rest', () => {
    const sim = createSimulation(TUNE);
    sim.setGraph([box('a', 0, 0), box('b', 800, 0)], [{ from: 'a', to: 'b' }]);
    expect(sim.tick()).toBe(false); // never heated
    sim.reheat();
    expect(sim.tick()).toBe(true); // the stretched spring is above the dead zone
    let guard = 10000;
    while (sim.tick() && guard-- > 0);
    expect(guard).toBeGreaterThan(0);
    const settled = sim.positions().get('a');
    sim.tick();
    expect(sim.positions().get('a')).toEqual(settled);
    // A settled graph stays asleep even when reheated — residual forces
    // below the dead zone don't wake it.
    sim.reheat();
    expect(sim.tick()).toBe(false);
    expect(sim.positions().get('a')).toEqual(settled);
  });

  it('keeps far-away settled nodes frozen while a drag disturbs neighbors', () => {
    const sim = createSimulation(TUNE);
    sim.setGraph([box('a', 0, 0), box('b', 300, 0), box('far', 2000, 0)], []);
    const farBefore = sim.positions().get('far');
    sim.pin('a', 260, 24); // shove a into b
    run(sim, 100);
    const pos = sim.positions();
    expect(pos.get('far')).toEqual(farBefore); // untouched by the drag
    expect(pos.get('b').x).toBeGreaterThan(300); // pushed out of the way
  });

  it('stays live while a pin is held even after cooling', () => {
    const sim = createSimulation(TUNE);
    sim.setGraph([box('a', 0, 0), box('b', 300, 0)], []);
    sim.pin('a', 0, 24);
    run(sim, 500);
    expect(sim.tick()).toBe(true);
    sim.release('a');
    let guard = 10000;
    while (sim.tick() && guard-- > 0);
    expect(guard).toBeGreaterThan(0);
  });

  it('preserves existing positions across setGraph, seeding only new keys', () => {
    const sim = createSimulation(TUNE);
    sim.setGraph([box('a', 0, 0), box('b', 300, 0)], []);
    sim.reheat();
    run(sim, 100);
    const before = sim.positions();
    sim.setGraph([box('a', 999, 999), box('b', 999, 0), box('c', 40, 40)], []);
    const after = sim.positions();
    expect(after.get('a')).toEqual(before.get('a'));
    expect(after.get('b')).toEqual(before.get('b'));
    expect(after.get('c')).toEqual({ x: 40, y: 40 });
  });

  it('pushes overlapping nodes apart until their boxes are disjoint', () => {
    const sim = createSimulation(TUNE);
    sim.setGraph([box('a', 0, 0), box('b', 10, 5)], []);
    sim.reheat();
    run(sim, 300);
    const pos = sim.positions();
    const a = pos.get('a');
    const b = pos.get('b');
    const overlapX = a.x < b.x + 100 && b.x < a.x + 100;
    const overlapY = a.y < b.y + 48 && b.y < a.y + 48;
    expect(overlapX && overlapY).toBe(false);
  });

  it('keeps every coordinate finite on a dense hub graph', () => {
    const sim = createSimulation(TUNE);
    const nodes = Array.from({ length: 12 }, (_, i) => box(`n${i}`, (i % 4) * 20, Math.floor(i / 4) * 20));
    const edges = nodes.slice(1).map((n) => ({ from: 'n0', to: n.key }));
    sim.setGraph(nodes, edges);
    sim.reheat();
    run(sim, 400);
    for (const [, p] of sim.positions()) {
      expect(Number.isFinite(p.x)).toBe(true);
      expect(Number.isFinite(p.y)).toBe(true);
    }
  });

  it('enforces the minimum distance, even against a pulling spring', () => {
    // Strong spring wants the pair at ~linkDistance 30 + radii, but the
    // min-gap constraint is hard: they can never sit closer than
    // radii + minGap.
    const sim = createSimulation({ ...TUNE, linkDistance: 30, spring: 0.3, minGap: 100 });
    sim.setGraph([box('a', 0, 0), box('b', 400, 0)], [{ from: 'a', to: 'b' }]);
    sim.reheat();
    run(sim, 300);
    const radius = Math.hypot(100, 48) / 2;
    expect(centerDist(sim, 'a', 'b')).toBeGreaterThanOrEqual(2 * radius + 100 - 1);
    // Raising minGap live pushes an already-settled pair further apart.
    sim.setParams({ minGap: 200 });
    sim.reheat();
    run(sim, 300);
    expect(centerDist(sim, 'a', 'b')).toBeGreaterThanOrEqual(2 * radius + 200 - 1);
  });

  it('applies setParams live: zeroing spring and gravity stops the pull', () => {
    const sim = createSimulation({ ...TUNE, gravity: 0.05 });
    sim.setGraph([box('a', 0, 0), box('b', 800, 0)], [{ from: 'a', to: 'b' }]);
    sim.setParams({ spring: 0, gravity: 0 });
    sim.reheat();
    run(sim, 300);
    // Nothing attracts at this range with spring + gravity off.
    expect(centerDist(sim, 'a', 'b')).toBeGreaterThan(700);
    sim.setParams({ spring: 0.1 });
    sim.reheat();
    run(sim, 300);
    expect(centerDist(sim, 'a', 'b')).toBeLessThan(400);
  });

  it('drops self-loops and edges to unknown keys', () => {
    const sim = createSimulation(TUNE);
    sim.setGraph(
      [box('a', 0, 0), box('b', 300, 0)],
      [
        { from: 'a', to: 'a' },
        { from: 'a', to: 'ghost' },
      ],
    );
    sim.reheat();
    run(sim, 200);
    for (const [, p] of sim.positions()) {
      expect(Number.isFinite(p.x)).toBe(true);
      expect(Number.isFinite(p.y)).toBe(true);
    }
  });
});
