/* Curated native-editor registry for the Systems view — which WAD kinds get
   a type-specific editor in the details modal (a cli_command reads like a
   man page, a code_item :api like API docs, a screen as its rendered
   wireframe / terminal mock-up). Curated on purpose, like systems.rs's
   PERSPECTIVES: a man-page layout IS kind-specific knowledge, and the
   canonical WAD vocabulary is the stable thing to key on. Extension kinds
   keep the generic schema forms.

   Everything here is pure (no Solid, no API): the modal decides what to do
   with the matches. */

/** A cell's text with a leading symbol colon stripped (`:api` → `api`). */
const cellText = (cells, name) => {
  const t = cells?.[name]?.text ?? '';
  return t.startsWith(':') ? t.slice(1) : t;
};

/* `label` names the aggregate tab on a component/container; `ownLabel` the
   tab on the unit itself. `when` gates kinds that host several payloads
   (only a `code_item kind = :api` is a Web API). `create` seeds fields the
   aggregate Add button must set beyond the schema's required ones.
   `hostKinds` are the ComponentKind (and ContainerKind) symbols that MARK a
   node as this surface — a `component { kind = :cli }` shows the CLI tab
   even before any command is attached. */
export const SURFACES = [
  { id: 'cli', kind: 'cli_command', label: 'CLI', ownLabel: 'CLI', hostKinds: ['cli'] },
  {
    id: 'api',
    kind: 'code_item',
    label: 'API',
    ownLabel: 'API',
    when: (n) => cellText(n.cells, 'kind') === 'api',
    create: { kind: 'api' },
    hostKinds: ['web_api'],
  },
  {
    id: 'screen',
    kind: 'screen',
    label: 'Screens',
    ownLabel: 'Screen',
    hostKinds: ['ui', 'tui'],
  },
];

/** A plausible id from a display name: lowercase, non-identifier → `_`. */
export const slugify = (name) =>
  String(name ?? '')
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .replace(/^(\d)/, '_$1');

/** The surface a node (or detail payload) itself is, or null. */
export function surfaceOf(node) {
  if (!node) return null;
  return SURFACES.find((s) => s.kind === node.kind && (!s.when || s.when(node))) ?? null;
}

/** `node.id` plus every id nested under it through the model's parent
    links (cycle-safe). Containment is transitive on purpose: a container's
    CLI commands hang off its COMPONENTS, and the container should still
    aggregate them. */
export function subtreeIds(node, model) {
  const childrenOf = new Map();
  for (const n of model?.nodes ?? []) {
    for (const p of n.parents ?? []) {
      if (!childrenOf.has(p.id)) childrenOf.set(p.id, []);
      childrenOf.get(p.id).push(n.id);
    }
  }
  const ids = new Set();
  const stack = node?.id ? [node.id] : [];
  while (stack.length) {
    const id = stack.pop();
    if (ids.has(id)) continue;
    ids.add(id);
    for (const c of childrenOf.get(id) ?? []) stack.push(c);
  }
  return ids;
}

/** The surface units attached to `node` or anything nested under it: the
    model's nodes of the surface kind whose parent links name a member of
    the node's containment subtree. The `/api/systems` payload carries
    every node regardless of canvas visibility, so nothing extra is
    fetched. */
export function attachedUnits(node, model, surface) {
  if (!node?.id) return [];
  const ids = subtreeIds(node, model);
  return (model?.nodes ?? []).filter(
    (n) =>
      n.kind === surface.kind &&
      n.id !== node.id &&
      (n.parents ?? []).some((p) => ids.has(p.id)) &&
      (!surface.when || surface.when(n)),
  );
}

/** Does the node's own `kind` cell mark it as a host of this surface
    (a `component { kind = :cli }`, a `container { kind = :cli }`)? */
export function hostsSurface(node, surface) {
  return (surface.hostKinds ?? []).includes(cellText(node?.cells, 'kind'));
}

/** Every surface with at least one unit attached to `node` — or whose
    `hostKinds` the node's own kind names, so a component MARKED :cli gets
    its CLI tab (with the Add bar) before any command exists. A node that
    IS a surface never aggregates its own kind. */
export function attachedSurfaces(node, model) {
  return SURFACES.filter((s) => s.kind !== node?.kind)
    .map((s) => ({ surface: s, units: attachedUnits(node, model, s) }))
    .filter((x) => x.units.length > 0 || hostsSurface(node, x.surface));
}

/** `name <arg> [arg] [--flag <val>]` rendered from a cli_command's detail
    payload. Arg names that already carry brackets ("<file>", "[template]")
    are kept verbatim; otherwise required args get `<…>` and optional `[…]`.
    Flags are always optional. */
export function usageLine(detail) {
  const items = (kind) => detail?.children?.find((f) => f.kind === kind)?.items ?? [];
  const name = detail?.cells?.name?.text ?? detail?.id ?? '';
  const args = items('cli_arg').map((a) => {
    const n = a.cells?.name?.text ?? a.label ?? 'arg';
    if (/^[<[]/.test(n)) return n;
    const required = (a.cells?.required?.text ?? 'true') !== 'false';
    return required ? `<${n}>` : `[${n}]`;
  });
  const flags = items('cli_flag').map((f) => {
    const n = f.cells?.name?.text ?? f.label ?? '--flag';
    const v = f.cells?.value?.text;
    return v ? `[${n} ${v}]` : `[${n}]`;
  });
  return [name, ...args, ...flags].filter(Boolean).join(' ');
}
