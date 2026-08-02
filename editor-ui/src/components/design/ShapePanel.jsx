/* The diagram-shape properties sidebar: docked right of the canvas while a
   shape anchor is selected. The form is generated from the shape kind's
   schema metadata (palette().diagram_kinds — effective_fields introspection)
   merged with the instance's current values from /api/block/source, so any
   `extends SvgBlock` kind — including user-declared shapes — gets a form.

   Geometry (x/y/width/height) leads, then the remaining scalar fields, each
   as the shared FieldControl — so a shape's properties offer exactly what
   the same field offers in any other panel. Save batches set_label /
   set_field / remove_field ops into one atomic commit through the shared
   save rule; Reset position/size drop the explicit fields so the layout (or
   the kind's defaults) takes over. */

import { For, Show, createEffect, createResource, createSignal } from 'solid-js';
import { Braces, RotateCcw, Scaling } from 'lucide-solid';
import { Button } from '@forge/ui';

import { api } from '../../api';
import { busy, commitOps, palette, selection, setPopover } from '../../state/design';
import { draftOps, orderFields } from '../../preview/schemaform';
import FieldControl from './FieldControl';

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

  const cells = () => (src()?.ok ? src().cells : null);

  /** The form rows: schema-driven when the kind is known, else generic
      rows for whatever fields the instance carries. */
  const rows = () => {
    const s = src();
    if (!s?.ok) return [];
    const known = schema()?.fields;
    if (known) return orderFields(known);
    return Object.keys(s.cells?.fields ?? {}).map((name) => ({ name, type: '', optional: true }));
  };

  const save = () => {
    const a = anchor();
    const s = src();
    if (!a || !s?.ok) return;
    const ops = draftOps(rows(), cells(), draft(), a.span);
    // Nothing actually changed — settle the form rather than committing.
    if (!ops.length) return setDraft({});
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
                  <FieldControl
                    field={f}
                    cells={cells()}
                    schema={schema()}
                    value={f.name in draft() ? draft()[f.name] : undefined}
                    onChange={(v) => setDraft({ ...draft(), [f.name]: v })}
                  />
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
