/* Plain-DOM helpers for in-place commits (`commitOpsLocal`): after a
   reorder / visibility write the file reformats and every stamped span in
   it goes stale — instead of rebuilding + reloading the iframe, the caller
   patches the live anchors from the response's `span_map` and mirrors the
   change directly (move the element, re-stamp the visibility attributes).
   Same-origin, same style as frame.js; the stamps themselves are read and
   written through preview/anchors.js. */

import {
  anchorEls,
  pageEls,
  pageSpanOf,
  restampPageSpan,
  restampSpan,
  spanKey,
  spanOf,
} from './anchors';

/** Rewrite every stamped block span (and the page wrapper's) for `file`
    from a block_ops `span_map` (`[{from: {start,end}, to: {start,end}}]`).
    Spans not in the map are left untouched. Returns the number of patched
    attributes. */
export function patchAnchors(doc, file, spanMap) {
  if (!doc || !spanMap?.length) return 0;
  const map = new Map(spanMap.map((e) => [spanKey(e.from), spanKey(e.to)]));
  let patched = 0;
  for (const el of anchorEls(doc, file)) {
    const span = spanOf(el);
    const to = span && map.get(spanKey(span));
    if (to) {
      restampSpan(el, to);
      patched += 1;
    }
  }
  for (const el of pageEls(doc, file)) {
    const span = pageSpanOf(el);
    const to = span && map.get(spanKey(span));
    if (to) {
      restampPageSpan(el, to);
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

/** Move `el` before `refBefore` (or to its parent's end when null),
    returning a `revert()` that restores the original position. */
export function moveDomBlock(el, refBefore) {
  const parent = el.parentElement;
  const next = el.nextSibling;
  if (refBefore) refBefore.parentElement.insertBefore(el, refBefore);
  else parent.appendChild(el);
  return () => parent.insertBefore(el, next);
}
