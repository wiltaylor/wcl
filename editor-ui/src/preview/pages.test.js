/* Page addressing and the manifest probe — the two pure helpers every
   preview host used to re-derive for itself. */

import { afterEach, describe, expect, it, vi } from 'vitest';

import { builtPageExists, dirOf, pageHref } from './pages';

describe('page addressing', () => {
  it('takes the output directory off a built page URL', () => {
    expect(dirOf('/api/preview/sites/docs_book_9f/index.html')).toBe(
      '/api/preview/sites/docs_book_9f/',
    );
    expect(dirOf(null)).toBe(null);
  });

  it('addresses a page beside the build it was given', () => {
    expect(pageHref('/api/preview/sites/docs_book_9f/index.html', 'concept_spans')).toBe(
      '/api/preview/sites/docs_book_9f/concept_spans.html',
    );
  });

  it('has no address without a build or without a page', () => {
    expect(pageHref(null, 'alpha')).toBe(null);
    expect(pageHref('/api/preview/sites/x/index.html', null)).toBe(null);
  });
});

describe('builtPageExists', () => {
  afterEach(() => vi.unstubAllGlobals());

  const withManifest = (body) =>
    vi.stubGlobal('fetch', async (url) => {
      expect(url).toBe('/api/preview/sites/x/_wdoc/pages.json');
      if (body instanceof Error) throw body;
      return { ok: true, json: async () => body };
    });

  it('reads the page list beside the build', async () => {
    withManifest({ start: 'index', pages: ['index', 'alpha'] });
    await expect(builtPageExists('/api/preview/sites/x/index.html', 'alpha')).resolves.toBe(true);
    await expect(builtPageExists('/api/preview/sites/x/index.html', 'beta')).resolves.toBe(false);
  });

  it('reads a missing or unreadable manifest as "no page"', async () => {
    withManifest(new Error('404'));
    await expect(builtPageExists('/api/preview/sites/x/index.html', 'alpha')).resolves.toBe(false);
  });

  it('needs both a build and a page', async () => {
    await expect(builtPageExists(null, 'alpha')).resolves.toBe(false);
    await expect(builtPageExists('/api/preview/sites/x/index.html', null)).resolves.toBe(false);
  });
});
