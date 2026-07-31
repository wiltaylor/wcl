/* Data mode: schema-driven tables over every `@wdoc.editable` type in the
   selected document. Left = the registered type list; main = one table of
   the active type's instances (columns from the schema's field metadata),
   with add / edit / delete. Cell edits write through the same validated
   block-op pipeline as Design mode (disk-only; entering saved dirty
   buffers); adds go through /api/unit/create with the decorator's target
   file. Rows and forms read the same cells, and offer the same controls,
   as every other surface — scalar values (numbers, symbols, identifiers)
   round-trip as parsed WCL. */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { Plus, RefreshCw, Trash2 } from 'lucide-solid';
import { Badge, Button, IconButton, Input, Modal, Spinner, toast } from '@forge/ui';

import { api } from '../../api';
import { createFields, draftOps, fieldText, isSlot } from '../../preview/schemaform';
import FieldControl from '../design/FieldControl';
import { activeEntry } from '../../state/sites';
import { busy, commitOpsQuiet } from '../../state/design';

export default function DataView() {
  const [types, setTypes] = createSignal(null);
  const [activeKind, setActiveKind] = createSignal(null);
  const [rows, setRows] = createSignal(null);
  const [loading, setLoading] = createSignal(false);
  /** { mode: 'add' } | { mode: 'edit', row } | { mode: 'delete', row } */
  const [dialog, setDialog] = createSignal(null);
  const [form, setForm] = createSignal({});

  const typeDef = () => types()?.find((t) => t.kind === activeKind());
  /** Scalar (non-inline) columns of the active type. */
  const columns = () => (typeDef()?.fields ?? []).filter((f) => !isSlot(f));
  const idField = () => (typeDef()?.fields ?? []).find((f) => f.inline_slot === 0);

  const loadTypes = async () => {
    const entry = activeEntry();
    if (!entry) return;
    const res = await api.dataTypes(entry);
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      return;
    }
    setTypes(res.types);
    if (!res.types.some((t) => t.kind === activeKind())) {
      setActiveKind(res.types[0]?.kind ?? null);
    }
  };
  const loadRows = async () => {
    const entry = activeEntry();
    const kind = activeKind();
    if (!entry || !kind) {
      setRows(null);
      return;
    }
    setLoading(true);
    const res = await api.dataRows(entry, kind);
    setLoading(false);
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      return;
    }
    setRows(res.rows);
  };
  createEffect(() => {
    void activeEntry();
    loadTypes();
  });
  createEffect(() => {
    void activeKind();
    loadRows();
  });

  // ------------------------------------------------------------------
  // CRUD
  // ------------------------------------------------------------------

  const openAdd = () => {
    const seed = {};
    for (const f of columns()) if (f.default != null) seed[f.name] = String(f.default);
    setForm(seed);
    setDialog({ mode: 'add' });
  };

  const openEdit = (row) => {
    // The dialog opens empty: a field only commits once it is touched, and
    // the controls read the row's cells for what is already there.
    setForm({});
    setDialog({ mode: 'edit', row });
  };

  const submitAdd = async () => {
    const id = (form().__id ?? '').trim();
    if (!id) return toast('The row needs an id', { tone: 'danger', duration: 4000 });
    const fields = createFields(columns(), form());
    const res = await api.unitCreate({
      entry: activeEntry(),
      unit: { kind: activeKind(), id, fields, file: typeDef()?.file ?? undefined },
    });
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 8000 });
      return;
    }
    setDialog(null);
    loadRows();
  };

  const submitEdit = async () => {
    const { row } = dialog();
    const ops = draftOps(columns(), row.cells, form(), row.span);
    if (ops.length === 0) return setDialog(null);
    const res = await commitOpsQuiet(row.file, ops, { etag: row.etag });
    if (res.ok) {
      setDialog(null);
      loadRows();
    }
  };

  const submitDelete = async () => {
    const { row } = dialog();
    const res = await commitOpsQuiet(row.file, [{ op: 'delete', span: row.span }], {
      etag: row.etag,
    });
    if (res.ok) {
      setDialog(null);
      loadRows();
    }
  };

  // ------------------------------------------------------------------

  /** A table cell: its value, or a marker for what a column can't show. */
  const columnText = (row, f) => {
    const state = row.cells?.fields?.[f.name]?.state;
    if (!state) return '';
    if (state === 'computed') return '(expr)';
    if (state === 'rows') return '(grid)';
    return fieldText(f, row.cells);
  };

  return (
    <div class="ed-data">
      <div class="ed-data-types">
        <div class="ed-nav-head">
          <strong>Data types</strong>
          <span class="spacer" />
          <IconButton icon={RefreshCw} label="Reload" onClick={loadTypes} />
        </div>
        <Show
          when={(types() ?? []).length > 0}
          fallback={
            <div class="ed-data-empty">
              No <code>@wdoc.editable</code> types in this document. Mark a schema type with{' '}
              <code>@wdoc.editable("data/file.wcl")</code> to manage its instances here.
            </div>
          }
        >
          <ul class="ed-data-typelist">
            <For each={types()}>
              {(t) => (
                <li>
                  <button
                    type="button"
                    classList={{ 'is-active': activeKind() === t.kind }}
                    onClick={() => setActiveKind(t.kind)}
                  >
                    {t.kind}
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>

      <div class="ed-data-main">
        <div class="ed-design-note">
          <span class="ed-design-page">{activeKind() ?? 'no type selected'}</span>
          <Show when={typeDef()?.file}>
            <Badge>{typeDef().file}</Badge>
          </Show>
          <Show when={loading() || busy()}>
            <Spinner size={12} label="Working" />
          </Show>
          <span class="spacer" />
          <Button size="sm" onClick={openAdd} disabled={!activeKind() || busy()}>
            <Plus size={13} /> Add row
          </Button>
        </div>
        <div class="ed-data-tablewrap">
          <Show when={rows()} fallback={<div class="ed-empty">Pick a data type</div>}>
            <table class="ed-data-table">
              <thead>
                <tr>
                  <th>{idField()?.name ?? 'id'}</th>
                  <For each={columns()}>{(f) => <th>{f.name}</th>}</For>
                  <th />
                </tr>
              </thead>
              <tbody>
                <For each={rows()}>
                  {(row) => (
                    <tr onDblClick={() => openEdit(row)}>
                      <td class="ed-data-id">{row.label}</td>
                      <For each={columns()}>{(f) => <td>{columnText(row, f)}</td>}</For>
                      <td class="ed-data-actions">
                        <Button size="sm" onClick={() => openEdit(row)}>
                          Edit
                        </Button>
                        <IconButton
                          icon={Trash2}
                          label="Delete row"
                          onClick={() => setDialog({ mode: 'delete', row })}
                        />
                      </td>
                    </tr>
                  )}
                </For>
                <Show when={rows()?.length === 0}>
                  <tr>
                    <td colspan={columns().length + 2} class="ed-data-empty">
                      No instances yet — add the first row.
                    </td>
                  </tr>
                </Show>
              </tbody>
            </table>
          </Show>
        </div>
      </div>

      {/* dialogs */}
      <Modal
        open={dialog() !== null && dialog().mode !== 'delete'}
        onClose={() => setDialog(null)}
        title={dialog()?.mode === 'add' ? `Add ${activeKind()}` : `Edit ${dialog()?.row?.label}`}
        footer={
          <>
            <Button onClick={() => setDialog(null)}>Cancel</Button>
            <Button
              variant="primary"
              disabled={busy()}
              onClick={() => (dialog()?.mode === 'add' ? submitAdd() : submitEdit())}
            >
              {dialog()?.mode === 'add' ? 'Add' : 'Save'}
            </Button>
          </>
        }
      >
        <div class="ed-form">
          <Show when={dialog()?.mode === 'add'}>
            <Input
              value={form().__id ?? ''}
              onInput={(e) => setForm({ ...form(), __id: e.currentTarget.value })}
              placeholder={`${idField()?.name ?? 'id'} (identifier)`}
            />
          </Show>
          <For each={columns()}>
            {(f) => (
              <FieldControl
                field={f}
                cells={dialog()?.mode === 'edit' ? dialog().row.cells : undefined}
                schema={typeDef()}
                value={f.name in form() ? form()[f.name] : undefined}
                placeholder={`${f.name}: ${f.type}${f.optional ? '?' : ''}`}
                onChange={(v) => setForm({ ...form(), [f.name]: v })}
              />
            )}
          </For>
        </div>
      </Modal>

      <Modal
        open={dialog()?.mode === 'delete'}
        onClose={() => setDialog(null)}
        title="Delete row"
        footer={
          <>
            <Button onClick={() => setDialog(null)}>Cancel</Button>
            <Button variant="danger" disabled={busy()} onClick={submitDelete}>
              Delete
            </Button>
          </>
        }
      >
        <p>
          Delete <code>{dialog()?.row?.label}</code> from <code>{dialog()?.row?.file}</code>?
        </p>
      </Modal>
    </div>
  );
}
