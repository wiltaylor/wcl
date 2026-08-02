/* "Add to the systems model": pick one of the kinds that may sit where the
   user clicked (derived from the schema's parent links — the palette under a
   container offers only the kinds that name a container), give it an id, and
   fill the kind's own fields. The parent field is pre-filled and locked, so
   the new node lands exactly where it was asked for.

   Ids are validated against every id already in the model: a WAD's diagram
   kinds share ONE id space, which the per-kind check on /api/unit/create
   doesn't cover. */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { Button, Input, Modal, Select, toast } from '@forge/ui';

import { createFields, slugify } from '../../preview/schemaform';
import { busy } from '../../state/design';
import FieldControl from './FieldControl';

export default function AddNodeDialog(props) {
  const [form, setForm] = createSignal({ kind: '', id: '', idTouched: false, fields: {} });
  /** Free-text fields switched to typing by picking "Custom…" — a kind's
      suggestions are what other instances use, never the whole vocabulary.
      Cleared with the form: the escape belongs to one field of one kind. */
  const [custom, setCustom] = createSignal({});
  const reset = (kind = '') => {
    setForm({ kind, id: '', idTouched: false, fields: {} });
    setCustom({});
  };
  const patch = (p) => setForm({ ...form(), ...p });
  const kinds = () => props.kinds ?? [];
  const def = () => kinds().find((k) => k.kind === form().kind) ?? null;
  /** The parent field this kind uses for the clicked-in parent. */
  const parentField = () =>
    props.parent ? (def()?.parents ?? []).find((p) => p.kind === props.parent.kind)?.field : null;

  // Default to the only offered kind, and clear a kind the palette no longer
  // offers. The effect reads `form` as well as `kinds`, so it must write only
  // when the value actually changes — an unconditional setForm re-triggers
  // the effect on its own write and loops, wiping every pick as it is made.
  createEffect(() => {
    const list = kinds();
    if (!props.open) return;
    const cur = form().kind;
    const want = list.some((k) => k.kind === cur) ? cur : list.length === 1 ? list[0].kind : '';
    if (want !== cur) reset(want);
  });

  /** Editable fields: the kind's own, minus the locked parent link. */
  const fields = () =>
    (def()?.fields ?? []).filter((f) => f.inline_slot == null && f.name !== parentField());

  const setField = (name, value) => {
    const next = { ...form().fields, [name]: value };
    // A name typed before the id auto-fills the id, until the id is edited.
    const idFrom = name === 'name' || name === 'title' ? slugify(value) : null;
    patch({ fields: next, ...(idFrom !== null && !form().idTouched ? { id: idFrom } : {}) });
  };

  const submit = async () => {
    const f = form();
    const d = def();
    if (!d || !f.id) {
      toast('Pick a kind and give it an id', { tone: 'danger', duration: 4000 });
      return;
    }
    if (!/^[A-Za-z_]\w*$/.test(f.id)) {
      toast(`"${f.id}" is not an identifier — letters, digits and _, not starting with a digit`, {
        tone: 'danger',
        duration: 5000,
      });
      return;
    }
    if ((props.takenIds ?? []).includes(f.id)) {
      toast(`"${f.id}" is already used — ids are shared across kinds here`, {
        tone: 'danger',
        duration: 5000,
      });
      return;
    }
    const missing = fields().filter(
      (fd) => fd.optional === false && fd.default == null && !(f.fields[fd.name] ?? '').trim(),
    );
    if (missing.length) {
      toast(`Fill: ${missing.map((m) => m.name).join(', ')}`, { tone: 'danger', duration: 4000 });
      return;
    }
    const out = createFields(fields(), f.fields);
    const pf = parentField();
    if (pf) out[pf] = { ident: props.parent.id };
    const res = await props.onSubmit({
      kind: d.kind,
      type_name: d.type_name,
      id: f.id,
      fields: out,
    });
    if (res?.ok) {
      reset();
      props.onClose();
    }
  };

  return (
    <Modal
      open={props.open}
      onClose={props.onClose}
      title={props.parent ? `Add inside ${props.parent.title}` : 'Add to the model'}
      footer={
        <>
          <Button onClick={props.onClose}>Cancel</Button>
          <Button variant="primary" onClick={submit} disabled={busy()}>
            Add
          </Button>
        </>
      }
    >
      <div class="ed-form ed-add-unit">
        <Show
          when={kinds().length}
          fallback={<p class="ed-empty">Nothing can be added inside a {props.parent?.kind}</p>}
        >
          <Select
            options={kinds().map((k) => ({ value: k.kind, label: k.kind }))}
            value={form().kind || undefined}
            placeholder="Kind…"
            onChange={(kind) => {
              // The id survives a kind change (it may already be typed);
              // the field values and their Custom… escapes do not.
              patch({ kind, fields: {} });
              setCustom({});
            }}
          />
          <Input
            value={form().id}
            onInput={(e) => patch({ id: e.currentTarget.value, idTouched: true })}
            placeholder="id (identifier)"
          />
          <Show when={parentField()}>
            <p class="ed-sys-panel-parent">
              {parentField()} = {props.parent.id}
            </p>
          </Show>
          <Show when={def()?.doc}>
            <p class="ed-sys-panel-doc">{def().doc}</p>
          </Show>
          {/* The same controls the edit forms offer — a create form that
              disagreed about a kind's shape would invent values the edit
              form then refuses. */}
          <div class="ed-add-unit-fields">
            <For each={fields()}>
              {(f) => (
                <FieldControl
                  field={f}
                  schema={def()}
                  value={form().fields[f.name] ?? ''}
                  placeholder={`${f.name}${f.optional === false ? ' (required)' : ''}`}
                  custom={custom()[f.name]}
                  onCustom={() => {
                    setCustom({ ...custom(), [f.name]: true });
                    setField(f.name, '');
                  }}
                  onChange={(v) => setField(f.name, v)}
                />
              )}
            </For>
          </div>
        </Show>
      </div>
    </Modal>
  );
}
