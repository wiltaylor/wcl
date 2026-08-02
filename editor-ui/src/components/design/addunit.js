export const UNPINNED = '__wcl_unpinned__';

export function placementOptions(indexes) {
  return [
    ...indexes.map((n) => ({ value: n.id, label: `Pin into: ${n.title}` })),
    { value: UNPINNED, label: 'Exception: create unpinned' },
  ];
}

export function selectedPin(indexes, selection) {
  if (indexes.length > 0 && !selection) {
    return { error: 'Choose a section or the unpinned exception' };
  }
  return { pin: selection && selection !== UNPINNED ? { index_id: selection } : null };
}
