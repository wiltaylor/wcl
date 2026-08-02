/* The preview module's observable behaviour, with the request layer stubbed
   — the same seam the rest of the state layer is tested through, so a whole
   build/commit/staleness lifecycle is assertable without a server or an
   iframe. Nothing here asserts how the cache is keyed internally; every
   assertion is about what was requested and what a host would mount. */

import { createRoot, createSignal } from 'solid-js';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// Importing the state layer pulls in the LSP client, which opens a WebSocket
// at module load, and the review long-poll, which fires a fetch. Stub both
// before the import graph is evaluated (vi.hoisted runs first). The fetch
// stub also stands in for the built site's `_wdoc/pages.json` manifest —
// `MANIFEST.pages` is what the probe sees.
const state = vi.hoisted(() => {
  const manifest = { pages: ['alpha', 'beta'] };
  globalThis.WebSocket = class {
    constructor() {
      this.readyState = 0;
    }
    send() {}
    close() {}
    addEventListener() {}
  };
  globalThis.fetch = async () => ({ ok: true, json: async () => manifest });
  return { manifest };
});

import { emitCommit } from './commits';
import { createPreview } from './preview';

/** Let Solid's effects and the awaited request settle. */
const tick = async () => {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
  await new Promise((r) => setTimeout(r, 0));
};

/** A stub request layer recording every call. */
function stubRequest({ fail = false } = {}) {
  const calls = [];
  const fn = async (entry, site, files, extra) => {
    calls.push({ entry, site, files, extra });
    if (fail) return { ok: false, error: 'boom' };
    return { ok: true, mode: 'targeted', href: `/api/preview/sites/${entry}_${site ?? ''}/index.html` };
  };
  fn.calls = calls;
  return fn;
}

const SITE = { entry: 'docs/main.wcl', site: 'book' };

/** Run `body` inside a Solid root and dispose it afterwards. */
function withRoot(body) {
  let dispose;
  const out = createRoot((d) => {
    dispose = d;
    return body();
  });
  return { ...out, dispose };
}

beforeEach(() => {
  state.manifest.pages = ['alpha', 'beta'];
});

describe('createPreview', () => {
  it('mounts the built page for a page target', async () => {
    const request = stubRequest();
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => ({ ...SITE, page: 'alpha' }), { request, files: () => [] }),
    }));
    await tick();
    expect(request.calls).toHaveLength(1);
    expect(request.calls[0].extra.pages).toEqual(['alpha']);
    expect(preview.src()).toBe('/api/preview/sites/docs/main.wcl_book/alpha.html');
    expect(preview.hasPage()).toBe(true);
    expect(preview.status()).toBe('ready');
    dispose();
  });

  it('mounts the build index (and does not name a page) for a whole-site target', async () => {
    const request = stubRequest();
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, { request, files: () => [] }),
    }));
    await tick();
    expect(request.calls[0].extra.pages).toBeUndefined();
    expect(preview.src()).toBe('/api/preview/sites/docs/main.wcl_book/index.html');
    dispose();
  });

  it('reports a page the build does not contain rather than mounting the index', async () => {
    state.manifest.pages = ['beta'];
    const request = stubRequest();
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => ({ ...SITE, page: 'alpha' }), { request, files: () => [] }),
    }));
    await tick();
    expect(preview.hasPage()).toBe(false);
    expect(preview.src()).toBe(null);
    dispose();
  });

  it('does not start the same build twice while one is in flight', async () => {
    const request = stubRequest();
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, { request, auto: false, files: () => [] }),
    }));
    const a = preview.build();
    const b = preview.build();
    expect(a).toBe(b);
    await Promise.all([a, b]);
    expect(request.calls).toHaveLength(1);
    dispose();
  });

  it('reuses a cached build and rebuilds when the target changes', async () => {
    const request = stubRequest();
    const [page, setPage] = createSignal('alpha');
    const [shown, setShown] = createSignal(true);
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => ({ ...SITE, page: page() }), {
        request,
        active: shown,
        files: () => [],
      }),
    }));
    await tick();
    expect(request.calls).toHaveLength(1);

    // Hiding and re-showing the same target is a cache hit: no build.
    setShown(false);
    await tick();
    setShown(true);
    await tick();
    expect(request.calls).toHaveLength(1);

    // A different page is a different target.
    setPage('beta');
    await tick();
    expect(request.calls).toHaveLength(2);
    expect(preview.src()).toBe('/api/preview/sites/docs/main.wcl_book/beta.html');

    // …and going back to the first mounts its cached build unchanged.
    setPage('alpha');
    await tick();
    expect(request.calls).toHaveLength(2);
    expect(preview.src()).toBe('/api/preview/sites/docs/main.wcl_book/alpha.html');
    dispose();
  });

  it('rebuilds in place when the target is unchanged but the build is newer', async () => {
    const request = stubRequest();
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, { request, files: () => [] }),
    }));
    await tick();
    const before = { src: preview.src(), seq: preview.reloadSeq() };
    await preview.build({ changed: ['docs/a.wcl'] });
    expect(preview.src()).toBe(before.src);
    expect(preview.reloadSeq()).toBeGreaterThan(before.seq);
    expect(request.calls[1].extra.changed).toEqual(['docs/a.wcl']);
    dispose();
  });

  it('surfaces a failed build as an error the host can retry, and does not loop', async () => {
    let fail = true;
    const calls = [];
    const request = async (...args) => {
      calls.push(args);
      return fail ? { ok: false, error: 'boom' } : { ok: true, href: '/api/preview/sites/x/index.html' };
    };
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, { request, files: () => [] }),
    }));
    await tick();
    expect(preview.status()).toBe('error');
    expect(preview.error()).toBe('boom');
    expect(preview.src()).toBe(null);
    // A failed build is not retried behind the author's back.
    await tick();
    expect(calls).toHaveLength(1);

    fail = false;
    await preview.build();
    expect(preview.status()).toBe('ready');
    expect(preview.error()).toBe(null);
    dispose();
  });

  it('goes stale on a commit that repaired another surface, and rebuilds when shown', async () => {
    const request = stubRequest();
    const [shown, setShown] = createSignal(true);
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, { request, active: shown, files: () => [] }),
    }));
    await tick();
    expect(request.calls).toHaveLength(1);

    // Hidden when the commit lands: it must not rebuild until it is shown.
    setShown(false);
    await tick();
    emitCommit({ surface: 'preview-somewhere-else' });
    await tick();
    expect(request.calls).toHaveLength(1);
    expect(preview.stale()).toBe(true);

    setShown(true);
    await tick();
    expect(request.calls).toHaveLength(2);
    expect(preview.stale()).toBe(false);
    dispose();
  });

  it('leaves the surface a commit rebuilt through alone', async () => {
    const request = stubRequest();
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, { request, files: () => [] }),
    }));
    await tick();
    emitCommit({ surface: preview.id });
    await tick();
    expect(request.calls).toHaveLength(1);
    expect(preview.stale()).toBe(false);
    dispose();
  });

  it('does not reload a surface that patched its own DOM — until it is hidden', async () => {
    const request = stubRequest();
    const [shown, setShown] = createSignal(true);
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, { request, active: shown, files: () => [] }),
    }));
    await tick();
    // An in-place commit: the frame on screen is right, the build is not.
    emitCommit({ surface: preview.id, patched: true });
    await tick();
    expect(request.calls).toHaveLength(1);
    expect(preview.stale()).toBe(true);

    // Hiding it throws the patched DOM away, so showing it again has to
    // rebuild rather than reload the stale bytes from the same URL.
    setShown(false);
    await tick();
    setShown(true);
    await tick();
    expect(request.calls).toHaveLength(2);
    dispose();
  });

  it('drops the in-place exemption when a later commit lands elsewhere', async () => {
    const request = stubRequest();
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, { request, files: () => [] }),
    }));
    await tick();
    emitCommit({ surface: preview.id, patched: true });
    await tick();
    expect(request.calls).toHaveLength(1);
    // Someone else wrote the same source: the patched DOM is out of date now.
    emitCommit({ surface: null });
    await tick();
    expect(request.calls).toHaveLength(2);
    dispose();
  });

  it('spares only the repaired surface’s own target, not its other cached ones', async () => {
    const request = stubRequest();
    const [page, setPage] = createSignal('alpha');
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => ({ ...SITE, page: page() }), { request, files: () => [] }),
    }));
    await tick();
    setPage('beta');
    await tick();
    expect(request.calls).toHaveLength(2);

    // The commit landed while `beta` was on screen: `alpha`'s cached build
    // is as stale as any other surface's.
    emitCommit({ surface: preview.id });
    await tick();
    expect(request.calls).toHaveLength(2); // beta is untouched
    setPage('alpha');
    await tick();
    expect(request.calls).toHaveLength(3);
    dispose();
  });

  it('accepts a list of spared surfaces (the all-views tab rebuilds several)', async () => {
    const request = stubRequest();
    const { a, b, dispose } = withRoot(() => ({
      a: createPreview(() => SITE, { request, files: () => [] }),
      b: createPreview(() => ({ ...SITE, site: 'deck' }), { request, files: () => [] }),
    }));
    await tick();
    expect(request.calls).toHaveLength(2);
    emitCommit({ surface: [a.id, b.id] });
    await tick();
    expect(request.calls).toHaveLength(2);
    expect(a.stale()).toBe(false);
    expect(b.stale()).toBe(false);
    dispose();
  });

  it('builds a skill target as a folder listing, with nothing to mount', async () => {
    const request = async () => ({ ok: true, mode: 'skill', base: '/api/preview/sites/s/', files: ['SKILL.md'] });
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => ({ ...SITE, skill: true }), { request, files: () => [] }),
    }));
    await tick();
    expect(preview.mode()).toBe('skill');
    expect(preview.base()).toBe('/api/preview/sites/s/');
    expect(preview.files()).toEqual(['SKILL.md']);
    expect(preview.src()).toBe(null);
    dispose();
  });

  it('sends the merged / unit / overlay parts of a target verbatim', async () => {
    const request = stubRequest();
    const { dispose } = withRoot(() => ({
      preview: createPreview(
        () => ({ ...SITE, page: 'alpha', merged: true, unit: { kind: 'screen', id: 'login' } }),
        { request, files: () => ['docs/a.wcl'] },
      ),
    }));
    await tick();
    expect(request.calls[0].files).toEqual(['docs/a.wcl']);
    expect(request.calls[0].extra).toMatchObject({
      pages: ['alpha'],
      merged: true,
      unit: { kind: 'screen', id: 'login' },
    });
    dispose();
  });

  it('takes its build hint from refreshPage when the target names no page', async () => {
    const request = stubRequest();
    const { dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, {
        request,
        files: () => [],
        refreshPage: () => 'beta',
      }),
    }));
    await tick();
    expect(request.calls[0].extra.pages).toEqual(['beta']);
    dispose();
  });

  it('stops listening for commits once its host is gone', async () => {
    const request = stubRequest();
    const { preview, dispose } = withRoot(() => ({
      preview: createPreview(() => SITE, { request, files: () => [] }),
    }));
    await tick();
    dispose();
    emitCommit({ surface: null });
    await tick();
    expect(request.calls).toHaveLength(1);
    expect(preview.stale()).toBe(false);
  });
});
