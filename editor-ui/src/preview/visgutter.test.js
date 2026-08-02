/* Unit tests for the per-block gutter: placement on top-level content
   blocks only, the profile button (with merged indicator tints), the
   drag-reorder step math, and the chrome/nested-block exclusions. The
   stamps behind the tints are read through the anchor model (tested in
   anchors.test.js). Runs under happy-dom like the wysiwyg tests. */

import { describe, expect, it, vi } from 'vitest';

import { moveSteps, placeVisGutters } from './visgutter';

const PAGE = `
<div data-wcl-page-file="main.wcl" data-wcl-page-span="0:300"
     data-wcl-page-name="concept_x" data-wcl-slot="content">
  <p id="plain" data-wcl-block data-wcl-kind="p"
     data-wcl-span="10:20" data-wcl-file="main.wcl">always</p>
  <p id="hidden" data-wcl-block data-wcl-kind="p"
     data-wcl-span="30:60" data-wcl-file="main.wcl"
     data-wcl-except="deck other">not in deck</p>
  <p id="custom" data-wcl-block data-wcl-kind="p"
     data-wcl-span="70:90" data-wcl-file="main.wcl"
     data-wcl-vis="custom">only-decorated</p>
  <div id="outer" data-wcl-block data-wcl-kind="list"
       data-wcl-span="100:200" data-wcl-file="main.wcl">
    <p id="nested" data-wcl-block data-wcl-kind="li"
       data-wcl-span="110:130" data-wcl-file="main.wcl">nested</p>
  </div>
  <div id="chrome" data-wcl-block data-wcl-kind="p"
       data-wcl-span="0:5" data-wcl-file="chrome-placeholder">chrome</div>
</div>
<div data-wcl-page-file="main.wcl" data-wcl-page-span="0:300"
     data-wcl-page-name="concept_x" data-wcl-slot="hero">
  <h1 id="hero" data-wcl-block data-wcl-kind="h1"
      data-wcl-span="210:230" data-wcl-file="main.wcl">Hero</h1>
</div>`;

function setup() {
  document.body.innerHTML = PAGE;
  // happy-dom keeps `&lt;` undecoded in attribute values, so stamp the
  // system path (what the build emits for template chrome) directly.
  document
    .getElementById('chrome')
    .setAttribute('data-wcl-file', '<wcl-system>/templates.wcl');
  return document;
}

describe('moveSteps', () => {
  it('maps an insertion slot to a signed sibling displacement', () => {
    const list = [1, 2, 3, 4];
    expect(moveSteps(list, 0, 3)).toBe(2); // drop before index 3 → down 2
    expect(moveSteps(list, 3, 0)).toBe(-3); // to the top → up 3
    expect(moveSteps(list, 1, 4)).toBe(2); // to the end → down 2
  });

  it('is a no-op for the slots around the block itself', () => {
    const list = [1, 2, 3];
    expect(moveSteps(list, 1, 1)).toBe(0); // before itself
    expect(moveSteps(list, 1, 2)).toBe(0); // right after itself
  });
});

describe('placeVisGutters', () => {
  it('decorates top-level content blocks only', () => {
    setup();
    placeVisGutters(document, { onProfile: () => {} });
    expect(document.querySelectorAll('#plain > .wcl-vis-gutter').length).toBe(1);
    expect(document.querySelectorAll('#hidden > .wcl-vis-gutter').length).toBe(1);
    expect(document.querySelectorAll('#outer > .wcl-vis-gutter').length).toBe(1);
    expect(document.querySelectorAll('#hero > .wcl-vis-gutter').length).toBe(1);
    // Nested blocks keep the block toolbar's controls instead.
    expect(document.querySelectorAll('#nested > .wcl-vis-gutter').length).toBe(0);
    // Template chrome gets no gutter.
    expect(document.querySelectorAll('#chrome > .wcl-vis-gutter').length).toBe(0);
  });

  it('is idempotent per placement pass', () => {
    setup();
    placeVisGutters(document, { onProfile: () => {} });
    placeVisGutters(document, { onProfile: () => {} });
    // Four content blocks + the hero — one gutter each, no duplicates.
    expect(document.querySelectorAll('.wcl-vis-gutter').length).toBe(5);
  });

  it('profile button pops the editor with the block anchor', () => {
    setup();
    const onProfile = vi.fn();
    placeVisGutters(document, { onProfile });
    const btn = document.querySelector('#hidden .wcl-vis-profile');
    btn.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(onProfile).toHaveBeenCalledWith({ file: 'main.wcl', span: { start: 30, end: 60 } });
  });

  it('tints the profile button from the merged stamps', () => {
    setup();
    placeVisGutters(document, { merged: true, onProfile: () => {} });
    const btn = (id) => document.querySelector(`#${id} .wcl-vis-profile`);
    expect(btn('plain').classList.contains('is-partial')).toBe(false);
    expect(btn('hidden').classList.contains('is-partial')).toBe(true);
    expect(btn('hidden').title).toContain('deck');
    expect(btn('custom').classList.contains('is-custom')).toBe(true);
    // Outside merged builds there are no stamps to reflect.
    placeVisGutters(document, { onProfile: () => {} });
    expect(btn('hidden').classList.contains('is-partial')).toBe(false);
  });

  it('ghosts blocks hidden in the rendering view', () => {
    setup();
    placeVisGutters(document, { merged: true, currentSite: 'deck', onProfile: () => {} });
    expect(document.getElementById('hidden').classList.contains('wcl-vis-ghost')).toBe(true);
    expect(document.getElementById('plain').classList.contains('wcl-vis-ghost')).toBe(false);
  });

  it('ignores profile clicks while disabled', () => {
    setup();
    const onProfile = vi.fn();
    placeVisGutters(document, { onProfile, enabled: () => false });
    document
      .querySelector('#plain .wcl-vis-profile')
      .dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(onProfile).not.toHaveBeenCalled();
  });

  it('drag on the handle commits a reorder with the step delta', () => {
    setup();
    const onReorder = vi.fn();
    placeVisGutters(document, { onReorder });
    // Fake layout: the four same-file blocks stacked 100px apart.
    const ids = ['plain', 'hidden', 'custom', 'outer'];
    ids.forEach((id, i) => {
      document.getElementById(id).getBoundingClientRect = () => ({
        top: i * 100,
        bottom: i * 100 + 80,
        height: 80,
        left: 0,
        right: 500,
        width: 500,
      });
    });
    const handle = document.querySelector('#plain .wcl-vis-handle');
    expect(handle).toBeTruthy();
    handle.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, clientY: 20 }));
    // Below `custom`'s midpoint (240), above `outer`'s (340) → slot 3.
    document.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientY: 300 }));
    document.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, clientY: 300 }));
    expect(onReorder).toHaveBeenCalledWith(
      expect.objectContaining({
        file: 'main.wcl',
        span: { start: 10, end: 20 },
        steps: 2,
        dropIdx: 3,
        el: document.getElementById('plain'),
      }),
    );
  });

  it('drag cancelled with Escape commits nothing', () => {
    setup();
    const onReorder = vi.fn();
    placeVisGutters(document, { onReorder });
    const handle = document.querySelector('#hidden .wcl-vis-handle');
    handle.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, clientY: 120 }));
    document.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientY: 400 }));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    document.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, clientY: 400 }));
    expect(onReorder).not.toHaveBeenCalled();
  });
});
