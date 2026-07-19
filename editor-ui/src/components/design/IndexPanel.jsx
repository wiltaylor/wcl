/* The graph tab's left bar: the wskill's indexes as an editable pick-list.
   Selecting an index scopes the graph to it (non-members fade) and opens
   its ordered member list — reorder with the arrows, unpin with the ×,
   pin more units via the select or by DRAGGING a node from the graph onto
   this panel (GraphView hit-tests the drop against the element registered
   here). A view filter narrows the list to indexes visible in one view.
   All writes go through the quiet nav-op path (pin_unit / unpin_unit /
   reorder_children) and refetch the graph keeping positions. */

import { For, Show, createSignal, onCleanup, onMount } from 'solid-js';
import { ArrowDown, ArrowUp, Pin, X } from 'lucide-solid';
import { Badge, IconButton, Select } from '@forge/ui';

import { busy, commitNavOpQuiet } from '../../state/design';
import {
  graphData,
  nodeDragging,
  reloadGraph,
  selectedIndex,
  selectedIndexNode,
  setIndexPanelEl,
  setSelectedIndex,
} from '../../state/graph';

export default function IndexPanel() {
  let el;
  onMount(() => setIndexPanelEl(el));
  onCleanup(() => setIndexPanelEl(null));

  const [viewFilter, setViewFilter] = createSignal(''); // '' = every view
  const indexes = () =>
    (graphData()?.nodes ?? [])
      .filter((n) => n.type === 'index')
      .filter((n) => !viewFilter() || n.views?.[viewFilter()] !== false);
  const nodeById = (id) => graphData()?.nodes.find((n) => n.id === id);
  const idx = selectedIndexNode;

  const afterOp = (res) => {
    if (res.ok) reloadGraph({ keepPositions: true });
  };
  const move = (id, dir) => {
    const ids = [...(idx()?.pinned ?? [])];
    const i = ids.indexOf(id);
    const j = dir === 'up' ? i - 1 : i + 1;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    commitNavOpQuiet({ op: 'reorder_children', index_id: idx().id, order: ids }).then(afterOp);
  };
  const unpin = (id) =>
    commitNavOpQuiet({ op: 'unpin_unit', index_id: idx().id, unit_id: id }).then(afterOp);
  const pin = (id) =>
    id && commitNavOpQuiet({ op: 'pin_unit', index_id: idx().id, unit_id: id }).then(afterOp);

  const unpinned = () =>
    (graphData()?.nodes ?? []).filter(
      (n) => n.type === 'unit' && !(idx()?.pinned ?? []).includes(n.id),
    );

  return (
    <div
      ref={el}
      class="ed-nav-panel ed-index-panel"
      classList={{ 'is-dropready': !!nodeDragging() && !!idx() }}
    >
      <div class="ed-nav-head">
        <strong>Indexes</strong>
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
                <button
                  type="button"
                  class="ed-index-row"
                  classList={{ 'is-selected': selectedIndex() === n.key }}
                  onClick={() => setSelectedIndex(selectedIndex() === n.key ? null : n.key)}
                >
                  <span class="ed-index-title">{n.title}</span>
                  <span class="ed-index-count">{n.pinned?.length ?? 0}</span>
                </button>
                <Show when={selectedIndex() === n.key}>
                  <ul class="ed-index-members">
                    <For each={n.pinned ?? []}>
                      {(id) => {
                        const unit = () => nodeById(id);
                        return (
                          <li class="ed-index-member">
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
                                disabled={busy()}
                                onClick={() => move(id, 'up')}
                              />
                              <IconButton
                                icon={ArrowDown}
                                label="Move down"
                                disabled={busy()}
                                onClick={() => move(id, 'down')}
                              />
                              <IconButton
                                icon={X}
                                label="Unpin"
                                disabled={busy()}
                                onClick={() => unpin(id)}
                              />
                            </span>
                          </li>
                        );
                      }}
                    </For>
                    <Show when={(n.pinned ?? []).length === 0}>
                      <li class="ed-graph-noblocks">nothing pinned yet</li>
                    </Show>
                    <li class="ed-nav-pinrow">
                      <Pin size={12} />
                      <Select
                        options={unpinned().map((u) => ({
                          value: u.id,
                          label: `${u.title} (${u.kind})`,
                        }))}
                        placeholder="Pin a unit…"
                        value={undefined}
                        disabled={busy()}
                        onChange={pin}
                      />
                    </li>
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
