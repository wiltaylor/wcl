/* Plain-DOM helpers for in-place commits (`commitOpsLocal`): after a
   reorder / visibility write the file reformats and every stamped
   `data-wcl-span` in it goes stale — instead of rebuilding + reloading the
   iframe, the caller patches the live anchors from the response's
   `span_map` and mirrors the change directly (move the element, restamp
   the visibility attributes). Same-origin, same style as frame.js. */

import { blockChildren, pageInfo } from './frame';

const key = (s) => `${s.start}:${s.end}`;

/** Rewrite every `data-wcl-span` (and the page wrapper's
    `data-wcl-page-span`) stamped for `file` from a block_ops `span_map`
    (`[{from: {start,end}, to: {start,end}}]`). Spans not in the map are
    left untouched. Returns the number of patched attributes. */
export function patchAnchors(doc, file, spanMap) {
  if (!doc || !spanMap?.length) return 0;
  const map = new Map(spanMap.map((e) => [key(e.from), key(e.to)]));
  let patched = 0;
  for (const el of doc.querySelectorAll(`[data-wcl-file="${CSS.escape(file)}"][data-wcl-span]`)) {
    const to = map.get(el.getAttribute('data-wcl-span'));
    if (to) {
      el.setAttribute('data-wcl-span', to);
      patched += 1;
    }
  }
  for (const el of doc.querySelectorAll(
    `[data-wcl-page-file="${CSS.escape(file)}"][data-wcl-page-span]`,
  )) {
    const to = map.get(el.getAttribute('data-wcl-page-span'));
    if (to) {
      el.setAttribute('data-wcl-page-span', to);
      patched += 1;
    }
  }
  return patched;
}

/** The post-format span for a pre-edit one, or null when unmapped. */
export function mappedSpan(spanMap, span) {
  const hit = spanMap?.find((e) => e.from.start === span.start && e.from.end === span.end);
  return hit ? { start: hit.to.start, end: hit.to.end } : null;
}

/** Every element anchored at (file, span) — repeater/template output can
    render one source block more than once. */
export function elsBySpan(doc, file, span) {
  return [
    ...doc.querySelectorAll(
      `[data-wcl-file="${CSS.escape(file)}"][data-wcl-span="${span.start}:${span.end}"]`,
    ),
  ];
}

/** The adjacent same-file block sibling of `el` in `dir` ('up' | 'down'),
    or null. Siblings are the nearest [data-wcl-block] container's block
    children (the page root at top level), filtered to `el`'s file —
    mirroring the server `move` op's "adjacent sibling in the same
    container" semantics under the established DOM-order == AST-order
    assumption for same-file runs. */
export function adjacentSameFileSibling(doc, el, dir) {
  const page = pageInfo(doc);
  if (!page) return null;
  let container = null;
  let p = el.parentElement;
  while (p && p !== page.el) {
    if (p.hasAttribute('data-wcl-block')) {
      container = p;
      break;
    }
    p = p.parentElement;
  }
  const file = el.getAttribute('data-wcl-file');
  const siblings = blockChildren(container ?? page.el).filter(
    (b) => b.getAttribute('data-wcl-file') === file,
  );
  const i = siblings.indexOf(el);
  if (i < 0) return null;
  return siblings[dir === 'up' ? i - 1 : i + 1] ?? null;
}

/** Move `el` before `refBefore` (or to its parent's end when null),
    returning a `revert()` that restores the original position. */
export function moveDomBlock(el, refBefore) {
  const parent = el.parentElement;
  const next = el.nextSibling;
  if (refBefore) refBefore.parentElement.insertBefore(el, refBefore);
  else parent.appendChild(el);
  return () => parent.insertBefore(el, next);
}

/** Rewrite the merged-build visibility stamp to `exceptSites` (space-
    joined; removed when empty) — matches `visOf` in visgutter.js. */
export function restampExcept(el, exceptSites) {
  if (exceptSites?.length) el.setAttribute('data-wcl-except', exceptSites.join(' '));
  else el.removeAttribute('data-wcl-except');
}
