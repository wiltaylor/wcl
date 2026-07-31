/* The Systems view's property dock: the selected node's schema-generated
   form, docked to the right of the canvas.

   Rows come from the kind's `effective_fields` metadata in the /api/systems
   payload merged with the instance's cells, and every row is the shared
   FieldControl — so a kind added to the schema (and any symbol added to one
   of its vocabularies) gets a form with no change here, and the form matches
   the details modal's exactly — including the id picker a field naming
   another kind (`container`, `owner`) gets, which both read from the shared
   `idOptions`. Save batches every touched field into one atomic commit
   through the shared save rule. */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { Braces, FileCode2, Maximize2, Plus, Trash2 } from 'lucide-solid';
import { Button, toast } from '@forge/ui';

import { draftOps, orderFields } from '../../preview/schemaform';
import FieldControl from './FieldControl';
import { openFile } from '../../state/buffers';
import { revealSpan } from '../../state/views';
import { busy, commitOpsQuiet, exitDesign, setPopover } from '../../state/design';
import {
  deletePlan,
  idOptions,
  kindOf,
  loadSystems,
  model,
  nodeByKey,
  nodeById,
  selectedNode,
  setSelectedNode,
} from '../../state/systems';

export default function NodePanel(props) {
  const node = () => (selectedNode() ? nodeByKey(selectedNode()) : null);
  const schema = () => (node() ? kindOf(node()) : null);
  const [draft, setDraft] = createSignal({});
  const [confirmDelete, setConfirmDelete] = createSignal(false);
  /** Free-text fields switched to typing by picking "Custom…". */
  const [custom, setCustom] = createSignal({});

  createEffect(() => {
    void selectedNode();
    void model();
    setDraft({});
    setConfirmDelete(false);
    setCustom({});
  });

  const rows = () => orderFields(schema()?.fields ?? []);
  const cells = () => node()?.cells;

  const save = async () => {
    const n = node();
    if (!n) return;
    const ops = draftOps(rows(), cells(), draft(), n.span);
    // Nothing actually changed — settle the form rather than committing.
    if (!ops.length) return setDraft({});
    const res = await commitOpsQuiet(n.file, ops, { etag: n.etag });
    if (res.ok) {
      setDraft({});
      await (props.onReload?.() ?? loadSystems({ keep: true }));
    }
  };

  /** The schema-decided consequences of deleting the selection. */
  const plan = () => (node() ? deletePlan(node().key) : { deleted: [], detached: [] });

  /** Delete the node (plus every child that cannot exist without it), free
      the children that merely referenced it, and drop every relation
      touching anything deleted — batched per file so each commit's spans
      stay pre-reformat fresh. */
  const remove = async () => {
    const n = node();
    if (!n) return;
    const { deleted, detached } = plan();
    const doomed = deleted.map(nodeByKey).filter(Boolean);
    const ids = new Set(doomed.map((d) => d.id));
    const byFile = new Map();
    const push = (file, op) => byFile.set(file, [...(byFile.get(file) ?? []), op]);
    for (const e of model()?.edges ?? []) {
      if (ids.has(e.from) || ids.has(e.to)) push(e.file, { op: 'delete', span: e.span });
    }
    for (const { key, field } of detached) {
      const c = nodeByKey(key);
      if (c) push(c.file, { op: 'remove_field', span: c.span, field });
    }
    for (const d of doomed) push(d.file, { op: 'delete', span: d.span });
    for (const [file, ops] of byFile) {
      const res = await commitOpsQuiet(file, ops);
      if (!res.ok) return;
    }
    const extra = [
      doomed.length > 1 ? `${doomed.length - 1} nested` : null,
      detached.length ? `${detached.length} freed` : null,
    ].filter(Boolean);
    toast(`Deleted ${n.title}${extra.length ? ` (${extra.join(', ')})` : ''}`, {
      tone: 'success',
      duration: 3000,
    });
    setSelectedNode(null);
    await (props.onReload?.() ?? loadSystems({ keep: true }));
  };

  const openCode = async () => {
    const n = node();
    exitDesign();
    const res = await openFile(n.file);
    if (res.ok) revealSpan(n.file, n.span.start, n.span.end);
    else toast(res.error, { tone: 'danger', duration: 6000 });
  };

  const parentLabel = () => {
    const p = node()?.parent;
    if (!p) return null;
    const owner = nodeById(p.id);
    return `${p.field} = ${owner ? owner.title : p.id}`;
  };

  return (
    <Show when={node()}>
      <div class="ed-sys-panel">
        <div class="ed-sys-panel-head">
          <span class="ed-design-kind">{node().kind}</span>
          <span class="spacer" />
          <Button
            size="sm"
            title="Edit as source"
            onClick={() =>
              setPopover({
                type: 'fragment',
                anchor: { file: node().file, span: node().span, kind: node().kind },
              })
            }
          >
            <Braces size={13} />
          </Button>
        </div>
        <p class="ed-sys-panel-id">{node().id}</p>
        <Show when={parentLabel()}>
          <p class="ed-sys-panel-parent">{parentLabel()}</p>
        </Show>
        <Show when={schema()?.doc}>
          <p class="ed-sys-panel-doc">{schema().doc}</p>
        </Show>

        <div class="ed-form">
          <For each={rows()}>
            {(f) => (
              <label class="ed-sys-field" title={f.doc ?? undefined}>
                <span class="ed-sys-fieldname">{f.name}</span>
                <FieldControl
                  field={f}
                  cells={cells()}
                  schema={schema()}
                  ids={idOptions(schema(), f, node()?.id)}
                  value={f.name in draft() ? draft()[f.name] : undefined}
                  custom={custom()[f.name]}
                  onCustom={() => {
                    setCustom({ ...custom(), [f.name]: true });
                    setDraft({ ...draft(), [f.name]: '' });
                  }}
                  onChange={(v) => setDraft({ ...draft(), [f.name]: v })}
                />
              </label>
            )}
          </For>
        </div>

        <div class="ed-sys-panel-actions">
          <Button
            size="sm"
            title="Everything about this object (or double-click its box)"
            onClick={() => props.onOpenDetail?.(node())}
          >
            <Maximize2 size={13} /> details
          </Button>
          <Button size="sm" onClick={() => props.onAddChild?.(node().key)} disabled={busy()}>
            <Plus size={13} /> child
          </Button>
          <Button size="sm" onClick={openCode}>
            <FileCode2 size={13} /> code
          </Button>
          <span class="spacer" />
          <Show
            when={!confirmDelete()}
            fallback={
              <>
                <Button size="sm" onClick={() => setConfirmDelete(false)}>
                  Keep
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  disabled={busy()}
                  onClick={remove}
                  title={
                    plan().detached.length
                      ? `${plan().detached.length} child object(s) keep existing, without this link`
                      : undefined
                  }
                >
                  Delete {plan().deleted.length > 1 ? `+${plan().deleted.length - 1}` : ''}
                </Button>
              </>
            }
          >
            <Button size="sm" variant="ghost" onClick={() => setConfirmDelete(true)}>
              <Trash2 size={13} />
            </Button>
          </Show>
          <Button
            size="sm"
            variant="primary"
            disabled={busy() || !Object.keys(draft()).length}
            onClick={save}
          >
            Save
          </Button>
        </div>
      </div>
    </Show>
  );
}
