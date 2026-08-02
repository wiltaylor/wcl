/* The "Add unit" dialog, shared by the NavPanel (canvas tab) and the graph
   view: pick a kind (from the palette's introspected unit kinds), give it
   an id, fill the kind's metadata fields (symbol sets render as selects),
   place it into a section (or explicitly take the unpinned exception). The caller supplies the submit path —
   the canvas commits through the full rebuild loop, the graph through the
   quiet one — and the pinnable section list. */

import { For, Show, createSignal } from 'solid-js';
import { Button, Input, Modal, Select, toast } from '@forge/ui';

import { createFields, isSlot } from '../../preview/schemaform';
import { busy, palette } from '../../state/design';
import FieldControl from './FieldControl';
import { placementOptions, selectedPin } from './addunit';

export default function AddUnitDialog(props) {
  const [form, setForm] = createSignal({ kind: '', id: '', fields: {}, pin: '' });
  const patch = (p) => setForm({ ...form(), ...p });
  const kindDef = () => (palette()?.unit_kinds ?? []).find((k) => k.kind === form().kind);

  const submit = async () => {
    const f = form();
    if (!f.id || !f.kind) {
      toast('Pick a kind and an id', { tone: 'danger', duration: 4000 });
      return;
    }
    const placement = selectedPin(props.indexes ?? [], f.pin);
    if (placement.error) {
      toast(placement.error, { tone: 'danger', duration: 4000 });
      return;
    }
    const fields = createFields(kindDef()?.fields ?? [], f.fields);
    const res = await props.onSubmit(
      { kind: f.kind, id: f.id, fields },
      placement.pin,
    );
    if (res.ok) {
      setForm({ kind: '', id: '', fields: {}, pin: '' });
      props.onClose();
    }
  };

  return (
    <Modal
      open={props.open}
      onClose={props.onClose}
      title="Add unit"
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
        {/* Kind and id identify the unit, so they stay on their own full-width
            rows; the kind's own metadata fields pair up below (see app.css) —
            a kind with several fields would otherwise run past the fold. */}
        <Select
          options={(palette()?.unit_kinds ?? [])
            .filter((k) => k.kind !== 'index')
            .map((k) => ({ value: k.kind, label: k.kind }))}
          value={form().kind || undefined}
          placeholder="Kind…"
          onChange={(kind) => patch({ kind, fields: {} })}
        />
        <Input
          value={form().id}
          onInput={(e) => patch({ id: e.currentTarget.value })}
          placeholder="id (identifier)"
        />
        <Show when={(kindDef()?.fields ?? []).some((f) => !isSlot(f))}>
          <div class="ed-add-unit-fields">
            <For each={(kindDef()?.fields ?? []).filter((f) => !isSlot(f))}>
              {(f) => (
                <FieldControl
                  field={f}
                  schema={kindDef()}
                  value={form().fields?.[f.name] ?? ''}
                  placeholder={`${f.name}${f.optional ? '' : ' (required)'}`}
                  onChange={(v) => patch({ fields: { ...form().fields, [f.name]: v } })}
                />
              )}
            </For>
          </div>
        </Show>
        <Show when={(props.indexes ?? []).length > 0}>
          <Select
            options={placementOptions(props.indexes ?? [])}
            value={form().pin || undefined}
            placeholder="Choose a section…"
            onChange={(v) => patch({ pin: v })}
          />
        </Show>
        <Show when={(props.indexes ?? []).length === 0}>
          <div class="ed-empty">No sections exist; this unit will be created unpinned.</div>
        </Show>
      </div>
    </Modal>
  );
}
