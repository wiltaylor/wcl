/* The preview module: one place that knows how to get a built page on
   screen and keep it current.

   A host declares a TARGET — an entry document, the site within it,
   optionally one page, the merged all-views render, a synthetic unit page,
   or a skill folder — and mounts what it gets back: a source location, a
   reload counter, and a build status. The module owns everything between:

   - the `/api/preview` request, including which unsaved buffers to overlay
   - mapping a build location plus a page name to a page address
     (preview/pages.js) and probing the manifest for whether it exists
   - suppressing a duplicate in-flight build of the same target
   - one cache, keyed by target, whose entries live and die with the
     preview that owns them
   - reload-in-place when the target is unchanged but the build is newer

   Staleness is a SUBSCRIPTION, not a flag: the preview listens on the
   commit bus (state/commits.js) and marks itself stale unless the commit
   names it as the surface that already reflects the change. Nothing has to
   remember to mark anything.

   Only the server decides whether a build can be targeted at one page or
   must be full (crates/wcl/src/editor/preview.rs) — the target names the
   page the host wants on screen and nothing here second-guesses the mode. */

import { createEffect, createRoot, createSignal, getOwner, onCleanup } from 'solid-js';

import { api } from '../api';
import { builtPageExists, pageHref } from '../preview/pages';
import { dirtyFiles } from './buffers';
import { onCommit } from './commits';
import { activeEntry, activeSite, activeView } from './sites';

/** The page the active surface currently shows: { name, file, span } | null.
    Set by whichever iframe host loaded a frame last; the main preview uses
    it as the page a rebuild should refresh. */
const [currentPage, setCurrentPage] = createSignal(null);
export { currentPage, setCurrentPage };

let nextId = 1;

/** Stable identity of a build target — two targets with this key produce
    the same output, so one build serves both. */
export function targetKey(t) {
  if (!t?.entry) return null;
  return [
    t.entry,
    t.site ?? '',
    t.page ?? '',
    t.merged ? 'merged' : '',
    t.unit ? `${t.unit.kind}/${t.unit.id}` : '',
    t.skill ? 'skill' : '',
  ].join('|');
}

/** Does the built site for `target` contain its page? A probe, not a mount:
    it asks for the build (the server decides targeted vs full) and reads the
    manifest. Used by the graph's view-aware "Open page", which has to find
    which view builds a unit's page before it shows anything. */
export async function targetHasPage(target) {
  if (!target?.entry || !target.page) return false;
  const res = await api.preview(target.entry, target.site ?? null, dirtyFiles(), {
    pages: [target.page],
  });
  return res.ok && (await builtPageExists(res.href, target.page));
}

/**
 * Create a preview.
 *
 * `target` is an accessor returning the target (or null for "nothing to
 * show"); omit it and the preview owns a settable one (`setTarget`) — for
 * hosts whose child decides what to show.
 *
 * opts:
 * - `active`      — `() => bool`, whether the host is showing this preview.
 *                   A hidden tab defers its rebuild until it is shown again.
 * - `auto`        — build automatically when the active target has no fresh
 *                   build (default true)
 * - `refreshPage` — `() => page | null` for previews that mount a whole site
 *                   and let the user navigate: the page a rebuild should
 *                   refresh, which is not part of the target's identity
 * - `request`     — the request layer, for tests (default `api.preview`)
 * - `files`       — the unsaved buffers to overlay, for tests (default
 *                   `dirtyFiles` from the buffer store)
 */
export function createPreview(target, opts = {}) {
  const id = `preview-${nextId++}`;
  const active = opts.active ?? (() => true);
  const auto = opts.auto ?? true;
  const request = opts.request ?? ((...args) => api.preview(...args));
  const files = opts.files ?? dirtyFiles;

  const [ownTarget, setTarget] = createSignal(null);
  const readTarget = target ?? ownTarget;

  /** targetKey → { status, error, href, src, hasPage, mode, base, files } */
  const [builds, setBuilds] = createSignal({});
  /** Keys whose build no longer matches disk. Non-reactive (a Set) with an
      explicit counter, so marking one stale never re-runs an effect that
      only cares about another. */
  const stale = new Set();
  /** Stale keys whose MOUNTED page nonetheless matches, because the commit
      patched the live DOM instead of rebuilding. That only holds while the
      host keeps showing it: a frame remounted from the same URL reloads the
      stale bytes, so the exemption is dropped the moment this preview stops
      being the active one. */
  const patchedInPlace = new Set();
  const [staleSeq, setStaleSeq] = createSignal(0);
  const [reloadSeq, setReloadSeq] = createSignal(0);
  const inFlight = new Map();

  const key = () => targetKey(readTarget());
  const at = (k) => (k ? (builds()[k] ?? null) : null);
  const here = () => at(key());
  const patch = (k, fields) =>
    setBuilds((b) => ({ ...b, [k]: { ...(b[k] ?? {}), ...fields } }));

  const send = async (t, k, changed) => {
    patch(k, { status: 'building', error: null });
    const hint = t.page ?? opts.refreshPage?.() ?? null;
    const res = await request(t.entry, t.site ?? null, files(), {
      ...(hint ? { pages: [hint] } : {}),
      ...(t.merged ? { merged: true } : {}),
      ...(t.unit ? { unit: t.unit } : {}),
      ...(t.skill ? { skill: true } : {}),
      ...(changed?.length ? { changed } : {}),
    });
    if (!res.ok) {
      patch(k, { status: 'error', error: res.error ?? 'the preview build failed' });
      return res;
    }
    if (t.skill) {
      // A skill view builds a folder, not a site: there is no page to mount.
      patch(k, {
        status: 'ready',
        error: null,
        mode: 'skill',
        base: res.base ?? null,
        files: res.files ?? [],
        src: null,
        hasPage: null,
      });
    } else {
      // A page target mounts that page; without one the host mounts the
      // build's index and navigates the site itself. Not every view builds
      // a page for every unit, so a page the manifest doesn't list has
      // nothing to mount — the host says so rather than showing the view's
      // index in its place.
      const hasPage = t.page ? await builtPageExists(res.href, t.page) : true;
      patch(k, {
        status: 'ready',
        error: null,
        mode: res.mode ?? null,
        href: res.href,
        src: !hasPage ? null : t.page ? pageHref(res.href, t.page) : res.href,
        hasPage,
      });
    }
    setReloadSeq((n) => n + 1);
    return res;
  };

  /** Build the current target. A build already running for it is returned
      as-is rather than started twice. `changed` names the files a commit
      wrote (the server maps them onto pages). */
  const build = (o = {}) => {
    const t = readTarget();
    const k = targetKey(t);
    if (!k) return Promise.resolve({ ok: false, error: 'no preview target' });
    const running = inFlight.get(k);
    if (running) return running;
    stale.delete(k);
    patchedInPlace.delete(k);
    const p = send(t, k, o.changed ?? []).finally(() => inFlight.delete(k));
    inFlight.set(k, p);
    return p;
  };

  /** Mark every cached build stale. `spare` is the one target a commit left
      current — `{ key, patched }`, where `patched` says it was the live DOM
      that was fixed and not the build behind it. */
  const invalidate = (spare = null) => {
    for (const k of Object.keys(builds())) {
      if (spare && k === spare.key) continue;
      stale.add(k);
      patchedInPlace.delete(k);
    }
    if (spare?.patched) {
      stale.add(spare.key);
      patchedInPlace.add(spare.key);
    }
    setStaleSeq((n) => n + 1);
  };

  // A commit that named this preview as a surface it left current spares
  // the target on screen — but only that one: this preview's OTHER cached
  // targets (the view tabs it isn't showing) are as stale as anyone else's.
  const off = onCommit((e) => {
    const named = Array.isArray(e?.surface) ? e.surface : [e?.surface];
    invalidate(named.includes(id) ? { key: key(), patched: e?.patched === true } : null);
  });

  if (getOwner()) onCleanup(off);

  createEffect(() => {
    void staleSeq();
    const t = readTarget();
    if (!active()) {
      // Off screen: whatever was patched into the live DOM is gone with it,
      // so the stale build behind it has to be rebuilt before it is shown.
      patchedInPlace.clear();
      return;
    }
    if (!auto || !t?.entry) return;
    const k = targetKey(t);
    // A cached build (even a failed one — retry is the host's Retry button)
    // is left alone until something marks it stale.
    if (builds()[k] && (!stale.has(k) || patchedInPlace.has(k))) return;
    build();
  });

  return {
    /** The surface identifier commits are keyed by. */
    id,
    setTarget,
    target: readTarget,
    /** The URL to mount, or null. */
    src: () => here()?.src ?? null,
    /** The build's index href (a whole-site mount navigates from here). */
    href: () => here()?.href ?? null,
    /** Bump with an unchanged src ⇒ reload the frame in place. */
    reloadSeq,
    /** 'idle' | 'building' | 'ready' | 'error' */
    status: () => here()?.status ?? 'idle',
    building: () => here()?.status === 'building',
    error: () => here()?.error ?? null,
    /** Whether the build contains the target page (null = not a page
        target, or not built yet). */
    hasPage: () => here()?.hasPage ?? null,
    /** What the last build did: 'targeted' | 'full' | 'skill' | null. */
    mode: () => here()?.mode ?? null,
    /** Skill-folder builds. */
    base: () => here()?.base ?? null,
    files: () => here()?.files ?? [],
    /** Does the build behind the mounted page no longer match disk? True
        even for a page whose live DOM was patched in place — that repair
        does not survive a reload. */
    stale: () => {
      void staleSeq();
      return stale.has(key());
    },
    build,
    invalidate,
  };
}

/** The main preview: the selected site, mounted whole so the author can
    navigate it. Shared by the two surfaces that show exactly that — the
    code-mode preview pane and the design canvas — which are two views of
    one app-level surface and are never on screen together; it is created
    under a root so that app lifetime is explicit rather than accidental.
    Every OTHER preview is created by, and dies with, its host. */
const mainTarget = () => {
  const entry = activeEntry();
  if (!entry) return null;
  return {
    entry,
    site: activeSite(),
    // A skill view builds the actual skill folder (Markdown backend); the
    // canvas browses its files instead of mounting an iframe.
    skill: activeView()?.skill === true,
  };
};

/** Whether a surface that auto-rebuilds the main preview is on screen (the
    design canvas). The code-mode pane never sets it: there, a build runs
    only when the author presses Rebuild. */
const [mainActive, setMainActive] = createSignal(false);
export { mainActive, setMainActive };

export const mainPreview = createRoot(() => {
  const p = createPreview(mainTarget, {
    active: mainActive,
    refreshPage: () => currentPage()?.name ?? null,
  });
  // Selecting another site or wskill view retargets the build; the page the
  // previous one had on screen is not a refresh hint for it.
  createEffect(() => {
    void targetKey(mainTarget());
    setCurrentPage(null);
  });
  return p;
});
