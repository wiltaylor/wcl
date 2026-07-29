/* The Systems view's side panel: the containment outline, the kind
   checkboxes, and the reference-edge toggles.

   The outline mirrors the canvas exactly (same forest, same order) — click a
   row to select and reveal it, use the chevron to fold. The kind list is the
   whole schema surface: every gathered kind the document declares, with the
   ones on the canvas checked. Turning a kind on is how a growing schema
   reaches the canvas without an editor change; turning one off is how a
   200-object WAD stays readable. Reference toggles draw the non-containment
   identifier fields (`repo`, `built_by`, `supersedes`) as dashed edges. */

import { For, Show, createMemo } from 'solid-js';
import { ChevronDown, ChevronRight } from 'lucide-solid';
import { Checkbox } from '@forge/ui';

import { buildForest } from '../../preview/c4layout';
import {
  collapsed,
  model,
  refEdges,
  selectedNode,
  setSelectedNode,
  toggleCollapsed,
  toggleKind,
  toggleRefEdge,
  visibleKinds,
} from '../../state/systems';

export default function SystemsPanel() {
  const forest = createMemo(() =>
    buildForest(model()?.nodes ?? [], { visibleKinds: visibleKinds() ?? null }),
  );

  /** Per-kind instance counts — the panel's "is this worth showing?" hint. */
  const counts = createMemo(() => {
    const out = {};
    for (const n of model()?.nodes ?? []) out[n.kind] = (out[n.kind] ?? 0) + 1;
    for (const e of model()?.edges ?? []) out[e.kind] = (out[e.kind] ?? 0) + 1;
    return out;
  });

  /** Every reference field the schema declares, deduplicated by name. */
  const refFields = createMemo(() => {
    const seen = new Map();
    for (const k of model()?.kinds ?? []) {
      for (const r of k.refs ?? []) {
        if (r.list) continue; // list refs would fan out into noise
        seen.set(r.field, (seen.get(r.field) ?? 0) + 1);
      }
    }
    return [...seen.keys()].sort();
  });

  const Row = (props) => {
    const kids = () => forest().childrenOf.get(props.node.key) ?? [];
    const folded = () => collapsed().has(props.node.key);
    return (
      <>
        <div
          class="ed-sys-row"
          classList={{ 'is-selected': selectedNode() === props.node.key }}
          style={{ 'padding-left': `${6 + props.depth * 14}px` }}
          onClick={() => setSelectedNode(props.node.key)}
        >
          <Show when={kids().length} fallback={<span class="ed-sys-rowdot" />}>
            <button
              type="button"
              class="ed-sys-rowfold"
              title={folded() ? 'Expand' : 'Collapse'}
              onClick={(e) => {
                e.stopPropagation();
                toggleCollapsed(props.node.key);
              }}
            >
              {folded() ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
            </button>
          </Show>
          <span class="ed-sys-rowtitle">{props.node.title}</span>
          <span class="ed-sys-rowkind">{props.node.kind}</span>
        </div>
        <Show when={!folded()}>
          <For each={kids()}>{(k) => <Row node={k} depth={props.depth + 1} />}</For>
        </Show>
      </>
    );
  };

  return (
    <div class="ed-sys-side">
      <div class="ed-sys-side-head">Outline</div>
      <div class="ed-sys-outline">
        <For each={forest().roots}>{(n) => <Row node={n} depth={0} />}</For>
        <Show when={!forest().roots.length}>
          <div class="ed-empty">No objects of the shown kinds</div>
        </Show>
      </div>

      <div class="ed-sys-side-head">Kinds</div>
      <div class="ed-sys-kinds">
        <For each={(model()?.kinds ?? []).filter((k) => !k.edge)}>
          {(k) => (
            <Checkbox
              checked={(visibleKinds() ?? new Set()).has(k.kind)}
              onChange={() => toggleKind(k.kind)}
            >
              <span class="ed-sys-kindname" title={k.doc ?? undefined}>
                {k.kind}
              </span>
              <span class="ed-sys-kindcount">{counts()[k.kind] ?? 0}</span>
            </Checkbox>
          )}
        </For>
      </div>

      <Show when={refFields().length}>
        <div class="ed-sys-side-head">Reference edges</div>
        <div class="ed-sys-kinds">
          <For each={refFields()}>
            {(field) => (
              <Checkbox checked={refEdges().has(field)} onChange={() => toggleRefEdge(field)}>
                <span class="ed-sys-kindname">{field}</span>
              </Checkbox>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
