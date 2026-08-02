/* The Design-mode canvas: the main preview hosted in the shared EditSurface
   (iframe + selection/session layer + block toolbar), plus the canvas
   chrome — the Canvas|Graph tabs, the wskill view tabs, and nav-driven page
   navigation.

   The build itself is the main preview's (state/preview.js): mounting marks
   it active, so it rebuilds itself whenever a commit elsewhere left it
   stale, and re-targets when the selected site or view changes. */

import { Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import { FileCode2 } from 'lucide-solid';
import { Button, Spinner, Tabs, ToggleGroup } from '@forge/ui';

import { activeSite, activeView, selectView, selected, viewLabel } from '../../state/sites';
import { currentPage, mainPreview, setMainActive } from '../../state/preview';
import { pageHref } from '../../preview/pages';
import {
  busy,
  designTab,
  designTabOptions,
  gotoPage,
  loadNav,
  loadPalette,
  openPageSource,
  setDesignTab,
  setGotoPage,
  setPopover,
} from '../../state/design';
import EditSurface from './EditSurface';
import SkillBrowser from './SkillBrowser';

export { viewLabel } from '../../state/sites';

export default function DesignCanvas() {
  /** The mounted surface's handle (null while unmounted) — a signal, so a
      navigation requested before the first build lands re-runs once the
      surface is there. */
  const [surface, setSurface] = createSignal(null);

  // While the canvas is the shown surface the main preview rebuilds itself
  // on demand (a commit elsewhere, a view switch); off-screen it holds
  // still — the code-mode pane builds only from its Rebuild button.
  onMount(() => setMainActive(true));
  onCleanup(() => setMainActive(false));

  // NavPanel / graph "Open page" navigation: swap the iframe to the
  // requested page. A goto arriving before the first canvas build lands
  // (no href yet) stays PENDING — clearing it here used to drop the
  // navigation and leave the freshly built front page instead; the effect
  // re-runs once the href arrives and performs it then.
  createEffect(() => {
    const page = gotoPage();
    if (!page) return;
    const url = pageHref(mainPreview.href(), page);
    const s = surface();
    if (!url || !s) return;
    setGotoPage(null);
    s.goto(url);
  });

  return (
    <div class="ed-design-col">
      <div class="ed-design-note">
        {/* Which surfaces this document offers is designTabOptions()' — a
            wskill has a unit graph and an audit, a WAD has its systems
            model, anything else has neither. */}
        <ToggleGroup
          options={designTabOptions()}
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
            onChange={(id) => {
              // Retargets the main preview, which builds the new view itself.
              selectView(id);
              loadNav();
              loadPalette();
            }}
          />
        </Show>
        <span class="ed-design-page">{currentPage()?.name ?? 'no page'}</span>
        <Show when={busy() || mainPreview.building()}>
          <Spinner size={12} label="Applying edit" />
          <span>saving…</span>
        </Show>
        {/* A build that failed over an already-mounted page leaves the last
            good render on screen; Design mode hides the topbar's Rebuild, so
            the way back has to be here. */}
        <Show when={mainPreview.error() && mainPreview.src()}>
          <span class="err">Build failed: {mainPreview.error()}</span>
          <Button size="sm" onClick={() => mainPreview.build()}>
            Retry
          </Button>
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
          preview={mainPreview}
          site={activeSite()}
          ref={setSurface}
          fallback={
            <div class="ed-empty">
              <Show
                when={mainPreview.error()}
                fallback={
                  selected()
                    ? 'Building the design canvas…'
                    : 'No wdoc sites found in this directory'
                }
              >
                {/* Sticky, with a Retry: Design mode hides the topbar's
                    Rebuild, so a failed canvas build has no other way back. */}
                <p>Build failed: {mainPreview.error()}</p>
                <Button size="sm" onClick={() => mainPreview.build()}>
                  Retry
                </Button>
              </Show>
            </div>
          }
        />
      </Show>
    </div>
  );
}
