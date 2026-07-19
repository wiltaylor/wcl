/* The graph view's "Content & visibility" modal for one unit: its content
   blocks with per-view visibility toggles on the left, a live preview of
   the unit's rendered page on the right (tab per profile — each tab builds
   that view on demand and shows the unit's page in an iframe), plus the
   wskill profile on/off switches (enable re-scaffolds the view, disable
   removes it — same endpoint as the Design-mode Profiles dialog). */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { Button, Checkbox, Modal, Spinner, ToggleGroup, toast } from '@forge/ui';

import { api } from '../../api';
import { dirtyFiles } from '../../state/buffers';
import { loadSites, selected } from '../../state/sites';
import { busy, commitOpsQuiet, loadNav, loadPalette } from '../../state/design';
import { graphData, reloadGraph } from '../../state/graph';
import { viewLabel } from './DesignCanvas';

const ALL_PROFILES = ['book', 'ai_skill', 'presentation', 'training'];

export default function ContentModal(props) {
  const node = () => graphData()?.nodes.find((n) => n.key === props.nodeKey);
  const sites = () => graphData()?.sites ?? [];
  const views = () => (selected()?.wskill ? (selected().views ?? []) : []);
  // The skill view has no HTML pages — it can't be previewed here.
  const previewViews = () => views().filter((v) => !v.skill);
  const [tab, setTab] = createSignal(null);
  const currentView = () => previewViews().find((v) => v.id === tab()) ?? previewViews()[0] ?? null;

  // Per-view built preview hrefs, invalidated on every visibility commit.
  const [hrefs, setHrefs] = createSignal({});
  const [buildingPreview, setBuildingPreview] = createSignal(false);

  const slug = () => {
    const n = node();
    if (!n) return null;
    const prefix = n.kind === 'procedure' ? 'process' : n.kind;
    return `${prefix}_${n.id}`;
  };

  const ensurePreview = async (view) => {
    if (!view || hrefs()[view.id]) return;
    setBuildingPreview(true);
    const res = await api.preview(view.entry, view.site, dirtyFiles());
    setBuildingPreview(false);
    if (res.ok) setHrefs({ ...hrefs(), [view.id]: res.href });
    else toast(res.error, { tone: 'danger', duration: 6000 });
  };
  createEffect(() => {
    ensurePreview(currentView());
  });

  const previewSrc = () => {
    const v = currentView();
    const href = v && hrefs()[v.id];
    if (!href) return null;
    // A deck has no per-unit pages — show the presentation itself.
    if (v.kind === 'presentation') return href;
    return `${href.slice(0, href.lastIndexOf('/') + 1)}${slug()}.html`;
  };

  // ---- visibility toggles (same set_visibility op as everywhere) ----
  const toggleView = async (block, site) => {
    if (block.visibility?.custom) {
      toast('Custom visibility — edit this block as source', { duration: 4000 });
      return;
    }
    const except = new Set(block.visibility?.except_sites ?? []);
    if (except.has(site)) except.delete(site);
    else except.add(site);
    const res = await commitOpsQuiet(block.file, [
      { op: 'set_visibility', span: block.span, except_sites: [...except] },
    ]);
    if (res.ok) {
      await reloadGraph({ keepPositions: true });
      setHrefs({}); // every built preview is stale now
      ensurePreview(currentView());
    }
  };

  const unitRow = () => {
    const n = node();
    return (
      n && {
        kind: n.kind,
        file: n.file,
        span: n.span,
        views: n.views,
        visibility: n.visibility,
      }
    );
  };

  // ---- profiles (whole views on/off) ----
  const enabled = (kind) => views().some((v) => v.kind === kind);
  const [confirm, setConfirm] = createSignal(null); // kind pending disable
  const [working, setWorking] = createSignal(false);
  const applyProfile = async (kind, enable) => {
    setWorking(true);
    const res = await api.wskillProfile(selected().registry, kind, enable);
    setWorking(false);
    setConfirm(null);
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 8000 });
      return;
    }
    toast(enable ? `Enabled ${viewLabel(kind)}` : `Removed ${viewLabel(kind)}`, {
      tone: 'success',
      duration: 3000,
    });
    // The view set changed: refresh discovery, the design models, the graph.
    setHrefs({});
    setTab(null);
    await loadSites();
    loadNav();
    loadPalette();
    reloadGraph({ keepPositions: true });
  };

  return (
    <Modal
      open
      onClose={props.onClose}
      title={`${node()?.title ?? ''} — content & visibility`}
      footer={<Button onClick={props.onClose}>Close</Button>}
    >
      <Show when={node()} fallback={<div class="ed-empty">The unit is gone — reload the graph.</div>}>
        <div class="ed-content-modal">
          <div class="ed-content-side">
            <div class="ed-content-blocks">
              <ViewToggles label="whole unit" block={unitRow()} sites={sites()} onToggle={toggleView} />
              <For each={node()?.blocks ?? []}>
                {(b) => (
                  <ViewToggles
                    label={b.preview ? `${b.kind} — ${b.preview}` : b.kind}
                    block={b}
                    sites={sites()}
                    onToggle={toggleView}
                  />
                )}
              </For>
              <Show when={(node()?.blocks ?? []).length === 0}>
                <div class="ed-graph-noblocks">no content blocks</div>
              </Show>
            </div>
            <div class="ed-content-profiles">
              <strong>Profiles</strong>
              <For each={ALL_PROFILES}>
                {(kind) => (
                  <div class="ed-profile-row">
                    <Checkbox
                      checked={enabled(kind)}
                      disabled={working() || busy() || (kind === 'book' && enabled('book'))}
                      onChange={(on) => (on ? applyProfile(kind, true) : setConfirm(kind))}
                    >
                      {viewLabel(kind)}
                    </Checkbox>
                    <Show when={confirm() === kind}>
                      <span class="ed-profile-confirm">
                        Delete its <code>wdoc/</code> folder? (recoverable via git)
                        <Button
                          size="sm"
                          variant="danger"
                          disabled={working()}
                          onClick={() => applyProfile(kind, false)}
                        >
                          Remove
                        </Button>
                        <Button size="sm" onClick={() => setConfirm(null)}>
                          Keep
                        </Button>
                      </span>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </div>
          <div class="ed-content-preview">
            <div class="ed-content-preview-head">
              <ToggleGroup
                options={previewViews().map((v) => ({ value: v.id, label: viewLabel(v.kind) }))}
                value={currentView()?.id}
                onChange={setTab}
              />
              <Show when={buildingPreview()}>
                <Spinner size={12} label="Building preview" />
              </Show>
            </div>
            <Show
              when={previewSrc()}
              fallback={
                <div class="ed-empty">
                  {buildingPreview() ? 'Building preview…' : 'No previewable view'}
                </div>
              }
            >
              <iframe src={previewSrc()} title="unit preview" />
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

/** Best-effort site → artifact kind mapping for tooltips. */
function siteKindOf(site) {
  const hit = (selected()?.views ?? []).find((v) => v.site === site);
  return hit?.kind ?? site;
}
