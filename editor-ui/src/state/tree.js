/* File-tree state: loads /api/files and folds the flat listing into a
   nested node tree (dirs first, alphabetical — the walk is presorted, but
   dirs interleave with files at each level). */

import { createResource } from 'solid-js';

import { api } from '../api';
import { setWorkspaceRoot } from '../lsp/client';

const [treeData, { refetch: refreshTree }] = createResource(async () => {
  const res = await api.files();
  if (!res.ok) throw new Error(res.error);
  setWorkspaceRoot(res.root);
  return { root: res.root, nodes: buildTree(res.files) };
});

export { treeData, refreshTree };

/** Fold [{path, type}] into [{name, path, type, children?}] (recursive). */
export function buildTree(files) {
  const root = { children: new Map() };
  for (const f of files) {
    const parts = f.path.split('/');
    let node = root;
    for (let i = 0; i < parts.length; i++) {
      const name = parts[i];
      const last = i === parts.length - 1;
      if (!node.children.has(name)) {
        node.children.set(name, {
          name,
          path: parts.slice(0, i + 1).join('/'),
          type: last ? f.type : 'dir',
          children: new Map(),
        });
      }
      node = node.children.get(name);
      if (!last) node.type = 'dir';
    }
  }
  const toList = (n) => {
    const kids = [...n.children.values()].map((c) => ({ ...c, children: toList(c) }));
    kids.sort((a, b) =>
      a.type === b.type ? a.name.localeCompare(b.name) : a.type === 'dir' ? -1 : 1,
    );
    return kids;
  };
  return toList(root);
}
