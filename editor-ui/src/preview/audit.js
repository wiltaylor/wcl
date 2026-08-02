/* Pure readers over the /api/audit payload — the union graph of a git
   range, before ∪ after, with removals marked.

   Everything the audit view decides that isn't rendering lives here: which
   section a row belongs to, which changed links a row reports under itself,
   what the header strip says, and which slice of the union graph is drawn.
   The endpoint is a thin adapter over `wcl_wskill::Audit`, so nothing here
   re-derives a fact the model already carries — a node's `change`, its
   `findings`, `graphed`, an edge's `writer`. */

/** The changelog's sections, in reading order — the model's own `Change`
    order, plus the one row kind that has no change of its own: a node this
    range left alone and BROKE. Its findings are the whole of its news. */
export const SECTIONS = [
  { key: 'added', label: 'Added' },
  { key: 'removed', label: 'Removed' },
  { key: 'modified', label: 'Modified' },
  { key: 'broken', label: 'Broken by this range' },
];

/** Which section a node's row belongs to. An unchanged node is only ever in
    the changelog because the range broke something in it (the endpoint's
    `news` flag), so it reads as broken rather than as a fourth kind of
    change. */
export function sectionOf(node) {
  return node.change === 'unchanged' ? 'broken' : node.change;
}

/** The changed links a node WROTE, keyed by the writer the model names —
    which for a nested pin is the sub-index holding it, not the top-level
    index the edge is drawn from. Returns `{ [nodeKey]: edge[] }`. */
export function edgeNews(data) {
  const out = {};
  for (const e of data?.edges ?? []) {
    if (e.change === 'unchanged') continue;
    (out[e.writer] ??= []).push(e);
  }
  return out;
}

/** The changelog: every newsworthy node under its section, each row
    carrying the links it wrote. Empty sections are dropped — a heading
    over nothing is noise. */
export function newsRows(data) {
  const churn = edgeNews(data);
  const news = (data?.nodes ?? []).filter((n) => n.news);
  return SECTIONS.map((s) => ({
    ...s,
    rows: news
      .filter((n) => sectionOf(n) === s.key)
      .map((node) => ({ node, edges: churn[node.key] ?? [] })),
  })).filter((s) => s.rows.length > 0);
}

/** How many findings of each severity the range's rows carry. A candidate
    is a nomination, never a defect, so the three are counted apart and the
    header never adds them up. */
export function severityTally(data) {
  const out = { error: 0, warn: 0, candidate: 0 };
  for (const n of data?.nodes ?? []) {
    for (const f of n.findings ?? []) out[f.severity] = (out[f.severity] ?? 0) + 1;
  }
  return out;
}

/** The metrics that moved in the wrong direction — every metric is
    oriented so lower is better, which is what lets the header say "worse"
    without carrying a direction per metric. */
export function worseMetrics(data) {
  return (data?.health ?? []).filter((m) => m.worse);
}

/** `<worse> of <total>` for the header strip. */
export function healthTally(data) {
  const health = data?.health ?? [];
  return { worse: health.filter((m) => m.worse).length, total: health.length };
}

/** A sha as a header shows it. */
export function shortSha(sha) {
  return (sha ?? '').slice(0, 8);
}

/** Where a row's `file` + `span` can be opened, and how far they can be
    trusted once there.

    A row is anchored where the AFTER revision writes it, so the offsets are
    only the working tree's when the after end IS the working tree
    (`range.after === null`). Audit `a..b` with `b` a commit and every
    surviving row's span addresses `b` — following it into the working-tree
    file would select the wrong bytes with total confidence, which is the
    caller's half of the rule `NodeDelta::span` states. A removal has no
    current file at all: the path is where it WAS written.

    Returns `'span'` (open and select), `'file'` (open, select nothing) or
    `'none'`. */
export function openTarget(data, node) {
  if (node?.change === 'removed') return 'none';
  if (!node?.file) return 'none';
  return data?.range?.after || !node.span ? 'file' : 'span';
}

/** `<before>..<after>`, naming the working tree for what it is — an audit
    of uncommitted output must say so, or its reader cannot reproduce it. */
export function rangeLabel(data) {
  const before = shortSha(data?.range?.before) || '(before the wskill)';
  const after = data?.range?.after ? shortSha(data.range.after) : '(working tree)';
  return `${before}..${after}`;
}

/** `+3 −1 ~2` over one family's counts, dropping the zeros: a count that
    did not move is not news. */
export function countsText(counts) {
  const parts = [
    ['+', counts?.added],
    ['−', counts?.removed],
    ['~', counts?.modified],
  ]
    .filter(([, n]) => n > 0)
    .map(([sign, n]) => `${sign}${n}`);
  return parts.length ? parts.join(' ') : '—';
}

/** The union graph as it is drawn: the drawn nodes, the edges between two
    of them, and the world box that holds them.

    `onlyChanged` keeps what the range touched — a node that changed, or one
    an added/removed edge lands on — which is the reading most audits want;
    the full union is one checkbox away. Removals are never filtered out:
    a surface that cannot show a removal cannot audit an agent that
    removes. */
export function graphModel(data, { onlyChanged = false } = {}) {
  const drawn = (data?.nodes ?? []).filter((n) => n.graphed);
  const byKey = new Map(drawn.map((n) => [n.key, n]));
  const edges = (data?.edges ?? []).filter((e) => byKey.has(e.from) && byKey.has(e.to));

  const changedEdges = edges.filter((e) => e.change !== 'unchanged');
  let nodes = drawn;
  let shown = edges;
  if (onlyChanged) {
    const touched = new Set(changedEdges.flatMap((e) => [e.from, e.to]));
    nodes = drawn.filter((n) => n.change !== 'unchanged' || touched.has(n.key));
    const keep = new Set(nodes.map((n) => n.key));
    shown = edges.filter(
      (e) => e.change !== 'unchanged' && keep.has(e.from) && keep.has(e.to),
    );
  }
  return { nodes, edges: shown, byKey, box: boxOf(nodes) };
}

/** The world box holding every node, with a margin. */
function boxOf(nodes) {
  if (!nodes.length) return { x: 0, y: 0, w: 800, h: 600 };
  const x0 = Math.min(...nodes.map((n) => n.x));
  const y0 = Math.min(...nodes.map((n) => n.y));
  const x1 = Math.max(...nodes.map((n) => n.x + n.w));
  const y1 = Math.max(...nodes.map((n) => n.y + n.h));
  const m = 40;
  return { x: x0 - m, y: y0 - m, w: x1 - x0 + m * 2, h: y1 - y0 + m * 2 };
}
