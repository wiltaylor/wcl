// Pure helpers over the /api/graph payload: membership counts and the
// nested-index lookups behind the index panel's focus-reveal.
import { describe, expect, it } from 'vitest';

import { indexHitsForUnit, pinCounts, subtreePinnedIds } from './graph';

/** A payload with one top index, a sub-index, and a sub-sub-index:
    `a` pinned at top AND sub-sub level, `b` only in the sub level. */
const data = {
  sites: ['book', 'deck'],
  nodes: [
    {
      key: 'index:top',
      type: 'index',
      id: 'top',
      pinned: ['a'],
      views: { book: true, deck: false },
      children: [
        {
          id: 'sub',
          pinned: ['b'],
          children: [{ id: 'subsub', pinned: ['a'], children: [] }],
        },
      ],
    },
    { key: 'concept:a', type: 'unit', id: 'a' },
    { key: 'concept:b', type: 'unit', id: 'b' },
  ],
  edges: [
    { from: 'index:top', to: 'concept:a', kind: 'pin', index_id: 'top' },
    { from: 'index:top', to: 'concept:b', kind: 'pin', index_id: 'sub' },
    { from: 'index:top', to: 'concept:a', kind: 'pin', index_id: 'subsub' },
    { from: 'concept:a', to: 'concept:b', kind: 'related' },
  ],
};

describe('pinCounts', () => {
  it('counts every pin edge, including sub-index attributions', () => {
    const c = pinCounts(data);
    expect(c['concept:a']).toEqual({ total: 2, sites: { book: 2 } });
    expect(c['concept:b']).toEqual({ total: 1, sites: { book: 1 } });
  });

  it('ignores related edges and handles a missing payload', () => {
    expect(pinCounts(null)).toEqual({});
    expect(pinCounts(data)['concept:a'].total).toBe(2);
  });
});

describe('subtreePinnedIds', () => {
  it('collects ids across every nesting level', () => {
    const ids = subtreePinnedIds(data.nodes[0]);
    expect([...ids].sort()).toEqual(['a', 'b']);
  });

  it('is empty for a bare level', () => {
    expect(subtreePinnedIds({ pinned: [], children: [] }).size).toBe(0);
  });
});

describe('indexHitsForUnit', () => {
  it('reports every owning level in payload order', () => {
    expect(indexHitsForUnit(data, 'a')).toEqual([
      { topKey: 'index:top', indexId: 'top' },
      { topKey: 'index:top', indexId: 'subsub' },
    ]);
    expect(indexHitsForUnit(data, 'b')).toEqual([{ topKey: 'index:top', indexId: 'sub' }]);
  });

  it('returns no hits for an unknown unit or missing payload', () => {
    expect(indexHitsForUnit(data, 'zzz')).toEqual([]);
    expect(indexHitsForUnit(null, 'a')).toEqual([]);
  });
});
