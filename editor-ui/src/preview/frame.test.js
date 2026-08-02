import { beforeEach, describe, expect, it, vi } from 'vitest';

import { placePins } from './frame';

beforeEach(() => {
  document.body.innerHTML = `
    <div data-wcl-page-file="unit.wcl" data-wcl-page-span="0:100" data-wcl-page-name="concept_alpha">
      <button id="edit" data-wcl-edit-kind="concept" data-wcl-edit-target="alpha">Edit this</button>
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
});
