/* Topbar site selector for the preview: the discovered site tree flattened
   into an indented Select option list (nesting shows which picks pull in
   whole sub-site trees), plus a refresh button that re-runs discovery. */

import { RefreshCw } from 'lucide-solid';
import { IconButton, Select } from '@forge/ui';

import { flatSites, loadSites, selectSite, selected, siteTree } from '../state/sites';

/* Stable option value for a node — JSON survives any characters the entry
   path or site name might contain. */
const keyOf = (node) => JSON.stringify([node.entry, node.site ?? null]);

const INDENT = '  '; // NBSP — plain spaces collapse when rendered

export default function SitePicker() {
  const flat = () => flatSites(siteTree());
  const options = () =>
    flat().map(({ node, depth }) => ({
      value: keyOf(node),
      label: `${INDENT.repeat(depth)}${depth ? '└ ' : ''}${node.label}`,
    }));

  return (
    <div class="ed-site-picker">
      <Select
        options={options()}
        value={selected() ? keyOf(selected()) : undefined}
        onChange={(value) => {
          const hit = flat().find(({ node }) => keyOf(node) === value);
          if (hit) selectSite(hit.node);
        }}
        placeholder={options().length ? 'Select a site…' : 'No wdoc sites found'}
        disabled={options().length === 0}
      />
      <IconButton icon={RefreshCw} label="Rescan sites" onClick={() => loadSites()} />
    </div>
  );
}
