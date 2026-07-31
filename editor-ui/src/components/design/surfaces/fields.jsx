/* Shared building blocks for the Systems view's detail modal and its
   native surface editors (CLI / API / Screens):

   - FieldForm — the schema-generated property form: the modal's layout
     around the shared FieldControl, saved through the shared `draftOps`
     rule (schemaform.js), so a field behaves here exactly as it does in
     the canvas dock.
   - createDetail — load + keep one object's /api/systems/detail across
     commits: every write reformats the file and moves spans, so after a
     commit the object is re-found by (kind, id) in the refreshed model and
     refetched.
   - FamilySection — one `@child`/`@children` family as compact rows with
     expand-to-form editing, add and delete; nested families (an
     api_endpoint's params) recurse through `item.children`. */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { ChevronDown, ChevronRight, Plus, Trash2 } from 'lucide-solid';
import { Button, toast } from '@forge/ui';

import { api } from '../../../api';
import { blockSnippet, cellText, draftOps, orderFields } from '../../../preview/schemaform';
import { busy, commitOpsQuiet } from '../../../state/design';
import { activeEntry } from '../../../state/sites';
import { loadSystems, model } from '../../../state/systems';
import FieldControl from '../FieldControl';

/** A form row per editable field, shared by the object and its children. */
export function FieldForm(props) {
  const rows = () => orderFields(props.schema?.fields ?? []);
  const set = (name, value) => props.onChange({ ...props.draft, [name]: value });
  /** Fields switched to free typing by picking "Custom…". */
  const [custom, setCustom] = createSignal({});

  return (
    <div class="ed-detail-form">
      <For each={rows()}>
        {(f) => (
          <label class="ed-sys-field" title={f.doc ?? undefined}>
            <span class="ed-sys-fieldname">
              {f.name}
              {/* Required means the author must supply it — a field with a
                  declared default never needs one. */}
              <Show when={f.optional === false && f.default == null}>
                <span class="ed-detail-req">*</span>
              </Show>
            </span>
            <FieldControl
              field={f}
              cells={props.cells}
              schema={props.schema}
              value={f.name in props.draft ? props.draft[f.name] : undefined}
              custom={custom()[f.name]}
              onCustom={() => {
                setCustom({ ...custom(), [f.name]: true });
                set(f.name, '');
              }}
              onChange={(v) => set(f.name, v)}
            />
          </label>
        )}
      </For>
    </div>
  );
}

/**
 * Load + keep one object's `/api/systems/detail`. `anchor` is an accessor of
 * `{ file, span }`; the payload refetches whenever it changes. `commit`
 * writes ops against the object's file (quiet path — no canvas rebuild),
 * refreshes the systems model, re-finds the object by (kind, id) — spans
 * moved with the reformat — refetches, then runs `onAfterCommit` so the
 * host can re-anchor whatever else it holds on this file.
 */
export function createDetail(anchor, { onAfterCommit } = {}) {
  const [detail, setDetail] = createSignal(null);
  const [saving, setSaving] = createSignal(false);
  let key = null; // { kind, id } once loaded

  const fetchAt = async (a) => {
    const entry = activeEntry();
    if (!entry || !a) return null;
    const res = await api.systemsDetail({ entry, file: a.file, span: a.span });
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      return null;
    }
    key = { kind: res.kind, id: res.id };
    setDetail(res);
    return res;
  };

  createEffect(() => {
    const a = anchor();
    if (a) fetchAt(a);
  });

  const commit = async (ops, message) => {
    const d = detail();
    if (!d || !ops.length) return true;
    setSaving(true);
    const res = await commitOpsQuiet(d.file, ops, { etag: d.etag });
    setSaving(false);
    if (!res.ok) return false;
    if (message) toast(message, { duration: 3000 });
    await loadSystems({ keep: true });
    const fresh =
      key && model()?.nodes?.find((n) => n.kind === key.kind && n.id === key.id);
    await fetchAt(fresh ? { file: fresh.file, span: fresh.span } : anchor());
    await onAfterCommit?.();
    return true;
  };

  return { detail, saving, commit };
}

/** A fresh child id: `<kind>_<n>` dodging the family's existing labels. */
export function freshChildId(family) {
  const used = new Set(family.items.map((i) => i.label).filter(Boolean));
  let id = `${family.kind}_1`;
  for (let n = 2; used.has(id); n += 1) id = `${family.kind}_${n}`;
  return id;
}

/**
 * One child-block family as compact rows: `summary(item)` renders the
 * collapsed line, expanding opens the schema form (and, when the payload
 * carries them, the item's own nested families — an api_endpoint's params
 * and responses). Adds `insert_child` into `ownerSpan`; every write goes
 * through the host's `commit(ops, message)`.
 *
 * Open/draft state keys on the item label (stable across the reformat every
 * commit causes) so an expanded row stays open through its own save.
 */
export function FamilySection(props) {
  const [open, setOpen] = createSignal({});
  const [drafts, setDrafts] = createSignal({});
  const keyOf = (item) => item.label ?? `@${item.span.start}`;
  const toggler = (item) => () => {
    const k = keyOf(item);
    setOpen({ ...open(), [k]: !open()[k] });
  };
  const draftFor = (item) => drafts()[keyOf(item)] ?? {};
  const setDraft = (item, next) => setDrafts({ ...drafts(), [keyOf(item)]: next });

  const save = (item) => {
    const changes = drafts()[keyOf(item)];
    if (!changes || !Object.keys(changes).length) return;
    return props
      .commit(
        draftOps(props.family.schema?.fields, item.cells, changes, item.span),
        `Saved ${props.family.kind}`,
      )
      .then((ok) => {
        if (ok) setDrafts({ ...drafts(), [keyOf(item)]: {} });
      });
  };

  const add = () =>
    props.commit(
      [
        {
          op: 'insert_child',
          span: props.ownerSpan,
          index: 9999,
          source: blockSnippet(props.family.schema ?? { kind: props.family.kind, fields: [] }, {
            id: freshChildId(props.family),
            values: props.addValues ?? {},
          }),
        },
      ],
      `Added a ${props.family.kind}`,
    );

  const remove = (item) =>
    props.commit(
      [{ op: 'delete', span: item.span }],
      `Deleted ${item.label ?? props.family.kind}`,
    );

  return (
    <section class="ed-surface-family">
      <header>
        <strong>{props.title ?? props.family.field}</strong>
        <span class="spacer" />
        <Button
          size="sm"
          disabled={busy() || props.saving || (!props.family.many && props.family.items.length > 0)}
          onClick={add}
        >
          <Plus size={13} /> Add
        </Button>
      </header>
      <Show when={props.family.doc}>
        <p class="ed-sys-panel-doc">{props.family.doc}</p>
      </Show>
      <For each={props.family.items}>
        {(item) => (
          <div class="ed-surface-item" classList={{ 'is-open': !!open()[keyOf(item)] }}>
            <div class="ed-surface-row" onClick={toggler(item)}>
              <Show when={open()[keyOf(item)]} fallback={<ChevronRight size={13} />}>
                <ChevronDown size={13} />
              </Show>
              {props.summary(item)}
              <span class="spacer" />
              <Button
                size="sm"
                variant="ghost"
                title={`Delete this ${props.family.kind}`}
                disabled={busy() || props.saving}
                onClick={(e) => {
                  e.stopPropagation();
                  remove(item);
                }}
              >
                <Trash2 size={13} />
              </Button>
            </div>
            <Show when={open()[keyOf(item)]}>
              <div class="ed-surface-detail">
                <FieldForm
                  schema={props.family.schema}
                  cells={item.cells}
                  draft={draftFor(item)}
                  onChange={(next) => setDraft(item, next)}
                />
                <div class="ed-detail-actions">
                  <Button
                    size="sm"
                    variant="primary"
                    disabled={busy() || props.saving || !Object.keys(draftFor(item)).length}
                    onClick={() => save(item)}
                  >
                    Save
                  </Button>
                </div>
                <For each={(item.children ?? []).filter((f) => f.kind !== 'body')}>
                  {(nested) => (
                    <FamilySection
                      family={nested}
                      ownerSpan={item.span}
                      commit={props.commit}
                      saving={props.saving}
                      summary={props.nestedSummaryFor?.(nested) ?? defaultSummary}
                      nestedSummaryFor={props.nestedSummaryFor}
                    />
                  )}
                </For>
              </div>
            </Show>
          </div>
        )}
      </For>
      <Show when={!props.family.items.length}>
        <div class="ed-empty">No {props.family.kind} blocks yet.</div>
      </Show>
    </section>
  );
}

/** The fallback collapsed row: label + the first descriptive cell. */
export function defaultSummary(item) {
  const text =
    cellText(item.cells, 'description') ||
    cellText(item.cells, 'summary') ||
    cellText(item.cells, 'name');
  return (
    <>
      <code>{item.label ?? ''}</code>
      <span class="ed-surface-desc">{text}</span>
    </>
  );
}
