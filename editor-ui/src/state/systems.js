/* Systems-view state: the `/api/systems` model, which kinds are drawn,
   what is collapsed, and what is selected.

   The payload is entirely schema-derived (see crates/wcl/src/editor/
   systems.rs) — this module never names a WAD kind. Which kinds start
   visible IS a choice: the C4 chain, discovered by walking the containment
   graph down from the deepest root, so a schema that grows a level (or a
   whole new estate model) still opens on something meaningful. Everything
   else is one checkbox away, and the choice persists per repo. */

import { batch, createSignal } from 'solid-js';

import { api } from '../api';
import { activeEntry } from './sites';
import { treeData } from './tree';

const [model, setModel] = createSignal(null);
const [loading, setLoading] = createSignal(false);
/** Kind names drawn on the canvas (null until the model has loaded). */
const [visibleKinds, setVisibleKinds] = createSignal(null);
/** The active perspective id (`systems` / `personas` / `deployment` / `all`). */
const [perspective, setPerspectiveId] = createSignal(null);
/** Node keys whose children are folded away. */
const [collapsed, setCollapsed] = createSignal(new Set());
/** Selected node key, or null. */
const [selectedNode, setSelectedNode] = createSignal(null);
/** Reference-edge field names drawn as dashed edges (`repo`, `built_by`…). */
const [refEdges, setRefEdges] = createSignal(new Set());
/**
 * Hand-placed boxes: node key → `{x, y}` relative to the parent's content
 * origin. SESSION ONLY — nothing is written to disk, so a layout you
 * arrange lives as long as the tab does and never lands in git. It survives
 * the model refetch every edit triggers (the keys don't change), and is
 * dropped when the document, the perspective, or the layout is reset.
 */
const [positions, setPositions] = createSignal(new Map());

export {
  model,
  setModel,
  loading,
  visibleKinds,
  setVisibleKinds,
  perspective,
  collapsed,
  setCollapsed,
  selectedNode,
  setSelectedNode,
  refEdges,
  setRefEdges,
  positions,
};

const storageKey = () => `wcl-editor:systems:${treeData()?.root ?? ''}`;

/**
 * Pin `key` at a parent-local point. A drop above or left of the parent's
 * content origin would push the box outside its own parent, so the whole
 * sibling set shifts by the same delta instead — the arrangement is
 * preserved and the stored coordinates keep matching what is drawn.
 */
export function setNodePosition(key, point, siblingKeys = []) {
  const next = new Map(positions());
  next.set(key, { x: point.x, y: point.y });
  const family = [key, ...siblingKeys.filter((k) => k !== key)];
  const placed = family.filter((k) => next.has(k)).map((k) => next.get(k));
  const dx = Math.min(0, ...placed.map((p) => p.x));
  const dy = Math.min(0, ...placed.map((p) => p.y));
  if (dx < 0 || dy < 0) {
    for (const k of family) {
      const p = next.get(k);
      if (p) next.set(k, { x: p.x - dx, y: p.y - dy });
    }
  }
  setPositions(next);
}

/** Freeze boxes where they currently sit, so moving one leaves the rest be. */
export function pinPositions(entries) {
  if (!entries.length) return;
  const next = new Map(positions());
  for (const [key, point] of entries) if (!next.has(key)) next.set(key, point);
  setPositions(next);
}

/** Forget every hand-placed box — the canvas re-packs itself. */
export function clearPositions() {
  if (positions().size) setPositions(new Map());
}

/** The perspectives the document offers (empty for a non-WAD model). */
export const perspectives = () => model()?.perspectives ?? [];

/** The active perspective record, or null. */
export function activePerspective() {
  const list = perspectives();
  return list.find((p) => p.id === perspective()) ?? list[0] ?? null;
}

/** The kind entry for a node (or a kind name). */
export function kindOf(nameOrNode) {
  const kind = typeof nameOrNode === 'string' ? nameOrNode : nameOrNode?.kind;
  return model()?.kinds?.find((k) => k.kind === kind) ?? null;
}

export function nodeByKey(key) {
  return model()?.nodes?.find((n) => n.key === key) ?? null;
}

export function nodeById(id) {
  return model()?.nodes?.find((n) => n.id === id) ?? null;
}

/**
 * The ids a field may name, as picker options — `null` when it names no
 * kind and should stay a plain input. A field is a reference when the
 * kind's schema declares it a parent link (`component.container`), or when
 * its NAME is another gathered kind's (the same convention `/api/systems`
 * derives the model from).
 *
 * Shared so every form agrees: the property dock and the details modal must
 * not offer a picker in one and a text box in the other. `selfId` drops the
 * object being edited, which can never reference itself.
 */
export function idOptions(schema, field, selfId) {
  const link =
    (schema?.parents ?? []).find((p) => p.field === field.name)?.kind ??
    (model()?.kinds ?? []).find((k) => k.kind === field.name)?.kind;
  if (!link) return null;
  return (model()?.nodes ?? [])
    .filter((n) => n.kind === link && n.id !== selfId)
    .map((n) => ({ value: n.id, label: `${n.title} (${n.id})` }));
}

/** How many objects the canvas opens with before it stops drilling down. */
const DEFAULT_BUDGET = 80;

/**
 * The containment chain the canvas opens on: start at the kinds nothing
 * else contains (the roots of the parent graph), then follow child kinds
 * downward a level at a time. Kinds outside that chain (glossary terms,
 * ADRs, specs) stay off until asked for.
 *
 * With `counts` (kind → instance count) the walk stops before the level
 * that would blow the budget — a WAD opens on its C4 drill-down rather than
 * on every CLI flag and code item at once. Everything omitted is one
 * checkbox away in the panel; without counts every level is included.
 */
export function defaultVisibleKinds(kinds, counts = null, budget = DEFAULT_BUDGET) {
  // `kinds` may be a SLICE of the model (one perspective), so a parent link
  // can name a kind that isn't in it — those links are ignored rather than
  // dragging the outside kind onto the canvas.
  const known = new Set(kinds.map((k) => k.kind));
  const parentsOf = new Map(
    kinds.map((k) => [k.kind, (k.parents ?? []).map((p) => p.kind).filter((p) => known.has(p))]),
  );
  const childKinds = new Map();
  for (const [kind, parents] of parentsOf) {
    for (const p of parents) {
      if (p === kind) continue; // self-nesting doesn't make a level
      childKinds.set(p, [...(childKinds.get(p) ?? []), kind]);
    }
  }
  // Roots: kinds that contain something but sit inside nothing.
  const roots = [...childKinds.keys()].filter(
    (k) => (parentsOf.get(k) ?? []).filter((p) => p !== k).length === 0,
  );
  const out = new Set(roots);
  const size = (k) => counts?.[k] ?? 0;
  let total = roots.reduce((n, k) => n + size(k), 0);
  let level = roots;
  while (level.length) {
    const next = [...new Set(level.flatMap((k) => childKinds.get(k) ?? []))].filter(
      (k) => !out.has(k),
    );
    if (!next.length) break;
    // The first level below the roots always goes in — a canvas of bare
    // roots is useless. Below that, take the cheap kinds of a level first
    // and stop descending as soon as one doesn't fit: a C4 model then opens
    // on its components without also drawing every code item.
    const forced = out.size === roots.length;
    const taken = [];
    let skipped = false;
    for (const k of [...next].sort((a, b) => size(a) - size(b))) {
      if (!forced && counts && total + size(k) > budget) {
        skipped = true;
        continue;
      }
      taken.push(k);
      out.add(k);
      total += size(k);
    }
    if (skipped) break;
    level = taken;
  }
  // A model with no containment at all (nothing to drill into) shows
  // everything rather than an empty canvas.
  return out.size ? out : new Set(kinds.filter((k) => !k.edge).map((k) => k.kind));
}

/** Everything stored for this repo: `{ perspective, refs, kinds: {id: […]} }`. */
function readPrefs() {
  try {
    return JSON.parse(localStorage.getItem(storageKey()) ?? 'null') ?? {};
  } catch {
    return {}; // storage unavailable or corrupt
  }
}

/**
 * The kinds a perspective opens on: the user's own selection for it when
 * they have one, else its declared kinds narrowed by the object budget
 * (`defaultVisibleKinds` over that slice only), else — for a model with no
 * perspectives at all — the budgeted walk over everything.
 */
function kindsForPerspective(id, kinds, counts, saved) {
  const known = new Set(kinds.map((k) => k.kind));
  const kept = (saved?.kinds?.[id] ?? []).filter((k) => known.has(k));
  if (kept.length) return new Set(kept);
  const p = perspectives().find((x) => x.id === id);
  if (!p) return defaultVisibleKinds(kinds, counts);
  const slice = kinds.filter((k) => p.kinds.includes(k.kind));
  // "All" is an explicit ask for the whole model — no budget there.
  return id === 'all' ? new Set(p.kinds) : defaultVisibleKinds(slice, counts);
}

/** kind → instance count over a payload's nodes. */
function nodeCounts(nodes) {
  const out = {};
  for (const n of nodes ?? []) out[n.kind] = (out[n.kind] ?? 0) + 1;
  return out;
}

export function persistPrefs() {
  const id = activePerspective()?.id ?? 'all';
  try {
    const saved = readPrefs();
    localStorage.setItem(
      storageKey(),
      JSON.stringify({
        ...saved,
        perspective: id,
        refs: [...refEdges()],
        kinds: { ...(saved.kinds ?? {}), [id]: [...(visibleKinds() ?? [])] },
      }),
    );
  } catch {
    /* private mode — preferences just don't stick */
  }
}

/**
 * Open the model on its remembered perspective (or its first), with that
 * perspective's remembered kinds. Called on every fresh load.
 */
export function applyPrefs(res = model()) {
  const saved = readPrefs();
  const list = res?.perspectives ?? [];
  const id = list.find((p) => p.id === saved.perspective)?.id ?? list[0]?.id ?? null;
  setPerspectiveId(id);
  setVisibleKinds(kindsForPerspective(id, res?.kinds ?? [], nodeCounts(res?.nodes), saved));
  setRefEdges(new Set(saved.refs ?? []));
}

/** Switch perspective: the canvas re-opens on that slice of the model. */
export function selectPerspective(id) {
  if (id === perspective()) return;
  setPerspectiveId(id);
  setSelectedNode(null);
  // Another slice is another picture: its boxes have never been arranged.
  setPositions(new Map());
  setVisibleKinds(
    kindsForPerspective(id, model()?.kinds ?? [], nodeCounts(model()?.nodes), readPrefs()),
  );
  persistPrefs();
}

export function toggleKind(kind) {
  const next = new Set(visibleKinds() ?? []);
  if (next.has(kind)) next.delete(kind);
  else next.add(kind);
  setVisibleKinds(next);
  persistPrefs();
}

export function toggleRefEdge(field) {
  const next = new Set(refEdges());
  if (next.has(field)) next.delete(field);
  else next.add(field);
  setRefEdges(next);
  persistPrefs();
}

export function toggleCollapsed(key) {
  const next = new Set(collapsed());
  if (next.has(key)) next.delete(key);
  else next.add(key);
  setCollapsed(next);
}

/**
 * Can a node of `childKind` hang off a node of `parentKind`? Answers the
 * parent field to write, or null when the schema has no such link — the
 * drag-and-drop legality check.
 */
export function parentField(childKind, parentKind) {
  const k = kindOf(childKind);
  return (k?.parents ?? []).find((p) => p.kind === parentKind)?.field ?? null;
}

/** Is `field` optional on `kind` (i.e. can a node be detached)? */
export function fieldOptional(kind, field) {
  const f = kindOf(kind)?.fields?.find((x) => x.name === field);
  return f ? f.optional !== false : false;
}

/** Would making `child` a descendant of `parent` create a cycle? */
export function wouldCycle(childKey, parentKey) {
  let cur = parentKey;
  const seen = new Set();
  while (cur && !seen.has(cur)) {
    if (cur === childKey) return true;
    seen.add(cur);
    const n = nodeByKey(cur);
    const p = n?.parent?.id ? nodeById(n.parent.id) : null;
    cur = p?.key ?? null;
  }
  return false;
}

/** Every node key under `key`, inclusive. */
export function subtreeKeys(key, keys = new Set()) {
  if (keys.has(key)) return keys;
  keys.add(key);
  const node = nodeByKey(key);
  if (!node) return keys;
  for (const n of model()?.nodes ?? []) {
    if (n.parent?.id === node.id) subtreeKeys(n.key, keys);
  }
  return keys;
}

/**
 * What deleting `key` implies, decided by the schema rather than by the
 * picture: a child whose parent field is REQUIRED cannot outlive its parent
 * (a container must name a system), so it is deleted too — recursively. A
 * child linked by an OPTIONAL field just loses the link (deleting a boundary
 * frees the systems it grouped; it does not delete the estate).
 *
 * @returns { deleted: [key], detached: [{ key, field }] }
 */
export function deletePlan(key) {
  const deleted = new Set();
  const detached = new Map(); // key → field to drop
  const walk = (k) => {
    if (deleted.has(k)) return;
    deleted.add(k);
    const node = nodeByKey(k);
    if (!node) return;
    for (const child of model()?.nodes ?? []) {
      if (child.parent?.id !== node.id || deleted.has(child.key)) continue;
      if (fieldOptional(child.kind, child.parent.field)) {
        detached.set(child.key, child.parent.field);
      } else {
        walk(child.key);
      }
    }
  };
  walk(key);
  return {
    deleted: [...deleted],
    detached: [...detached]
      .filter(([k]) => !deleted.has(k))
      .map(([k, field]) => ({ key: k, field })),
  };
}

/** Reload the model. `keep` retains the current view preferences. */
export async function loadSystems({ keep = true } = {}) {
  const entry = activeEntry();
  if (!entry) return { ok: false, error: 'no site selected' };
  setLoading(true);
  const res = await api.systems(entry);
  setLoading(false);
  if (!res.ok) return res;
  // One batch: a model visible for even a tick without its kind filter
  // would lay the WHOLE document out, and the canvas would fit its view to
  // that transient.
  batch(() => {
    setModel(res);
    if (!keep || !visibleKinds()) applyPrefs(res);
    // Drop selections, placements and collapse entries the reload invalidated.
    const keys = new Set((res.nodes ?? []).map((n) => n.key));
    if (selectedNode() && !keys.has(selectedNode())) setSelectedNode(null);
    if ([...positions().keys()].some((k) => !keys.has(k))) {
      setPositions(new Map([...positions()].filter(([k]) => keys.has(k))));
    }
    const stillCollapsed = new Set([...collapsed()].filter((k) => keys.has(k)));
    if (stillCollapsed.size !== collapsed().size) setCollapsed(stillCollapsed);
  });
  return res;
}

/** Reset to a fresh model + default view preferences. */
export function resetSystems() {
  setModel(null);
  setVisibleKinds(null);
  setPerspectiveId(null);
  setPositions(new Map());
  setCollapsed(new Set());
  setSelectedNode(null);
}
