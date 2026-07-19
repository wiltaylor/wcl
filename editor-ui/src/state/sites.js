/* Site-preview state: the discovered site tree (/api/sites), the selected
   site, and manual rebuilds. Nothing here reacts to buffer edits — a build
   runs only when the user presses Rebuild, and the awaited POST doubles as
   the progress signal. */

import { createSignal } from 'solid-js';

import { api } from '../api';
import { dirtyFiles } from './buffers';
import { treeData } from './tree';

const [siteTree, setSiteTree] = createSignal([]);
/** A plain site node {entry, site, label, …} or a grouped wskill node
    {wskill: true, root, label, views: […]}. */
const [selected, setSelected] = createSignal(null);
/** The active view (artifact id) of a selected wskill node. */
const [selectedView, setSelectedView] = createSignal(null);
const [building, setBuilding] = createSignal(false);
const [previewHref, setPreviewHref] = createSignal(null);
const [buildSeq, setBuildSeq] = createSignal(0); // bumps on every finished build
const [buildError, setBuildError] = createSignal(null);
/** A skill view's built folder: { base, files } | null. */
const [skillPreview, setSkillPreview] = createSignal(null);

export {
  siteTree,
  selected,
  selectedView,
  building,
  previewHref,
  buildSeq,
  buildError,
  skillPreview,
};

const storageKey = () => `wcl-editor:site:${treeData()?.root ?? ''}`;

/** Every selectable node, depth-first. Skill sites can't be HTML-built, so
    they are skipped as targets — their children still list (indent kept).
    A wskill node is one selectable entry (its views pick the projection). */
export function flatSites(nodes = siteTree(), depth = 0, out = []) {
  for (const n of nodes ?? []) {
    if (n.wskill) {
      out.push({ node: n, depth });
      continue;
    }
    if (!n.skill) out.push({ node: n, depth });
    flatSites(n.children, n.skill ? depth : depth + 1, out);
  }
  return out;
}

/** The selected wskill's active view record (never the skill view unless
    explicitly chosen), or null for plain site nodes. */
export function activeView() {
  const s = selected();
  if (!s?.wskill) return null;
  const views = s.views ?? [];
  return views.find((v) => v.id === selectedView()) ?? views.find((v) => !v.skill) ?? views[0] ?? null;
}

/** The entry/site every build & design endpoint should target, resolved
    through the active wskill view when one is selected. */
export function activeEntry() {
  const s = selected();
  return s?.wskill ? (activeView()?.entry ?? null) : (s?.entry ?? null);
}

export function activeSite() {
  const s = selected();
  return s?.wskill ? (activeView()?.site ?? null) : (s?.site ?? null);
}

export function selectView(id) {
  setSelectedView(id);
  persistSelection();
}

/** Stable identity of a selectable node for persistence/matching. */
export function nodeKey(node) {
  return node.wskill ? `wskill:${node.root}` : `${node.entry} ${node.site ?? ''}`;
}

function persistSelection() {
  const node = selected();
  if (!node) return;
  try {
    localStorage.setItem(
      storageKey(),
      JSON.stringify({ key: nodeKey(node), view: selectedView() ?? null }),
    );
  } catch {
    /* storage unavailable */
  }
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
    restored?.key && flat.find(({ node }) => nodeKey(node) === restored.key);
  setSelected(match?.node ?? flat[0]?.node ?? null);
  setSelectedView(restored?.view ?? null);
  return res;
}

export function selectSite(node) {
  setSelected(node);
  setSelectedView(null);
  // The previous build stays visible (and labelled) until the next Rebuild.
  persistSelection();
}

/** `extra` may carry { pages, changed } — the Design-mode targeted-rebuild
    hint (ignored by the server on a cold output dir). */
export async function rebuild(extra = {}) {
  const entry = activeEntry();
  if (!entry || building()) return { ok: true };
  // A skill view builds the actual skill folder (Markdown backend) and the
  // canvas browses its files instead of an iframe.
  const skill = activeView()?.skill === true;
  setBuilding(true);
  const res = await api.preview(entry, activeSite(), dirtyFiles(), {
    ...extra,
    ...(skill ? { skill: true } : {}),
  });
  setBuilding(false);
  if (res.ok) {
    setBuildError(null);
    if (skill) {
      setSkillPreview({ base: res.base, files: res.files ?? [] });
    } else {
      setPreviewHref(res.href);
    }
    setBuildSeq(buildSeq() + 1);
  } else {
    setBuildError(res.error);
  }
  return res;
}
