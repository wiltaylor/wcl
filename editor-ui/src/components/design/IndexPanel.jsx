/* The graph tab's left bar: the wskill's indexes as an editable pick-list.
   Selecting an index scopes the graph to its subtree (non-members fade)
   and opens its member tree — top-level pins plus nested sub-indexes as
   indented sub-headings, always fully expanded. Reorder with the arrows
   (within one level), unpin with the −, pin more units via the select or
   by DRAGGING a node from the graph onto this panel (GraphView hit-tests
   the drop against the element registered here). While a unit node is
   focused on the graph, every heading and sub-heading grows a + that pins
   it right there, and the panel auto-expands the index containing the
   unit (reveal only — selection/scoping stays an explicit row click),
   scrolling to and highlighting its rows. A view filter narrows the list
   to indexes visible in one view. All writes go through the quiet nav-op
   path (pin_unit / unpin_unit / reorder_children — id-addressed, so they
   reach nested levels) and refetch the graph keeping positions. */

import { For, Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import { ArrowDown, ArrowUp, Minus, Pin, Plus } from 'lucide-solid';
import { Badge, IconButton, Select } from '@forge/ui';

import { busy, commitNavOpQuiet } from '../../state/design';
import {
  focusedUnitNode,
  graphData,
  indexHitsForUnit,
  nodeDragging,
  reloadGraph,
  revealedIndex,
  selectedIndex,
  selectedIndexNode,
  setFocusedNode,
  setIndexPanelEl,
  setRevealedIndex,
  setSelectedIndex,
  subtreePinnedIds,
} from '../../state/graph';

const afterOp = (res) => {
  if (res.ok) reloadGraph({ keepPositions: true });
};
/** Swap a pinned id with its neighbour within one index level. */
const moveIn = (level, id, dir) => {
  const ids = [...(level.pinned ?? [])];
  const i = ids.indexOf(id);
  const j = dir === 'up' ? i - 1 : i + 1;
  if (i < 0 || j < 0 || j >= ids.length) return;
  [ids[i], ids[j]] = [ids[j], ids[i]];
  commitNavOpQuiet({ op: 'reorder_children', index_id: level.id, order: ids }).then(afterOp);
};
const unpinFrom = (level, id) =>
  commitNavOpQuiet({ op: 'unpin_unit', index_id: level.id, unit_id: id }).then(afterOp);
const pinInto = (level, id) =>
  id && commitNavOpQuiet({ op: 'pin_unit', index_id: level.id, unit_id: id }).then(afterOp);

const nodeById = (id) => graphData()?.nodes.find((n) => n.id === id);

/** The focused-unit "+" for one heading level: pin it into this level's
    own `related` list. Hidden while nothing is focused, the unit is
    already here, or the level's list is computed. */
function PinHereButton(props) {
  const canPin = () => {
    const unit = focusedUnitNode();
    return (
      unit &&
      props.level.related_editable !== false &&
      !props.level.syllabus &&
      !(props.level.pinned ?? []).includes(unit.id)
    );
  };
  return (
    <Show when={canPin()}>
      <IconButton
        icon={Plus}
        label={`Pin “${focusedUnitNode().title}” into ${props.level.title}`}
        disabled={busy()}
        onClick={() => pinInto(props.level, focusedUnitNode().id)}
      />
    </Show>
  );
}

/** One index level's member rows plus its sub-headings, recursively.
    Depth 0 is the accordion entry itself (its heading is the index row);
    deeper levels render an indented sub-heading first. */
function IndexSection(props) {
  const indent = (extra = 0) => ({ 'padding-left': `${props.depth * 14 + extra}px` });
  const editable = () => props.level.related_editable !== false;
  return (
    <>
      <Show when={props.depth > 0}>
        <li class="ed-index-subhead" style={indent()}>
          <span class="ed-index-title" title={props.level.id}>
            {props.level.title}
          </span>
          <span class="ed-index-count">{props.level.pinned?.length ?? 0}</span>
          <PinHereButton level={props.level} />
        </li>
      </Show>
      <For each={props.level.pinned ?? []}>
        {(id) => {
          const unit = () => nodeById(id);
          // Clicking a member focuses its graph node — the same selection a
          // click on the node itself makes, so the panel doubles as a way to
          // find a unit by name. Clicks on the row's own controls (reorder /
          // unpin) are theirs; a dangling pin has no node to select.
          const pick = (e) => {
            if (e.target.closest?.('.ed-nav-actions')) return;
            const n = unit();
            if (n) setFocusedNode(n.key);
          };
          return (
            <li
              class="ed-index-member"
              classList={{ 'is-hit': focusedUnitNode()?.id === id, 'is-pickable': !!unit() }}
              style={indent()}
              onClick={pick}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  pick(e);
                }
              }}
              tabindex={unit() ? 0 : undefined}
              role={unit() ? 'button' : undefined}
            >
              <span class="ed-index-title" title={id}>
                {unit()?.title ?? id}
              </span>
              <Show when={unit()} fallback={<Badge tone="danger">missing</Badge>}>
                <Badge>{unit().kind}</Badge>
              </Show>
              <span class="ed-nav-actions">
                <IconButton
                  icon={ArrowUp}
                  label="Move up"
                  disabled={busy() || !editable()}
                  onClick={() => moveIn(props.level, id, 'up')}
                />
                <IconButton
                  icon={ArrowDown}
                  label="Move down"
                  disabled={busy() || !editable()}
                  onClick={() => moveIn(props.level, id, 'down')}
                />
                <Show when={!props.level.syllabus}>
                  <IconButton
                    icon={Minus}
                    label="Unpin from here"
                    disabled={busy() || !editable()}
                    onClick={() => unpinFrom(props.level, id)}
                  />
                </Show>
              </span>
            </li>
          );
        }}
      </For>
      <For each={props.level.children ?? []}>
        {(c) => <IndexSection level={c} depth={props.depth + 1} />}
      </For>
    </>
  );
}

export default function IndexPanel() {
  let el;
  onMount(() => setIndexPanelEl(el));
  onCleanup(() => setIndexPanelEl(null));

  const [viewFilter, setViewFilter] = createSignal(''); // '' = every view
  const indexes = () =>
    (graphData()?.nodes ?? [])
      .filter((n) => n.type === 'index')
      .filter((n) => !viewFilter() || n.views?.[viewFilter()] !== false);
  const idx = selectedIndexNode;

  // Units offered by an index's bottom "pin a unit…" select — anything not
  // already in that index's own top-level list (sub-index membership does
  // not block an intentional top-level pin).
  const unpinnedFor = (n) =>
    (graphData()?.nodes ?? []).filter(
      (u) => u.type === 'unit' && !(n.pinned ?? []).includes(u.id),
    );

  // Reveal where the focused unit lives: expand (not select) the first
  // containing index — preferring one already open — and scroll to its
  // highlighted row once per focused unit (graph refetches re-run this).
  let lastRevealed = null;
  createEffect(() => {
    const unit = focusedUnitNode();
    if (!unit) {
      lastRevealed = null;
      setRevealedIndex(null);
      return;
    }
    const hits = indexHitsForUnit(graphData(), unit.id);
    if (hits.length === 0) {
      setRevealedIndex(null);
      return;
    }
    const open = hits.find((h) => h.topKey === selectedIndex() || h.topKey === revealedIndex());
    if (!open) setRevealedIndex(hits[0].topKey);
    if (lastRevealed === unit.id) return;
    lastRevealed = unit.id;
    requestAnimationFrame(() =>
      el?.querySelector('.ed-index-member.is-hit')?.scrollIntoView({ block: 'nearest' }),
    );
  });

  return (
    <div
      ref={el}
      class="ed-nav-panel ed-index-panel"
      classList={{ 'is-dropready': !!nodeDragging() && !!idx() }}
    >
      <div class="ed-nav-head">
        <strong>Indexes</strong>
        <Show when={focusedUnitNode()}>
          <span class="ed-index-focushint" title="The + buttons below pin the focused graph node">
            <Pin size={11} /> {focusedUnitNode().title}
          </span>
        </Show>
      </div>
      <Show when={graphData()} fallback={<div class="ed-empty">Loading the graph…</div>}>
        <div class="ed-index-viewfilter">
          <button
            type="button"
            class="ed-graph-filterchip"
            classList={{ 'is-on': viewFilter() === '' }}
            onClick={() => setViewFilter('')}
          >
            all
          </button>
          <For each={graphData().sites}>
            {(site) => (
              <button
                type="button"
                class="ed-graph-filterchip"
                classList={{ 'is-on': viewFilter() === site }}
                title={`Indexes visible in the ${site} view`}
                onClick={() => setViewFilter(viewFilter() === site ? '' : site)}
              >
                {site}
              </button>
            )}
          </For>
        </div>

        <ul class="ed-index-list">
          <For each={indexes()}>
            {(n) => (
              <li>
                <div class="ed-index-rowwrap">
                  <button
                    type="button"
                    class="ed-index-row"
                    classList={{ 'is-selected': selectedIndex() === n.key }}
                    onClick={() => setSelectedIndex(selectedIndex() === n.key ? null : n.key)}
                  >
                    <span class="ed-index-title">{n.title}</span>
                    <span class="ed-index-count">{subtreePinnedIds(n).size}</span>
                  </button>
                  <PinHereButton level={n} />
                </div>
                <Show when={selectedIndex() === n.key || revealedIndex() === n.key}>
                  <ul class="ed-index-members">
                    <IndexSection level={n} depth={0} />
                    <Show when={subtreePinnedIds(n).size === 0}>
                      <li class="ed-graph-noblocks">
                        {n.syllabus ? 'no lessons yet' : 'nothing pinned yet'}
                      </li>
                    </Show>
                    {/* A course has nothing to pin — every lesson is already in
                        it; the arrows reorder it (rewriting each lesson's `n`). */}
                    <Show when={!n.syllabus}>
                      <li class="ed-nav-pinrow">
                        <Pin size={12} />
                        <Select
                          options={unpinnedFor(n).map((u) => ({
                            value: u.id,
                            label: `${u.title} (${u.kind})`,
                          }))}
                          placeholder="Pin a unit…"
                          value={undefined}
                          disabled={busy()}
                          onChange={(id) => pinInto(n, id)}
                        />
                      </li>
                    </Show>
                  </ul>
                </Show>
              </li>
            )}
          </For>
          <Show when={indexes().length === 0}>
            <li class="ed-graph-noblocks">no indexes{viewFilter() ? ` in ${viewFilter()}` : ''}</li>
          </Show>
        </ul>

        <Show when={nodeDragging() && idx()}>
          <div class="ed-index-drophint">Drop to pin into “{idx().title}”</div>
        </Show>
        <Show when={nodeDragging() && !idx()}>
          <div class="ed-index-drophint is-muted">Select an index to drop into</div>
        </Show>
      </Show>
    </div>
  );
}
