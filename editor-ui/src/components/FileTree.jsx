/* Recursive file tree over the /api/files listing. No forge tree component
   exists — plain rows styled with tokens, chevron toggles per directory. */

import { For, Show, createSignal } from 'solid-js';
import { Spinner, toast } from '@forge/ui';

import { treeData } from '../state/tree';
import { active, openFile } from '../state/buffers';

/* NOTE: never name this `open` — a module-level `open` that gets compiled
   into the delegated click handler can resolve to `window.open`, silently
   swallowing every tree click into a blocked popup. */
async function openTreeFile(path) {
  const res = await openFile(path);
  if (!res.ok) toast(res.error, { tone: 'danger', duration: 6000 });
}

function Node(props) {
  const [open, setOpen] = createSignal(props.depth < 1);
  const indent = () => `${8 + props.depth * 14}px`;

  return (
    <Show
      when={props.node.type === 'dir'}
      fallback={
        <button
          type="button"
          class="ed-tree-row"
          classList={{ 'is-active': active() === props.node.path }}
          style={{ 'padding-left': indent() }}
          onClick={() => openTreeFile(props.node.path)}
          title={props.node.path}
        >
          <span class="glyph">▪</span>
          {props.node.name}
        </button>
      }
    >
      <button
        type="button"
        class="ed-tree-row"
        style={{ 'padding-left': indent() }}
        onClick={() => setOpen(!open())}
      >
        <span class="chev">{open() ? '▾' : '▸'}</span>
        {props.node.name}
      </button>
      <Show when={open()}>
        <For each={props.node.children}>
          {(child) => <Node node={child} depth={props.depth + 1} />}
        </For>
      </Show>
    </Show>
  );
}

export default function FileTree() {
  return (
    <div class="ed-tree">
      <Show when={treeData()} fallback={<div class="ed-empty"><Spinner /></div>}>
        <For each={treeData().nodes}>{(node) => <Node node={node} depth={0} />}</For>
      </Show>
    </div>
  );
}
