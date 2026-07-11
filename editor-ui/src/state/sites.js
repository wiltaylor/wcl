/* Site-preview state: the discovered site tree (/api/sites), the selected
   site, and manual rebuilds. Nothing here reacts to buffer edits — a build
   runs only when the user presses Rebuild, and the awaited POST doubles as
   the progress signal. */

import { createSignal } from 'solid-js';

import { api } from '../api';
import { dirtyFiles } from './buffers';
import { treeData } from './tree';

const [siteTree, setSiteTree] = createSignal([]);
const [selected, setSelected] = createSignal(null); // {entry, site, label} | null
const [building, setBuilding] = createSignal(false);
const [previewHref, setPreviewHref] = createSignal(null);
const [buildSeq, setBuildSeq] = createSignal(0); // bumps on every finished build
const [buildError, setBuildError] = createSignal(null);

export { siteTree, selected, building, previewHref, buildSeq, buildError };

const storageKey = () => `wcl-editor:site:${treeData()?.root ?? ''}`;

/** Every selectable node, depth-first. Skill sites can't be HTML-built, so
    they are skipped as targets — their children still list (indent kept). */
export function flatSites(nodes = siteTree(), depth = 0, out = []) {
  for (const n of nodes ?? []) {
    if (!n.skill) out.push({ node: n, depth });
    flatSites(n.children, n.skill ? depth : depth + 1, out);
  }
  return out;
}

export async function loadSites() {
  const res = await api.sites();
  if (!res.ok) return res;
  setSiteTree(res.sites ?? []);
  const flat = flatSites(res.sites ?? []);
  // Restore the last selection for this repo when it still exists,
  // else default to the first (topmost root) site.
  let restored = null;
  try {
    restored = JSON.parse(localStorage.getItem(storageKey()) ?? 'null');
  } catch {
    /* stale/corrupt entry */
  }
  const match =
    restored &&
    flat.find(({ node }) => node.entry === restored.entry && (node.site ?? null) === (restored.site ?? null));
  setSelected(match?.node ?? flat[0]?.node ?? null);
  return res;
}

export function selectSite(node) {
  setSelected(node);
  // The previous build stays visible (and labelled) until the next Rebuild.
  try {
    localStorage.setItem(storageKey(), JSON.stringify({ entry: node.entry, site: node.site ?? null }));
  } catch {
    /* storage unavailable */
  }
}

export async function rebuild() {
  const s = selected();
  if (!s || building()) return { ok: true };
  setBuilding(true);
  const res = await api.preview(s.entry, s.site ?? null, dirtyFiles());
  setBuilding(false);
  if (res.ok) {
    setBuildError(null);
    setPreviewHref(res.href);
    setBuildSeq(buildSeq() + 1);
  } else {
    setBuildError(res.error);
  }
  return res;
}
