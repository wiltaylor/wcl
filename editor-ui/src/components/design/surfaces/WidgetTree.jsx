/* The wireframe structure tree: every widget in the rendered frame, nested
   as in the source — which control sits inside which frame/panel — walked
   from the anchored shapes (preview/widgetdnd.js `widgetTreeFrom`).

   Clicking a row selects the widget on the canvas (same selection calls
   EditSurface makes, so the toolbar and property dock open); the canvas
   selection highlights its row in return. Rows drag with the shared
   pointer gesture: drop on a container row → append inside, on a leaf row
   → insert after it, on the wireframe root → out to the diagram — the
   same `relocateOps` batch the canvas moves commit. */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { ChevronDown, ChevronRight } from 'lucide-solid';

import { refreshShapeHandles } from '../../../preview/diagram';
import { widgetTreeFrom } from '../../../preview/widgetdnd';
import { anchorOf, markSelected } from '../../../preview/wysiwyg';
import { busy, selection, setSelection } from '../../../state/design';
import { startPointerDrag } from './pointerDrag';

export default function WidgetTree(props) {
  const [roots, setRoots] = createSignal([]);
  /** Folded container elements (identity — the tree rebuilds per frame
      load, so folds reset with the frame; that matches the anchors). */
  const [folded, setFolded] = createSignal(new Set());
  const [dropRow, setDropRow] = createSignal(null); // highlighted row el

  /** DOM row → tree node, for hit-testing during row drags. */
  const rowNodes = new Map();

  createEffect(() => {
    void props.seq();
    rowNodes.clear();
    setRoots(widgetTreeFrom(props.doc?.()));
    setFolded(new Set());
  });

  const selectNode = (node) => {
    const doc = props.doc?.();
    const a = doc && anchorOf(doc, node.el);
    if (!doc || !a) return;
    setSelection(a);
    markSelected(doc, node.el, a.shared);
    refreshShapeHandles(doc, a.shape ? node.el : null);
    node.el.scrollIntoView?.({ block: 'nearest' });
  };

  const isSelected = (node) => selection()?.el === node.el;

  const toggleFold = (node, e) => {
    e.stopPropagation();
    const next = new Set(folded());
    if (next.has(node.el)) next.delete(node.el);
    else next.add(node.el);
    setFolded(next);
  };

  /** The tree node under a page point, or null. */
  const rowAt = (p) => {
    const el = document.elementFromPoint(p.x, p.y)?.closest?.('[data-wcl-tree-row]');
    return el ? { rowEl: el, node: rowNodes.get(el) } : null;
  };

  /** Drop `src` on `node`: append inside containers (and the wireframe
      root), after leaves — refusing self/descendant drops. */
  const dropModeOn = (src, node) => {
    if (!node || node === src) return null;
    if (src.el.contains(node.el)) return null; // no self-nesting
    if (node.kind === 'diagram') return 'diagram';
    return props.acceptsChildren?.(node.kind) ? 'inside' : 'after';
  };

  const beginRowDrag = (node, e) => {
    if (busy() || node.kind === 'diagram') return;
    startPointerDrag(e, {
      chipText: node.label ?? node.kind.replace(/^wf_/, ''),
      onClick: () => selectNode(node),
      onMove: (p) => {
        const hit = rowAt(p);
        setDropRow(hit && dropModeOn(node, hit.node) ? hit.rowEl : null);
      },
      onCancel: () => setDropRow(null),
      onDrop: (p) => {
        setDropRow(null);
        const hit = rowAt(p);
        const mode = hit && dropModeOn(node, hit.node);
        if (mode) props.onRelocate?.(node, { el: hit.node.el, mode });
      },
    });
  };

  const row = (node, depth) => {
    let rowEl;
    return (
    <>
      <div
        class="ed-surface-treerow"
        classList={{ 'is-selected': isSelected(node), 'is-droprow': dropRow() === rowEl }}
        data-wcl-tree-row
        ref={(el) => {
          rowEl = el;
          rowNodes.set(el, node);
        }}
        style={{ 'padding-left': `${6 + depth * 14}px` }}
        onPointerDown={(e) => beginRowDrag(node, e)}
        onClick={() => node.kind === 'diagram' && selectNode(node)}
      >
        <Show
          when={node.children.length > 0}
          fallback={<span class="ed-surface-treepad" />}
        >
          <button type="button" class="ed-surface-treefold" onPointerDown={(e) => e.stopPropagation()} onClick={(e) => toggleFold(node, e)}>
            <Show when={folded().has(node.el)} fallback={<ChevronDown size={11} />}>
              <ChevronRight size={11} />
            </Show>
          </button>
        </Show>
        <span class="ed-surface-treekind">
          {node.kind === 'diagram' ? 'wireframe' : node.kind.replace(/^wf_/, '')}
        </span>
        <Show when={node.label && node.kind !== 'diagram'}>
          <span class="ed-surface-treelabel">{node.label}</span>
        </Show>
      </div>
      <Show when={!folded().has(node.el)}>
        <For each={node.children}>{(c) => row(c, depth + 1)}</For>
      </Show>
    </>
    );
  };

  return (
    <div class="ed-surface-tree">
      <div class="ed-surface-widgets-target">Structure</div>
      <Show
        when={roots().length}
        fallback={<div class="ed-empty">Nothing rendered yet.</div>}
      >
        <For each={roots()}>{(r) => row(r, 0)}</For>
      </Show>
    </div>
  );
}
