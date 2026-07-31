/* The screen surface editor: the screen's `body` — its wireframe diagram or
   terminal mock-up — rendered live in the same WYSIWYG EditSurface as the
   design canvas, via the merged synthetic-unit-page preview (the projected
   blocks keep their REAL file/span anchors, so drag/resize, the add-shape
   palette, the shape dock and the fragment editor all work natively).

   A wireframe screen also gets a WIDGET PALETTE sidebar: click a widget to
   add it — inside the selected container widget (a browser frame, a
   panel), after a selected leaf widget (stacking order), or into the
   diagram itself when nothing is selected. The palette is the wf_* slice
   of `/api/palette`'s diagram_kinds, so user-declared widgets appear too;
   `accepts_children` (schema-derived) decides append-inside vs
   insert-after.

   While mounted it owns the commit loop (setSurfaceRebuild): a WYSIWYG
   commit rebuilds just the synthetic page, then refreshes the systems
   model and tells the host to re-anchor (every span in the file moved).

   A screen with no body yet gets seed buttons instead — a starter
   wireframe (`diagram { wf_browser … }`) or terminal block. */

import { For, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { Button, Spinner, toast } from '@forge/ui';

import { api } from '../../../api';
import { clientToUser } from '../../../preview/diagram';
import { freshShapeId, shapeSnippet } from '../../../preview/schemaform';
import { markDropTarget, relocateOps, resolveWidgetDrop } from '../../../preview/widgetdnd';
import { anchorOf, diagramIn, isManualLayout, shapeEls } from '../../../preview/anchors';
import WidgetTree from './WidgetTree';
import { startPointerDrag } from './pointerDrag';
import { dirtyFiles } from '../../../state/buffers';
import {
  busy,
  commitOps,
  palette,
  selection,
  setEditingSession,
  setSelection,
  setSurfaceRebuild,
} from '../../../state/design';
import { activeEntry, activeSite } from '../../../state/sites';
import { loadSystems, model } from '../../../state/systems';
import EditSurface from '../EditSurface';
import { createDetail } from './fields';

/** The page name synthetic unit previews build under (preview.rs). */
const UNIT_PAGE = '__wcl_unit_preview';

// Wireframe diagrams need an explicit size — a free-layout diagram renders
// zero-sized without one (every doc example sets width/height too). The
// inner snippets seed INTO an existing (empty) body block; the wrapper is
// added when the screen has no body at all.
const SEED_WIREFRAME = `diagram {
  layout = :free
  width = 720
  height = 500
  wf_browser "https://example.com" {
    x = 20.0
    y = 20.0
  }
}`;

const SEED_TERMINAL = `terminal {
  title = "terminal"
  text = "$ "
}`;

const wrapBody = (inner) =>
  `body {\n${inner
    .split('\n')
    .map((l) => `  ${l}`)
    .join('\n')}\n}`;

/** The last successful synthetic build per (entry, site). Unit previews
    share ONE server-side `__unit` slug, so the first screen pays the full
    book build once and every other screen is a warm single-page rebuild;
    this cache goes one further — re-opening the SAME screen reuses the
    built page with no build at all (a stale page refreshes lazily on GET). */
const builtCache = new Map(); // `${entry}|${site}` → { href, unitId }

/** The palette's curated order; unlisted wf_* kinds land under "More". */
const WIDGET_GROUPS = [
  { title: 'Frames', kinds: ['wf_browser', 'wf_window', 'wf_phone', 'wf_tablet'] },
  { title: 'Layout', kinds: ['wf_panel', 'wf_column', 'wf_row', 'wf_grid'] },
  {
    title: 'Controls',
    kinds: ['wf_label', 'wf_button', 'wf_input', 'wf_dropdown', 'wf_checkbox', 'wf_radio', 'wf_toggle'],
  },
];

export default function ScreenEditor(props) {
  const { detail, saving, commit } = createDetail(() => props.anchor, {
    onAfterCommit: props.onCommitted,
  });
  const d = detail;
  const [href, setHref] = createSignal(null);
  const [reloadSeq, setReloadSeq] = createSignal(0);
  const [building, setBuilding] = createSignal(false);
  const [buildError, setBuildError] = createSignal(null);
  /** Bumped when the frame (re)loads — the structure tree rebuilds on it. */
  const [treeSeq, setTreeSeq] = createSignal(0);
  let inFlight = null;
  /** EditSurface handle: () => the iframe's document. */
  let surfaceDoc = null;

  /** Build from the WAD model root when the payload names one: same
      content and anchors as the site entry, without evaluating the book's
      ~160 template pages first (a minute-class difference on big WADs in
      debug builds). The model root declares no sites, so `site` is omitted
      with it. */
  const previewEntry = () => model()?.model_entry ?? activeEntry();
  const previewSite = () => (model()?.model_entry ? null : activeSite());

  const doBuild = async (changed) => {
    const dd = d();
    const entry = previewEntry();
    if (!dd?.body || !entry) return { ok: false, error: 'no body to render' };
    setBuilding(true);
    setBuildError(null);
    const res = await api.preview(entry, previewSite(), dirtyFiles(), {
      pages: [UNIT_PAGE],
      merged: true,
      changed,
      unit: { kind: dd.kind, id: dd.id },
    });
    setBuilding(false);
    if (!res.ok) {
      // Sticky, with a Retry — a toast alone leaves "Rendering…" forever.
      setBuildError(res.error ?? 'the preview build failed');
      toast(res.error, { tone: 'danger', duration: 6000 });
      return res;
    }
    const page = `${res.href.slice(0, res.href.lastIndexOf('/') + 1)}${UNIT_PAGE}.html`;
    builtCache.set(`${entry}|${previewSite() ?? ''}`, { href: page, unitId: dd.id });
    setHref(page);
    setReloadSeq((s) => s + 1);
    return res;
  };
  const build = (changed = []) => {
    if (inFlight) return inFlight;
    inFlight = doBuild(changed).finally(() => {
      inFlight = null;
    });
    return inFlight;
  };

  const bodyKinds = () => d()?.body?.block_kinds ?? [];
  /** An EMPTY body (a bare `body` from the dock's + child button) is the
      same as none: nothing to render, seed content into it instead. */
  const hasContent = () => bodyKinds().length > 0;

  // First render once the detail lands with body CONTENT (and again after
  // a seed commit fills it). Re-opening the screen last built reuses the
  // page on disk without any build.
  createEffect(() => {
    const dd = d();
    if (!dd?.body || !hasContent() || href()) return;
    const hit = builtCache.get(`${previewEntry()}|${previewSite() ?? ''}`);
    if (hit && hit.unitId === dd.id) {
      setHref(hit.href);
      setReloadSeq((s) => s + 1);
    } else {
      build();
    }
  });

  // Own the commit loop while mounted: a WYSIWYG commit inside the iframe
  // rebuilds the synthetic page, then the model + host re-anchor (the
  // reformat moved every span this editor and its host hold).
  setSurfaceRebuild(async ({ changed }) => {
    const res = await build(changed);
    if (res.ok) {
      await loadSystems({ keep: true });
      await props.onCommitted?.();
    }
    return res;
  });
  onCleanup(() => {
    setSurfaceRebuild(null);
    setSelection(null);
    setEditingSession(null);
  });

  const isWireframe = () => bodyKinds().includes('diagram');
  const kindsBadge = () => {
    if (bodyKinds().includes('terminal')) return 'terminal mock-up';
    if (isWireframe()) return 'wireframe';
    return null;
  };

  /** Seed content: into the existing (empty) body when there is one,
      wrapped in a fresh `body { … }` otherwise. */
  const seed = (inner, what) => {
    const body = d().body;
    const op = body
      ? { op: 'insert_child', span: body.span, index: 9999, source: inner }
      : { op: 'insert_child', span: d().span, index: 9999, source: wrapBody(inner) };
    return commit([op], `Seeded a ${what}`);
  };

  // ---- the widget palette ------------------------------------------
  const widgetEntries = () =>
    (palette()?.diagram_kinds ?? []).filter((k) => k.kind.startsWith('wf_'));
  const entryFor = (kind) => widgetEntries().find((k) => k.kind === kind);
  const ungrouped = () => {
    const named = new Set(WIDGET_GROUPS.flatMap((g) => g.kinds));
    return widgetEntries().filter((k) => !named.has(k.kind));
  };
  const acceptsChildren = (kind) =>
    (palette()?.diagram_kinds ?? []).find((k) => k.kind === kind)?.accepts_children === true;

  /** Where a new widget goes: inside the selected container widget, after
      a selected leaf widget (stacking order), else inside the wireframe
      diagram itself. */
  const insertTarget = () => {
    const sel = selection();
    if (sel?.shape) {
      return { anchor: sel, mode: acceptsChildren(sel.kind) ? 'inside' : 'after' };
    }
    if (sel?.kind === 'diagram') return { anchor: sel, mode: 'inside' };
    const doc = surfaceDoc?.();
    const svg = doc && diagramIn(doc);
    const anchor = svg && anchorOf(doc, svg);
    return anchor ? { anchor, mode: 'inside' } : null;
  };

  const targetLabel = () => {
    const t = insertTarget();
    if (!t) return 'the wireframe';
    if (t.mode === 'after') return `after the ${t.anchor.kind}`;
    return t.anchor.kind === 'diagram' ? 'the wireframe' : `inside the ${t.anchor.kind}`;
  };

  /** Insert `entry` at a resolved target — shared by click-to-add (target
      from the selection) and palette drops (target from the drop point;
      `at` places the widget there on a manual-layout diagram, `slot` a
      resolved layout-guide zone — a grid cell / row gap — inserts at that
      position instead of appending). */
  const insertWidget = (entry, t, at = null, slot = null) => {
    // Widget ids share the screen block's space — scan its whole source.
    const uid = freshShapeId(entry.kind, d()?.source ?? '');
    // Children stack inside containers; only top-of-diagram adds under a
    // manual layout get coordinates (the drop point, else the stagger).
    const manual =
      t.mode === 'inside' && t.anchor.kind === 'diagram' && isManualLayout(t.anchor.layout);
    const index = shapeEls(surfaceDoc?.()).length;
    const snippet = shapeSnippet(entry, { uid, manual, index, at: manual ? at : null });
    const op =
      t.mode === 'inside'
        ? { op: 'insert_child', span: t.anchor.span, index: slot ?? 9999, source: snippet }
        : { op: 'insert_after', span: t.anchor.span, source: snippet };
    commitOps(t.anchor.file, [op], { reveal: 'inserted' });
  };

  const addWidget = (entry) => {
    const t = insertTarget();
    if (!t) {
      toast("The wireframe hasn't rendered yet", { duration: 4000 });
      return;
    }
    insertWidget(entry, t);
  };

  /** A page point translated into the iframe's coordinate space, or null
      when it isn't over the canvas. */
  let frameWrap;
  const framePoint = (p) => {
    const iframe = frameWrap?.querySelector('iframe');
    if (!iframe) return null;
    const r = iframe.getBoundingClientRect();
    if (p.x < r.left || p.x > r.right || p.y < r.top || p.y > r.bottom) return null;
    return { x: p.x - r.left, y: p.y - r.top };
  };

  /** The drop the cursor is over: hit-test inside the iframe, resolve with
      the shared semantics. Carries the iframe-space point for coordinates. */
  const dropAt = (p) => {
    const frameDoc = surfaceDoc?.();
    const fp = framePoint(p);
    if (!frameDoc || !fp) return null;
    const t = resolveWidgetDrop(frameDoc.elementFromPoint?.(fp.x, fp.y), acceptsChildren);
    return t ? { ...t, point: fp } : null;
  };

  /** Palette drags are POINTER-based (capture on the button; events keep
      firing over the iframe) — native HTML5 drag into the iframe proved
      unreliable in real browsers. A below-threshold release is the click. */
  const beginPaletteDrag = (entry, e) => {
    if (busy()) return;
    startPointerDrag(e, {
      chipText: entry.kind.slice(3).replaceAll('_', ' '),
      onClick: () => addWidget(entry),
      onMove: (p) => {
        const t = dropAt(p);
        markDropTarget(surfaceDoc?.(), t?.el ?? null, t?.cellEl ?? null);
      },
      onCancel: () => markDropTarget(surfaceDoc?.(), null),
      onDrop: (p) => {
        const frameDoc = surfaceDoc?.();
        markDropTarget(frameDoc, null);
        const t = dropAt(p);
        if (!t) return;
        const anchor = frameDoc && anchorOf(frameDoc, t.el);
        if (!anchor) return;
        const at = t.mode === 'diagram' ? clientToUser(t.el, t.point.x, t.point.y) : null;
        insertWidget(
          entry,
          { anchor, mode: t.mode === 'after' ? 'after' : 'inside' },
          at,
          t.slot ?? null,
        );
      },
    });
  };

  const widgetButton = (entry) => (
    <button
      type="button"
      class="ed-surface-widget"
      title={`${entry.doc ?? entry.kind} — click to add, or drag onto the canvas`}
      disabled={busy()}
      onPointerDown={(e) => beginPaletteDrag(entry, e)}
    >
      {entry.kind.slice(3).replaceAll('_', ' ')}
    </button>
  );

  /** A structure-tree drop: the same structural-move batch as canvas
      drags — insert the widget's slice at the target, delete the original. */
  const relocateNode = async (srcNode, target) => {
    const frameDoc = surfaceDoc?.();
    const a = frameDoc && anchorOf(frameDoc, srcNode.el);
    const t = frameDoc && anchorOf(frameDoc, target.el);
    if (!a || !t) return;
    if (a.shared || t.shared) {
      return toast('Generated content — edit its source data instead', { duration: 5000 });
    }
    if (a.file !== t.file) {
      return toast('Cannot move a widget across files — edit the source instead', {
        duration: 5000,
      });
    }
    const src = await api.blockSource({ file: a.file, span: a.span });
    if (!src.ok) return toast(src.error, { tone: 'danger', duration: 5000 });
    const ops = relocateOps({
      slice: src.source,
      mode: target.mode,
      targetSpan: t.span,
      sourceSpan: a.span,
      slot: target.slot ?? null,
    });
    commitOps(a.file, ops, { etag: src.etag, reveal: 'inserted' });
  };

  return (
    <Show when={d()} fallback={<div class="ed-empty">Loading the screen…</div>}>
      <Show
        when={d().body && hasContent()}
        fallback={
          <div class="ed-surface-seed">
            <p>
              {d().body
                ? 'This screen has an empty body — seed some content to start drawing.'
                : 'This screen has no body yet — seed one to start drawing.'}{' '}
              A wireframe is a `diagram` of draggable widgets; a terminal holds a text
              mock-up.
            </p>
            <div class="ed-detail-actions">
              <Button disabled={busy() || saving()} onClick={() => seed(SEED_WIREFRAME, 'wireframe')}>
                Seed a wireframe
              </Button>
              <Button disabled={busy() || saving()} onClick={() => seed(SEED_TERMINAL, 'terminal')}>
                Seed a terminal
              </Button>
            </div>
          </div>
        }
      >
        <div class="ed-surface-screen">
          <div class="ed-surface-screenhead">
            <Show when={kindsBadge()}>
              <span class="ed-surface-note">{kindsBadge()}</span>
            </Show>
            <span class="ed-surface-hint">
              Drag widgets from the palette onto the canvas — or drag ones already there to
              move, re-nest and re-order them; corners resize, the dock edits properties.
            </span>
            <span class="spacer" />
            <Show when={building() || busy()}>
              <Spinner size={12} label="Building the preview" />
            </Show>
          </div>
          <div class="ed-surface-screenbody">
            <Show when={isWireframe()}>
              <div class="ed-surface-widgets">
                <div class="ed-surface-widgets-target">Adds {targetLabel()}</div>
                <For each={WIDGET_GROUPS}>
                  {(g) => (
                    <Show when={g.kinds.some(entryFor)}>
                      <div class="ed-surface-widgets-group">{g.title}</div>
                      <For each={g.kinds.map(entryFor).filter(Boolean)}>{widgetButton}</For>
                    </Show>
                  )}
                </For>
                <Show when={ungrouped().length > 0}>
                  <div class="ed-surface-widgets-group">More</div>
                  <For each={ungrouped()}>{widgetButton}</For>
                </Show>
                <Show when={widgetEntries().length === 0}>
                  <div class="ed-empty">No widget kinds in the palette.</div>
                </Show>
              </div>
            </Show>
            <div class="ed-surface-frame" ref={frameWrap}>
              <Show
                when={!buildError()}
                fallback={
                  <div class="ed-empty">
                    <p>The preview build failed: {buildError()}</p>
                    <Button size="sm" onClick={() => build()}>
                      Retry
                    </Button>
                  </div>
                }
              >
                <EditSurface
                  src={href}
                  reloadSeq={reloadSeq}
                  hideChrome
                  surfaceRef={(h) => {
                    surfaceDoc = h.doc;
                  }}
                  onFrameLoad={() => setTreeSeq((s) => s + 1)}
                  fallback={
                    <div class="ed-empty">
                      Rendering the screen… (the first screen opened in a document builds the
                      whole book once — subsequent screens render in seconds)
                    </div>
                  }
                />
              </Show>
            </div>
            <Show when={isWireframe()}>
              <WidgetTree
                doc={() => surfaceDoc?.()}
                seq={treeSeq}
                acceptsChildren={acceptsChildren}
                onRelocate={relocateNode}
              />
            </Show>
          </div>
        </div>
      </Show>
    </Show>
  );
}
