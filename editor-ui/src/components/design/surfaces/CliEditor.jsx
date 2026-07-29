/* The CLI surface editor: one cli_command read and edited like a man page.
   A rendered usage line + summary up top (the scalar fields stay on the
   modal's Properties tab), then Arguments / Flags / Examples as compact
   rows that expand into the schema-generated form — add, edit, delete.

   Self-contained over `/api/systems/detail` (createDetail): the host hands
   it an anchor and an `onCommitted` callback to re-anchor itself after a
   write moved every span in the file. */

import { For, Show } from 'solid-js';

import { usageLine } from '../surfaces';
import { FamilySection, createDetail } from './fields';

/** Collapsed rows per family, man-page flavoured. */
const cell = (item, name) => item.cells?.[name]?.text ?? '';

const argSummary = (item) => (
  <>
    <code>{cell(item, 'name') || item.label}</code>
    <span class="ed-surface-desc">{cell(item, 'description')}</span>
    <Show when={cell(item, 'required') === 'false'}>
      <span class="ed-surface-note">optional</span>
    </Show>
  </>
);

const flagSummary = (item) => (
  <>
    <code>
      {cell(item, 'name') || item.label}
      {cell(item, 'value') ? ` ${cell(item, 'value')}` : ''}
    </code>
    <span class="ed-surface-desc">{cell(item, 'description')}</span>
    <Show when={cell(item, 'default')}>
      <span class="ed-surface-note">default: {cell(item, 'default')}</span>
    </Show>
    <Show when={cell(item, 'repeatable') === 'true'}>
      <span class="ed-surface-note">repeatable</span>
    </Show>
  </>
);

const exampleSummary = (item) => (
  <>
    <code>{cell(item, 'command') || item.label}</code>
    <span class="ed-surface-desc">{cell(item, 'description')}</span>
  </>
);

/** family kind → (title, row renderer); unlisted families use defaults. */
const CLI_FAMILIES = {
  cli_arg: { title: 'Arguments', summary: argSummary },
  cli_flag: { title: 'Flags', summary: flagSummary },
  cli_example: { title: 'Examples', summary: exampleSummary },
};

export default function CliEditor(props) {
  const { detail, saving, commit } = createDetail(() => props.anchor, {
    onAfterCommit: props.onCommitted,
  });
  const d = detail;
  const families = () => (d()?.children ?? []).filter((f) => f.kind !== 'body');

  return (
    <Show when={d()} fallback={<div class="ed-empty">Loading the command…</div>}>
      <div class="ed-surface-cli">
        <div class="ed-surface-usage">
          <code>{usageLine(d())}</code>
        </div>
        <Show when={d().cells?.summary?.text}>
          <p class="ed-surface-summary">{d().cells.summary.text}</p>
        </Show>
        <For each={families()}>
          {(family) => (
            <FamilySection
              family={family}
              ownerSpan={d().span}
              commit={commit}
              saving={saving()}
              title={CLI_FAMILIES[family.kind]?.title}
              summary={CLI_FAMILIES[family.kind]?.summary ?? argSummary}
            />
          )}
        </For>
      </div>
    </Show>
  );
}
