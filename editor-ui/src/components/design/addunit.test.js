import { describe, expect, it } from 'vitest';

import { UNPINNED, placementOptions, selectedPin } from './addunit';

const indexes = [
  { id: 'start', title: 'Start here' },
  { id: 'depth', title: 'In depth' },
];

describe('add-unit placement', () => {
  it('makes an index placement the normal path and unpinned creation an explicit exception', () => {
    expect(placementOptions(indexes)).toEqual([
      { value: 'start', label: 'Pin into: Start here' },
      { value: 'depth', label: 'Pin into: In depth' },
      { value: UNPINNED, label: 'Exception: create unpinned' },
    ]);
    expect(selectedPin(indexes, '')).toEqual({ error: 'Choose a section or the unpinned exception' });
    expect(selectedPin(indexes, 'start')).toEqual({ pin: { index_id: 'start' } });
    expect(selectedPin(indexes, UNPINNED)).toEqual({ pin: null });
  });

  it('allows unpinned creation when there is no index to choose', () => {
    expect(selectedPin([], '')).toEqual({ pin: null });
  });
});
