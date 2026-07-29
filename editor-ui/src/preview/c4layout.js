/* Nested-box layout for the Systems view — the WAD's containment forest
   drawn as boxes inside boxes.

   Nothing here is persisted: a box's position and size are DERIVED every
   time the model changes, so adding, removing, collapsing or re-parenting a
   node reflows the picture without any stored geometry. The pass is
   bottom-up — a leaf sizes itself from its title, a parent arranges its
   children and grows to fit them plus its header band — and deterministic in
   payload order, so two layouts of the same model are identical (no jitter
   after a refetch).

   Children a user has DRAGGED are the exception: `positions` (session-only,
   parent-local) places those exactly where they were dropped, and since a
   parent's size is the union of its children's boxes, dragging one past the
   edge simply makes the parent bigger. Everything else keeps auto-packing.

   Coordinates are absolute world units; the canvas pans and zooms by
   moving its viewBox over them. */

/** Layout constants, in world units. */
export const LAYOUT = {
  /** Minimum leaf box. */
  minW: 150,
  minH: 56,
  maxW: 320,
  /** Per-character title width used to size leaves. */
  charW: 7.2,
  /** Title band at the top of a box that has children. */
  header: 34,
  /** Space between a parent's edge and its children. */
  padX: 16,
  padY: 14,
  /** Space between siblings. */
  gapX: 20,
  gapY: 18,
};

/** The width a leaf box needs for its label. */
function leafWidth(node) {
  const chars = Math.max((node.title ?? node.id ?? '').length, (node.subtitle ?? '').length + 2);
  return Math.min(Math.max(chars * LAYOUT.charW + 28, LAYOUT.minW), LAYOUT.maxW);
}

/**
 * Build the containment forest from a flat `/api/systems` node list.
 *
 * A node hangs off the FIRST of its parent links whose target is on the
 * canvas — `parents` lists every parent field the instance sets, so a
 * `deploy_target` nests under its container in the C4 perspective and under
 * its environment in the deployment one, from the same data. With none of
 * them visible the node is promoted to a root: a filtered-out or dangling
 * parent hides the relationship, never the node itself. Cycles (a corrupt
 * parent chain) are broken by promoting the node that closes the loop.
 *
 * @param nodes  the payload's `nodes` array
 * @param opts   { visibleKinds?: Set<string>, collapsed?: Set<string> }
 * @returns { roots, byKey, childrenOf, parentOf } — `childrenOf` is empty
 *          for a collapsed node (it renders as a leaf with a count badge).
 */
export function buildForest(nodes, { visibleKinds = null, collapsed = new Set() } = {}) {
  const visible = (n) => !visibleKinds || visibleKinds.has(n.kind);
  const shown = (nodes ?? []).filter(visible);
  const byKey = new Map(shown.map((n) => [n.key, n]));
  const byId = new Map();
  for (const n of shown) if (!byId.has(n.id)) byId.set(n.id, n);

  const parentOf = new Map();
  for (const n of shown) {
    const links = n.parents?.length ? n.parents : n.parent ? [n.parent] : [];
    for (const link of links) {
      const p = byId.get(link.id);
      if (p && p.key !== n.key) {
        parentOf.set(n.key, p.key);
        break;
      }
    }
  }
  // Break cycles: walk each node's ancestor chain and drop the link that
  // revisits the node itself.
  for (const n of shown) {
    const seen = new Set([n.key]);
    let cur = parentOf.get(n.key);
    while (cur) {
      if (seen.has(cur)) {
        parentOf.delete(n.key);
        break;
      }
      seen.add(cur);
      cur = parentOf.get(cur);
    }
  }

  const childrenOf = new Map(shown.map((n) => [n.key, []]));
  const roots = [];
  for (const n of shown) {
    const pkey = parentOf.get(n.key);
    if (pkey) childrenOf.get(pkey).push(n);
    else roots.push(n);
  }
  // Collapsed nodes keep their real children in `hiddenCount` only.
  const hiddenCount = new Map();
  for (const key of collapsed) {
    const kids = childrenOf.get(key);
    if (kids?.length) {
      hiddenCount.set(key, descendantCount(key, childrenOf));
      childrenOf.set(key, []);
    }
  }
  return { roots, byKey, childrenOf, parentOf, hiddenCount };
}

function descendantCount(key, childrenOf) {
  let n = 0;
  for (const c of childrenOf.get(key) ?? []) n += 1 + descendantCount(c.key, childrenOf);
  return n;
}

/** Pack `sizes` into a near-square grid; returns row-major placements. */
function packGrid(sizes) {
  const cols = Math.max(1, Math.ceil(Math.sqrt(sizes.length)));
  const rows = [];
  for (let i = 0; i < sizes.length; i += cols) rows.push(sizes.slice(i, i + cols));
  // Column widths are shared across rows so the grid reads as a grid.
  const colW = [];
  for (const row of rows) {
    row.forEach((s, c) => {
      colW[c] = Math.max(colW[c] ?? 0, s.w);
    });
  }
  const placed = [];
  let y = 0;
  for (const row of rows) {
    let x = 0;
    const rowH = Math.max(...row.map((s) => s.h));
    row.forEach((s, c) => {
      placed.push({ ...s, x, y });
      x += colW[c] + LAYOUT.gapX;
    });
    y += rowH + LAYOUT.gapY;
  }
  const w = colW.reduce((a, b) => a + b, 0) + LAYOUT.gapX * Math.max(0, colW.length - 1);
  const h = Math.max(0, y - LAYOUT.gapY);
  return { placed, w, h };
}

/**
 * Arrange one box's children: those the user has dragged sit exactly where
 * they were put (`positions`, in parent-local units), the rest auto-pack
 * into a grid placed BELOW them so a manual arrangement is never disturbed
 * by an automatic one. The returned extent is the union — which is what
 * makes a parent grow to contain a child dragged past its edge.
 */
function placeChildren(sizes, positions) {
  const manual = [];
  const auto = [];
  for (const s of sizes) (positions.get(s.key) ? manual : auto).push(s);
  const placed = manual.map((s) => ({ ...s, ...positions.get(s.key) }));
  const below = placed.length
    ? Math.max(...placed.map((p) => p.y + p.h)) + LAYOUT.gapY
    : 0;
  const grid = packGrid(auto);
  placed.push(...grid.placed.map((p) => ({ ...p, y: p.y + below })));
  return {
    placed,
    w: Math.max(0, ...placed.map((p) => p.x + p.w)),
    h: Math.max(0, ...placed.map((p) => p.y + p.h)),
  };
}

/**
 * Lay the forest out. Returns absolute boxes keyed by node key, plus the
 * total extent — the canvas fits its initial viewBox to that.
 *
 * @param opts.positions  Map<key, {x, y}> of dragged boxes, in coordinates
 *                        relative to their parent's content origin (or to
 *                        the world origin for a root).
 * @returns { boxes: Map<key, {x, y, w, h, depth, origin}>, w, h, order }
 *          `order` lists keys outermost-first (SVG paint order), and
 *          `origin` is where a box's children start — the frame a drag
 *          converts a world point into.
 */
export function layoutForest(forest, { positions = new Map() } = {}) {
  const { roots, childrenOf } = forest;
  const pos = positions instanceof Map ? positions : new Map(Object.entries(positions ?? {}));
  /** Size one subtree, bottom-up; returns {w, h, kids: [{key, x, y}]}. */
  const sized = new Map();
  const size = (node) => {
    const kids = childrenOf.get(node.key) ?? [];
    if (kids.length === 0) {
      const box = { w: leafWidth(node), h: LAYOUT.minH, kids: [] };
      sized.set(node.key, box);
      return box;
    }
    const inner = placeChildren(
      kids.map((k) => ({ key: k.key, ...size(k) })),
      pos,
    );
    const box = {
      w: Math.max(inner.w + LAYOUT.padX * 2, leafWidth(node)),
      h: inner.h + LAYOUT.header + LAYOUT.padY,
      kids: inner.placed,
    };
    sized.set(node.key, box);
    return box;
  };
  for (const r of roots) size(r);

  const top = placeChildren(
    roots.map((r) => ({ key: r.key, ...sized.get(r.key) })),
    pos,
  );
  const boxes = new Map();
  const order = [];
  const place = (key, x, y, depth) => {
    const s = sized.get(key);
    // Children are packed left-aligned; centre the block horizontally so a
    // wide parent (a long title) doesn't leave its children hugging one side.
    const innerW = Math.max(...s.kids.map((k) => k.x + k.w), 0);
    // Centring is stable under hand placement: a parent sized by its own
    // content has exactly `padX` to spare, so the origin only floats for a
    // box made wider by a long title — and then its content width is fixed.
    const offX = x + Math.max(LAYOUT.padX, (s.w - innerW) / 2);
    const offY = y + LAYOUT.header;
    boxes.set(key, { x, y, w: s.w, h: s.h, depth, origin: { x: offX, y: offY } });
    order.push(key);
    for (const k of s.kids) place(k.key, offX + k.x, offY + k.y, depth + 1);
  };
  for (const p of top.placed) place(p.key, p.x, p.y, 0);
  return { boxes, order, w: top.w, h: top.h, origin: { x: 0, y: 0 } };
}

/** Where a box's children are measured from (the world origin at the top). */
export const originOf = (layout, parentKey) =>
  (parentKey ? layout.boxes.get(parentKey)?.origin : layout.origin) ?? { x: 0, y: 0 };

/**
 * The chain of boxes under a world point, outermost first. The LAST entry
 * is the innermost hit — the canvas selects that (nearest-first, like the
 * preview's anchor chain) and Esc pops back outward.
 */
export function hitChain(point, layout) {
  const hits = [];
  for (const key of layout.order) {
    const b = layout.boxes.get(key);
    if (
      point.x >= b.x &&
      point.x <= b.x + b.w &&
      point.y >= b.y &&
      point.y <= b.y + b.h
    ) {
      hits.push(key);
    }
  }
  // `order` is a pre-order walk, so deeper boxes come later; sorting by
  // depth makes that explicit and survives future ordering changes.
  return hits.sort((a, b) => layout.boxes.get(a).depth - layout.boxes.get(b).depth);
}

/** The innermost box at a point, or null. */
export function hitTest(point, layout) {
  const chain = hitChain(point, layout);
  return chain.length ? chain[chain.length - 1] : null;
}

/**
 * The nearest laid-out ancestor of a node id — an edge endpoint inside a
 * collapsed (or filtered-out) parent attaches to the box that stands in for
 * it. `null` when neither the node nor any ancestor is on the canvas.
 */
export function rollUp(id, forest, layout) {
  let node = null;
  for (const n of forest.byKey.values()) {
    if (n.id === id) {
      node = n;
      break;
    }
  }
  let key = node?.key ?? null;
  while (key) {
    if (layout.boxes.has(key)) return key;
    key = forest.parentOf.get(key) ?? null;
  }
  return null;
}

/** Where an edge leaves/enters a box: the centre of its nearest side. */
function anchor(from, to) {
  const cx = from.x + from.w / 2;
  const cy = from.y + from.h / 2;
  const tx = to.x + to.w / 2;
  const ty = to.y + to.h / 2;
  const dx = tx - cx;
  const dy = ty - cy;
  if (Math.abs(dx) * from.h >= Math.abs(dy) * from.w) {
    return { x: dx >= 0 ? from.x + from.w : from.x, y: cy, side: dx >= 0 ? 'r' : 'l' };
  }
  return { x: cx, y: dy >= 0 ? from.y + from.h : from.y, side: dy >= 0 ? 'b' : 't' };
}

/**
 * A cubic Bézier between two boxes, leaving and entering through the facing
 * sides with tangents that keep the curve clear of the boxes themselves.
 */
export function edgePath(a, b) {
  if (!a || !b) return '';
  const o = anchor(a, b);
  const i = anchor(b, a);
  const t = Math.min(Math.max(Math.hypot(i.x - o.x, i.y - o.y) / 3, 30), 120);
  const tan = (p) =>
    p.side === 'r'
      ? [p.x + t, p.y]
      : p.side === 'l'
        ? [p.x - t, p.y]
        : p.side === 'b'
          ? [p.x, p.y + t]
          : [p.x, p.y - t];
  const [c1x, c1y] = tan(o);
  const [c2x, c2y] = tan(i);
  return `M ${o.x} ${o.y} C ${c1x} ${c1y}, ${c2x} ${c2y}, ${i.x} ${i.y}`;
}

/** Self-edge (both endpoints roll up to the same box): a small loop. */
export function selfPath(box) {
  const x = box.x + box.w;
  const y = box.y + box.h / 2;
  return `M ${x} ${y - 10} C ${x + 34} ${y - 26}, ${x + 34} ${y + 26}, ${x} ${y + 10}`;
}
