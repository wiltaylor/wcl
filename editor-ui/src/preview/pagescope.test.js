import { describe, expect, it } from 'vitest';

import { createPageScopes } from './pagescope';

describe('createPageScopes', () => {
  it('hands the outer page back when the only surface releases', () => {
    const scopes = createPageScopes();
    const canvas = scopes.push('index');
    canvas.note('guide');
    expect(canvas.release()).toEqual({ restore: true, page: 'index' });
  });

  it('restores the page BELOW it — a modal over the canvas', () => {
    const scopes = createPageScopes();
    const canvas = scopes.push(null);
    canvas.note('index');
    const modal = scopes.push('index');
    modal.note('unit-page');
    expect(modal.release()).toEqual({ restore: true, page: 'index' });
  });

  it('restores a null below it — a modal opened before anything was showing', () => {
    const scopes = createPageScopes();
    scopes.push(null); // the canvas, still building
    const modal = scopes.push(null);
    modal.note('unit-page');
    // The canvas had no page yet, so the modal's own page must not linger.
    expect(modal.release()).toEqual({ restore: true, page: null });
  });

  it('keeps the page when the LAST surface unmounts having found none', () => {
    // Nothing is waiting for it, and the shared page scopes every commit —
    // the graph and systems views read it after the canvas is gone.
    const scopes = createPageScopes();
    const canvas = scopes.push(null);
    canvas.note('index');
    expect(canvas.release()).toEqual({ restore: false });
  });

  it('gives nothing back when it is no longer on top', () => {
    // A tab switch mounts the next surface before dropping the last: the
    // one underneath must not claw the page back from the one above.
    const scopes = createPageScopes();
    const first = scopes.push(null);
    first.note('a');
    const second = scopes.push('a');
    second.note('b');
    expect(first.release()).toEqual({ restore: false });
    // Nothing is left underneath, so the page showing when it mounted.
    expect(second.release()).toEqual({ restore: true, page: 'a' });
  });

  it('ignores a double release', () => {
    const scopes = createPageScopes();
    const one = scopes.push('index');
    expect(one.release()).toEqual({ restore: true, page: 'index' });
    expect(one.release()).toEqual({ restore: false });
  });
});
