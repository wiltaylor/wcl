import { beforeEach, describe, expect, it, vi } from 'vitest';

import { elByLoc, locOf, placePins } from './frame';

beforeEach(() => {
  document.body.innerHTML = `
    <div data-wcl-page-file="unit.wcl" data-wcl-page-span="0:100"
         data-wcl-page-name="concept_alpha" data-wcl-slot="content">
      <p id="body" data-wcl-block data-wcl-kind="p"
         data-wcl-span="1:10" data-wcl-file="unit.wcl">Body</p>
      <button id="edit" data-wcl-edit-kind="concept" data-wcl-edit-target="alpha">Edit this</button>
    </div>
    <div data-wcl-page-file="unit.wcl" data-wcl-page-span="0:100"
         data-wcl-page-name="concept_alpha" data-wcl-slot="hero">
      <h1 id="hero" data-wcl-block data-wcl-kind="h1"
          data-wcl-span="20:40" data-wcl-file="unit.wcl">Hero</h1>
    </div>`;
});

describe('comment pins', () => {
  it('pins a page-less object comment to its edit-object anchor', () => {
    const onPin = vi.fn();
    const comment = {
      id: 'c-object',
      object_kind: 'concept',
      object_id: 'alpha',
      body: 'This unit should not exist.',
    };

    placePins(document, [comment], onPin);

    const target = document.getElementById('edit');
    expect(target.getAttribute('data-wcl-comment-id')).toBe('c-object');
    target.querySelector('.wcl-pin').click();
    expect(onPin).toHaveBeenCalledWith(comment);
  });

  it('round-trips and pins a block locator through its owning slot wrapper', () => {
    const content = document.querySelector('[data-wcl-slot="content"]');
    const hero = document.getElementById('hero');
    const loc = locOf(content, hero);
    expect(loc).toBe('@hero/0');
    expect(elByLoc(content, loc)).toBe(hero);

    placePins(document, [{ id: 'c-hero', loc, body: 'Sharpen this.' }]);
    expect(hero.getAttribute('data-wcl-comment-id')).toBe('c-hero');
  });

  it('resolves old unqualified locators against the content slot', () => {
    const firstWrapper = document.querySelector('[data-wcl-slot="content"]');
    expect(elByLoc(firstWrapper, '0')).toBe(document.getElementById('body'));
  });
});
