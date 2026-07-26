/* The "Add unit" dialog, shared by the NavPanel (canvas tab) and the graph
   view: pick a kind (from the palette's introspected unit kinds), give it
   an id, fill the kind's metadata fields (symbol sets render as selects),
   optionally pin it into a section. The caller supplies the submit path —
   the canvas commits through the full rebuild loop, the graph through the
   quiet one — and the pinnable section list. */

import { For, Show, createSignal } from 'solid-js';
import { Button, Input, Modal, Select, toast } from '@forge/ui';

import { busy, palette } from '../../state/design';

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
    const fields = {};
    for (const fd of kindDef()?.fields ?? []) {
      if (fd.inline_slot != null) continue;
      const v = f.fields?.[fd.name];
      if (v == null || v === '') continue;
      fields[fd.name] = fd.symbols ? { sym: v } : v;
    }
    const res = await props.onSubmit(
      { kind: f.kind, id: f.id, fields },
      f.pin ? { index_id: f.pin } : null,
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
        <Show when={(kindDef()?.fields ?? []).some((f) => f.inline_slot == null)}>
          <div class="ed-add-unit-fields">
            <For each={kindDef()?.fields ?? []}>
              {(f) => (
                <Show when={f.inline_slot == null}>
                  <Show
                    when={f.symbols}
                    fallback={
                      <Input
                        value={form().fields?.[f.name] ?? ''}
                        onInput={(e) =>
                          patch({ fields: { ...form().fields, [f.name]: e.currentTarget.value } })
                        }
                        placeholder={`${f.name}${f.optional ? '' : ' (required)'}`}
                      />
                    }
                  >
                    <Select
                      options={f.symbols.map((s) => ({ value: s, label: `:${s}` }))}
                      value={form().fields?.[f.name] || undefined}
                      placeholder={f.name}
                      onChange={(v) => patch({ fields: { ...form().fields, [f.name]: v } })}
                    />
                  </Show>
                </Show>
              )}
            </For>
          </div>
        </Show>
        <Show when={(props.indexes ?? []).length > 0}>
          <Select
            options={[
              { value: '', label: '(no section)' },
              ...(props.indexes ?? []).map((n) => ({ value: n.id, label: `Pin into: ${n.title}` })),
            ]}
            value={form().pin ?? ''}
            placeholder="Pin into a section…"
            onChange={(v) => patch({ pin: v })}
          />
        </Show>
      </div>
    </Modal>
  );
}
