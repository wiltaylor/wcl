/* The client-side owner of the edit-mode ANCHOR format.

   An edit-mode preview build stamps its output with the source that
   produced it: `data-wcl-file` + `data-wcl-span` ("start:end" byte offsets)
   on every editable block, its `data-wcl-kind`, the diagram family
   (`data-wcl-shape` / `-shape-id`, the owning svg's `data-wcl-layout`, the
   layout-container guides' `data-wf-slot` zones), the visibility stamps a
   merged build adds (`data-wcl-except` / `data-wcl-vis`), the page
   wrapper's `data-wcl-page-*`, the `edit_field` bindings
   (`data-wcl-field-*`) and the `edit_object` buttons (`data-wcl-edit-*`).

   Every reader in the preview layer comes through here. The names of the
   attributes the BUILD stamps live in `ATTR`/`SEL` and nowhere else (each
   editing layer's own injected chrome — comment pins, resize handles and
   ports, tree rows — is that layer's to name), and the facts callers used
   to recompute for themselves are fields on the anchor `anchorOf` returns:

     - `chrome` — the declaring file is a stdlib/registry source, so the
       element is template chrome rather than editable content;
     - `shared` — another element carries the same (file, span), i.e. the
       block renders more than once and an edit affects every instance;
     - `box`    — the shape's content geometry, from this module's store.

   Adding a stamp to the renderer therefore means extending `ATTR` and
   `anchorOf` here; every reader picks it up. A reader that has NOT learned
   about a stamp silently treats the element as unanchored — the failure
   mode this single owner exists to prevent.

   One part of the format is deliberately not an anchor field: the
   layout-guide `data-wf-slot` zones are guide chrome, never anchored
   elements, so they are read through `slotOf`/`slotZoneIn` instead. */

// ---------------------------------------------------------------------------
// The format
// ---------------------------------------------------------------------------

/** Every attribute the edit-mode build stamps, by role. */
export const ATTR = {
  file: 'data-wcl-file',
  span: 'data-wcl-span',
  kind: 'data-wcl-kind',
  block: 'data-wcl-block',
  shape: 'data-wcl-shape',
  shapeId: 'data-wcl-shape-id',
  layout: 'data-wcl-layout',
  // Stamped on every edit-mode edge path (`"from:to"` shape ids). Named
  // here so the inventory is complete and the next reader extends this
  // module rather than re-inventing the name; nothing reads it yet.
  edge: 'data-wcl-edge',
  slot: 'data-wf-slot',
  guide: 'data-wf-guide',
  except: 'data-wcl-except',
  vis: 'data-wcl-vis',
  pageName: 'data-wcl-page-name',
  pageFile: 'data-wcl-page-file',
  pageSpan: 'data-wcl-page-span',
  fieldName: 'data-wcl-field-name',
  fieldKind: 'data-wcl-field-kind',
  fieldTarget: 'data-wcl-field-target',
  fieldPlain: 'data-wcl-field-plain',
  editKind: 'data-wcl-edit-kind',
  editTarget: 'data-wcl-edit-target',
};

/** Selectors built from `ATTR` — the only place they are spelled out. */
export const SEL = {
  anchor: `[${ATTR.span}][${ATTR.file}]`,
  block: `[${ATTR.block}]`,
  shape: `[${ATTR.shape}]`,
  /** Shapes that can be an edge endpoint (a connection names ids). */
  identifiedShape: `[${ATTR.shape}][${ATTR.shapeId}]`,
  diagram: `svg[${ATTR.layout}]`,
  slot: `[${ATTR.slot}]`,
  guide: `[${ATTR.guide}]`,
  field: `[${ATTR.fieldName}]`,
  editButton: `[${ATTR.editKind}]`,
  page: `[${ATTR.pageFile}]`,
};

// ---------------------------------------------------------------------------
// Spans
// ---------------------------------------------------------------------------

/** Parse a raw `start:end` span attribute value; null when absent or
    malformed (a half-parsed span would poison every selector built from
    it, so a bad stamp reads as no stamp). */
export function parseSpanAttr(raw) {
  if (!raw) return null;
  const parts = String(raw).split(':');
  if (parts.length !== 2 || parts.some((p) => p.trim() === '')) return null;
  const [start, end] = parts.map(Number);
  if (!Number.isFinite(start) || !Number.isFinite(end)) return null;
  return { start, end };
}

/** The attribute form of a span — `{start, end}` or an already-formatted
    `"start:end"` string. */
export function spanKey(span) {
  return typeof span === 'string' ? span : `${span.start}:${span.end}`;
}

/** `el`'s parsed source span, or null when unstamped/malformed. */
export function spanOf(el) {
  return parseSpanAttr(el?.getAttribute?.(ATTR.span));
}

/** CSS selector matching every element anchored at (file, span). */
export function spanSelector(file, span) {
  return `[${ATTR.file}="${CSS.escape(file)}"][${ATTR.span}="${spanKey(span)}"]`;
}

/** CSS selector matching every anchored element declared by `file`. */
export function fileSelector(file) {
  return `[${ATTR.file}="${CSS.escape(file)}"][${ATTR.span}]`;
}

// ---------------------------------------------------------------------------
// Derived facts
// ---------------------------------------------------------------------------

/** Template chrome (sidebar, rails, layout parts) comes from stdlib /
    registry sources (`<wcl-system>/…` paths) — not editable content, so it
    passes clicks through to what it wraps and grows no gutter. */
export function isChrome(el) {
  return (el?.getAttribute?.(ATTR.file) ?? '').startsWith('<');
}

/** The block kind stamped on `el` (`p`, `table`, a widget kind, …). */
export function kindOf(el) {
  return el?.getAttribute?.(ATTR.kind) || null;
}

/** Is `el` a diagram-shape anchor (an SVG shape rather than an HTML block)? */
export function isShape(el) {
  return !!el?.hasAttribute?.(ATTR.shape);
}

/** A shape's declared id — only shapes that HAVE one can be an edge end. */
export function shapeIdOf(el) {
  return el?.getAttribute?.(ATTR.shapeId) || null;
}

/** The diagram `<svg>` owning `el` (itself, when `el` IS the diagram). */
export function diagramOf(el) {
  return el?.closest?.(SEL.diagram) ?? null;
}

/** The effective layout mode of `el`'s diagram, or null outside one.

    Usually the `<svg>` is `el` or an ancestor. An INTERACTIVE diagram
    (`pan_zoom`, or any map) renders inside a viewport `<div>`, and the
    block anchor lands on that div — so the layout sits one level BELOW the
    anchored element there. Hence the direct-child fallback, which is tight
    on purpose: a block that merely contains a diagram deeper down (a
    `demo`, a list) is not itself a diagram and has no layout. */
export function layoutOf(el) {
  const svg = diagramOf(el) ?? el?.querySelector?.(`:scope > ${SEL.diagram}`);
  return svg?.getAttribute(ATTR.layout) ?? null;
}

/** The layout modes that honor per-shape x/y. */
const MANUAL_LAYOUTS = ['free', 'none'];

/** Does this layout honor per-shape x/y? THE predicate behind dragging a
    shape, seeding a new one with coordinates and offering convert-to-manual
    — asked of an anchor's `layout` so those three cannot disagree. A null
    layout means `el` is not a diagram at all: nothing to place into. */
export function isManualLayout(layout) {
  return MANUAL_LAYOUTS.includes(layout ?? '');
}

/** The insertion index of a layout-guide drop zone, or null when `el` is
    not one. */
export function slotOf(el) {
  const raw = el?.getAttribute?.(ATTR.slot);
  const n = raw == null ? NaN : Number(raw);
  return Number.isInteger(n) ? n : null;
}

/** The guide drop zone under `hitEl` that belongs to `containerEl`, or
    null — the layout containers' `data-wf-slot` cells and gap strips. */
export function slotZoneIn(hitEl, containerEl) {
  const zone = hitEl?.closest?.(SEL.slot) ?? null;
  return zone && containerEl?.contains?.(zone) && slotOf(zone) != null ? zone : null;
}

/** The visibility state stamped on a merged-build anchor: the profiles the
    block is hidden in (`except`), and `vis` = 'custom' when its visibility
    is custom (`@only` / other axes, which the checkbox editor can't
    represent). These ARE the anchor's two visibility fields, under the same
    names, so nothing renames them on the way through. Written back by
    `restampExcept` — read and write live together so they cannot drift. */
export function visOf(el) {
  const raw = el?.getAttribute?.(ATTR.except);
  return {
    except: raw ? raw.split(/\s+/).filter(Boolean) : [],
    vis: el?.getAttribute?.(ATTR.vis) === 'custom' ? 'custom' : null,
  };
}

// ---------------------------------------------------------------------------
// The anchor
// ---------------------------------------------------------------------------

/** Everything stamped on `el`, plus the derived facts — or null when `el`
    carries no (file, span) anchor.

    `shared` and `box` are derived on FIRST READ and then remembered: one
    scans the whole document and the other forces SVG layout, while the
    hottest callers (hover hit-testing, one anchor per gutter, the block
    tree) read neither. Each is still a snapshot, taken at that first read
    rather than when the anchor was built. */
export function anchorOf(doc, el) {
  const span = spanOf(el);
  const file = el?.getAttribute?.(ATTR.file);
  if (!span || !file) return null;
  const shape = isShape(el);
  const owner = doc ?? el.ownerDocument;
  let shared;
  let box;
  return {
    el,
    file,
    span,
    kind: kindOf(el) || 'block',
    shape,
    shapeId: shapeIdOf(el),
    layout: layoutOf(el),
    ...visOf(el),
    chrome: isChrome(el),
    get shared() {
      shared ??= (owner?.querySelectorAll?.(spanSelector(file, span))?.length ?? 0) > 1;
      return shared;
    },
    get box() {
      if (box === undefined) box = shape ? shapeBox(el) : null;
      return box;
    },
  };
}

/** Every anchored element declared by `file` — optionally narrowed to one
    span (repeater/template output renders one source block several times). */
export function anchorEls(doc, file, span = null) {
  const sel = span ? spanSelector(file, span) : fileSelector(file);
  return [...(doc?.querySelectorAll?.(sel) ?? [])];
}

/** Every anchor in `doc` declared by `file`. */
export function anchorsIn(doc, file) {
  return anchorEls(doc, file).map((el) => anchorOf(doc, el)).filter(Boolean);
}

/** The first element anchored at (file, span) — how a caller re-finds a
    block after a rebuild refreshed the anchors. */
export function elBySpan(doc, file, span) {
  return anchorEls(doc, file, span)[0] ?? null;
}

/** The nearest ancestor-or-self matching `selector`, skipping candidates
    `skip` rejects and continuing outward. The one walk behind selection
    (skipping template chrome) and drop resolution (skipping the subtree
    being moved), so what can be dropped onto matches what can be selected. */
export function closestMatching(el, selector, skip) {
  let cur = el?.closest?.(selector) ?? null;
  while (cur && skip?.(cur)) {
    cur = cur.parentElement?.closest?.(selector) ?? null;
  }
  return cur;
}

/** The nearest selectable content-block element at `target` — template
    chrome skipped so its links/toggles keep working, and a malformed span
    skipped too: `SEL.anchor` matches the attribute's PRESENCE, so without
    this a click could resolve to an element `anchorOf` then rejects and
    silently do nothing. */
export function anchorElAt(target) {
  return closestMatching(target, SEL.anchor, (el) => isChrome(el) || !spanOf(el));
}

/** The anchor enclosing `el` — the outward step (Esc pops the selection
    one level; a shape's owning diagram). */
export function outerAnchorEl(el) {
  return el?.parentElement ? anchorElAt(el.parentElement) : null;
}

/** Every selectable anchor at or above `target`, innermost → outermost
    (chrome skipped). Nested anchors happen for diagram shapes (a shape
    inside the diagram's <svg>, possibly inside a container's <g>) and
    nested HTML blocks (an `li` inside a `list`, a table inside a `demo`). */
export function anchorChainAt(target) {
  const chain = [];
  let el = anchorElAt(target);
  while (el) {
    chain.push(el);
    el = outerAnchorEl(el);
  }
  return chain;
}

// ---------------------------------------------------------------------------
// Page wrapper + block tree
// ---------------------------------------------------------------------------

/** The anchored page in `doc`, or null (non-wdoc content, not yet loaded).
    `span` (edit-mode builds only) is the page block's byte span in `file`. */
export function pageInfo(doc) {
  const el = doc?.querySelector?.(SEL.page);
  if (!el) return null;
  return {
    el,
    name: el.getAttribute(ATTR.pageName),
    file: el.getAttribute(ATTR.pageFile),
    span: pageSpanOf(el),
  };
}

/** The page wrapper's own source span, or null. */
export function pageSpanOf(el) {
  return parseSpanAttr(el?.getAttribute?.(ATTR.pageSpan));
}

/** Every page wrapper in `doc` declared by `file` (edit-mode builds). */
export function pageEls(doc, file) {
  return [
    ...(doc?.querySelectorAll?.(
      `[${ATTR.pageFile}="${CSS.escape(file)}"][${ATTR.pageSpan}]`,
    ) ?? []),
  ];
}

/** Nearest [data-wcl-block] descendants of `node` not separated from it
    by another block — the block tree's direct children. */
export function blockChildren(node) {
  return [...node.querySelectorAll(SEL.block)].filter((b) => {
    let p = b.parentElement;
    while (p && p !== node) {
      if (p.hasAttribute(ATTR.block)) return false;
      p = p.parentElement;
    }
    return p === node;
  });
}

/** The block container `el` sits in: its nearest [data-wcl-block] ancestor
    below the page root, else the page root itself. The other half of the
    block-tree walk — `blockChildren(containerOf(page, el))` is the sibling
    run `el` belongs to, which is how both the reorder walk and the comment
    locator address a block. */
export function containerOf(pageEl, el) {
  let p = el?.parentElement;
  while (p && p !== pageEl) {
    if (p.hasAttribute(ATTR.block)) return p;
    p = p.parentElement;
  }
  return pageEl;
}

/** `el`'s ordered same-file block siblings, itself included: the block
    children of its container (the page root at top level), filtered to
    `el`'s file. THE definition of "a neighbouring block" — the toolbar
    move, the gutter drag's drop line and the server `move` op's "adjacent
    sibling in the same container" all agree because they ask this, under
    the established DOM-order == AST-order assumption for same-file runs. */
export function sameFileSiblings(doc, el) {
  const page = pageInfo(doc);
  const file = el?.getAttribute?.(ATTR.file);
  if (!page || !file) return [];
  return blockChildren(containerOf(page.el, el)).filter(
    (b) => b.getAttribute(ATTR.file) === file && spanOf(b),
  );
}

/** The adjacent same-file block sibling of `el` in `dir` ('up' | 'down'),
    or null. */
export function adjacentSameFileSibling(doc, el, dir) {
  const siblings = sameFileSiblings(doc, el);
  const i = siblings.indexOf(el);
  if (i < 0) return null;
  return siblings[dir === 'up' ? i - 1 : i + 1] ?? null;
}

/** The previous DOM sibling of `el` stamped with `kind`, or null (list
    items skip past whatever the template renders between them). */
export function prevSiblingOfKind(el, kind) {
  let prev = el?.previousElementSibling ?? null;
  while (prev && kindOf(prev) !== kind) prev = prev.previousElementSibling;
  return prev;
}

/** The nearest ancestor-or-self stamped with `kind`, or null. */
export function closestOfKind(el, kind) {
  return el?.closest?.(`[${ATTR.kind}="${CSS.escape(kind)}"]`) ?? null;
}

/** The first diagram `<svg>` in `root` (a screen's wireframe), or null. */
export function diagramIn(root) {
  return root?.querySelector?.(SEL.diagram) ?? null;
}

/** Every shape anchor inside `root`. */
export function shapeEls(root) {
  return [...(root?.querySelectorAll?.(SEL.shape) ?? [])];
}

/** The shape anchors with no OTHER shape anchor (or diagram) between them
    and `root` — its direct structural children. */
export function shapeChildren(root) {
  return shapeEls(root).filter(
    (g) => g.parentElement?.closest(`${SEL.shape}, ${SEL.diagram}`) === root,
  );
}

// ---------------------------------------------------------------------------
// Field bindings + edit_object buttons
// ---------------------------------------------------------------------------

/** The `edit_field` binding on (or above) `el`, or null. */
export function fieldBindingOf(el) {
  const bound = el?.closest?.(SEL.field);
  if (!bound) return null;
  return {
    el: bound,
    kind: bound.getAttribute(ATTR.fieldKind),
    target: bound.getAttribute(ATTR.fieldTarget) || null,
    field: bound.getAttribute(ATTR.fieldName),
    plain: bound.hasAttribute(ATTR.fieldPlain),
  };
}

/** The `edit_object` "Edit this …" button on (or above) `el`, or null. */
export function editButtonOf(el) {
  const btn = el?.closest?.(SEL.editButton);
  if (!btn) return null;
  return {
    el: btn,
    kind: btn.getAttribute(ATTR.editKind),
    target: btn.getAttribute(ATTR.editTarget) || null,
  };
}

// ---------------------------------------------------------------------------
// Re-stamping (in-place commits)
// ---------------------------------------------------------------------------

/** Re-stamp a block's span after a commit reformatted its file. */
export function restampSpan(el, span) {
  el.setAttribute(ATTR.span, spanKey(span));
}

/** Re-stamp the page wrapper's span after the same commit. */
export function restampPageSpan(el, span) {
  el.setAttribute(ATTR.pageSpan, spanKey(span));
}

/** Rewrite the merged-build visibility stamp to `except` (space-joined;
    removed when empty) — the write half of `visOf`, taking back exactly the
    field it hands out. */
export function restampExcept(el, except) {
  if (except?.length) el.setAttribute(ATTR.except, except.join(' '));
  else el.removeAttribute(ATTR.except);
}

// ---------------------------------------------------------------------------
// Shape geometry store
// ---------------------------------------------------------------------------

/* Selection chrome (the dashed box, the resize handles, the out-port) is
   injected INTO the shape's own <g> so it rides the g's transform — which
   means a later getBBox would measure the chrome too, and the handles would
   creep outward on every re-selection. The pre-chrome measurement is kept
   here, keyed weakly by element, so it is a declared part of this module's
   interface rather than a property nobody wrote down, and entries expire
   with the elements a rebuild replaces. */
const shapeBoxes = new WeakMap();

/** A live `getBBox`, or null outside a real SVG renderer (happy-dom tests)
    and for a detached/unrendered shape. */
function liveShapeBox(el) {
  if (typeof el?.getBBox !== 'function') return null;
  try {
    return el.getBBox();
  } catch {
    return null; // detached / unrendered SVG
  }
}

/** A shape's CONTENT geometry: the stashed measurement in preference to a
    live one (which would include whatever chrome is currently inside it),
    falling back to a live measurement when nothing was stashed. */
export function shapeBox(el) {
  if (!el) return null;
  if (shapeBoxes.has(el)) return shapeBoxes.get(el);
  return liveShapeBox(el);
}

/** Record and return a shape's content geometry — the ONE writer, called by
    the selection layer as it (re-)selects a shape.

    `chromeInside` says the shape already carries injected chrome, so
    measuring now would swallow it and the stored measurement stands: that
    is what stops the handles creeping outward when the selected shape is
    re-selected. A CLEAN shape — every other shape, and this one once the
    selection has moved off it — re-measures instead, so one whose geometry
    changed while its element survived (an in-place commit) doesn't serve a
    stale box for the life of that element.

    A dirty shape with nothing recorded is the one case with no good answer:
    a live measurement including the chrome beats no geometry at all. */
export function stashShapeBox(el, { chromeInside = false } = {}) {
  if (!el) return null;
  const live = chromeInside ? null : liveShapeBox(el);
  if (live) shapeBoxes.set(el, live);
  return live ?? shapeBox(el);
}
