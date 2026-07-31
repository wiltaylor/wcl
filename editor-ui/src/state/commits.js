/* The commit bus.

   Every write that lands on disk announces itself here once, naming the
   SURFACE that already reflects it — the one that repaired its own DOM in
   place, or rebuilt through its own preview. Everything else derived from
   the old bytes (another view tab's cached build, the canvas behind a
   modal, the graph payload) is stale by definition.

   Previews subscribe (state/preview.js) instead of components remembering
   to raise a flag: a surface that forgets to participate can no longer
   show output that quietly no longer matches disk. */

const listeners = new Set();

/** Subscribe. Returns the unsubscribe function. */
export function onCommit(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Announce a landed commit. `event`:
    - `surface` — the id (or ids) of the previews whose mounted target
      already shows the change (null when nothing on screen does)
    - `patched` — that surface fixed its live DOM rather than rebuilding, so
      what it shows is current only until the frame reloads

    Deliberately not the files the commit wrote: which pages they affect is
    the server's decision (a preview's rebuild passes `changed` straight
    through), so a subscriber has nothing to do with them. Add it when one
    does. */
export function emitCommit(event) {
  for (const fn of [...listeners]) fn(event);
}
