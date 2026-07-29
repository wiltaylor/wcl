/* Unit tests for the Systems view's schema-derived rules: which kinds the
   canvas opens on, whether a drop is legal, cycle rejection, and the delete
   cascade's scope. All of it reads the /api/systems payload — no WAD kind is
   named in the implementation, so these use invented kinds on purpose. */

import { beforeEach, describe, expect, it, vi } from 'vitest';

// The module chain reaches state/buffers → lsp/client, whose singleton opens
// a WebSocket at import time. Nothing here talks to the LSP.
vi.mock('../lsp/client', () => ({
  lsp: { didOpen() {}, didChange() {}, didClose() {} },
  isWcl: () => false,
  docUri: (path) => `file://${path}`,
  setWorkspaceRoot() {},
}));
// state/tree's module-level createResource fetches /api/files on import.
vi.mock('./tree', () => ({ treeData: () => ({ root: '/repo' }), refreshTree() {}, buildTree: () => [] }));

import {
  activePerspective,
  applyPrefs,
  clearPositions,
  defaultVisibleKinds,
  deletePlan,
  fieldOptional,
  kindOf,
  parentField,
  model,
  pinPositions,
  positions,
  selectPerspective,
  setModel,
  setNodePosition,
  subtreeKeys,
  toggleKind,
  visibleKinds,
  wouldCycle,
} from './systems';

/** zone ⊃ system ⊃ part, plus a standalone glossary kind and an edge kind. */
const KINDS = [
  { kind: 'zone', type_name: 'w.Zone', parents: [], refs: [], edge: null, fields: [] },
  {
    kind: 'system',
    type_name: 'w.System',
    parents: [{ field: 'zone', kind: 'zone' }],
    refs: [{ field: 'repo', list: false }],
    edge: null,
    fields: [
      { name: 'zone', type: 'identifier?', optional: true },
      { name: 'name', type: 'utf8', optional: false },
    ],
  },
  {
    kind: 'part',
    type_name: 'w.Part',
    parents: [{ field: 'system', kind: 'system' }],
    refs: [],
    edge: null,
    fields: [{ name: 'system', type: 'identifier', optional: false }],
  },
  { kind: 'term', type_name: 'w.Term', parents: [], refs: [], edge: null, fields: [] },
  {
    kind: 'link',
    type_name: 'w.Link',
    parents: [],
    refs: [],
    edge: { source: 'source', destination: 'destination' },
    fields: [],
  },
];

const node = (kind, id, parent = null) => ({
  key: `${kind}:${id}`,
  kind,
  id,
  title: id,
  file: 'model.wcl',
  span: { start: 0, end: 1 },
  parent,
  cells: {},
});

beforeEach(() => {
  localStorage.clear();
  clearPositions();
  setModel({
    ok: true,
    kinds: KINDS,
    nodes: [
      node('zone', 'z'),
      node('system', 's', { field: 'zone', kind: 'zone', id: 'z' }),
      node('part', 'p', { field: 'system', kind: 'system', id: 's' }),
      node('term', 't'),
    ],
    edges: [
      { key: 'link:l', kind: 'link', id: 'l', from: 's', to: 'p', file: 'model.wcl', span: { start: 2, end: 3 } },
    ],
    ids: ['z', 's', 'p', 't'],
  });
});

describe('defaultVisibleKinds', () => {
  it('opens on the containment chain and leaves standalone kinds off', () => {
    const shown = defaultVisibleKinds(KINDS);
    expect([...shown].sort()).toEqual(['part', 'system', 'zone']);
    expect(shown.has('term')).toBe(false);
    expect(shown.has('link')).toBe(false);
  });

  it('shows every non-edge kind when nothing contains anything', () => {
    const flat = [
      { kind: 'a', parents: [], edge: null },
      { kind: 'b', parents: [], edge: null },
      { kind: 'e', parents: [], edge: { source: 'source', destination: 'destination' } },
    ];
    expect([...defaultVisibleKinds(flat)].sort()).toEqual(['a', 'b']);
  });

  it('does not treat self-nesting as a containment level', () => {
    const selfish = [{ kind: 'host', parents: [{ field: 'parent', kind: 'host' }], edge: null }];
    expect([...defaultVisibleKinds(selfish)]).toEqual(['host']);
  });

  it('stops drilling down before a level blows the object budget', () => {
    const counts = { zone: 1, system: 2, part: 200 };
    const shown = defaultVisibleKinds(KINDS, counts, 80);
    expect([...shown].sort()).toEqual(['system', 'zone']);
  });

  it('always takes the first level below the roots, however big', () => {
    const counts = { zone: 1, system: 500, part: 1 };
    expect([...defaultVisibleKinds(KINDS, counts, 80)].sort()).toEqual(['system', 'zone']);
  });

  it('never returns a kind outside the list it was given', () => {
    // A perspective slice: `part` links to a `system` that isn't in it, so
    // the link is ignored — the slice must not drag `system` in.
    const slice = KINDS.filter((k) => ['part', 'term'].includes(k.kind));
    expect([...defaultVisibleKinds(slice)].sort()).toEqual(['part', 'term']);
  });

  it('takes the affordable kinds of a level and stops there', () => {
    // system contains both a cheap `part` and an expensive `detail`.
    const withDetail = [
      ...KINDS,
      {
        kind: 'detail',
        parents: [{ field: 'system', kind: 'system' }],
        refs: [],
        edge: null,
        fields: [],
      },
    ];
    const shown = defaultVisibleKinds(withDetail, { zone: 1, system: 2, part: 4, detail: 400 }, 80);
    expect([...shown].sort()).toEqual(['part', 'system', 'zone']);
  });
});

describe('perspectives', () => {
  const withPerspectives = () => {
    setModel({
      ...model(),
      perspectives: [
        { id: 'systems', label: 'Systems', kinds: ['zone', 'system', 'part'] },
        { id: 'people', label: 'People', kinds: ['term'] },
        { id: 'all', label: 'All', kinds: ['zone', 'system', 'part', 'term'] },
      ],
    });
    applyPrefs();
  };

  it('opens on the first perspective and its budgeted kinds', () => {
    withPerspectives();
    expect(activePerspective().id).toBe('systems');
    expect([...visibleKinds()].sort()).toEqual(['part', 'system', 'zone']);
  });

  it('switching narrows the canvas to that slice', () => {
    withPerspectives();
    selectPerspective('people');
    expect(activePerspective().id).toBe('people');
    expect([...visibleKinds()]).toEqual(['term']);
    // …and switching back restores the systems slice.
    selectPerspective('systems');
    expect([...visibleKinds()].sort()).toEqual(['part', 'system', 'zone']);
  });

  it('remembers a per-perspective kind selection', () => {
    withPerspectives();
    selectPerspective('systems');
    toggleKind('part'); // drop parts from the systems view
    expect([...visibleKinds()].sort()).toEqual(['system', 'zone']);
    selectPerspective('people');
    selectPerspective('systems');
    expect([...visibleKinds()].sort()).toEqual(['system', 'zone']);
  });

  it('takes the whole model for "All", budget or not', () => {
    withPerspectives();
    selectPerspective('all');
    expect([...visibleKinds()].sort()).toEqual(['part', 'system', 'term', 'zone']);
  });
});

describe('setNodePosition', () => {
  it('places a box where it was dropped', () => {
    setNodePosition('part:p', { x: 40, y: 90 }, ['part:p']);
    expect(positions().get('part:p')).toEqual({ x: 40, y: 90 });
  });

  it('shifts the whole family rather than letting one escape its parent', () => {
    pinPositions([
      ['system:s', { x: 0, y: 0 }],
      ['term:t', { x: 200, y: 40 }],
    ]);
    // Dropped 60 above and 30 left of the parent's content origin.
    setNodePosition('system:s', { x: -60, y: -30 }, ['system:s', 'term:t']);
    expect(positions().get('system:s')).toEqual({ x: 0, y: 0 });
    // The sibling moved by the same delta, so the arrangement is intact.
    expect(positions().get('term:t')).toEqual({ x: 260, y: 70 });
  });

  it('pins only boxes that are not placed yet', () => {
    setNodePosition('part:p', { x: 10, y: 10 }, ['part:p']);
    pinPositions([
      ['part:p', { x: 999, y: 999 }],
      ['term:t', { x: 5, y: 5 }],
    ]);
    expect(positions().get('part:p')).toEqual({ x: 10, y: 10 });
    expect(positions().get('term:t')).toEqual({ x: 5, y: 5 });
  });

  it('clears back to the automatic layout', () => {
    setNodePosition('part:p', { x: 10, y: 10 }, ['part:p']);
    clearPositions();
    expect(positions().size).toBe(0);
  });

  it('drops placements for nodes a switch of perspective removed', () => {
    setModel({
      ...model(),
      perspectives: [
        { id: 'a', label: 'A', kinds: ['zone', 'system'] },
        { id: 'b', label: 'B', kinds: ['term'] },
      ],
    });
    applyPrefs();
    setNodePosition('system:s', { x: 10, y: 10 }, ['system:s']);
    selectPerspective('b');
    expect(positions().size).toBe(0);
  });
});

describe('parentField', () => {
  it('answers the field a legal drop would write', () => {
    expect(parentField('part', 'system')).toBe('system');
    expect(parentField('system', 'zone')).toBe('zone');
  });

  it('is null when the schema declares no such containment', () => {
    expect(parentField('part', 'zone')).toBeNull();
    expect(parentField('zone', 'system')).toBeNull();
  });
});

describe('fieldOptional', () => {
  it('reports whether a node can be detached from its parent', () => {
    expect(fieldOptional('system', 'zone')).toBe(true);
    expect(fieldOptional('part', 'system')).toBe(false);
  });
});

describe('wouldCycle', () => {
  it('rejects dropping a node into its own descendant', () => {
    expect(wouldCycle('zone:z', 'part:p')).toBe(true);
    expect(wouldCycle('system:s', 'part:p')).toBe(true);
  });

  it('allows an unrelated target', () => {
    expect(wouldCycle('term:t', 'system:s')).toBe(false);
  });
});

describe('deletePlan', () => {
  it('cascades through children that REQUIRE the parent', () => {
    // part.system is required, so a part cannot outlive its system.
    const plan = deletePlan('system:s');
    expect(plan.deleted.sort()).toEqual(['part:p', 'system:s']);
    expect(plan.detached).toEqual([]);
  });

  it('frees children whose link is optional instead of deleting them', () => {
    // system.zone is optional: deleting the zone must not delete the estate.
    const plan = deletePlan('zone:z');
    expect(plan.deleted).toEqual(['zone:z']);
    expect(plan.detached).toEqual([{ key: 'system:s', field: 'zone' }]);
  });

  it('deletes a leaf alone', () => {
    expect(deletePlan('part:p')).toEqual({ deleted: ['part:p'], detached: [] });
  });
});

describe('subtreeKeys', () => {
  it('covers the node and everything under it', () => {
    expect([...subtreeKeys('zone:z')].sort()).toEqual(['part:p', 'system:s', 'zone:z']);
    expect([...subtreeKeys('part:p')]).toEqual(['part:p']);
  });
});

describe('kindOf', () => {
  it('resolves by kind name and by node', () => {
    expect(kindOf('system').type_name).toBe('w.System');
    expect(kindOf(node('part', 'x')).kind).toBe('part');
    expect(kindOf('nope')).toBeNull();
  });
});
