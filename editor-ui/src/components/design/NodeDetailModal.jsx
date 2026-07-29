/* The Systems view's details modal — everything about one object in one
   place, opened by double-clicking its box.

   The side dock edits an object's scalar properties; this is where the rest
   of it lives: the families of child blocks its schema declares (a
   `cli_command`'s args, flags and examples; an `external_system`'s contacts
   and endpoints), the relations at either end of it, and its prose body.
   Every form is generated from `/api/systems/detail`'s schema metadata, so a
   kind gains a field — or a whole child family — without a change here.

   On top of the generic tabs, the curated surface registry (surfaces.js)
   adds NATIVE editors: a cli_command opens on a man-page CLI tab, a
   `code_item kind = :api` on API docs, a screen on its rendered wireframe /
   terminal; a component or container aggregates its attached surface units
   into the same tabs, with an Add that creates a pre-linked unit.

   Writes batch through the shared block-ops pipeline against the spans the
   payload carried, then refetch: a commit reformats the file and moves every
   span, so nothing is edited twice from one load. */

import { For, Index, Show, createEffect, createSignal } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import {
  ArrowLeft,
  ArrowRight,
  ChevronDown,
  ChevronRight,
  FileCode2,
  Plus,
  Trash2,
} from 'lucide-solid';
import { Badge, Button, Input, Modal, Select, Tabs, toast } from '@forge/ui';
import { CodeEditor } from '@forge/code';

import { api } from '../../api';
import { wclLanguage } from '../../lang/wcl';
import { blockSnippet } from '../../preview/schemaform';
import { openFile } from '../../state/buffers';
import { revealSpan } from '../../state/views';
import {
  busy,
  commitOpsQuiet,
  commitUnitCreateQuiet,
  exitDesign,
} from '../../state/design';
import { activeEntry } from '../../state/sites';
import { loadSystems, model } from '../../state/systems';
import { SURFACES, attachedSurfaces, slugify, subtreeIds, surfaceOf } from './surfaces';
import { FieldForm, draftOps, freshChildId } from './surfaces/fields';
import ApiEditor from './surfaces/ApiEditor';
import CliEditor from './surfaces/CliEditor';
import ScreenEditor from './surfaces/ScreenEditor';

/** surface id → its native editor component. */
const EDITORS = { cli: CliEditor, api: ApiEditor, screen: ScreenEditor };

export default function NodeDetailModal(props) {
  const [detail, setDetail] = createSignal(null);
  const [tab, setTab] = createSignal('properties');
  const [draft, setDraft] = createSignal({});
  /** Per-child drafts, keyed by `${family field}:${span.start}`. */
  const [childDrafts, setChildDrafts] = createSignal({});
  const [source, setSource] = createSignal('');
  const [bodySource, setBodySource] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const load = async () => {
    const entry = activeEntry();
    const a = props.anchor;
    if (!entry || !a) return;
    const res = await api.systemsDetail({ entry, file: a.file, span: a.span });
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      props.onClose();
      return;
    }
    // Committing re-anchors the SAME object (spans move on every reformat),
    // so only a genuinely different object resets which tab you're on — a
    // surface unit lands on its native editor, everything else on the form.
    const prev = detail();
    if (!prev || prev.kind !== res.kind || prev.id !== res.id) {
      setTab(surfaceOf(res) ? 'surface' : 'properties');
    }
    setDetail(res);
    setDraft({});
    setChildDrafts({});
    setSource(res.source ?? '');
    setBodySource(res.body?.source ?? '');
  };
  // The previous object stays on screen while the next one loads: blanking
  // it would flash the modal empty after every save.
  createEffect(() => {
    void props.anchor;
    load();
  });

  const d = detail;
  const dirty = () =>
    Object.keys(draft()).length > 0 || Object.keys(childDrafts()).some((k) => childDrafts()[k]);

  const ownSurface = () => (d() ? surfaceOf(d()) : null);
  const aggregates = () => (d() ? attachedSurfaces(d(), model()) : []);

  /** Re-anchor on the object's fresh span after something (this modal or a
      surface editor) committed and refreshed the model, then reload. */
  const reanchor = async () => {
    const fresh = model()?.nodes?.find((n) => n.kind === d().kind && n.id === d().id);
    props.onReanchor?.(fresh ? { file: fresh.file, span: fresh.span } : props.anchor);
    await load();
  };

  /** Commit `ops`, refresh the model, and reload this object's detail. */
  const commit = async (ops, message) => {
    if (!ops.length) return true;
    setSaving(true);
    const res = await commitOpsQuiet(d().file, ops, { etag: d().etag });
    setSaving(false);
    if (!res.ok) return false;
    if (message) toast(message, { duration: 3000 });
    await loadSystems({ keep: true });
    await reanchor();
    return true;
  };

  const saveProperties = () =>
    commit(draftOps(d().schema, draft(), d().span, d().cells), 'Saved properties');

  const saveChild = (family, item) => {
    const key = `${family.field}:${item.span.start}`;
    const changes = childDrafts()[key];
    if (!changes) return;
    return commit(draftOps(family.schema, changes, item.span, item.cells), `Saved ${family.kind}`);
  };

  const addChild = (family) =>
    commit(
      [
        {
          op: 'insert_child',
          span: d().span,
          index: 9999,
          source: blockSnippet(family.schema ?? { kind: family.kind, fields: [] }, {
            id: freshChildId(family),
          }),
        },
      ],
      `Added a ${family.kind}`,
    );

  const deleteBlock = (span, what) => commit([{ op: 'delete', span }], `Deleted ${what}`);

  const openCode = async () => {
    const { file, span } = d();
    exitDesign();
    props.onClose();
    const res = await openFile(file);
    if (res.ok) revealSpan(file, span.start, span.end);
    else toast(res.error, { tone: 'danger', duration: 6000 });
  };

  const childCount = () => (d()?.children ?? []).reduce((n, f) => n + f.items.length, 0);

  const tabList = () => [
    { id: 'properties', label: 'Properties' },
    ...(ownSurface() ? [{ id: 'surface', label: ownSurface().ownLabel }] : []),
    ...aggregates().map((a) => ({
      id: `agg:${a.surface.id}`,
      // A kind-marked host with nothing attached yet shows the bare label.
      label: a.units.length ? `${a.surface.label} (${a.units.length})` : a.surface.label,
    })),
    { id: 'children', label: `Contents${childCount() ? ` (${childCount()})` : ''}` },
    {
      id: 'relations',
      label: `Relations${d().relations.length ? ` (${d().relations.length})` : ''}`,
    },
    { id: 'source', label: 'Source' },
  ];

  return (
    <Modal
      open
      size="xl"
      onClose={props.onClose}
      title={d() ? `${d().id ?? d().kind} — ${d().kind}` : 'Loading…'}
      footer={
        <>
          <span class="ed-detail-file">
            {d()?.file}
            <Show when={dirty()}>
              <span class="ed-detail-dirty"> · unsaved changes</span>
            </Show>
          </span>
          <Button size="sm" onClick={openCode} disabled={!d()}>
            <FileCode2 size={13} /> Open code
          </Button>
          <Button onClick={props.onClose}>Close</Button>
          <Show when={tab() === 'properties'}>
            <Button
              variant="primary"
              disabled={busy() || saving() || !Object.keys(draft()).length}
              onClick={saveProperties}
            >
              Save
            </Button>
          </Show>
        </>
      }
    >
      <Show when={d()} fallback={<div class="ed-empty">Loading the object…</div>}>
        <Tabs tabs={tabList()} active={tab()} onChange={setTab} />

        <div class="ed-detail-body">
          <Show when={tab() === 'properties'}>
            <Show when={d().schema?.doc}>
              <p class="ed-sys-panel-doc">{d().schema.doc}</p>
            </Show>
            <FieldForm
              schema={d().schema}
              cells={d().cells}
              draft={draft()}
              onChange={setDraft}
            />
          </Show>

          <Show when={tab() === 'surface' && ownSurface()}>
            <Dynamic
              component={EDITORS[ownSurface().id]}
              anchor={props.anchor}
              onCommitted={reanchor}
            />
          </Show>

          {/* Iterate the STATIC registry, not aggregates(): the latter
              rebuilds its objects on every model refetch, which would
              remount the host — and the screen editor inside it — after
              every commit. */}
          <For each={SURFACES}>
            {(s) => (
              <Show when={tab() === `agg:${s.id}`}>
                <AggregateSurface
                  surface={s}
                  units={aggregates().find((a) => a.surface.id === s.id)?.units ?? []}
                  owner={d()}
                  onCommitted={reanchor}
                />
              </Show>
            )}
          </For>

          <Show when={tab() === 'children'}>
            <Show
              when={(d().children ?? []).length}
              fallback={<div class="ed-empty">This kind declares no nested blocks.</div>}
            >
              <For each={d().children}>
                {(family) => (
                  <section class="ed-detail-family">
                    <header>
                      <strong>{family.field}</strong>
                      <Badge>{family.kind}</Badge>
                      <span class="spacer" />
                      <Button
                        size="sm"
                        disabled={busy() || saving() || (!family.many && family.items.length > 0)}
                        onClick={() => addChild(family)}
                      >
                        <Plus size={13} /> Add
                      </Button>
                    </header>
                    <Show when={family.doc}>
                      <p class="ed-sys-panel-doc">{family.doc}</p>
                    </Show>
                    <For each={family.items}>
                      {(item) => {
                        const key = `${family.field}:${item.span.start}`;
                        const changes = () => childDrafts()[key] ?? {};
                        return (
                          <div class="ed-detail-child">
                            <div class="ed-detail-childhead">
                              <code>{item.label ?? family.kind}</code>
                              <span class="spacer" />
                              <Button
                                size="sm"
                                variant="primary"
                                disabled={busy() || saving() || !Object.keys(changes()).length}
                                onClick={() => saveChild(family, item)}
                              >
                                Save
                              </Button>
                              <Button
                                size="sm"
                                variant="ghost"
                                title={`Delete this ${family.kind}`}
                                disabled={busy() || saving()}
                                onClick={() => deleteBlock(item.span, item.label ?? family.kind)}
                              >
                                <Trash2 size={13} />
                              </Button>
                            </div>
                            <FieldForm
                              schema={family.schema}
                              cells={item.cells}
                              draft={changes()}
                              onChange={(next) => setChildDrafts({ ...childDrafts(), [key]: next })}
                            />
                          </div>
                        );
                      }}
                    </For>
                    <Show when={!family.items.length}>
                      <div class="ed-empty">No {family.kind} blocks yet.</div>
                    </Show>
                  </section>
                )}
              </For>
            </Show>
          </Show>

          <Show when={tab() === 'relations'}>
            <Show
              when={d().relations.length}
              fallback={
                <div class="ed-empty">
                  Nothing is wired to this object — drag from its port on the canvas to add a
                  relation.
                </div>
              }
            >
              <For each={d().relations}>
                {(r) => (
                  <div class="ed-detail-rel">
                    <Show when={r.direction === 'out'} fallback={<ArrowLeft size={14} />}>
                      <ArrowRight size={14} />
                    </Show>
                    <strong>{r.other_title ?? r.other}</strong>
                    <Show when={r.rel_kind}>
                      <Badge>{r.rel_kind}</Badge>
                    </Show>
                    <span class="ed-detail-rellabel">{r.label ?? ''}</span>
                    <span class="spacer" />
                    <code>{r.id}</code>
                    <Button
                      size="sm"
                      variant="ghost"
                      title="Delete this relation"
                      disabled={busy() || saving()}
                      onClick={async () => {
                        setSaving(true);
                        const res = await commitOpsQuiet(r.file, [{ op: 'delete', span: r.span }]);
                        setSaving(false);
                        if (res.ok) {
                          toast(`Deleted ${r.id}`, { duration: 3000 });
                          await loadSystems({ keep: true });
                          await load();
                        }
                      }}
                    >
                      <Trash2 size={13} />
                    </Button>
                  </div>
                )}
              </For>
            </Show>
          </Show>

          <Show when={tab() === 'source'}>
            <p class="ed-sys-panel-doc">
              The object's own WCL. Saving replaces the block — the escape hatch for anything the
              forms above can't express.
            </p>
            <div class="ed-design-code">
              <CodeEditor
                value={source()}
                onChange={setSource}
                language={wclLanguage}
                height="320px"
              />
            </div>
            <div class="ed-detail-actions">
              <Button
                variant="primary"
                disabled={busy() || saving() || source() === d().source}
                onClick={() =>
                  commit(
                    [{ op: 'replace_source', span: d().span, source: source() }],
                    'Replaced the block',
                  )
                }
              >
                Save source
              </Button>
            </div>
            <Show when={d().body}>
              <p class="ed-sys-panel-doc">Its prose body:</p>
              <div class="ed-design-code">
                <CodeEditor
                  value={bodySource()}
                  onChange={setBodySource}
                  language={wclLanguage}
                  height="240px"
                />
              </div>
              <div class="ed-detail-actions">
                <Button
                  disabled={busy() || saving() || bodySource() === d().body.source}
                  onClick={() =>
                    commit(
                      [{ op: 'replace_source', span: d().body.span, source: bodySource() }],
                      'Saved the body',
                    )
                  }
                >
                  Save body
                </Button>
              </div>
            </Show>
          </Show>
        </div>
      </Show>
    </Modal>
  );
}

/**
 * One aggregated surface tab on a component / container: the surface units
 * attached anywhere in its containment subtree (a container's CLI commands
 * hang off its components) as expandable rows hosting the SAME editor the
 * unit's own modal tab uses. Rows attached below the opened node carry a
 * `via <owner>` note. Add creates a pre-linked unit (parent field from the
 * schema, required fields seeded, the surface's `create` hints — a new API
 * code_item gets `kind = :api` — applied); when the schema has no link to
 * the opened node's own kind, an owner picker offers the subtree nodes the
 * surface CAN attach to (a cli_command added from a container picks which
 * component owns it).
 *
 * One row expands at a time: the screen editor owns the global commit-loop
 * hook while mounted, so two live at once would fight over it.
 */
function AggregateSurface(props) {
  const [openId, setOpenId] = createSignal(null);
  const [name, setName] = createSignal('');
  const [ownerId, setOwnerId] = createSignal('');
  const [adding, setAdding] = createSignal(false);

  const entryOf = () => model()?.kinds?.find((k) => k.kind === props.surface.kind);
  const parentKinds = () => entryOf()?.parents ?? [];
  const parentField = () =>
    parentKinds().find((p) => p.kind === props.owner.kind)?.field ?? null;
  /** Subtree nodes the surface can attach to, when it can't attach here. */
  const ownerChoices = () => {
    if (parentField()) return [];
    const ids = subtreeIds(props.owner, model());
    return (model()?.nodes ?? []).filter(
      (n) => n.id !== props.owner.id && ids.has(n.id) && parentKinds().some((p) => p.kind === n.kind),
    );
  };
  const via = (u) => (u.parents ?? []).find((p) => p.id !== props.owner.id)?.id;
  const Editor = EDITORS[props.surface.id];

  const addUnit = async () => {
    const entry = entryOf();
    const nm = name().trim();
    const owner = parentField()
      ? props.owner
      : ownerChoices().find((n) => n.id === (ownerId() || ownerChoices()[0]?.id));
    const pf = owner && parentKinds().find((p) => p.kind === owner.kind)?.field;
    if (!entry || !pf || !nm) return;
    const base = slugify(nm) || `${props.surface.kind}_new`;
    const taken = new Set((model()?.ids ?? []).concat(props.units.map((u) => u.id)));
    let id = base;
    for (let n = 2; taken.has(id); n += 1) id = `${base}_${n}`;
    const fields = { [pf]: { ident: owner.id } };
    for (const f of entry.fields ?? []) {
      if (f.inline_slot != null || f.optional !== false || f.default != null) continue;
      if (f.name in fields) continue;
      const hint = props.surface.create?.[f.name];
      if (f.name === 'name' || f.name === 'title') fields[f.name] = nm;
      else if (f.symbols?.length) fields[f.name] = { sym: hint ?? f.symbols[0] };
      else fields[f.name] = hint ?? '';
    }
    // Surface gate fields must be set even when the schema calls them
    // optional — without them the new unit wouldn't appear in this tab.
    for (const [k, v] of Object.entries(props.surface.create ?? {})) {
      if (k in fields) continue;
      const f = (entry.fields ?? []).find((x) => x.name === k);
      fields[k] = f?.symbols ? { sym: v } : v;
    }
    setAdding(true);
    const res = await commitUnitCreateQuiet({
      kind: entry.kind,
      type_name: entry.type_name,
      id,
      fields,
    });
    setAdding(false);
    if (!res.ok) return;
    toast(`Added ${id}`, { duration: 3000 });
    setName('');
    await loadSystems({ keep: true });
    await props.onCommitted?.();
    setOpenId(id);
  };

  return (
    <div class="ed-surface-agg">
      <div class="ed-surface-add">
        <Input
          value={name()}
          placeholder={`New ${props.surface.kind} name…`}
          onInput={(e) => setName(e.currentTarget.value)}
          onKeyDown={(e) => e.key === 'Enter' && addUnit()}
        />
        <Show when={ownerChoices().length > 0}>
          <Select
            options={ownerChoices().map((n) => ({ value: n.id, label: `${n.title} (${n.kind})` }))}
            value={ownerId() || ownerChoices()[0]?.id}
            onChange={setOwnerId}
          />
        </Show>
        <Button
          size="sm"
          disabled={busy() || adding() || !name().trim() || (!parentField() && !ownerChoices().length)}
          onClick={addUnit}
        >
          <Plus size={13} /> Add
        </Button>
      </div>
      {/* Index, not For: every commit refetches the model and replaces the
          unit objects — For would remount the row (and any expanded editor,
          re-rendering its whole preview) on every save. Index re-renders in
          place; the anchor prop stays reactive through the accessor. */}
      <Index each={props.units}>
        {(u) => (
          <div class="ed-surface-item" classList={{ 'is-open': openId() === u().id }}>
            <div
              class="ed-surface-row"
              onClick={() => setOpenId(openId() === u().id ? null : u().id)}
            >
              <Show when={openId() === u().id} fallback={<ChevronRight size={13} />}>
                <ChevronDown size={13} />
              </Show>
              <strong>{u().title}</strong>
              <span class="ed-surface-desc">{u().summary ?? ''}</span>
              <span class="spacer" />
              <Show when={via(u())}>
                <span class="ed-surface-note">via {via(u())}</span>
              </Show>
              <code>{u().id}</code>
            </div>
            <Show when={openId() === u().id}>
              <div class="ed-surface-detail">
                <Editor
                  anchor={{ file: u().file, span: u().span }}
                  onCommitted={props.onCommitted}
                />
              </div>
            </Show>
          </div>
        )}
      </Index>
      <Show when={!props.units.length}>
        <div class="ed-empty">No {props.surface.kind} attached yet.</div>
      </Show>
    </div>
  );
}
