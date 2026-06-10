# Performance report: `wcl wdoc build` time grows superlinearly with document size

Found 2026-06-10 while growing the `wad` example project (`~/dev/wad`). Measured on the binary
built from `d45f0eaa`.

## Measurements (wad repo, by commit)

| wad commit | content | pages | build time |
|---|---|---|---|
| pre-session (`0ff67af`) | 1 system, 3 containers, 1 component | 41 | a few seconds |
| `98e9a3b` (deepen data: +8 components, +4 DB tables, +5 REST ops, +5 test suites, edges) | 49 | **53 s** |
| `efa16b4` (namespace the schemas — same data) | 49 | **85 s** |
| `c48e99d` (+ Decisions/Testing/Schema-Reference chapters) | 55 | **120 s** |

Two observations:

1. A roughly 2× growth in data blocks took the build from seconds to ~1 minute, and each further
   addition compounds — the cost looks superlinear in (blocks × pages × template `let`s).
   The wad templates are heavy on `filter`/`map` over flattened edge lists per entity page
   (O(n²) in WCL-land), but an 8× data increase producing a ~50× time increase suggests
   evaluator-level re-evaluation on top — e.g. document-level `let` bindings (like wad's
   `all_edges`, `containers`, `apis`) being recomputed per reference / per page instead of once.
2. The namespace migration alone (`efa16b4`, zero data change) added ~60% — name resolution in
   the hot path?

## Why it bit hard

`wcl wdoc build` is also the only schema checker for wdoc projects (`wcl check` can't resolve
`<wdoc.wcl>`), so a 2-minute build is a 2-minute edit-check loop. During the session several
builds were misdiagnosed as hangs and killed; a `--verbose`/progress flag (e.g. "page 12/55…")
would distinguish slow from stuck.

## Suggested directions

- Memoise document-level `let` bindings once per document evaluation.
- Profile one wad build (`wcl parse --profile`-style flag for `wdoc build`?) — wad at `38f7152`
  is a ready-made benchmark: `wcl wdoc build main.wcl --out _site` in `~/dev/wad`.
- Progress output for multi-page builds.

## Side observation (possibly separate bug)

While investigating, structurally identical `wdoc_component`s with a block-valued slot behaved
differently in different documents: in `wad`, `probe_card { ap = a }` with body `h3 $"${ap.name}"`
renders fine, but the same shape in a fresh minimal project fails fast with
`unresolved reference 'ap.name'` (repros preserved in `/tmp/wcl-slot-hang/`, see out4/out10
variants — typed blocks, records, static and generated pages, in/outside diagrams all fail there).
Also, binding a slot from a same-named caller variable (`api_operation_card { op = op }` where the
caller's repeater variable is also `op`) errors with unresolved references inside the component
body — renaming one side fixes it. Both worth a look while in the resolver.
