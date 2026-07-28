/* View selection for a grouped wskill node.
   A wskill collapses into one picker entry whose projections (book / deck /
   training / skill) are views of the same model, so the preview target comes
   from `activeView()`. Regression: with no view chosen this fell back to the
   first non-skill view — the book — which made the course and the deck
   unreachable from Code mode, where no view control existed at all. */

import { describe, expect, it, beforeEach, vi } from 'vitest';

// Importing the state layer pulls in the LSP client, which opens a WebSocket
// at module load, and the review long-poll, which fires a fetch. Stub both
// before the import graph is evaluated (vi.hoisted runs first) — this suite
// exercises pure selection logic, not the transport. happy-dom implements
// fetch for real, so an unstubbed call leaves an ECONNREFUSED to its default
// origin dangling after the run.
vi.hoisted(() => {
  globalThis.WebSocket = class {
    constructor() {
      this.readyState = 0;
    }
    send() {}
    close() {}
    addEventListener() {}
  };
  globalThis.fetch = () => new Promise(() => {});
});

import {
  activeEntry,
  activeSite,
  activeView,
  selectSite,
  selectView,
  viewLabel,
} from './sites';

const WSKILL = {
  wskill: true,
  label: 'wskill',
  root: '',
  views: [
    { id: 'book', kind: 'book', entry: 'wdoc/book/main.wcl', site: 'book', skill: false },
    { id: 'ai_skill', kind: 'ai_skill', entry: 'wdoc/skill/main.wcl', site: 'skill', skill: true },
    {
      id: 'training',
      kind: 'training',
      entry: 'wdoc/training/main.wcl',
      site: 'training',
      skill: false,
    },
  ],
};

describe('wskill view selection', () => {
  beforeEach(() => selectSite(WSKILL));

  it('defaults to the first non-skill view', () => {
    expect(activeView().id).toBe('book');
    expect(activeEntry()).toBe('wdoc/book/main.wcl');
  });

  it('never defaults to a skill view (it builds a folder, not a site)', () => {
    expect(activeView().skill).toBe(false);
  });

  it('points the build at the chosen view', () => {
    selectView('training');
    expect(activeView().id).toBe('training');
    expect(activeEntry()).toBe('wdoc/training/main.wcl');
    expect(activeSite()).toBe('training');
  });

  it('switches back off training cleanly', () => {
    selectView('training');
    selectView('book');
    expect(activeEntry()).toBe('wdoc/book/main.wcl');
    expect(activeSite()).toBe('book');
  });

  it('resets to the default view when the site changes', () => {
    selectView('training');
    selectSite(WSKILL);
    expect(activeView().id).toBe('book');
  });

  it('falls back to the default view for an unknown id', () => {
    selectView('nope');
    expect(activeView().id).toBe('book');
  });

  it('reads a plain (non-wskill) node directly', () => {
    selectSite({ entry: 'docs/main.wcl', site: 'docs', label: 'docs' });
    expect(activeEntry()).toBe('docs/main.wcl');
    expect(activeSite()).toBe('docs');
  });

  it('labels every artifact kind the picker can show', () => {
    expect(viewLabel('book')).toBe('Book');
    expect(viewLabel('training')).toBe('Training');
    expect(viewLabel('presentation')).toBe('Deck');
    expect(viewLabel('ai_skill')).toBe('Skill');
    expect(viewLabel('custom')).toBe('Custom');
  });
});
