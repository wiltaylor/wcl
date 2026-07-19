/* Topbar site selector for the preview: the discovered site tree flattened
   into an indented Select option list (nesting shows which picks pull in
   whole sub-site trees), plus a refresh button that re-runs discovery. */

import { RefreshCw } from 'lucide-solid';
import { IconButton, Select } from '@forge/ui';

import { flatSites, loadSites, nodeKey, selectSite, selected, siteTree } from '../state/sites';


const INDENT = '  '; // NBSP — plain spaces collapse when rendered

export default function SitePicker() {
  const flat = () => flatSites(siteTree());
  const options = () =>
    flat().map(({ node, depth }) => ({
      value: nodeKey(node),
      label: `${INDENT.repeat(depth)}${depth ? '└ ' : ''}${node.label}${node.wskill ? ' — wskill' : ''}`,
    }));

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
      <IconButton icon={RefreshCw} label="Rescan sites" onClick={() => loadSites()} />
    </div>
  );
}
