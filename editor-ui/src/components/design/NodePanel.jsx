/* The Systems view's property dock: the selected node's schema-generated
   form, docked to the right of the canvas.

   Rows come from the kind's `effective_fields` metadata in the /api/systems
   payload merged with the instance's classified cell values, so a kind added
   to the schema — and any symbol added to one of its vocabularies — gets a
   form with no change here. Strings are inputs, symbol sets are selects,
   bools are checkboxes, and a field naming another kind (`container`,
   `owner`) becomes a select over that kind's ids. Computed values are
   read-only and hand off to the fragment editor. Save batches every touched
   field into one atomic commit. */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { Braces, FileCode2, Maximize2, Plus, Trash2 } from 'lucide-solid';
import { Button, Checkbox, Input, Select, toast } from '@forge/ui';

import {
  CUSTOM_OPTION,
  bareType,
  fieldState,
  fieldText,
  formEditable,
  isSlot,
  orderFields,
  suggestOptions,
  valueOp,
} from '../../preview/schemaform';
import { openFile } from '../../state/buffers';
import { revealSpan } from '../../state/views';
import { busy, commitOpsQuiet, exitDesign, setPopover } from '../../state/design';
import {
  deletePlan,
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
  /** List-valued fields have no form control — the source editor owns them. */
  const isList = (f) => /^list</.test(bareType(f));
  const cells = () => node()?.cells ?? {};
  const current = (f) => (f.name in draft() ? draft()[f.name] : fieldText(f, cells()));
  const stateOf = (f) => fieldState(f, cells());

  /** Values already in use for a free-text field, when there are any. */
  const vocab = (f) => (custom()[f.name] ? null : suggestOptions(f, schema(), current(f)));

  /** Ids this field may name, when it points at another kind. */
  const idOptions = (f) => {
    const link =
      (schema()?.parents ?? []).find((p) => p.field === f.name)?.kind ??
      (model()?.kinds ?? []).find((k) => k.kind === f.name)?.kind;
    if (!link) return null;
    return (model()?.nodes ?? [])
      .filter((n) => n.kind === link && n.id !== node()?.id)
      .map((n) => ({ value: n.id, label: `${n.title} (${n.id})` }));
  };

  const save = async () => {
    const n = node();
    if (!n) return;
    const ops = [];
    for (const f of rows()) {
      if (!(f.name in draft())) continue;
      const text = draft()[f.name];
      if (text === '' && !isSlot(f)) {
        if (f.optional !== false) ops.push({ op: 'remove_field', span: n.span, field: f.name });
        continue;
      }
      if (text === '') continue;
      ops.push(valueOp(n.span, f, text));
    }
    if (!ops.length) return;
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
                <Show
                  when={formEditable(stateOf(f)) && !isList(f)}
                  fallback={
                    <Input
                      value={isList(f) ? '(list — edit as source)' : '(computed — edit as source)'}
                      disabled
                    />
                  }
                >
                  <Show
                    when={f.symbols}
                    fallback={
                      <Show
                        when={idOptions(f)}
                        fallback={
                          <Show
                            when={(f.type ?? '').replace(/\?$/, '') === 'bool'}
                            fallback={
                              <Show
                                when={vocab(f)}
                                fallback={
                                  <Input
                                    value={current(f)}
                                    placeholder={f.default ?? (f.optional ? '(unset)' : '')}
                                    onInput={(e) =>
                                      setDraft({ ...draft(), [f.name]: e.currentTarget.value })
                                    }
                                  />
                                }
                              >
                                <Select
                                  options={vocab(f)}
                                  value={current(f)}
                                  onChange={(v) => {
                                    if (v === CUSTOM_OPTION) {
                                      setCustom({ ...custom(), [f.name]: true });
                                      setDraft({ ...draft(), [f.name]: '' });
                                    } else setDraft({ ...draft(), [f.name]: v });
                                  }}
                                />
                              </Show>
                            }
                          >
                            <Checkbox
                              checked={current(f) === 'true'}
                              onChange={(on) =>
                                setDraft({ ...draft(), [f.name]: on ? 'true' : 'false' })
                              }
                            />
                          </Show>
                        }
                      >
                        <Select
                          options={[
                            ...(f.optional !== false ? [{ value: '', label: '(unset)' }] : []),
                            ...idOptions(f),
                          ]}
                          value={current(f)}
                          onChange={(v) => setDraft({ ...draft(), [f.name]: v })}
                        />
                      </Show>
                    }
                  >
                    <Select
                      options={[
                        ...(f.optional !== false ? [{ value: '', label: '(unset)' }] : []),
                        ...f.symbols.map((sym) => ({ value: sym, label: `:${sym}` })),
                      ]}
                      value={current(f)}
                      onChange={(v) => setDraft({ ...draft(), [f.name]: v })}
                    />
                  </Show>
                </Show>
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
