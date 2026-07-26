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
   scrolling to and highlighting its rows. Clicking a member row focuses its
   node on the canvas; double-clicking opens its editor. A view filter narrows
   the list
   to indexes visible in one view.

   The headings themselves are editable too: the head's folder button opens an
   inline create form (name → derived id), and every heading carries reorder
   arrows, nest / un-nest (a top-level index nests under the one above it; a
   sub-index promotes back out), an add-sub-index button and a confirming
   delete. All writes go through the quiet nav-op path (pin_unit /
   unpin_unit / reorder_children / create_ / delete_ / move_ / promote_ /
   demote_index — id-addressed, so they reach nested levels) and refetch the
   graph keeping positions. */

import { For, Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import {
  ArrowDown,
  ArrowUp,
  FolderPlus,
  IndentDecrease,
  IndentIncrease,
  Minus,
  Pin,
  Plus,
  Trash2,
} from 'lucide-solid';
import { Badge, Button, IconButton, Input, Select } from '@forge/ui';

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
  setContentFor,
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

// ---- index structure (the headings themselves, not their members) ----
// Id-addressed like the pin ops; the server refuses what the projections
// can't render (nesting deeper than one level, moving past a file edge) and
// the error surfaces as a toast.
const structOp = (payload) => commitNavOpQuiet(payload).then(afterOp);
const moveIndex = (level, dir) => structOp({ op: 'move_index', index_id: level.id, dir });
const promoteIndex = (level) => structOp({ op: 'promote_index', index_id: level.id });
const demoteIndex = (level) => structOp({ op: 'demote_index', index_id: level.id });
const deleteIndex = (level) => structOp({ op: 'delete_index', index_id: level.id });

/** A WCL identifier derived from a display name (`"Getting started"` →
    `getting_started`), the same shape the add-unit form uses. */
const slugId = (name) => {
  const s = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '');
  return !s ? '' : /^[0-9]/.test(s) ? `i_${s}` : s;
};

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

/** The structural controls one index heading carries: reorder among its
    siblings, nest/unnest it, add a sub-index (top level only — sub-indexes
    render one level deep), and delete. Delete confirms in place, since it
    takes the whole subtree with it. `onSub` opens the create form. */
function IndexActions(props) {
  const [confirming, setConfirming] = createSignal(false);
  const sub = () => props.depth > 0;
  return (
    <>
      <span class="ed-nav-actions">
        <IconButton
          icon={ArrowUp}
          label="Move up"
          disabled={busy()}
          onClick={() => moveIndex(props.level, 'up')}
        />
        <IconButton
          icon={ArrowDown}
          label="Move down"
          disabled={busy()}
          onClick={() => moveIndex(props.level, 'down')}
        />
        <Show
          when={sub()}
          fallback={
            <IconButton
              icon={IndentIncrease}
              label="Nest under the index above"
              disabled={busy()}
              onClick={() => demoteIndex(props.level)}
            />
          }
        >
          <IconButton
            icon={IndentDecrease}
            label="Promote to a top-level index"
            disabled={busy()}
            onClick={() => promoteIndex(props.level)}
          />
        </Show>
        <Show when={!sub()}>
          <IconButton
            icon={FolderPlus}
            label="Add a sub-index"
            disabled={busy()}
            onClick={() => props.onSub()}
          />
        </Show>
        <IconButton
          icon={Trash2}
          label="Delete this index"
          disabled={busy()}
          onClick={() => setConfirming(true)}
        />
      </span>
      {/* A span, not a row element: this renders inside both the top-level
          `div` wrapper and the sub-heading `li`, wrapping onto its own line. */}
      <Show when={confirming()}>
        <span class="ed-index-confirm">
          Delete “{props.level.title}”{props.level.children?.length ? ' and its sub-indexes' : ''}?
          The units stay — only the grouping goes. (recoverable via git)
          <span class="ed-index-confirm-actions">
            <Button
              size="sm"
              variant="danger"
              disabled={busy()}
              onClick={() => {
                setConfirming(false);
                deleteIndex(props.level);
              }}
            >
              Delete
            </Button>
            <Button size="sm" onClick={() => setConfirming(false)}>
              Keep
            </Button>
          </span>
        </span>
      </Show>
    </>
  );
}

/** Name → id form for a new index (top level, or nested when `parentId` is
    set). The id is derived from the name, like the add-unit dialog. */
function NewIndexForm(props) {
  const [name, setName] = createSignal('');
  const id = () => slugId(name());
  const submit = () => {
    if (!id()) return;
    commitNavOpQuiet({
      op: 'create_index',
      id: id(),
      name: name().trim(),
      parent_id: props.parentId ?? null,
    }).then((res) => {
      if (!res.ok) return;
      afterOp(res);
      props.onDone();
    });
  };
  return (
    <li class="ed-index-newform">
      <Input
        value={name()}
        placeholder={props.parentId ? `Sub-index of ${props.parentId}…` : 'New index name…'}
        autofocus
        onInput={(e) => setName(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') submit();
          if (e.key === 'Escape') props.onDone();
        }}
      />
      <div class="ed-index-newform-foot">
        <code>{id() || '…'}</code>
        <span class="spacer" />
        <Button size="sm" variant="primary" disabled={busy() || !id()} onClick={submit}>
          Create
        </Button>
        <Button size="sm" onClick={props.onDone}>
          Cancel
        </Button>
      </div>
    </li>
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
          {/* A course's modules are the lesson data itself — no block to
              move, nest or delete. */}
          <Show when={!props.level.syllabus}>
            <IndexActions level={props.level} depth={props.depth} onSub={() => {}} />
          </Show>
        </li>
      </Show>
      <For each={props.level.pinned ?? []}>
        {(id) => {
          const unit = () => nodeById(id);
          // Clicking a member focuses its graph node; double-clicking opens
          // its editor, mirroring the canvas (where one click does both — the
          // panel keeps them apart so browsing the list doesn't throw modals).
          // Clicks on the row's own controls (reorder / unpin) are theirs; a
          // dangling pin has no node behind it at all.
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
              onDblClick={(e) => {
                if (e.target.closest?.('.ed-nav-actions')) return;
                const n = unit();
                if (n) setContentFor(n.key);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  pick(e);
                }
              }}
              tabindex={unit() ? 0 : undefined}
              role={unit() ? 'button' : undefined}
            >
              <span
                class="ed-index-title"
                title={unit() ? `${id} — double-click to open` : id}
              >
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
  // The inline new-index form: null | { parentId: <index id> | null }.
  const [creating, setCreating] = createSignal(null);
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
        <span class="spacer" />
        <IconButton
          icon={FolderPlus}
          label="New index"
          disabled={busy()}
          onClick={() => setCreating(creating() ? null : { parentId: null })}
        />
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
          <Show when={creating() && creating().parentId === null}>
            <NewIndexForm parentId={null} onDone={() => setCreating(null)} />
          </Show>
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
                  {/* The course is generated from the lesson data — there is
                      no `index` block behind it to restructure. */}
                  <Show when={!n.syllabus}>
                    <IndexActions
                      level={n}
                      depth={0}
                      onSub={() => setCreating({ parentId: n.id })}
                    />
                  </Show>
                </div>
                <Show when={creating()?.parentId === n.id}>
                  <ul class="ed-index-members">
                    <NewIndexForm parentId={n.id} onDone={() => setCreating(null)} />
                  </ul>
                </Show>
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
