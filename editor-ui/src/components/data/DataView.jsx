/* Data mode: schema-driven tables over every `@wdoc.editable` type in the
   selected document. Left = the registered type list; main = one table of
   the active type's instances (columns from the schema's field metadata),
   with add / edit / delete. Cell edits write through the same validated
   block-op pipeline as Design mode (disk-only; entering saved dirty
   buffers); adds go through /api/unit/create with the decorator's target
   file, expr-valued cells (numbers, symbols, identifiers) round-trip as
   parsed WCL. */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { Plus, RefreshCw, Trash2 } from 'lucide-solid';
import { Badge, Button, IconButton, Input, Modal, Select, Spinner, toast } from '@forge/ui';

import { api } from '../../api';
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
  const columns = () => (typeDef()?.fields ?? []).filter((f) => f.inline_slot == null);
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
    const seed = {};
    for (const f of columns()) {
      const cell = row.cells?.[f.name];
      if (cell?.state === 'literal') seed[f.name] = cell.text ?? '';
    }
    setForm(seed);
    setDialog({ mode: 'edit', row });
  };

  /** A form value → the unit_create JSON field value / block-op payload,
      honouring the column's declared type. */
  const isTextual = (f) => /utf8|ascii/.test(f.type ?? '') && !f.symbols;

  const submitAdd = async () => {
    const id = (form().__id ?? '').trim();
    if (!id) return toast('The row needs an id', { tone: 'danger', duration: 4000 });
    const fields = {};
    for (const f of columns()) {
      const v = (form()[f.name] ?? '').trim();
      if (v === '') continue;
      if (f.symbols) fields[f.name] = { sym: v.replace(/^:/, '') };
      else if (isTextual(f)) fields[f.name] = v;
      else fields[f.name] = { expr: v };
    }
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
    const ops = [];
    for (const f of columns()) {
      const cell = row.cells?.[f.name];
      const next = form()[f.name];
      if (next == null) continue;
      const prev = cell?.state === 'literal' ? (cell.text ?? '') : null;
      if (prev !== null && next === prev) continue;
      if (prev === null && next.trim() === '') continue;
      if (f.symbols) {
        ops.push({ op: 'set_field', span: row.span, field: f.name, expr: `:${next.replace(/^:/, '')}` });
      } else if (isTextual(f) && !cell?.expr) {
        ops.push({ op: 'set_field', span: row.span, field: f.name, text: next });
      } else {
        ops.push({ op: 'set_field', span: row.span, field: f.name, expr: next });
      }
    }
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

  const cellText = (row, name) => {
    const c = row.cells?.[name];
    if (!c) return '';
    return c.state === 'literal' ? (c.text ?? '') : '(expr)';
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
                      <For each={columns()}>{(f) => <td>{cellText(row, f.name)}</td>}</For>
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
            {(f) => {
              const cell = () => dialog()?.row?.cells?.[f.name];
              const locked = () => dialog()?.mode === 'edit' && cell()?.state === 'computed';
              return (
                <Show
                  when={f.symbols}
                  fallback={
                    <Input
                      value={form()[f.name] ?? ''}
                      disabled={locked()}
                      onInput={(e) => setForm({ ...form(), [f.name]: e.currentTarget.value })}
                      placeholder={
                        locked()
                          ? `${f.name} (computed — edit in code)`
                          : `${f.name}: ${f.type}${f.optional ? '?' : ''}`
                      }
                    />
                  }
                >
                  <Select
                    options={f.symbols.map((s) => ({ value: s, label: `:${s}` }))}
                    value={(form()[f.name] ?? '').replace(/^:/, '') || undefined}
                    placeholder={f.name}
                    onChange={(v) => setForm({ ...form(), [f.name]: v })}
                  />
                </Show>
              );
            }}
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
