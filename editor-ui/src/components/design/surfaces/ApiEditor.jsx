/* The Web-API surface editor: a `code_item kind = :api` read and edited
   like API docs. Endpoints list as `METHOD path — summary` rows; expanding
   one opens its own form plus its nested Params / Responses families (the
   recursive detail payload carries them two levels down). Add, edit and
   delete all commit through the shared detail store. */

import { For, Show } from 'solid-js';

import { cellText } from '../../../preview/schemaform';
import { FamilySection, createDetail, defaultSummary } from './fields';

const endpointSummary = (item) => (
  <>
    <span class="ed-surface-method">{(cellText(item.cells, 'method') || 'GET').toUpperCase()}</span>
    <code>{cellText(item.cells, 'path') || item.label}</code>
    <span class="ed-surface-desc">{cellText(item.cells, 'summary')}</span>
  </>
);

const paramSummary = (item) => (
  <>
    <code>{cellText(item.cells, 'name') || item.label}</code>
    <span class="ed-surface-note">{cellText(item.cells, 'location')}</span>
    <span class="ed-surface-desc">{cellText(item.cells, 'description') || cellText(item.cells, 'type')}</span>
    <Show when={cellText(item.cells, 'required') === 'true'}>
      <span class="ed-surface-note">required</span>
    </Show>
  </>
);

const responseSummary = (item) => (
  <>
    <span class="ed-surface-method">{cellText(item.cells, 'status') || item.label}</span>
    <span class="ed-surface-desc">{cellText(item.cells, 'description')}</span>
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
        <Show when={cellText(d().cells, 'summary')}>
          <p class="ed-surface-summary">{cellText(d().cells, 'summary')}</p>
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
