# Performance report: document-level `let` bindings need memoisation — wad build now ~45 min

> **Upstream status (2026-06-11, after `b331ee0b`):** investigated; partially
> fixed; **still open** for the asymptotic case. The surface hypothesis was
> wrong — document-level lets (including in imported files) were already
> memoised via `OnceLock` — but three real per-reference/per-call costs were
> found and fixed: every reference to a cached value deep-cloned it (lists /
> records / variant payloads are now `Arc`-shared; a 1000-reference synthetic
> went 25.7 s → 0.06 s), function bodies were AST-deep-cloned per literal
> evaluation and per named resolution (now `Arc<Expr>`), and argument coercion
> re-ran name resolution and rebuilt list arguments on every closure
> invocation (now a memoised union lookup + pass-through fast path). A
> wad-shaped nested-closure synthetic improved ~1.6× on top of the clone wins.
> **wad itself is still >30 min**: the profile's "10.9 s per `sec_dests`
> call" was *inclusive* time — the dominant cost is the document's own
> combinatorial chains (`alerts_for` → per-element `ops_dests` → full
> `ops_recs` scan, multiplied by pages × embeds × lookups), now executed with
> cheap clones but still O(refs × |list|) per level in a tree-walking
> interpreter. The remaining fix directions, in order of likely payoff:
> call-result memoisation for pure named functions; hoisting loop-invariant
> calls; or compiling the hot filter/map chains. Tracked here until one of
> those lands.


Follow-up to the earlier build-scaling report (taken upstream 2026-06-10; measurements
restated below so this stands alone). Measured against the binary built from `053919e2`
— i.e. **after** `b07ab2b0` (name-resolution caching), which did not move the needle for
this workload. Filed 2026-06-10 from the `wad` project (`~/dev/wad`).

## Measurements

Same machine, same example app, growing the document:

| wad commit | pages | content delta | build time |
|---|---|---|---|
| pre-session | 41 | baseline | seconds |
| `c48e99d` | 55 | deeper data + 3 chapters | **2 min** |
| `3777638` | 61 | + ops chapter (10 connection types) | **~5 min** |
| `997ffea` | 62 | + security chapter (16 connection types) | **~13 min** |
| `c614601` | 64 | + errors & product chapters | **~48 min** |
| `1aee969` + diagrams, new binary | 64 | + 5–7 backlink embeds per entity page | **>40 min** (killed) |

+16% pages → ×24 time. Pages are not the driver.

## Diagnosis: re-evaluation of document-level lets, compounding through chains

wad's templates hold **244 document-level `let` bindings** across `wdoc/*.wcl`. Every
chapter follows the same shape (it is the natural one the stdlib patterns suggest):

```wcl
// flatten this chapter's @connections slots into one normalised list…
let ops_recs = flatten([map(slo_covers_system, …), map(slo_covers_container, …), …×10])
// …and define lookup closures over it
let ops_dests = fn(sid, k) map(filter(ops_recs, …), …)
let alerts_for = fn(eid) filter(ops_alerts, fn(a) list_contains(ops_dests(a.id, :fires_for), eid))
```

Observed behaviour is consistent with each `let` being **re-evaluated on every
reference** rather than once per document evaluation:

1. Every call to `alerts_for(eid)` re-builds `ops_recs` from scratch (re-mapping every
   connection record of the chapter).
2. Lets chain — `suites_covering` → `covered_ids` → `covers_recs`;
   `sec_target_label` → `all_operations` → `apis` → `containers` → `systems` — so each
   level multiplies the recomputation.
3. The blow-up tracked the *reference count*, not the data: the final wiring step
   (every entity page embedding 5–7 `*_for_entity` components, each component calling
   3–6 lookups, each lookup re-deriving its chapter's full rec list and chain) is what
   took 13 min → ~48 min with only 2 new pages.

Cost model: pages × embeds/page × lookups/embed × let-chain depth × list sizes — and a
new chapter grows all five factors simultaneously. That is the superlinear curve.

## Why the author can't work around it

`let` is the only composition/abstraction tool a wdoc document has; the flatten-then-
lookup pattern is exactly what `testing.wcl`-style chapters are built from (and what
the C4 book template ships). There is no author-side cache. The only mitigation is
deleting cross-links, which defeats the point of a single queryable document.

## Requested fix

Memoise document-level `let` bindings once per document evaluation (per build, not per
page / per reference). All of wad's lets are pure functions of the document, so this is
semantically invisible. Expected effect: every `*_recs` list and helper chain evaluates
once; the residual work (rendering 64 pages of HTML + a few SVGs) is trivial — the
41-page document built in seconds with the same renderer.

If full memoisation is risky, memoising only *non-function* lets (the flattened lists)
would already collapse the dominant term — the closures are cheap once their captured
lists stop being rebuilt.

## Profile (confirms the diagnosis)

`wcl wdoc build main.wcl --out /tmp/wad-profile-site --profile` against wad `@ 1aee969`
(+ the sequence/state-diagram upgrades); ~49 min wall, call-tree JSON preserved at
`/tmp/wad-profile.json` (640 KB). Aggregating the tree by key:

| node | aggregated total | calls | mean / call |
|---|---|---|---|
| anonymous closures (`user_fn` name="") | 2 619 s | 47 314 | — |
| builtin `map` | 2 138 s | 9 951 | — |
| `field path=rows` (table rows) | 2 065 s | 109 | 19 s per table |
| `sec_dests` | 590 s | 54 | **10.9 s** |
| `ops_dests` | 377 s | 52 | **7.3 s** |
| `err_sources` | 315 s | 3 | **105 s** |
| `producer_ids` | 106 s | 3 | **35 s** |
| `rel_neighbor_ids` / `rel_node_ids` | 279 s each | ~260 | ~1 s |

`sec_dests` is a one-line `map(filter(sec_recs, …), …)` over **~25 connection records**
— taking 10.9 seconds per call. `err_sources` filters ~17 records — 105 seconds per
call. The only way a 25-element filter costs 11 s is that its captured document-level
`let` (`sec_recs`, a flatten over 16 `@connections` slots) and that let's entire
upstream chain are re-evaluated on every reference — recursively, since the flatten's
`map` closures re-reference further lets. Notably, lets do not appear as profile nodes
at all (only `user_fn` / `builtin` / `field`), which is consistent with let references
being inlined/re-evaluated rather than evaluated-once-and-read.

Reproduce the numbers: `python3` over `/tmp/wad-profile.json`, summing `total_ns` /
`count` per `key.name` across the tree.

## Benchmark

wad `@ 1aee969` (`~/dev/wad`): `wcl wdoc build main.wcl --out _site` — a ready-made
regression benchmark: seconds when memoised correctly, ~45 minutes today.

---

## Update 2026-06-11: post-`b331ee0b` profile — the cost is context-dependent CALL overhead, not list work

Re-profiled on the binary from `053919e2` (clone/coercion fixes in): ~43 min, profile at
`/tmp/wad-profile2.json` — **nearly identical to the pre-fix profile**. Drilling into
per-call-site subtrees produced the decisive observation:

**The same function costs ~0s or tens of seconds per call depending on where it is
called from**, with all the cost as *self time* (its children — the actual filter/map
work — sum to ~0):

| function | from `each` / filter contexts | from a table `rows` mapping closure |
|---|---|---|
| `sec_dests` (map+filter over ~25 recs) | 0.0s × 37 calls | **556s × 15 calls (~37s/call)**, children 0.0s |
| `ops_dests` | 0.0s × 14 | **346s × 8 (~43s/call)**, children 0.0s |
| `err_sources` (~17 recs) | — | **274s × 3 (~91s/call)**, children 0.0s |
| `prod_dests` | 0.0s × 21 | **211s × 12 (~18s/call)**, children 0.0s |
| `covered_ids` | 0.0s × 110 | **108s × 5 (~22s/call)**, children 0.0s |

`field:rows` nodes total **1,912s of the ~2,600s build (73%)** across 109 tables.
One outlier outside `rows`: `sys_cross_rels` 105s × **1** call (self time) via a
nested closure chain — so the toll is not exclusive to `rows`, it correlates with
the invocation context (depth / environment of the calling closure?), and per-call
toll appears to grow with document size — the same templates cost milliseconds in
small repro documents and in wad's own 55-page era.

**Author-side mitigation attempted and falsified** (wad `@ ecea5ec`): hoisted every
loop-invariant lookup out of filter predicates, and precomputed all static chapter
tables' `rows` as document-level lets (referenced by name — zero named-fn calls from
the rows context on those tables). Result: **2,761s (~46 min) — no change.** The toll
is spread across every remaining closure invocation in block-field contexts (component
tables, label helpers per cell, repeater bodies, inline rendering); no template shape
avoids closure calls.

**Where to look:** whatever the evaluator does *per user-fn invocation* that scales
with the calling context — re-running name resolution against the full document scope,
walking/copying a deep environment chain, or re-validating/coercing against the merged
document schema per call. A micro-benchmark that reproduces it: call a trivial named fn
from inside a `table { rows = map(...) }` closure in a document with many merged
@document roots / large scope, vs the same call from a top-level let — in wad the ratio
is ~10⁶.
