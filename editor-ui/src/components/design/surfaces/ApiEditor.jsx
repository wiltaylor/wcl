/* The Web-API surface editor: a `code_item kind = :api` read and edited
   like API docs. Endpoints list as `METHOD path — summary` rows; expanding
   one opens its own form plus its nested Params / Responses families (the
   recursive detail payload carries them two levels down). Add, edit and
   delete all commit through the shared detail store. */

import { For, Show } from 'solid-js';

import { FamilySection, createDetail, defaultSummary } from './fields';

const cell = (item, name) => item.cells?.[name]?.text ?? '';

const endpointSummary = (item) => (
  <>
    <span class="ed-surface-method">{(cell(item, 'method') || 'GET').toUpperCase()}</span>
    <code>{cell(item, 'path') || item.label}</code>
    <span class="ed-surface-desc">{cell(item, 'summary')}</span>
  </>
);

const paramSummary = (item) => (
  <>
    <code>{cell(item, 'name') || item.label}</code>
    <span class="ed-surface-note">{cell(item, 'location').replace(/^:/, '')}</span>
    <span class="ed-surface-desc">{cell(item, 'description') || cell(item, 'type')}</span>
    <Show when={cell(item, 'required') === 'true'}>
      <span class="ed-surface-note">required</span>
    </Show>
  </>
);

const responseSummary = (item) => (
  <>
    <span class="ed-surface-method">{cell(item, 'status') || item.label}</span>
    <span class="ed-surface-desc">{cell(item, 'description')}</span>
  </>
);

const NESTED = {
  api_param: paramSummary,
  api_response: responseSummary,
};

export default function ApiEditor(props) {
  const { detail, saving, commit } = createDetail(() => props.anchor, {
    onAfterCommit: props.onCommitted,
  });
  const d = detail;
  const endpoints = () => (d()?.children ?? []).find((f) => f.kind === 'api_endpoint');
  const others = () =>
    (d()?.children ?? []).filter((f) => f.kind !== 'api_endpoint' && f.kind !== 'body');

  return (
    <Show when={d()} fallback={<div class="ed-empty">Loading the API…</div>}>
      <div class="ed-surface-api">
        <Show when={d().cells?.summary?.text}>
          <p class="ed-surface-summary">{d().cells.summary.text}</p>
        </Show>
        <Show
          when={endpoints()}
          fallback={<div class="ed-empty">This code item declares no endpoint family.</div>}
        >
          <FamilySection
            family={endpoints()}
            ownerSpan={d().span}
            commit={commit}
            saving={saving()}
            title="Endpoints"
            summary={endpointSummary}
            nestedSummaryFor={(fam) => NESTED[fam.kind]}
            addValues={{ method: 'GET', path: '/', summary: '' }}
          />
        </Show>
        <For each={others()}>
          {(family) => (
            <FamilySection
              family={family}
              ownerSpan={d().span}
              commit={commit}
              saving={saving()}
              summary={defaultSummary}
            />
          )}
        </For>
      </div>
    </Show>
  );
}
