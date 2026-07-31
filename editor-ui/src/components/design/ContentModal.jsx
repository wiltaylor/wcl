/* The graph view's "Content & visibility" modal for one unit. The preview
   pane is the same WYSIWYG EditSurface as the design canvas — click a block
   to select it (the block toolbar carries move/insert/delete/visibility),
   click again to edit its contents in place; every commit rebuilds just the
   unit's page in the shown view (the modal registers its own rebuild for
   the commit loop while it is open, and hands it to the mounted surface as
   part of that surface's preview).

   Tabs: a Merged tab (the default landing) rendering the unit's page with
   EVERY block visible regardless of `@only` / `@except` — each block gets a
   left gutter of per-view letter chips (visgutter.js, fed by the merged
   build's visibility stamps) that toggle its `@except(sites = […])` per
   view — then one per previewable profile (Book / Deck / Training), a
   Skill tab showing the unit's generated Markdown (exactly what the AI
   skill ships), and an All tab rendering every profile side by side for
   comparison. The merged page renders under the first view whose site
   builds the unit's page (a lesson uses the Training template); no page
   anywhere shows an explicit empty state. The left column keeps the
   whole-unit visibility chips and the unit's index placement per view
   (pin / unpin where it sits in each view's navigation); the profile
   on/off switches live on the graph toolbar's Profiles button.
   Previews are chrome-stripped (injectBareCss hides the book
   sidebar/rail/pagenav — an unindexed unit has no TOC entry, so the chrome
   only reads as broken) and a tab showing the unit's own page carries a
   "not in any … index" badge when no index visible in that view pins it. */

import { For, Show, createEffect, createMemo, createResource, createSignal, onCleanup } from 'solid-js';

import {
  Badge,
  Button,
  IconButton,
  Modal,
  Select,
  Spinner,
  ToggleGroup,
  toast,
} from '@forge/ui';
import { CodeEditor } from '@forge/code';
import { FileCode2, Minus, Trash2 } from 'lucide-solid';

import { api } from '../../api';
import { selected } from '../../state/sites';
import {
  busy,
  commitNavOpQuiet,
  commitOps,
  commitOpsLocal,
  frameReady,
  pushSurfaceRebuild,
  setPopover,
} from '../../state/design';
import { createPreview, currentPage, setCurrentPage } from '../../state/preview';
import { graphData, indexLevelsForSite, pinCounts, reloadGraph } from '../../state/graph';
import { wclLanguage } from '../../lang/wcl';
import { injectBareCss } from '../../preview/frame';
import EditSurface from './EditSurface';
import { viewLabel } from './DesignCanvas';
import FlowPanel from './FlowPanel';

export default function ContentModal(props) {
  const node = () => graphData()?.nodes.find((n) => n.key === props.nodeKey);
  const sites = () => graphData()?.sites ?? [];
  const views = () => (selected()?.wskill ? (selected().views ?? []) : []);
  // The skill view has no HTML pages — it gets the Markdown tab instead.
  const previewViews = () => views().filter((v) => !v.skill);
  const skillView = () => views().find((v) => v.skill) ?? null;

  const [tab, setTab] = createSignal(null);

  const slug = () => {
    const n = node();
    if (!n) return null;
    const prefix = n.kind === 'procedure' ? 'process' : n.kind;
    return `${prefix}_${n.id}`;
  };

  // ---- one preview per tab ----
  // The view list is fixed for as long as the modal is open, so each tab
  // gets its own preview: its own cache, its own in-flight guard, and its
  // own identity on the commit bus — a commit that refreshed the tab on
  // screen leaves that one alone and marks the rest stale, so a tab is
  // never a snapshot from before the last edit.
  //
  // `viewList` is that snapshot, and it is what the TABS are built from too:
  // a view appearing while the modal is open (a profile enabled from the
  // graph toolbar) would otherwise offer a tab with no preview behind it.
  // Re-opening the modal picks the new view up.
  const viewList = previewViews();
  const viewPreviews = new Map(
    viewList.map((v) => [
      v.id,
      createPreview(() => ({ entry: v.entry, site: v.site, page: slug() }), {
        active: () => activeTab() === v.id || activeTab() === 'all',
      }),
    ]),
  );
  const previewFor = (view) => (view ? (viewPreviews.get(view.id) ?? null) : null);

  const activeTab = () => tab() ?? (viewList.length ? 'merged' : skillView() ? 'skill' : null);
  const currentView = () => viewList.find((v) => v.id === activeTab()) ?? null;

  // Without a manifest entry the tab shows an explicit "no page in this
  // view" state instead of the view's index.
  const hasPageIn = (view) => previewFor(view)?.hasPage() === true;
  const noPageIn = (view) => previewFor(view)?.hasPage() === false;
  const srcFor = (view) => previewFor(view)?.src() ?? null;

  // ---- the merged all-views preview (the Merged tab) ----
  // The merged page renders under one view's template with every block
  // visible; `mergedView` is that view (the first whose site builds the
  // unit's page). Units with no page in ANY view get a server-side
  // synthetic page (`unit: {kind, id}` — the unit's body projected
  // standalone, anchors pointing at the real source) so they render and
  // edit like the rest. The probe below drives the target, so this preview
  // owns a settable one rather than deriving it.
  const UNIT_PAGE = '__wcl_unit_preview';
  const [mergedView, setMergedView] = createSignal(null); // view | false (nothing renderable)
  const merged = createPreview(null, { active: () => activeTab() === 'merged' });

  const mergedTarget = (view, synthetic) => ({
    entry: view.entry,
    site: view.site,
    page: synthetic ? UNIT_PAGE : slug(),
    merged: true,
    ...(synthetic ? { unit: { kind: node()?.kind, id: node()?.id } } : {}),
  });

  // Land the Merged tab on a view that actually shows this unit: probe the
  // views in tab order with merged builds until one's manifest carries the
  // unit's page (a lesson merges under the Training template, not the
  // book's). No page anywhere → the synthetic standalone body preview;
  // units with nothing to render (no body) keep the block-list fallback.
  (async () => {
    for (const v of viewList) {
      merged.setTarget(mergedTarget(v, false));
      const res = await merged.build();
      if (!res.ok) return; // the tab shows the error
      if (merged.hasPage()) {
        setMergedView(v);
        return;
      }
    }
    const v0 = viewList[0];
    if (v0 && node()?.type === 'unit') {
      merged.setTarget(mergedTarget(v0, true));
      const res = await merged.build();
      if (res.ok && merged.hasPage()) {
        setMergedView(v0);
        return;
      }
    }
    merged.setTarget(null);
    setMergedView(false);
  })();

  // Per-view index membership (shared pinCounts semantics with the graph's
  // node badges): an unindexed unit's page still builds, but nothing links
  // it from that view's navigation — worth a warning on the preview. Only
  // when the tab shows the unit's own page (a deck/training tab falling
  // back to the view's index isn't making an index-membership claim).
  const unitPins = createMemo(() => pinCounts(graphData())[node()?.key]?.sites ?? {});
  const unindexedFor = (view) =>
    !!view && node()?.type === 'unit' && hasPageIn(view) && !(unitPins()[view.site] > 0);
  const unindexedBadge = (view) => (
    <Show when={unindexedFor(view)}>
      <span title="The page is built, but no index in this view lists the unit — it won't appear in the view's navigation.">
        <Badge tone="warning">Not in any {viewLabel(view.kind)} index</Badge>
      </span>
    </Show>
  );

  // ---- the skill projection's Markdown for this unit ----
  // The skill view builds a folder, not a site — same preview machinery,
  // mounted as a file listing instead of an iframe.
  const skillPreview = createPreview(
    () => {
      const sv = skillView();
      return sv ? { entry: sv.entry, site: sv.site, skill: true } : null;
    },
    { active: () => activeTab() === 'skill' || activeTab() === 'all' },
  );
  const [skillText, setSkillText] = createSignal(null);
  createEffect(() => {
    void skillPreview.reloadSeq();
    const list = skillPreview.files();
    setSkillText(null);
    if (!list.length) return;
    const want = `${slug()}.md`;
    const file =
      list.find((f) => f === `references/${want}`) ??
      list.find((f) => f.endsWith(`/${want}`) || f === want);
    if (!file) {
      setSkillText(false); // not part of the skill
      return;
    }
    fetch(`${skillPreview.base()}${file}`)
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(r.statusText))))
      .then(setSkillText)
      .catch((e) => setSkillText(`(failed to load: ${e})`));
  });

  /** The previews a commit made here must refresh: the tab on screen, or
      every view tab on the All tab. */
  const liveTabPreviews = () => {
    if (activeTab() === 'all') return viewList.map((v) => viewPreviews.get(v.id));
    if (activeTab() === 'merged') return mergedView() ? [merged] : [];
    return previewFor(currentView()) ? [previewFor(currentView())] : [];
  };
  const buildingPreview = () =>
    merged.building() || skillPreview.building() || liveTabPreviews().some((p) => p.building());
  /** A build that failed over an ALREADY-MOUNTED tab: the surface keeps the
      last good page (a failed build leaves `src` alone), so its own error
      state never renders and nothing else here would say so. */
  const failedPreview = () => liveTabPreviews().find((p) => p.error() && p.src()) ?? null;

  // ---- route the commit loop here while the modal is open ----
  // Modal-scoped, not surface-scoped: the Code / All / Skill tabs commit
  // too, and no editable surface is mounted on them. A mounted surface
  // pushes its own on top of this one (the stack in state/design.js), so
  // while a tab shows an EditSurface the commit tail goes through that.
  const dropRebuild = pushSurfaceRebuild({
    // Read at commit time: which tab is on screen decides what stays fresh.
    get id() {
      return liveTabPreviews().map((p) => p.id);
    },
    run: async ({ changed }) => {
      const targets = liveTabPreviews();
      let last = { ok: true };
      for (const p of targets) {
        last = await p.build({ changed });
        if (!last.ok) break;
      }
      if (last.ok) reloadGraph({ keepPositions: true });
      // Only a view/merged tab's EditSurface reload releases the busy gate
      // itself.
      if (activeTab() === 'all' || targets.length === 0) frameReady();
      return last;
    },
  });
  // Selection, session and the borrowed page are the mounted surface's own
  // to restore (EditSurface's mount scope).
  onCleanup(dropRebuild);

  // ---- whole-unit visibility ----
  // Two mechanisms govern a top-level unit: the wskill `audience` field
  // routes it into the book (`!= :ai`) and the skill (`!= :book`), while
  // `@except(sites = […])` hides it per view on top. Toggling a book /
  // skill chip therefore edits the audience (widening to `:both`, narrowing
  // to the other side) and only falls back to `@except` when the audience
  // alone can't express the change (hiding the last routed view); other
  // views (deck / training) stay pure `@except` toggles.
  const toggleUnitView = async (block, site) => {
    if (block.visibility?.custom) {
      toast('Custom visibility — edit this block as source', { duration: 4000 });
      return;
    }
    const kind = siteKindOf(site);
    const on = block.views?.[site] !== false;
    const except = new Set(block.visibility?.except_sites ?? []);
    const ops = [];
    const audienceOp = (value) => ({
      op: 'set_field',
      span: block.span,
      field: 'audience',
      expr: `:${value}`,
    });
    const exceptOp = () => ({
      op: 'set_visibility',
      span: block.span,
      except_sites: [...except],
    });
    if (kind === 'book' || kind === 'ai_skill') {
      const side = kind === 'book' ? 'book' : 'ai';
      const other = kind === 'book' ? 'ai' : 'book';
      const aud = block.audience ?? 'book';
      if (on) {
        if (aud === 'both') {
          ops.push(audienceOp(other));
        } else {
          except.add(site);
          ops.push(exceptOp());
        }
      } else {
        if (aud === other) ops.push(audienceOp('both'));
        if (except.has(site)) {
          except.delete(site);
          ops.push(exceptOp());
        }
      }
    } else if (node()?.type === 'index') {
      toast('Indexes only shape the book and skill views', { duration: 4000 });
      return;
    } else {
      if (except.has(site)) except.delete(site);
      else except.add(site);
      ops.push(exceptOp());
    }
    if (ops.length === 0) return;
    await commitOps(block.file, ops, { reveal: null });
  };

  const mergedGutter = () => ({
    merged: true,
    currentSite: mergedView() ? mergedView().site : undefined,
  });

  /** One tab's editing surface. `preview` is an ACCESSOR: a non-keyed `Show`
      builds its children once, so switching between two view tabs has to
      reach the new preview through the props, not through a captured one.
      A build that failed with nothing yet mounted shows the error and a
      Retry here (this modal has no Rebuild button, so an error with nothing
      to press is an indefinite "Building preview…"); one that failed OVER a
      mounted page keeps that page, and the head bar's `failedPreview` strip
      is what reports it. */
  const previewSurface = (preview, { building, gutter, site } = {}) => (
    <EditSurface
      preview={preview()}
      hideChrome
      gutter={gutter?.()}
      site={site?.()}
      fallback={
        <div class="ed-empty">
          <Show when={preview()?.error()} fallback={building ?? 'Building preview…'}>
            <p>The preview failed to build: {preview().error()}</p>
            <Button size="sm" onClick={() => preview().build()}>
              Retry
            </Button>
          </Show>
        </div>
      }
    />
  );

  // Not every unit has a body to render standalone (some kinds keep their
  // content in fields) — the merged tab then falls back to the graph
  // payload's block list: one row per content block with a drag handle
  // (re-order via batched `move` ops) and a profile button popping up the
  // visibility editor, mirroring the rendered gutter.
  const [dragIdx, setDragIdx] = createSignal(null);
  const [dropIdx, setDropIdx] = createSignal(null);
  const listReorder = async (from, drop) => {
    const blocks = node()?.blocks ?? [];
    const b = blocks[from];
    const target = drop > from ? drop - 1 : drop;
    if (!b || target === from) return;
    // Span-addressed `move_to` — correct even when the listed blocks span
    // mixed parents (spliced body children beside direct unit children).
    // In place: no iframe exists here — the graph reload inside
    // commitOpsLocal refreshes the rows.
    const ref = drop < blocks.length ? blocks[drop] : null;
    const op = ref
      ? { op: 'move_to', span: b.span, before: ref.span }
      : { op: 'move_to', span: b.span, after: blocks[blocks.length - 1].span };
    await commitOpsLocal(b.file, [op]);
  };

  const mergedBlockList = () => (
    <div class="ed-merged-list">
      <p class="ed-content-hint">
        No view builds a page for this unit and it carries no body content to render
        standalone — its blocks are listed here: drag to re-order, and the ◐ button
        picks the profiles each block shows in.
      </p>
      <Show
        when={(node()?.blocks ?? []).length}
        fallback={
          <div class="ed-empty">
            This unit has no content blocks — its content lives in fields. Use the
            whole-unit chips on the left.
          </div>
        }
      >
        <For each={node()?.blocks ?? []}>
          {(b, i) => (
            <div
              class="ed-merged-row"
              classList={{
                'is-dropline': dropIdx() === i(),
                'is-dropline-end':
                  dropIdx() === (node()?.blocks ?? []).length &&
                  i() === (node()?.blocks ?? []).length - 1,
                'is-dragging': dragIdx() === i(),
              }}
              onDragOver={(e) => {
                if (dragIdx() == null) return;
                e.preventDefault();
                const r = e.currentTarget.getBoundingClientRect();
                setDropIdx(e.clientY < r.top + r.height / 2 ? i() : i() + 1);
              }}
              onDrop={(e) => {
                e.preventDefault();
                const from = dragIdx();
                const drop = dropIdx();
                setDragIdx(null);
                setDropIdx(null);
                if (from != null && drop != null && !busy()) listReorder(from, drop);
              }}
            >
              <span
                class="ed-merged-handle"
                draggable
                title="Drag to re-order"
                onDragStart={(e) => {
                  if (busy()) {
                    e.preventDefault();
                    return;
                  }
                  setDragIdx(i());
                  e.dataTransfer.effectAllowed = 'move';
                }}
                onDragEnd={() => {
                  setDragIdx(null);
                  setDropIdx(null);
                }}
              >
                ⋮⋮
              </span>
              <span class="ed-merged-label">
                {b.preview ? `${b.kind} — ${b.preview}` : b.kind}
              </span>
              <button
                type="button"
                class="ed-merged-profile"
                classList={{
                  'is-partial': (b.visibility?.except_sites ?? []).length > 0,
                  'is-custom': b.visibility?.custom,
                }}
                title="Set which profiles this block shows in"
                disabled={busy()}
                onClick={() =>
                  setPopover({ type: 'visibility', anchor: { file: b.file, span: b.span } })
                }
              >
                ◐
              </button>
            </div>
          )}
        </For>
      </Show>
    </div>
  );

  const unitRow = () => {
    const n = node();
    return (
      n && {
        kind: n.kind,
        file: n.file,
        span: n.span,
        views: n.views,
        visibility: n.visibility,
        audience: n.audience,
      }
    );
  };

  // ---- index placement (where the unit is pinned, per view) ----
  // Index ids are document-wide unique, so a pin/unpin reaches any view's
  // index from here — no view switch, no entry juggling. Writes go through
  // the quiet nav-op path (id-addressed, immune to the reformats) and
  // refetch the graph keeping positions, exactly like the index panel.
  const placeOp = async (payload, msg) => {
    const res = await commitNavOpQuiet(payload);
    if (!res.ok) return;
    toast(msg, { duration: 3000 });
    reloadGraph({ keepPositions: true });
  };
  const pinInto = (level, unitId) =>
    placeOp(
      { op: 'pin_unit', index_id: level.id, unit_id: unitId },
      `Pinned into “${level.title}”`,
    );
  const unpinFrom = (level, unitId) =>
    placeOp(
      { op: 'unpin_unit', index_id: level.id, unit_id: unitId },
      `Unpinned from “${level.title}”`,
    );

  const tabOptions = () => [
    ...(viewList.length ? [{ value: 'merged', label: 'Merged' }] : []),
    ...viewList.map((v) => ({ value: v.id, label: viewLabel(v.kind) })),
    ...(skillView() ? [{ value: 'skill', label: 'Skill' }] : []),
    { value: 'code', label: 'Code' },
    { value: 'all', label: 'All' },
  ];

  // ---- the Code tab: the unit's own WCL source ----
  // Read from disk on open (and after every save) rather than from the graph
  // payload: a commit reprints the whole file, so the span moves and the text
  // has to come back with it. Saving goes through the same `replace_source`
  // op the fragment editor uses.
  // Unit deletion moved here from the graph's side panel (which a node click
  // now replaces). The work itself stays in GraphView — it strips inbound
  // links and pins across files and needs the whole payload — so this only
  // confirms and calls back.
  const [confirmDelete, setConfirmDelete] = createSignal(false);

  const [codeSeq, setCodeSeq] = createSignal(0);
  const [codeSrc] = createResource(
    () => (activeTab() === 'code' && node() ? { n: node(), seq: codeSeq() } : null),
    (k) => api.blockSource({ file: k.n.file, span: k.n.span }),
  );
  const [draft, setDraft] = createSignal(null);
  // A fresh fetch discards an unsaved draft — the text it was based on is gone.
  createEffect(() => {
    if (codeSrc()?.ok) setDraft(codeSrc().source);
  });

  const saveCode = async () => {
    const s = codeSrc();
    const text = draft();
    if (!s?.ok || text == null || text === s.source) return;
    const res = await commitOps(
      node().file,
      [{ op: 'replace_source', span: node().span, source: text }],
      { etag: s.etag, reveal: 'edited' },
    );
    if (res?.ok) {
      toast('Saved', { duration: 2000 });
      setCodeSeq((n) => n + 1);
      reloadGraph({ keepPositions: true });
    }
  };

  const codePane = () => (
    <Show
      when={codeSrc()?.ok}
      fallback={
        <div class="ed-empty">
          {codeSrc.loading ? 'Reading the source…' : (codeSrc()?.error ?? 'No source for this unit.')}
        </div>
      }
    >
      <div class="ed-content-code">
        <div class="ed-content-code-head">
          <code>{node()?.file}</code>
          <span class="spacer" />
          <Button
            size="sm"
            variant="primary"
            disabled={busy() || draft() === codeSrc().source}
            onClick={saveCode}
          >
            Save
          </Button>
        </div>
        <CodeEditor
          value={draft() ?? ''}
          onChange={setDraft}
          language={wclLanguage}
          height="100%"
        />
      </div>
    </Show>
  );

  const skillPane = () => (
    <Show
      when={skillText() !== null}
      fallback={<div class="ed-empty">Building the skill projection…</div>}
    >
      <Show
        when={skillText() !== false}
        fallback={
          <div class="ed-empty">
            This unit isn't part of the skill projection (audience is book-only).
          </div>
        }
      >
        <pre class="ed-skill-text">{skillText()}</pre>
      </Show>
    </Show>
  );

  return (
    <Modal
      open
      onClose={props.onClose}
      /* The kind rides in the title because the graph's side panel — where it
         showed as a badge — is covered while this is open, and several kinds
         (concept / fact / procedure / lesson) read alike from the title alone.
         A string, not JSX: Forge uses `title` for the panel's aria-label too. */
      title={`${node()?.title ?? ''}${node()?.kind ? ` (${node().kind})` : ''} — content & visibility`}
      footer={
        <>
          <Show when={node()?.type === 'unit' && props.onOpenPage}>
            <Button
              size="sm"
              disabled={props.opening || busy()}
              onClick={() => props.onOpenPage(node())}
            >
              {props.opening ? 'Opening…' : 'Open page'}
            </Button>
          </Show>
          <Show when={props.onOpenCode}>
            <Button size="sm" onClick={() => props.onOpenCode(node())}>
              <FileCode2 size={13} /> Open in code mode
            </Button>
          </Show>
          <Show when={node()?.type === 'unit' && props.onDelete}>
            <Show
              when={confirmDelete()}
              fallback={
                <Button
                  size="sm"
                  variant="danger"
                  disabled={busy() || props.deleting}
                  onClick={() => setConfirmDelete(true)}
                >
                  <Trash2 size={13} /> Delete unit…
                </Button>
              }
            >
              <span class="ed-content-confirm">
                Delete “{node().title}” and remove its pins/links? (recoverable via git)
                <Button
                  size="sm"
                  variant="danger"
                  disabled={props.deleting}
                  onClick={() => props.onDelete(node())}
                >
                  {props.deleting ? 'Deleting…' : 'Delete'}
                </Button>
                <Button size="sm" disabled={props.deleting} onClick={() => setConfirmDelete(false)}>
                  Keep
                </Button>
              </span>
            </Show>
          </Show>
          <span class="spacer" />
          <Button onClick={props.onClose}>Close</Button>
        </>
      }
    >
      <Show when={node()} fallback={<div class="ed-empty">The unit is gone — reload the graph.</div>}>
        <div class="ed-content-modal">
          <div class="ed-content-side">
            {/* A procedure's `from -> to` wiring. Not draggable on the canvas:
                its chart is repeater-generated, so the shapes share one span
                and the statements live here, on the unit block. */}
            <Show when={node()?.kind === 'procedure'}>
              <FlowPanel
                file={node().file}
                span={node().span}
                steps={(node().blocks ?? [])
                  .filter((b) => b.kind === 'step' && b.preview)
                  .map((b) => ({ id: b.preview, file: b.file, span: b.span }))}
                onChange={() => reloadGraph({ keepPositions: true })}
              />
            </Show>
            <div class="ed-content-blocks">
              <ViewToggles label="whole unit" block={unitRow()} sites={sites()} onToggle={toggleUnitView} />
              <p class="ed-content-hint">
                The Merged tab shows every block with per-view chips on its left — click a
                chip to toggle that view. Click a block to edit it; its toolbar carries the
                visibility eye too.
              </p>
            </div>
            <Show when={node()?.type === 'unit'}>
              <IndexPlacement node={node()} onPin={pinInto} onUnpin={unpinFrom} />
            </Show>
          </div>
          <div class="ed-content-preview">
            <div class="ed-content-preview-head">
              <ToggleGroup options={tabOptions()} value={activeTab()} onChange={setTab} />
              <Show when={currentView()}>{unindexedBadge(currentView())}</Show>
              <Show when={activeTab() === 'merged' && mergedView()}>
                <span title="Every block renders here regardless of visibility — the chips left of each block show and toggle the views it appears in.">
                  <Badge>All blocks — {viewLabel(mergedView().kind)} layout</Badge>
                </span>
              </Show>
              <Show when={buildingPreview() || busy()}>
                <Spinner size={12} label="Building preview" />
              </Show>
              <Show when={failedPreview()}>
                <span class="err">Build failed: {failedPreview().error()}</span>
                <Button size="sm" onClick={() => failedPreview().build()}>
                  Retry
                </Button>
              </Show>
            </div>
            <Show when={activeTab() === 'merged'}>
              <Show when={mergedView() !== false} fallback={mergedBlockList()}>
                {previewSurface(() => merged, {
                  building: 'Building the merged preview…',
                  gutter: mergedGutter,
                })}
              </Show>
            </Show>
            <Show when={currentView()}>
              <Show
                when={!noPageIn(currentView())}
                fallback={
                  <div class="ed-empty">
                    This unit has no page in the {viewLabel(currentView().kind)} view.
                  </div>
                }
              >
                {previewSurface(() => previewFor(currentView()), {
                  site: () => currentView().site,
                })}
              </Show>
            </Show>
            <Show when={activeTab() === 'skill'}>{skillPane()}</Show>
            <Show when={activeTab() === 'code'}>{codePane()}</Show>
            <Show when={activeTab() === 'all'}>
              <div class="ed-content-all">
                <For each={viewList}>
                  {(v) => (
                    <div class="ed-content-all-col">
                      <div class="ed-content-all-head">
                        {viewLabel(v.kind)}
                        {unindexedBadge(v)}
                      </div>
                      <Show
                        when={!noPageIn(v)}
                        fallback={<div class="ed-empty">No page in this view.</div>}
                      >
                        <Show
                          when={srcFor(v)}
                          fallback={<div class="ed-empty">Building…</div>}
                        >
                          <iframe
                            src={srcFor(v)}
                            title={`${viewLabel(v.kind)} preview`}
                            onLoad={(e) => injectBareCss(e.currentTarget.contentDocument)}
                          />
                        </Show>
                      </Show>
                    </div>
                  )}
                </For>
                <Show when={skillView()}>
                  <div class="ed-content-all-col">
                    <div class="ed-content-all-head">Skill</div>
                    {skillPane()}
                  </div>
                </Show>
              </div>
            </Show>
          </div>
        </div>
      </Show>
    </Modal>
  );
}

/** One row: a block label plus a per-view on/off chip per site. */
function ViewToggles(props) {
  return (
    <div
      class="ed-graph-blockrow"
      title={props.block?.visibility?.custom ? 'custom visibility — edit as source' : undefined}
    >
      <span class="ed-graph-blocklabel">{props.label}</span>
      <span class="ed-graph-toggles">
        <For each={props.sites}>
          {(site) => (
            <button
              type="button"
              class="ed-graph-viewtoggle"
              classList={{
                'is-on': props.block?.views?.[site] !== false,
                'is-custom': props.block?.visibility?.custom,
              }}
              disabled={busy()}
              title={`${viewLabel(siteKindOf(site))} (${site}) — click to toggle`}
              onClick={() => props.onToggle(props.block, site)}
            >
              {site.charAt(0).toUpperCase()}
            </button>
          )}
        </For>
      </span>
    </div>
  );
}

/** Where the unit sits in each view's navigation: one group per view site,
    listing every index level whose own `related` list pins it — nested
    sub-indexes shown as their "Top › Sub" trail — with a − to unpin, and a
    select of that view's remaining index levels to pin it into. Views whose
    navigation is structural (the training course, a deck) have nothing to
    pin: their levels come back read-only and the group says so. */
function IndexPlacement(props) {
  const sites = () => graphData()?.sites ?? [];
  return (
    <div class="ed-content-places">
      <strong>Index placement</strong>
      <For each={sites()}>
        {(site) => (
          <PlacementView
            site={site}
            node={props.node}
            onPin={props.onPin}
            onUnpin={props.onUnpin}
          />
        )}
      </For>
      <Show when={sites().length === 0}>
        <div class="ed-place-empty">no views to place this unit in</div>
      </Show>
    </div>
  );
}

function PlacementView(props) {
  const levels = createMemo(() => indexLevelsForSite(graphData(), props.site));
  const pinnedIn = createMemo(() => levels().filter((l) => l.pinned.includes(props.node.id)));
  const openLevels = createMemo(() =>
    levels().filter((l) => !l.syllabus && l.editable && !l.pinned.includes(props.node.id)),
  );
  // Placed by the view's own data-driven navigation (a lesson in the
  // course, the deck's presentation) rather than by any index.
  const organized = () => (props.node.organized ?? []).includes(props.site);
  const hidden = () => props.node.views?.[props.site] === false;

  return (
    <div class="ed-place-view">
      <div class="ed-place-head">
        <span class="ed-place-label">{viewLabel(siteKindOf(props.site))}</span>
        <span class="ed-place-site">{props.site}</span>
        <Show when={hidden()}>
          <span title="The unit itself is hidden from this view — its index entries won't render.">
            <Badge tone="warning">hidden</Badge>
          </span>
        </Show>
      </div>
      <For each={pinnedIn()}>
        {(l) => (
          <div class="ed-place-row">
            <span class="ed-place-path" title={l.id}>
              {l.path}
            </span>
            <Show
              when={!l.syllabus && l.editable}
              fallback={
                <span class="ed-place-note">
                  {l.syllabus ? 'by course order' : 'computed list'}
                </span>
              }
            >
              <IconButton
                icon={Minus}
                label={`Unpin from ${l.title}`}
                disabled={busy()}
                onClick={() => props.onUnpin(l, props.node.id)}
              />
            </Show>
          </div>
        )}
      </For>
      <Show when={pinnedIn().length === 0}>
        <div class="ed-place-empty">
          {organized()
            ? "placed by this view's own navigation"
            : levels().length === 0
              ? 'this view has no indexes'
              : 'not in any index'}
        </div>
      </Show>
      <Show when={openLevels().length > 0}>
        <Select
          options={openLevels().map((l) => ({ value: l.id, label: l.path }))}
          placeholder="Pin into…"
          value={undefined}
          disabled={busy()}
          onChange={(id) => props.onPin(levels().find((l) => l.id === id), props.node.id)}
        />
      </Show>
    </div>
  );
}

/** Best-effort site → artifact kind mapping for tooltips. */
function siteKindOf(site) {
  const hit = (selected()?.views ?? []).find((v) => v.site === site);
  return hit?.kind ?? site;
}
