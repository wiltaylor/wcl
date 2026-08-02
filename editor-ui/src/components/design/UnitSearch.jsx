/* The find-a-unit box: one plain search over every unit in the wskill —
   its id, name, summary and body prose (preview/search.js does the
   matching; the graph payload carries the text). Mounted wherever Design
   mode leaves the reader looking for a unit: the graph toolbar, the index
   panel's head and the content modal.

   Picking a hit focuses the unit — the whole "go here" protocol, which the
   graph already watches to pan and the index panel to reveal where the
   unit is pinned — plus whatever else the host does with a chosen unit
   (the content modal swaps to it). Every hit is listed: at 65 units there
   is nothing to page through, so the list scrolls and the arrows walk it.

   The result list positions itself in the VIEWPORT rather than inside the
   host: two of the three hosts are scroll containers narrower than the
   list (the index panel is 280px), and a scroll container clips both axes,
   so an absolutely-positioned dropdown loses its right-hand column. The
   box owning its own placement is what lets it be mounted anywhere. */

import { For, Show, createEffect, createMemo, createSignal, onCleanup } from 'solid-js';
import { Search } from 'lucide-solid';
import { Badge, Input } from '@forge/ui';

import { graphData, setFocusedNode } from '../../state/graph';
import { searchUnits } from '../../preview/search';

/** Widest the result list gets, and the gap it keeps from the viewport. */
const LIST_WIDTH = 380;
const MARGIN = 8;

export default function UnitSearch(props) {
  const [query, setQuery] = createSignal('');
  const [open, setOpen] = createSignal(false);
  const [active, setActive] = createSignal(0);
  const [box, setBox] = createSignal(null); // viewport placement of the list
  let anchorEl;
  let listEl;

  // Indexes are navigation structure, not units — the graph doesn't even
  // draw them, so a hit on one would point at nothing to jump to.
  const units = () => (graphData()?.nodes ?? []).filter((n) => n.type === 'unit');
  const hits = createMemo(() => searchUnits(units(), query()));
  const showing = () => open() && !!query().trim();

  /** Put the list under the input, kept inside the viewport. */
  const place = () => {
    if (!anchorEl) return;
    const r = anchorEl.getBoundingClientRect();
    const width = Math.max(r.width, LIST_WIDTH);
    setBox({
      left: Math.max(MARGIN, Math.min(r.left, window.innerWidth - width - MARGIN)),
      top: r.bottom + 4,
      width,
      maxHeight: Math.max(120, window.innerHeight - r.bottom - MARGIN * 2),
    });
  };

  // Re-place while the list is up: the hosts scroll (hence capture, which
  // sees inner scrollers too) and the window resizes under it.
  createEffect(() => {
    if (!showing()) return;
    place();
    const onMove = () => place();
    window.addEventListener('scroll', onMove, true);
    window.addEventListener('resize', onMove);
    onCleanup(() => {
      window.removeEventListener('scroll', onMove, true);
      window.removeEventListener('resize', onMove);
    });
  });

  // A fresh query starts at the top; the arrows walk from there and the
  // active row is kept in view, so a match 40 rows down is still reachable.
  createEffect(() => {
    query();
    setActive(0);
  });
  createEffect(() => {
    active();
    listEl?.querySelector('.is-active')?.scrollIntoView({ block: 'nearest' });
  });

  const pick = (hit) => {
    if (!hit) return;
    setFocusedNode(hit.node.key);
    props.onPick?.(hit.node);
    setOpen(false);
  };

  const onKeyDown = (e) => {
    const list = hits();
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setOpen(true);
      setActive(Math.min(active() + 1, list.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActive(Math.max(active() - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      pick(list[active()]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      if (query()) setQuery('');
      else setOpen(false);
    }
  };

  return (
    <div
      ref={anchorEl}
      class={props.class ? `ed-unitsearch ${props.class}` : 'ed-unitsearch'}
      onFocusOut={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) setOpen(false);
      }}
    >
      <Input
        icon={Search}
        type="search"
        value={query()}
        placeholder={props.placeholder ?? 'Find a unit…'}
        aria-label="Find a unit"
        onInput={(e) => {
          setQuery(e.currentTarget.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={onKeyDown}
      />
      <Show when={showing() && box()}>
        {/* The mousedown guard keeps focus in the input: without it the
            focusout closes the list before the click on a row lands. */}
        <div
          class="ed-unitsearch-results"
          ref={listEl}
          style={{
            left: `${box().left}px`,
            top: `${box().top}px`,
            width: `${box().width}px`,
            'max-height': `${box().maxHeight}px`,
          }}
          onMouseDown={(e) => e.preventDefault()}
        >
          <Show
            when={hits().length}
            fallback={<div class="ed-unitsearch-empty">No unit matches “{query().trim()}”</div>}
          >
            <div class="ed-unitsearch-count">
              {hits().length} {hits().length === 1 ? 'match' : 'matches'}
            </div>
            <For each={hits()}>
              {(hit, i) => (
                <button
                  type="button"
                  class="ed-unitsearch-hit"
                  classList={{ 'is-active': active() === i() }}
                  onMouseEnter={() => setActive(i())}
                  onClick={() => pick(hit)}
                >
                  <span class="ed-unitsearch-hit-head">
                    <span class="ed-unitsearch-hit-title">{hit.node.title}</span>
                    <Badge>{hit.node.kind}</Badge>
                  </span>
                  <span class="ed-unitsearch-hit-snip">
                    <span class="ed-unitsearch-hit-field">{hit.field}</span>
                    {hit.snippet.slice(0, hit.at)}
                    <mark>{hit.snippet.slice(hit.at, hit.at + hit.length)}</mark>
                    {hit.snippet.slice(hit.at + hit.length)}
                  </span>
                </button>
              )}
            </For>
          </Show>
        </div>
      </Show>
    </div>
  );
}
