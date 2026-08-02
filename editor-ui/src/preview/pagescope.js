/* Who owns the shown page.

   It belongs to whichever editable surface is on top: one mounted OVER
   another (a modal above the canvas) borrows the page and hands it back on
   close, so the canvas's targeted rebuild doesn't chase a page it was never
   showing. Surfaces come and go out of order — a tab switch mounts the next
   before dropping the last — so a surface that is no longer on top gives
   nothing back; the one above it already owns the page.

   A scope hands back what the surface BELOW it was showing — `null`
   included, since a modal opened before anything had loaded must still undo
   its own page. With nothing below, it hands back only a page it actually
   found: no one is waiting for it, and nulling the shared page would strip
   the `page_file` every commit is scoped by.

   A stack rather than a per-host `prevPage`, so a host cannot forget it.
   Pure: pages are opaque values in and out, and `release()` REPORTS what to
   restore rather than writing it, so the shared state stays with the
   caller. */

/** A stack of page scopes. One per application; the surfaces share it. */
export function createPageScopes() {
  const stack = [];
  return {
    /**
     * Open a scope for a surface mounting over `outer` (the page showing at
     * the time). Returns `{ note, release }`: `note(page)` records the page
     * this surface is showing; `release()` closes the scope and answers
     * `{ restore: true, page }` — the page the surface below was showing,
     * else the one this surface found — or `{ restore: false }` when there
     * is nothing to give back (this scope was not on top, or it was the
     * last one and found no page to begin with).
     */
    push(outer) {
      const entry = { outer, page: outer };
      stack.push(entry);
      return {
        note(page) {
          entry.page = page;
        },
        release() {
          const i = stack.indexOf(entry);
          if (i < 0) return { restore: false };
          const wasTop = i === stack.length - 1;
          stack.splice(i, 1);
          if (!wasTop) return { restore: false };
          const below = stack[stack.length - 1];
          if (below) return { restore: true, page: below.page };
          if (entry.outer == null) return { restore: false };
          return { restore: true, page: entry.outer };
        },
      };
    },
  };
}
