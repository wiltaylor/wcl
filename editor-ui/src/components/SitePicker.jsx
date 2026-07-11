/* Topbar site selector for the preview: the discovered site tree flattened
   into an indented Select option list (nesting shows which picks pull in
   whole sub-site trees), plus a refresh button that re-runs discovery. */

import { IconButton, Select } from '@forge/ui';

import { flatSites, loadSites, selectSite, selected, siteTree } from '../state/sites';

/* Lucide-style refresh glyph (1.5px stroke, currentColor) inlined so the
   app needs no icon dependency of its own — @forge/ui's Icon calls it with
   size/strokeWidth like any lucide component. */
function RefreshIcon(props) {
  return (
    <svg
      width={props.size ?? 16}
      height={props.size ?? 16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width={props.strokeWidth ?? 1.5}
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d="M21 12a9 9 0 1 1-2.64-6.36" />
      <path d="M21 3v6h-6" />
    </svg>
  );
}

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
      <IconButton icon={RefreshIcon} label="Rescan sites" onClick={() => loadSites()} />
    </div>
  );
}
