// Pure readers over the /api/audit payload: the changelog's sections, the
// link churn each row reports, the header strip's numbers, and the slice of
// the union graph that is drawn.
import { describe, expect, it } from 'vitest';

import {
  countsText,
  edgeNews,
  graphModel,
  healthTally,
  newsRows,
  openTarget,
  rangeLabel,
  sectionOf,
  severityTally,
  worseMetrics,
} from './audit';

/** One range over a five-node wskill: a unit added (unpinned), one removed,
    one modified, one left alone but broken by the removal, and an index
    whose nested level grew a pin. */
const data = {
  range: { before: '079d78a9c849f70d0147dd03e1283c871ebec6a7', after: null },
  summary: {
    units: { added: 1, removed: 1, modified: 1 },
    indexes: { added: 0, removed: 0, modified: 0 },
    edges: { added: 2, removed: 2 },
  },
  health: [
    { key: 'errors', label: 'errors', before: '0', after: '1', worse: true, moved: true },
    { key: 'unindexed_units', label: 'units no index pins', before: '1', after: '2', worse: true, moved: true },
    { key: 'edges_per_unit', label: 'links per unit', before: '0.33', after: '0.33', worse: false, moved: false },
  ],
  nodes: [
    {
      key: 'concept:delta',
      type: 'unit',
      id: 'delta',
      kind: 'concept',
      title: 'Delta',
      change: 'added',
      changed: [],
      graphed: true,
      news: true,
      findings: [{ severity: 'warn', rule: 'unindexed', message: 'no index pins it' }],
      x: 0, y: 0, w: 100, h: 48,
    },
    {
      key: 'concept:beta',
      type: 'unit',
      id: 'beta',
      kind: 'concept',
      title: 'Beta',
      change: 'removed',
      changed: [],
      file: 'data/concepts/beta.wcl',
      span: { start: 0, end: 30 },
      graphed: true,
      news: true,
      findings: [],
      x: 200, y: 0, w: 100, h: 48,
    },
    {
      key: 'concept:alpha',
      type: 'unit',
      id: 'alpha',
      kind: 'concept',
      title: 'Alpha',
      change: 'unchanged',
      changed: [],
      graphed: true,
      news: true,
      findings: [{ severity: 'error', rule: 'dangling-related', message: '`beta` names nothing' }],
      x: 400, y: 100, w: 100, h: 48,
    },
    {
      key: 'concept:gamma',
      type: 'unit',
      id: 'gamma',
      kind: 'concept',
      title: 'Gamma',
      change: 'modified',
      changed: ['title', 'related'],
      file: 'data/concepts/gamma.wcl',
      span: { start: 0, end: 42 },
      graphed: true,
      news: true,
      findings: [{ severity: 'candidate', rule: 'link-density', message: 'hub-shaped' }],
      x: 0, y: 200, w: 100, h: 48,
    },
    {
      key: 'concept:epsilon',
      type: 'unit',
      id: 'epsilon',
      kind: 'concept',
      title: 'Epsilon',
      change: 'unchanged',
      changed: [],
      graphed: true,
      news: false,
      findings: [],
      x: 400, y: 300, w: 100, h: 48,
    },
    {
      key: 'index:lang_sub',
      type: 'index',
      id: 'lang_sub',
      kind: 'index',
      title: 'Sub',
      change: 'modified',
      changed: ['related'],
      graphed: false,
      news: true,
      findings: [],
      x: 0, y: 0, w: 0, h: 0,
    },
  ],
  edges: [
    // A nested pin: drawn from the top-level index, WRITTEN in the sub-index.
    { from: 'index:lang', to: 'concept:delta', kind: 'pin', index_id: 'lang_sub', writer: 'index:lang_sub', change: 'added' },
    { from: 'concept:gamma', to: 'concept:alpha', kind: 'related', writer: 'concept:gamma', change: 'added' },
    { from: 'concept:alpha', to: 'concept:beta', kind: 'related', writer: 'concept:alpha', change: 'removed' },
    { from: 'index:lang', to: 'concept:beta', kind: 'pin', index_id: 'lang', writer: 'index:lang', change: 'removed' },
    { from: 'concept:epsilon', to: 'concept:alpha', kind: 'related', writer: 'concept:epsilon', change: 'unchanged' },
  ],
};

describe('the changelog', () => {
  it('sorts every newsworthy node into a section, unchanged-but-broken last', () => {
    const sections = newsRows(data);
    expect(sections.map((s) => s.key)).toEqual(['added', 'removed', 'modified', 'broken']);
    expect(sections[0].rows.map((r) => r.node.id)).toEqual(['delta']);
    expect(sections[1].rows.map((r) => r.node.id)).toEqual(['beta']);
    // A modified index rides the same section as a modified unit.
    expect(sections[2].rows.map((r) => r.node.id)).toEqual(['gamma', 'lang_sub']);
    expect(sections[3].rows.map((r) => r.node.id)).toEqual(['alpha']);
  });

  it('leaves a node the range neither touched nor broke out of it entirely', () => {
    const ids = newsRows(data).flatMap((s) => s.rows.map((r) => r.node.id));
    expect(ids).not.toContain('epsilon');
  });

  it('drops a section with no rows rather than heading an empty list', () => {
    const quiet = { ...data, nodes: data.nodes.filter((n) => n.change === 'added') };
    expect(newsRows(quiet).map((s) => s.key)).toEqual(['added']);
  });

  it('reports a nested pin under the sub-index that wrote it', () => {
    const churn = edgeNews(data);
    expect(churn['index:lang_sub'].map((e) => e.to)).toEqual(['concept:delta']);
    // Not under the top-level index it is DRAWN from, which did not change.
    expect(churn['index:lang'].map((e) => e.to)).toEqual(['concept:beta']);
    const sub = newsRows(data)
      .flatMap((s) => s.rows)
      .find((r) => r.node.id === 'lang_sub');
    expect(sub.edges.map((e) => e.change)).toEqual(['added']);
  });

  it('never reports an unchanged link as churn', () => {
    expect(edgeNews(data)['concept:epsilon']).toBeUndefined();
  });

  it('reads an unchanged node with findings as broken by the range', () => {
    expect(sectionOf({ change: 'unchanged' })).toBe('broken');
    expect(sectionOf({ change: 'added' })).toBe('added');
  });
});

describe('following a row to its source', () => {
  const row = (id) => data.nodes.find((n) => n.id === id);

  it('selects the block only when the spans are the working tree’s', () => {
    // `after: null` — the audit's other end IS the file on disk.
    expect(openTarget(data, row('gamma'))).toBe('span');
  });

  it('opens without selecting when the after end is a commit', () => {
    // Every surviving row is anchored where revision `b` writes it, so
    // those byte offsets address `b` and not the working-tree file.
    const committed = { ...data, range: { before: 'aaa', after: 'bbb' } };
    expect(openTarget(committed, row('gamma'))).toBe('file');
  });

  it('leads nowhere for a removal — the path is where it WAS written', () => {
    expect(openTarget(data, row('beta'))).toBe('none');
  });

  it('opens without selecting when a node carries no span at all', () => {
    expect(openTarget(data, { ...row('gamma'), span: undefined })).toBe('file');
    expect(openTarget(data, { ...row('gamma'), file: '' })).toBe('none');
  });
});

describe('the header strip', () => {
  it('counts the three severities apart', () => {
    // A candidate is a nomination, not a defect: it is never added to the
    // error count.
    expect(severityTally(data)).toEqual({ error: 1, warn: 1, candidate: 1 });
  });

  it('names only the metrics that moved the wrong way, over the total', () => {
    expect(worseMetrics(data).map((m) => m.key)).toEqual(['errors', 'unindexed_units']);
    expect(healthTally(data)).toEqual({ worse: 2, total: 3 });
  });

  it('names the working tree for what it is', () => {
    expect(rangeLabel(data)).toBe('079d78a9..(working tree)');
    expect(rangeLabel({ range: { before: 'aaaaaaaabbbb', after: 'ccccccccdddd' } })).toBe(
      'aaaaaaaa..cccccccc',
    );
  });

  it('drops the zero counts and says so when nothing moved', () => {
    expect(countsText(data.summary.units)).toBe('+1 −1 ~1');
    expect(countsText({ added: 0, removed: 2, modified: 0 })).toBe('−2');
    expect(countsText({ added: 0, removed: 0, modified: 0 })).toBe('—');
  });
});

describe('the union graph', () => {
  it('draws content, not navigation, and keeps removals as nodes', () => {
    const g = graphModel(data);
    expect(g.nodes.map((n) => n.id)).toEqual(['delta', 'beta', 'alpha', 'gamma', 'epsilon']);
    expect(g.nodes.find((n) => n.id === 'beta').change).toBe('removed');
    // An index is a changelog row, never a drawn node — so its pin edges
    // have no two drawn ends and drop out.
    expect(g.edges.every((e) => e.kind === 'related')).toBe(true);
    expect(g.box).toEqual({ x: -40, y: -40, w: 580, h: 428 });
  });

  it('scopes to what changed without ever dropping a removal', () => {
    const g = graphModel(data, { onlyChanged: true });
    // epsilon is untouched and touches no changed edge.
    expect(g.nodes.map((n) => n.id).sort()).toEqual(['alpha', 'beta', 'delta', 'gamma']);
    expect(g.edges.map((e) => e.change).sort()).toEqual(['added', 'removed']);
    expect(g.nodes.some((n) => n.change === 'removed')).toBe(true);
  });

  it('has a box even with nothing to draw', () => {
    expect(graphModel({ nodes: [], edges: [] }).box.w).toBeGreaterThan(0);
  });
});
