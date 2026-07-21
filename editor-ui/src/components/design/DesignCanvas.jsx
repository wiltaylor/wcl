/* The Design-mode canvas: the main edit-mode preview build hosted in the
   shared EditSurface (iframe + selection/session layer + block toolbar),
   plus the canvas chrome — the Canvas|Graph tabs, the wskill view tabs,
   nav-driven page navigation, and the graph-staleness rebuild. */

import { Show, createEffect } from 'solid-js';
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
  setCanvasStale,
  setDesignTab,
  setGotoPage,
  setPopover,
} from '../../state/design';
import EditSurface from './EditSurface';
import SkillBrowser from './SkillBrowser';

/** Display name for an artifact-kind view tab. */
export function viewLabel(kind) {
  switch (kind) {
    case 'book':
      return 'Book';
    case 'presentation':
      return 'Deck';
    case 'training':
      return 'Training';
    case 'ai_skill':
      return 'Skill';
    default:
      return kind.charAt(0).toUpperCase() + kind.slice(1);
  }
}

export default function DesignCanvas() {
  let surface = null; // { goto(url) } from EditSurface

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
    if (!base || !surface) return;
    setGotoPage(null);
    surface.goto(`${base.slice(0, base.lastIndexOf('/') + 1)}${page}.html`);
  });

  return (
    <div class="ed-design-col">
      <div class="ed-design-note">
        <ToggleGroup
          options={[
            { value: 'canvas', label: 'Canvas' },
            { value: 'graph', label: 'Graph', disabled: !selected()?.wskill },
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
          src={previewHref}
          reloadSeq={buildSeq}
          site={activeSite()}
          onNavigate={(handle) => {
            surface = handle;
          }}
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
