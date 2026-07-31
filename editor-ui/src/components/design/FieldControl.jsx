/* The one form control: a schema field plus its cell rendered as the right
   input, wherever a form is opened — the Systems property dock, the details
   modal's sections, the diagram shape panel, the add dialogs and the
   Data-mode row all mount this, so a field offering a picker in one place
   cannot be a plain text box in another.

   Deliberately thin: every decision lives in `controlFor` (schemaform.js),
   which is pure and unit-tested; this file is a flat switch from a control
   name to a Forge component, with no branching of its own. Layout stays
   with the host — the label, the row, the required marker and the save
   button are theirs. */

import { Match, Switch, createMemo } from 'solid-js';
import { Checkbox, Input, Select } from '@forge/ui';

import {
  CUSTOM_OPTION,
  cellOf,
  controlFor,
  fieldText,
  notEditable,
  suggestOptions,
} from '../../preview/schemaform';

/**
 * Props:
 * - `field` — one entry of a kind's `fields` metadata
 * - `cells` — the instance's cells (`{ labels, fields }`); omitted on a
 *   create form, where every field is absent
 * - `value` — the draft text, when the host holds one for this field
 * - `onChange(text)` — a touched field's new text
 * - `schema` — the kind entry, for its `suggestions`
 * - `ids` — the ids this field may name, when it points at another kind
 * - `custom` / `onCustom()` — the escape from a suggestion picker to typing
 * - `placeholder` — overrides the derived one (the add dialogs say
 *   "(required)")
 */
export default function FieldControl(props) {
  const cell = () => (props.cells ? cellOf(props.field, props.cells) : undefined);
  const current = () =>
    props.value != null ? props.value : props.cells ? fieldText(props.field, props.cells) : '';
  const control = createMemo(() =>
    controlFor(props.field, cell(), {
      ids: props.ids,
      suggestions: props.schema?.suggestions?.[props.field.name],
      custom: props.custom,
    }),
  );
  const set = (v) => props.onChange?.(v);
  const optional = () => props.field.optional !== false;
  /** An optional field can always be cleared back to nothing. */
  const unset = () => (optional() ? [{ value: '', label: '(unset)' }] : []);
  const placeholder = () =>
    props.placeholder ??
    (control() === 'list'
      ? 'comma separated'
      : (props.field.default ?? (optional() ? '(unset)' : '')));

  return (
    <Switch>
      <Match when={control() === 'computed'}>
        <Input value={notEditable(cell()).long} disabled title="Edit it in the source instead" />
      </Match>
      <Match when={control() === 'symbol'}>
        <Select
          options={[...unset(), ...props.field.symbols.map((s) => ({ value: s, label: `:${s}` }))]}
          value={current()}
          placeholder={props.placeholder ?? props.field.name}
          onChange={set}
        />
      </Match>
      <Match when={control() === 'idref'}>
        <Select options={[...unset(), ...props.ids]} value={current()} onChange={set} />
      </Match>
      <Match when={control() === 'bool'}>
        <Checkbox checked={current() === 'true'} onChange={(on) => set(on ? 'true' : 'false')} />
      </Match>
      <Match when={control() === 'suggest'}>
        <Select
          options={suggestOptions(props.field, props.schema, current()) ?? []}
          value={current()}
          onChange={(v) => (v === CUSTOM_OPTION ? props.onCustom?.() : set(v))}
        />
      </Match>
      <Match when={control() === 'text' || control() === 'list'}>
        <Input
          value={current()}
          placeholder={placeholder()}
          onInput={(e) => set(e.currentTarget.value)}
        />
      </Match>
    </Switch>
  );
}
