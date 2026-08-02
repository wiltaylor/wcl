import { describe, expect, it } from 'vitest';

import { commentsForPage } from './comments';

describe('commentsForPage', () => {
  it('includes object-only findings when the rendered page exposes that object', () => {
    const page = {
      name: 'concept_alpha',
      file: 'unit.wcl',
      objects: [{ kind: 'concept', id: 'alpha' }],
    };
    const pageComment = { id: 'page', page: 'concept_alpha', page_file: 'unit.wcl' };
    const objectComment = {
      id: 'object',
      object_kind: 'concept',
      object_id: 'alpha',
    };
    const elsewhere = { id: 'elsewhere', object_kind: 'concept', object_id: 'beta' };

    expect(commentsForPage([pageComment, objectComment, elsewhere], page)).toEqual([
      pageComment,
      objectComment,
    ]);
  });
});
