/* Plain search over the unit graph: what matches, which field a hit is
   attributed to, how hits rank, and what the reader sees of the match. */

import { describe, expect, it } from 'vitest';

import { searchUnits } from './search';

const unit = (over) => ({
  key: `concept:${over.id}`,
  type: 'unit',
  kind: 'concept',
  id: 'x',
  title: 'X',
  summary: '',
  text: '',
  ...over,
});

const ids = (hits) => hits.map((h) => h.node.id);

describe('searchUnits', () => {
  const nodes = [
    unit({ id: 'spans', title: 'Spans', summary: 'Byte ranges into a source' }),
    unit({
      id: 'etags',
      title: 'Conflict detection',
      summary: 'How a save refuses stale writes',
      text: 'The commit pipeline hashes the file.\nA stale etag answers 409.',
    }),
    unit({ id: 'lexer', title: 'The lexer', text: 'Hand-written, no generators.' }),
  ];

  it('finds nothing without a query', () => {
    expect(searchUnits(nodes, '')).toEqual([]);
    expect(searchUnits(nodes, '   ')).toEqual([]);
    expect(searchUnits(nodes, null)).toEqual([]);
  });

  it('survives a missing node list', () => {
    expect(searchUnits(null, 'spans')).toEqual([]);
    expect(searchUnits(undefined, 'spans')).toEqual([]);
  });

  it('matches on id, name, summary and body', () => {
    expect(ids(searchUnits(nodes, 'lexer'))).toEqual(['lexer']);
    expect(ids(searchUnits(nodes, 'conflict'))).toEqual(['etags']);
    expect(ids(searchUnits(nodes, 'refuses'))).toEqual(['etags']);
    expect(ids(searchUnits(nodes, 'generators'))).toEqual(['lexer']);
  });

  it('ignores case and matches inside a word', () => {
    expect(ids(searchUnits(nodes, 'LEXER'))).toEqual(['lexer']);
    expect(ids(searchUnits(nodes, 'enerat'))).toEqual(['lexer']);
  });

  it('attributes a hit to the narrowest field carrying it', () => {
    // "spans" is the id AND (as the title) the name — the id wins.
    expect(searchUnits(nodes, 'spans')[0].field).toBe('id');
    expect(searchUnits(nodes, 'conflict')[0].field).toBe('name');
    expect(searchUnits(nodes, 'refuses')[0].field).toBe('summary');
    expect(searchUnits(nodes, '409')[0].field).toBe('body');
  });

  it('requires every term, across fields', () => {
    // "etags" is the id, "409" is in the body — one unit carries both.
    expect(ids(searchUnits(nodes, 'etags 409'))).toEqual(['etags']);
    expect(searchUnits(nodes, 'etags nonesuch')).toEqual([]);
  });

  it('ranks an id hit over a name hit over prose', () => {
    const set = [
      unit({ id: 'span_math', title: 'Arithmetic', text: 'about spans' }),
      unit({ id: 'spans', title: 'Spans' }),
      unit({ id: 'ranges', title: 'Spans and ranges' }),
    ];
    expect(ids(searchUnits(set, 'spans'))).toEqual(['spans', 'ranges', 'span_math']);
  });

  it('shows the matching line as the snippet', () => {
    const hit = searchUnits(nodes, '409')[0];
    expect(hit.snippet).toBe('A stale etag answers 409.');
  });

  it('windows a long line around the match, leaving it near the front', () => {
    const long = unit({
      id: 'long',
      text: `${'filler word '.repeat(30)}needle${' trailing word'.repeat(30)}`,
    });
    const hit = searchUnits([long], 'needle')[0];
    expect(hit.snippet).toContain('needle');
    expect(hit.snippet.length).toBeLessThan(200);
    expect(hit.snippet.startsWith('…')).toBe(true);
    expect(hit.snippet.endsWith('…')).toBe(true);
    // The host clips the line: leading context would push the match out of
    // sight, so it stays close to the start.
    expect(hit.at).toBeLessThan(30);
  });

  it('reports where in the snippet the match sits, for highlighting', () => {
    const hit = searchUnits(nodes, '409')[0];
    expect(hit.snippet.slice(hit.at, hit.at + hit.length)).toBe('409');
  });
});
