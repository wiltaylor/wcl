/* The find-a-unit box: one plain search over every unit in the wskill —
   its id, name, summary and body prose (state/../preview/search.js does the
   matching; the graph payload carries the text). Mounted wherever Design
   mode leaves the reader looking for a unit: the graph toolbar, the index
   panel's head and the content modal.

   Picking a hit is `revealNode` — the graph pans to it and the index panel
   reveals where it is pinned — plus whatever else the host does with a
   chosen unit (the content modal swaps to it). Every hit is listed: at 65
   units there is nothing to page through, so the list scrolls and the
   arrows walk it. */

import { For, Show, createEffect, createMemo, createSignal } from 'solid-js';
import { Search } from 'lucide-solid';
import { Badge, Input } from '@forge/ui';

import { graphData, revealNode } from '../../state/graph';
import { searchUnits } from '../../preview/search';

/** What a hit's field is called in the list. */
const FIELD_LABEL = { id: 'id', name: 'name', summary: 'summary', body: 'body' };

export default function UnitSearch(props) {
  const [query, setQuery] = createSignal('');
  const [open, setOpen] = createSignal(false);
  const [active, setActive] = createSignal(0);
  let listEl;

  // Indexes are navigation structure, not units — the graph doesn't even
  // draw them, so a hit on one would point at nothing to jump to.
  const units = () => (graphData()?.nodes ?? []).filter((n) => n.type === 'unit');
  const hits = createMemo(() => searchUnits(units(), query()));

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
    revealNode(hit.node);
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
      <Show when={open() && query().trim()}>
        {/* The mousedown guard keeps focus in the input: without it the
            focusout closes the list before the click on a row lands. */}
        <div
          class="ed-unitsearch-results"
          ref={listEl}
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
                    <span class="ed-unitsearch-hit-field">{FIELD_LABEL[hit.field]}</span>
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
