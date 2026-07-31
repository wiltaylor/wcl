import { describe, expect, it, vi } from 'vitest';

import { createSurfaceHandle } from './surfaceref';

describe('createSurfaceHandle', () => {
  it('forwards calls (and their results) while attached', () => {
    const goto = vi.fn();
    const { handle } = createSurfaceHandle({ goto, doc: () => 'document' });
    handle.goto('/page.html');
    expect(goto).toHaveBeenCalledWith('/page.html');
    expect(handle.doc()).toBe('document');
    // Exactly `impl`'s members: a host asks the surface for things, it
    // doesn't interrogate the reference.
    expect(Object.keys(handle).sort()).toEqual(['doc', 'goto']);
  });

  it('goes inert once released — a held reference cannot act', () => {
    const goto = vi.fn();
    const { handle, release } = createSurfaceHandle({ goto, doc: () => 'document' });
    release();
    expect(handle.doc()).toBeUndefined();
    handle.goto('/page.html');
    expect(goto).not.toHaveBeenCalled();
  });

  it('releases every member, including ones captured before release', () => {
    const redecorate = vi.fn();
    const { handle, release } = createSurfaceHandle({ redecorate });
    const held = handle.redecorate;
    release();
    held();
    expect(redecorate).not.toHaveBeenCalled();
  });
});
