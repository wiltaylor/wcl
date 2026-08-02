/* Plain text search over the unit graph — the one matcher behind the
   find-a-unit box, wherever it is mounted (the graph toolbar, the index
   panel, the content modal).

   A unit is searched over four fields, and they are ORDERED: the id, the
   name, the summary, then the body prose the graph payload carries
   (`text`, one line per literal — see `block_text` in editor/graph.rs).
   The order is the whole ranking model: a query that names a unit outright
   should not sit below one that merely mentions it, and a hit is reported
   as belonging to the narrowest field that carries it, so the reader can
   tell "this IS the unit" from "this talks about it".

   Every term must match, but each may match a different field — typing
   "spans etag" finds the unit whose id is one and whose prose says the
   other. Matching is substring, not prefix: at 65 units the reader is
   recalling half a word, not driving an index. */

/** The searchable fields of a graph node, best first. */
const FIELDS = [
  { field: 'id', weight: 100, of: (n) => n.id ?? '' },
  { field: 'name', weight: 80, of: (n) => n.title ?? '' },
  { field: 'summary', weight: 60, of: (n) => n.summary ?? '' },
  { field: 'body', weight: 40, of: (n) => n.text ?? '' },
];

/** How much a snippet shows around the match when the line is longer than
    it. Asymmetric on purpose: the host shows one clipped line, so leading
    context pushes the highlighted term off the right edge — just enough to
    show the match is mid-sentence, then everything after it. */
const LEAD = 24;
const TRAIL = 96;

/** Terms of a query: lowercased, whitespace-separated, blanks dropped. */
const termsOf = (query) =>
  String(query ?? '')
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean);

/** Extra credit for WHERE the term landed: the whole field, its start, or
    the start of a word — so "span" ranks `spans` over `subspan_of`. */
function placement(text, term, at) {
  if (text.length === term.length) return 40;
  if (at === 0) return 20;
  return /[\w]/.test(text[at - 1]) ? 0 : 10;
}

/** The line of `text` containing byte `at`, and the offset within it. */
function lineAt(text, at) {
  const start = text.lastIndexOf('\n', at) + 1;
  const end = text.indexOf('\n', at);
  return { line: text.slice(start, end < 0 ? text.length : end), at: at - start };
}

/** What the reader sees of a match: the matching line, windowed around the
    term when it is too long to show whole, with the term's position in the
    result so a host can highlight it. */
function snippetOf(text, at, length) {
  const { line, at: lineAtIdx } = lineAt(text, at);
  if (line.length <= LEAD + TRAIL + length) {
    return { snippet: line, at: lineAtIdx, length };
  }
  const from = Math.max(0, lineAtIdx - LEAD);
  const to = Math.min(line.length, lineAtIdx + length + TRAIL);
  const head = from > 0 ? '…' : '';
  const tail = to < line.length ? '…' : '';
  return { snippet: head + line.slice(from, to) + tail, at: lineAtIdx - from + head.length, length };
}

/** Rank `nodes` (graph payload nodes — the caller chooses which kinds are
    offered) against a plain query. Returns the matches best first, each as
    `{ key, node, score, field, snippet, at, length }` where `at`/`length`
    locate the term inside `snippet`. An empty query matches nothing: the
    box shows no list until it has been asked something. */
export function searchUnits(nodes, query) {
  const terms = termsOf(query);
  if (terms.length === 0) return [];

  const hits = [];
  for (const node of nodes ?? []) {
    const sources = FIELDS.map((f) => {
      const text = f.of(node);
      return { ...f, text, lower: text.toLowerCase() };
    }).filter((s) => s.text);

    let score = 0;
    let best = null;
    let matchedAll = true;
    for (const term of terms) {
      let bestTerm = null;
      for (const s of sources) {
        const at = s.lower.indexOf(term);
        if (at < 0) continue;
        const termScore = s.weight + placement(s.lower, term, at);
        if (!bestTerm || termScore > bestTerm.score) {
          bestTerm = { score: termScore, source: s, at, length: term.length };
        }
      }
      if (!bestTerm) {
        matchedAll = false;
        break;
      }
      score += bestTerm.score;
      if (!best || bestTerm.score > best.score) best = bestTerm;
    }
    if (!matchedAll) continue;

    hits.push({
      key: node.key,
      node,
      score,
      field: best.source.field,
      ...snippetOf(best.source.text, best.at, best.length),
    });
  }

  hits.sort((a, b) => b.score - a.score || (a.node.title ?? '').localeCompare(b.node.title ?? ''));
  return hits;
}
