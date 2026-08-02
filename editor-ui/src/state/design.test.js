import { beforeEach, describe, expect, it, vi } from 'vitest';

const fakes = vi.hoisted(() => ({
  curator: vi.fn(),
  emitCommit: vi.fn(),
  toast: vi.fn(),
  selected: vi.fn(() => ({ registry: 'skills/demo/wskill.wcl', wskill: true })),
}));

vi.mock('../api', () => ({
  api: {
    curator: fakes.curator,
  },
}));
vi.mock('./buffers', () => ({
  applyDiskUpdate: vi.fn(),
  buffers: { buffers: {} },
  buffer: vi.fn(),
  openFile: vi.fn(),
  saveBuffer: vi.fn(),
}));
vi.mock('./commits', () => ({ emitCommit: fakes.emitCommit }));
vi.mock('./graph', () => ({ reloadGraph: vi.fn() }));
vi.mock('./preview', () => ({
  currentPage: vi.fn(),
  mainPreview: { id: 'main', invalidate: vi.fn(), build: vi.fn() },
}));
vi.mock('./sites', () => ({
  activeEntry: vi.fn(() => 'projection/main.wcl'),
  activeSite: vi.fn(() => 'book'),
  selected: fakes.selected,
}));
vi.mock('./tree', () => ({ treeData: vi.fn() }));
vi.mock('./views', () => ({ revealSpan: vi.fn() }));
vi.mock('@forge/ui', () => ({ toast: fakes.toast }));

import {
  busy,
  designTab,
  pendingAuditRange,
  runCurator,
  setDesignTab,
  setPendingAuditRange,
} from './design';

describe('curator → audit navigation', () => {
  beforeEach(() => {
    fakes.curator.mockReset();
    fakes.emitCommit.mockReset();
    fakes.toast.mockReset();
    setDesignTab('graph');
    setPendingAuditRange(null);
  });

  it('opens the audit tab on the exact committed range', async () => {
    fakes.curator.mockResolvedValue({
      ok: true,
      status: 'committed',
      commit: 'def456',
      range: 'abc123..def456',
      message: 'Curated the graph',
    });

    await runCurator({ scope: 'whole_graph' });

    expect(fakes.curator).toHaveBeenCalledWith('skills/demo/wskill.wcl', {
      scope: 'whole_graph',
    });
    expect(pendingAuditRange()).toBe('abc123..def456');
    expect(designTab()).toBe('audit');
    expect(fakes.emitCommit).toHaveBeenCalledWith({ surface: null });
    expect(busy()).toBe(false);
  });

  it('reports a failed gate without opening an audit or claiming a commit', async () => {
    fakes.curator.mockResolvedValue({
      ok: false,
      error: 'curator pass failed: projection `book` failed to build',
    });

    await runCurator({ scope: 'index', index: 'reference' });

    expect(designTab()).toBe('graph');
    expect(pendingAuditRange()).toBe(null);
    expect(fakes.emitCommit).not.toHaveBeenCalled();
    expect(fakes.toast).toHaveBeenCalledWith(
      'curator pass failed: projection `book` failed to build',
      expect.objectContaining({ tone: 'danger' }),
    );
    expect(busy()).toBe(false);
  });

  it('keeps a successful no-op on the graph', async () => {
    fakes.curator.mockResolvedValue({
      ok: true,
      status: 'no_changes',
      message: 'No candidates in scope',
    });

    await runCurator({ scope: 'index', index: 'reference' });

    expect(designTab()).toBe('graph');
    expect(pendingAuditRange()).toBe(null);
    expect(fakes.emitCommit).not.toHaveBeenCalled();
    expect(fakes.toast).toHaveBeenCalledWith(
      'No candidates in scope',
      expect.objectContaining({ tone: 'success' }),
    );
  });
});
