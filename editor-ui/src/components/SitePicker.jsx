/* Topbar site selector for the preview: the discovered site tree flattened
   into an indented Select option list (nesting shows which picks pull in
   whole sub-site trees), plus a refresh button that re-runs discovery.

   A wskill collapses into ONE entry whose projections (book / deck / training
   / skill) are views of the same model, so picking it needs a second control
   to say which view to preview — otherwise `activeView()` falls back to the
   first non-skill view and the course/deck are unreachable outside Design
   mode. Switching either control rebuilds, since both change the build
   target. */

import { RefreshCw } from 'lucide-solid';
import { IconButton, Select } from '@forge/ui';
import { Show } from 'solid-js';

import {
  activeView,
  flatSites,
  loadSites,
  nodeKey,
  rebuild,
  selectSite,
  selectView,
  selected,
  siteTree,
  viewLabel,
} from '../state/sites';

const INDENT = '  '; // NBSP — plain spaces collapse when rendered

export default function SitePicker() {
  const flat = () => flatSites(siteTree());
  const options = () =>
    flat().map(({ node, depth }) => ({
      value: nodeKey(node),
      label: `${INDENT.repeat(depth)}${depth ? '└ ' : ''}${node.label}${node.wskill ? ' — wskill' : ''}`,
    }));

  // Skill views build a folder rather than a browsable site, and only the
  // Design canvas renders that (SkillBrowser) — so they are not offered here.
  const viewOptions = () =>
    (selected()?.views ?? [])
      .filter((v) => !v.skill)
      .map((v) => ({ value: v.id, label: viewLabel(v.kind) }));

  return (
    <div class="ed-site-picker">
      <Select
        options={options()}
        value={selected() ? nodeKey(selected()) : undefined}
        onChange={(value) => {
          const hit = flat().find(({ node }) => nodeKey(node) === value);
          if (hit) selectSite(hit.node);
        }}
        placeholder={options().length ? 'Select a site…' : 'No wdoc sites found'}
        disabled={options().length === 0}
      />
      <Show when={selected()?.wskill && viewOptions().length > 1}>
        <Select
          options={viewOptions()}
          value={activeView()?.id}
          onChange={async (id) => {
            selectView(id);
            await rebuild();
          }}
        />
      </Show>
      <IconButton icon={RefreshCw} label="Rescan sites" onClick={() => loadSites()} />
    </div>
  );
}
