// Pure helpers over the /api/graph payload: membership counts and the
// nested-index lookups behind the index panel's focus-reveal.
import { describe, expect, it } from 'vitest';

import { indexHitsForUnit, panToInclude, pinCounts, subtreePinnedIds } from './graph';

/** A payload with one top index, a sub-index, and a sub-sub-index:
    `a` pinned at top AND sub-sub level, `b` only in the sub level; a
    lesson organized by the training syllabus (never index-pinned). */
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
    { key: 'lesson:l1', type: 'unit', id: 'l1', organized: ['training'] },
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

  it('counts structurally organized units as members of their site', () => {
    const c = pinCounts(data);
    // A lesson lives in the training syllabus by construction — never an
    // orphan, even with zero pin edges.
    expect(c['lesson:l1']).toEqual({ total: 1, sites: { training: 1 } });
    // Ordinary units are unaffected.
    expect(c['concept:a'].sites.training).toBeUndefined();
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

describe('panToInclude', () => {
  const VB = { x: 0, y: 0, w: 800, h: 600 };
  const BOX = { w: 120, h: 48 };
  const sees = (vb, p) =>
    p.x >= vb.x && p.x + BOX.w <= vb.x + vb.w && p.y >= vb.y && p.y + BOX.h <= vb.y + vb.h;

  it('leaves a visible node alone', () => {
    expect(panToInclude(VB, BOX, { x: 300, y: 200 })).toBe(null);
  });

  it('brings an off-screen node into view from any side', () => {
    for (const p of [
      { x: 900, y: 200 },
      { x: -200, y: 200 },
      { x: 300, y: 700 },
      { x: 300, y: -150 },
    ]) {
      const next = panToInclude(VB, BOX, p);
      expect(next, `expected a pan for ${JSON.stringify(p)}`).not.toBe(null);
      expect(sees(next, p)).toBe(true);
    }
  });

  it('pans without zooming', () => {
    const next = panToInclude(VB, BOX, { x: 2000, y: 2000 });
    expect(next.w).toBe(VB.w);
    expect(next.h).toBe(VB.h);
  });

  it('keeps a margin around the node', () => {
    const next = panToInclude(VB, BOX, { x: -200, y: 200 }, 40);
    expect(next.x).toBe(-240);
  });
});
