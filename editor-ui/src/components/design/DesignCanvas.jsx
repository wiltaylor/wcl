/* The Design-mode canvas: the main edit-mode preview build hosted in the
   shared EditSurface (iframe + selection/session layer + block toolbar),
   plus the canvas chrome — the Canvas|Graph tabs, the wskill view tabs,
   nav-driven page navigation, and the graph-staleness rebuild. */

import { Show, createEffect, createSignal } from 'solid-js';
import { FileCode2 } from 'lucide-solid';
import { Button, Spinner, Tabs, ToggleGroup } from '@forge/ui';

import {
  activeSite,
  activeView,
  buildSeq,
  previewHref,
  rebuild,
  selectView,
  selected,
  viewLabel,
} from '../../state/sites';
import {
  busy,
  canvasStale,
  currentPage,
  designTab,
  gotoPage,
  loadNav,
  loadPalette,
  openPageSource,
  palette,
  setCanvasStale,
  setDesignTab,
  setGotoPage,
  setPopover,
  SURFACE_CANVAS,
} from '../../state/design';
import EditSurface from './EditSurface';
import SkillBrowser from './SkillBrowser';

export { viewLabel } from '../../state/sites';

export default function DesignCanvas() {
  /** The mounted surface's handle (null while unmounted) — a signal, so a
      navigation requested before the first build lands re-runs once the
      surface is there. */
  const [surface, setSurface] = createSignal(null);

  /** The canvas's preview: the main site build. */
  const preview = { src: previewHref, reloadSeq: buildSeq };

  // Graph-mode commits leave the canvas anchors stale — rebuild when the
  // canvas becomes the active surface again, targeting the canvas's own
  // page (other pages materialize lazily server-side when navigated to).
  createEffect(() => {
    if (designTab() === 'canvas' && canvasStale()) {
      setCanvasStale(false);
      const page = currentPage()?.name;
      rebuild(page ? { pages: [page] } : {});
    }
  });

  // NavPanel / graph "Open page" navigation: swap the iframe to the
  // requested page. A goto arriving before the first canvas build lands
  // (previewHref still null) stays PENDING — clearing it here used to
  // drop the navigation and leave the freshly built front page instead;
  // the effect re-runs once previewHref arrives and performs it then.
  createEffect(() => {
    const page = gotoPage();
    if (!page) return;
    const base = previewHref();
    const s = surface();
    if (!base || !s) return;
    setGotoPage(null);
    s.goto(`${base.slice(0, base.lastIndexOf('/') + 1)}${page}.html`);
  });

  return (
    <div class="ed-design-col">
      <div class="ed-design-note">
        {/* The second surface depends on the document: a wskill has a unit
            graph, a WAD has its systems model, anything else has neither. */}
        <ToggleGroup
          options={[
            { value: 'canvas', label: 'Canvas' },
            ...(palette()?.wad && !selected()?.wskill
              ? [{ value: 'systems', label: 'Systems' }]
              : [{ value: 'graph', label: 'Graph', disabled: !selected()?.wskill }]),
          ]}
          value={designTab()}
          onChange={(t) => setDesignTab(t)}
        />
        <Show when={selected()?.wskill}>
          <Tabs
            tabs={(selected().views ?? []).map((v) => ({
              id: v.id,
              label: viewLabel(v.kind),
            }))}
            active={activeView()?.id}
            onChange={async (id) => {
              selectView(id);
              await rebuild();
              loadNav();
              loadPalette();
            }}
          />
        </Show>
        <span class="ed-design-page">{currentPage()?.name ?? 'no page'}</span>
        <Show when={busy()}>
          <Spinner size={12} label="Applying edit" />
          <span>saving…</span>
        </Show>
        <span class="spacer" />
        <Show when={selected()?.wskill}>
          <Button size="sm" onClick={() => setPopover({ type: 'profiles' })}>
            Profiles
          </Button>
        </Show>
        <Button size="sm" onClick={openPageSource} disabled={!currentPage()}>
          <FileCode2 size={13} /> Open code
        </Button>
      </div>
      <Show
        when={!activeView()?.skill}
        fallback={
          <div class="ed-design-canvas">
            <SkillBrowser />
          </div>
        }
      >
        <EditSurface
          preview={preview}
          surfaceId={SURFACE_CANVAS}
          site={activeSite()}
          ref={setSurface}
          fallback={
            <div class="ed-empty">
              {selected() ? 'Building the design canvas…' : 'No wdoc sites found in this directory'}
            </div>
          }
        />
      </Show>
    </div>
  );
}
