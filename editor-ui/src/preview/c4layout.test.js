/* Unit tests for the Systems view's nested-box layout: forest building
   (parent links, filtered/dangling parents, cycles, collapse), the
   bottom-up sizing that makes a box grow and shrink with its contents,
   nearest-first hit testing, and edge roll-up to the visible ancestor. */

import { describe, expect, it } from 'vitest';

import { buildForest, edgePath, hitChain, hitTest, layoutForest, rollUp } from './c4layout';

const node = (kind, id, parent = null, extra = {}) => ({
  key: `${kind}:${id}`,
  kind,
  id,
  title: id,
  parent: parent ? { field: parent.field, kind: parent.kind, id: parent.id } : null,
  ...extra,
});

/** system → two containers → one component in the first container. */
const model = () => [
  node('system', 'sys'),
  node('container', 'c1', { field: 'system', kind: 'system', id: 'sys' }),
  node('container', 'c2', { field: 'system', kind: 'system', id: 'sys' }),
  node('component', 'p1', { field: 'container', kind: 'container', id: 'c1' }),
];

const lay = (nodes, opts, positions) => {
  const forest = buildForest(nodes, opts);
  return { forest, layout: layoutForest(forest, positions ? { positions } : undefined) };
};

describe('buildForest', () => {
  it('nests by the parent id and roots the rest', () => {
    const f = buildForest(model());
    expect(f.roots.map((n) => n.id)).toEqual(['sys']);
    expect(f.childrenOf.get('system:sys').map((n) => n.id)).toEqual(['c1', 'c2']);
    expect(f.childrenOf.get('container:c1').map((n) => n.id)).toEqual(['p1']);
    expect(f.parentOf.get('component:p1')).toBe('container:c1');
  });

  it('promotes a node whose parent is filtered out — never drops it', () => {
    const f = buildForest(model(), { visibleKinds: new Set(['container', 'component']) });
    expect(f.roots.map((n) => n.id).sort()).toEqual(['c1', 'c2']);
    expect(f.childrenOf.get('container:c1').map((n) => n.id)).toEqual(['p1']);
  });

  it('falls back to the next parent link when the first is not shown', () => {
    // A deploy-target-shaped node: nests in a container by default, in its
    // environment when the C4 kinds are off the canvas.
    const target = {
      ...node('deploy', 'd'),
      parent: { field: 'container', kind: 'container', id: 'c1' },
      parents: [
        { field: 'container', kind: 'container', id: 'c1' },
        { field: 'environment', kind: 'environment', id: 'prod' },
      ],
    };
    const nodes = [...model(), node('environment', 'prod'), target];
    expect(buildForest(nodes).parentOf.get('deploy:d')).toBe('container:c1');
    const deployOnly = buildForest(nodes, {
      visibleKinds: new Set(['environment', 'deploy']),
    });
    expect(deployOnly.parentOf.get('deploy:d')).toBe('environment:prod');
  });

  it('promotes a node whose parent id does not exist', () => {
    const f = buildForest([node('container', 'orphan', { field: 'system', kind: 'system', id: 'gone' })]);
    expect(f.roots.map((n) => n.id)).toEqual(['orphan']);
  });

  it('breaks a parent cycle instead of looping', () => {
    const f = buildForest([
      node('host', 'a', { field: 'parent', kind: 'host', id: 'b' }),
      node('host', 'b', { field: 'parent', kind: 'host', id: 'a' }),
    ]);
    expect(f.roots.length).toBe(1);
    expect(f.roots.length + f.childrenOf.get(f.roots[0].key).length).toBe(2);
  });

  it('hides a collapsed node’s children behind a count', () => {
    const f = buildForest(model(), { collapsed: new Set(['system:sys']) });
    expect(f.childrenOf.get('system:sys')).toEqual([]);
    expect(f.hiddenCount.get('system:sys')).toBe(3);
  });
});

describe('layoutForest', () => {
  it('nests children strictly inside their parent', () => {
    const { layout } = lay(model());
    const sys = layout.boxes.get('system:sys');
    for (const key of ['container:c1', 'container:c2', 'component:p1']) {
      const b = layout.boxes.get(key);
      expect(b.x).toBeGreaterThanOrEqual(sys.x);
      expect(b.y).toBeGreaterThanOrEqual(sys.y);
      expect(b.x + b.w).toBeLessThanOrEqual(sys.x + sys.w + 0.001);
      expect(b.y + b.h).toBeLessThanOrEqual(sys.y + sys.h + 0.001);
    }
    expect(layout.boxes.get('system:sys').depth).toBe(0);
    expect(layout.boxes.get('component:p1').depth).toBe(2);
  });

  it('grows when a child is added and shrinks when it is removed', () => {
    const base = lay(model()).layout.boxes.get('system:sys');
    const added = lay([
      ...model(),
      node('container', 'c3', { field: 'system', kind: 'system', id: 'sys' }),
    ]).layout.boxes.get('system:sys');
    expect(added.w * added.h).toBeGreaterThan(base.w * base.h);

    const removed = lay(model().filter((n) => n.id !== 'c2')).layout.boxes.get('system:sys');
    expect(removed.w * removed.h).toBeLessThan(base.w * base.h);
  });

  it('resizes BOTH parents when a child is re-parented', () => {
    const before = lay(model()).layout;
    const after = lay(
      model().map((n) =>
        n.id === 'p1' ? node('component', 'p1', { field: 'container', kind: 'container', id: 'c2' }) : n,
      ),
    ).layout;
    const area = (l, k) => l.boxes.get(k).w * l.boxes.get(k).h;
    expect(area(after, 'container:c1')).toBeLessThan(area(before, 'container:c1'));
    expect(area(after, 'container:c2')).toBeGreaterThan(area(before, 'container:c2'));
    expect(after.boxes.get('component:p1').depth).toBe(2);
  });

  it('collapsing shrinks the parent to a leaf-sized box', () => {
    const open = lay(model()).layout.boxes.get('system:sys');
    const shut = lay(model(), { collapsed: new Set(['system:sys']) }).layout.boxes.get('system:sys');
    expect(shut.h).toBeLessThan(open.h);
    expect(shut.w).toBeLessThanOrEqual(open.w);
  });

  it('is deterministic for the same model', () => {
    const a = lay(model()).layout;
    const b = lay(model()).layout;
    for (const key of a.boxes.keys()) expect(b.boxes.get(key)).toEqual(a.boxes.get(key));
  });
});

describe('hand-placed boxes', () => {
  it('puts a box exactly where it was dropped, in parent-local units', () => {
    const positions = new Map([['component:p1', { x: 300, y: 120 }]]);
    const { layout } = lay(model(), {}, positions);
    const origin = layout.boxes.get('container:c1').origin;
    const p1 = layout.boxes.get('component:p1');
    expect(p1.x - origin.x).toBe(300);
    expect(p1.y - origin.y).toBe(120);
  });

  it('grows the parent — and its parent — to contain it', () => {
    const before = lay(model()).layout;
    const after = lay(model(), {}, new Map([['component:p1', { x: 600, y: 0 }]])).layout;
    const c1 = (l) => l.boxes.get('container:c1');
    const sys = (l) => l.boxes.get('system:sys');
    expect(c1(after).w).toBeGreaterThan(c1(before).w);
    expect(sys(after).w).toBeGreaterThan(sys(before).w);
    // Still fully contained: growing is exactly what keeps it inside.
    const p1 = after.boxes.get('component:p1');
    expect(p1.x + p1.w).toBeLessThanOrEqual(c1(after).x + c1(after).w + 0.001);
  });

  it('auto-packs the untouched siblings below the placed ones', () => {
    const nodes = [
      ...model(),
      node('container', 'c3', { field: 'system', kind: 'system', id: 'sys' }),
    ];
    const placed = new Map([['container:c1', { x: 0, y: 0 }]]);
    const { layout } = lay(nodes, {}, placed);
    const c1 = layout.boxes.get('container:c1');
    for (const key of ['container:c2', 'container:c3']) {
      expect(layout.boxes.get(key).y).toBeGreaterThanOrEqual(c1.y + c1.h);
    }
  });

  it('leaves an untouched model exactly as the auto layout had it', () => {
    const auto = lay(model()).layout;
    const withEmpty = lay(model(), {}, new Map()).layout;
    for (const key of auto.boxes.keys()) {
      expect(withEmpty.boxes.get(key)).toEqual(auto.boxes.get(key));
    }
  });
});

describe('hitTest', () => {
  it('returns the innermost box, with the chain outermost-first', () => {
    const { layout } = lay(model());
    const p1 = layout.boxes.get('component:p1');
    const point = { x: p1.x + 2, y: p1.y + 2 };
    expect(hitTest(point, layout)).toBe('component:p1');
    expect(hitChain(point, layout)).toEqual(['system:sys', 'container:c1', 'component:p1']);
  });

  it('returns null off the boxes', () => {
    const { layout } = lay(model());
    expect(hitTest({ x: -500, y: -500 }, layout)).toBeNull();
  });
});

describe('rollUp', () => {
  it('resolves an id to its own box when it is drawn', () => {
    const { forest, layout } = lay(model());
    expect(rollUp('p1', forest, layout)).toBe('component:p1');
  });

  it('climbs to the visible ancestor when the node is collapsed away', () => {
    const { forest, layout } = lay(model(), { collapsed: new Set(['system:sys']) });
    expect(rollUp('p1', forest, layout)).toBe('system:sys');
  });

  it('is null for an id that is nowhere on the canvas', () => {
    const { forest, layout } = lay(model());
    expect(rollUp('nope', forest, layout)).toBeNull();
  });
});

describe('edgePath', () => {
  it('leaves and enters through the facing sides', () => {
    const a = { x: 0, y: 0, w: 100, h: 50 };
    const b = { x: 300, y: 0, w: 100, h: 50 };
    // Horizontal neighbours: start on a's right edge, end on b's left.
    expect(edgePath(a, b)).toMatch(/^M 100 25 C /);
    expect(edgePath(a, b)).toMatch(/300 25$/);
    // Vertical neighbours swap to the top/bottom edges.
    expect(edgePath(a, { x: 0, y: 300, w: 100, h: 50 })).toMatch(/^M 50 50 C /);
  });

  it('is empty when either end is missing', () => {
    expect(edgePath(null, { x: 0, y: 0, w: 1, h: 1 })).toBe('');
  });
});
