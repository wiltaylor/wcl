/* The diagram-shape properties sidebar: docked right of the canvas while a
   shape anchor is selected. The form is generated from the shape kind's
   schema metadata (palette().diagram_kinds — effective_fields introspection)
   merged with the instance's current values from /api/block/source, so any
   `extends SvgBlock` kind — including user-declared shapes — gets a form.

   Geometry (x/y/width/height) leads, then the remaining scalar fields:
   numbers and strings as inputs, symbol sets as selects, bools as toggles.
   Computed (expression-valued) fields render read-only and defer to the
   fragment editor. Save batches set_label / set_field / remove_field ops
   into one atomic commit; Reset position/size drop the explicit fields so
   the layout (or the kind's defaults) takes over. */

import { For, Show, createEffect, createResource, createSignal } from 'solid-js';
import { Braces, RotateCcw, Scaling } from 'lucide-solid';
import { Button, Checkbox, Input, Select } from '@forge/ui';

import { api } from '../../api';
import { busy, commitOps, palette, selection, setPopover } from '../../state/design';

const GEOMETRY = ['x', 'y', 'width', 'height'];

/** Order schema fields for the form: inline slots, geometry, the rest. */
function orderFields(fields) {
  const slot = (f) => (f.inline_slot === null || f.inline_slot === undefined ? 1 : 0);
  const geo = (f) => {
    const i = GEOMETRY.indexOf(f.name);
    return i === -1 ? GEOMETRY.length : i;
  };
  return [...fields].sort(
    (a, b) =>
      slot(a) - slot(b) ||
      (a.inline_slot ?? 0) - (b.inline_slot ?? 0) ||
      geo(a) - geo(b) ||
      a.name.localeCompare(b.name),
  );
}

/** The value expr for a field edit, typed from the schema entry. */
function valueOp(span, field, text) {
  const ty = (field.type ?? '').replace(/\?$/, '');
  if (field.inline_slot !== null && field.inline_slot !== undefined) {
    const op = { op: 'set_label', span, slot: field.inline_slot };
    if (ty === 'identifier') return { ...op, expr: text };
    if (ty.startsWith('utf8') || ty.startsWith('ascii')) return { ...op, text };
    return { ...op, expr: text };
  }
  const op = { op: 'set_field', span, field: field.name };
  if (ty.startsWith('utf8') || ty.startsWith('ascii')) return { ...op, text };
  if (ty === 'bool') return { ...op, expr: text === 'true' ? 'true' : 'false' };
  if (field.symbols) return { ...op, expr: `:${text}` };
  if (/^[iu]/.test(ty)) return { ...op, expr: String(Math.round(Number(text))) };
  if (/^f/.test(ty)) {
    const n = Number(text);
    return { ...op, expr: Number.isInteger(n) ? `${n}.0` : String(n) };
  }
  // Unknown type: trust the author's raw expression text.
  return { ...op, expr: text };
}

export default function ShapePanel() {
  const anchor = () => (selection()?.shape ? selection() : null);
  const [src] = createResource(anchor, (a) =>
    api.blockSource({ file: a.file, span: a.span }),
  );
  const schema = () => palette()?.diagram_kinds?.find((k) => k.kind === anchor()?.kind) ?? null;

  // Draft edits keyed by field name (inline slots keyed too — names are
  // unique per schema). Only touched fields commit.
  const [draft, setDraft] = createSignal({});
  createEffect(() => {
    void anchor();
    void src();
    setDraft({});
  });

  /** The form rows: schema-driven when the kind is known, else generic
      rows for whatever fields the instance carries. */
  const rows = () => {
    const s = src();
    if (!s?.ok) return [];
    const known = schema()?.fields;
    if (known) return orderFields(known);
    return Object.keys(s.fields ?? {}).map((name) => ({ name, type: '', optional: true }));
  };

  const current = (f) => {
    if (f.name in draft()) return draft()[f.name];
    const s = src();
    if (!s?.ok) return '';
    const slot =
      f.inline_slot !== null && f.inline_slot !== undefined
        ? s.labels?.[f.inline_slot]
        : s.fields?.[f.name];
    return slot?.text ?? '';
  };
  const stateOf = (f) => {
    const s = src();
    if (!s?.ok) return 'absent';
    const slot =
      f.inline_slot !== null && f.inline_slot !== undefined
        ? s.labels?.[f.inline_slot]
        : s.fields?.[f.name];
    if (!slot) return 'absent';
    return slot.state;
  };
  const editable = (f) =>
    ['literal', 'number', 'bool', 'symbol', 'absent'].includes(stateOf(f));

  const save = () => {
    const a = anchor();
    const s = src();
    if (!a || !s?.ok) return;
    const ops = [];
    for (const f of rows()) {
      if (!(f.name in draft())) continue;
      const text = draft()[f.name];
      const isSlot = f.inline_slot !== null && f.inline_slot !== undefined;
      if (text === '' && !isSlot) {
        // Clearing an optional field drops it (tolerant of absence).
        if (f.optional !== false) ops.push({ op: 'remove_field', span: a.span, field: f.name });
        continue;
      }
      if (text === '') continue;
      ops.push(valueOp(a.span, f, text));
    }
    if (!ops.length) return;
    commitOps(a.file, ops, { etag: s.etag, reveal: 'edited' });
  };

  const resetOps = (fields) => {
    const a = anchor();
    if (!a) return;
    commitOps(
      a.file,
      fields.map((field) => ({ op: 'remove_field', span: a.span, field })),
      { reveal: 'edited' },
    );
  };

  return (
    <Show when={anchor()}>
      <div class="ed-shape-panel">
        <div class="ed-shape-head">
          <span class="ed-design-kind">{anchor().kind}</span>
          <span class="spacer" />
          <Button
            size="sm"
            title="Edit as source"
            onClick={() => setPopover({ type: 'fragment', anchor: anchor() })}
          >
            <Braces size={13} />
          </Button>
        </div>
        <Show when={schema()?.doc}>
          <p class="ed-shape-doc">{schema().doc}</p>
        </Show>
        <Show when={src()?.ok} fallback={<p class="ed-shape-doc">{src()?.error ?? 'Loading…'}</p>}>
          <div class="ed-form">
            <For each={rows()}>
              {(f) => (
                <label class="ed-shape-field" title={f.doc ?? undefined}>
                  <span class="ed-shape-name">{f.name}</span>
                  <Show
                    when={editable(f)}
                    fallback={
                      <Input
                        value={stateOf(f) === 'computed' ? '(computed)' : '(list)'}
                        disabled
                        title="Edit as source instead"
                      />
                    }
                  >
                    <Show
                      when={f.symbols}
                      fallback={
                        <Show
                          when={(f.type ?? '').replace(/\?$/, '') === 'bool'}
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
                          ...(f.optional ? [{ value: '', label: '(unset)' }] : []),
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
          <div class="ed-shape-actions">
            <Button size="sm" title="Let the layout place it" onClick={() => resetOps(['x', 'y'])}>
              <RotateCcw size={13} /> position
            </Button>
            <Button size="sm" title="Back to the default size" onClick={() => resetOps(['width', 'height'])}>
              <Scaling size={13} /> size
            </Button>
            <span class="spacer" />
            <Button
              variant="primary"
              size="sm"
              disabled={busy() || !Object.keys(draft()).length}
              onClick={save}
            >
              Save
            </Button>
          </div>
        </Show>
      </div>
    </Show>
  );
}
