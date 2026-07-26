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
/** The page name the preview iframe currently shows (data-wcl-page-name),
    kept by PreviewPane on every frame load — the manual Rebuild's target. */
const [previewPage, setPreviewPage] = createSignal(null);
/** What the last build did: 'targeted' | 'full' | null (no build yet). */
const [buildMode, setBuildMode] = createSignal(null);

export {
  siteTree,
  selected,
  selectedView,
  building,
  previewHref,
  buildSeq,
  buildError,
  skillPreview,
  previewPage,
  setPreviewPage,
  buildMode,
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

/** Display name for an artifact-kind view (the wskill projection tabs). */
export function viewLabel(kind) {
  switch (kind) {
    case 'book':
      return 'Book';
    case 'presentation':
      return 'Deck';
    case 'training':
      return 'Training';
    case 'ai_skill':
      return 'Skill';
    default:
      return kind.charAt(0).toUpperCase() + kind.slice(1);
  }
}

export function selectView(id) {
  setSelectedView(id);
  // A view switch changes the build target, so the shown page is stale.
  setPreviewPage(null);
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
  // The previous build stays visible (and labelled) until the next Rebuild;
  // its page belongs to the old site, so it's no longer a rebuild target.
  setPreviewPage(null);
  persistSelection();
}

/** `extra` may carry { pages, changed } — the Design-mode targeted-rebuild
    hint (ignored by the server on a cold output dir). Without an explicit
    hint the manual Rebuild targets the page the preview currently shows —
    the server re-renders just that page (stale siblings materialize lazily
    on navigation) and self-falls-back to a full build whenever a targeted
    one isn't possible. */
export async function rebuild(extra = {}) {
  const entry = activeEntry();
  if (!entry || building()) return { ok: true };
  // A skill view builds the actual skill folder (Markdown backend) and the
  // canvas browses its files instead of an iframe.
  const skill = activeView()?.skill === true;
  const page = previewPage();
  const targeted =
    !skill && extra.pages === undefined && extra.changed === undefined && page
      ? { pages: [page] }
      : {};
  setBuilding(true);
  const res = await api.preview(entry, activeSite(), dirtyFiles(), {
    ...targeted,
    ...extra,
    ...(skill ? { skill: true } : {}),
  });
  setBuilding(false);
  if (res.ok) {
    setBuildError(null);
    setBuildMode(skill ? null : (res.mode ?? null));
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
