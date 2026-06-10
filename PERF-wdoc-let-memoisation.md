# Performance report: document-level `let` bindings need memoisation — wad build now ~45 min

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
