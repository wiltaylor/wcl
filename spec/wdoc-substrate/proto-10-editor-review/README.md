# Prototype — the wskill audit surface (ticket 10)

The artifact behind [ticket 10](../issues/10-editor-review.md)'s primary question: *what did the
agent just change, and is the shape sane?*

## Run it

```bash
cd .scratch/wdoc-substrate/proto-10-editor-review
python3 -m http.server 8791          # then open http://localhost:8791/audit-proto.html
```

`audit-proto.html` is self-contained (the data is inlined). To rebuild it against a different
commit:

```bash
python3 extract.py <rev>~1 > b.json      # unit graph at a git rev
python3 extract.py <rev>   > a.json
python3 audit.py b.json a.json > audit.json
# then substitute __DATA__ / __SUBJECT__ in proto.template.html
```

## Subject

`99518181 docs(wskill/wcl): atomize language reference, complete builtins, data-drive grouping` —
a real 30-unit authoring commit on the `wcl` wskill. **+30 −5 units, +160 −19 edges, 8 new hubs.**
Not a synthetic change.

`extract.py` is a regex reader over WCL, not a parser — good enough for a prototype, wrong in the
margins. Known imprecision: units pinned by the *training course* structure rather than by an
`index` read as unpinned, which inflates the orphan count somewhat. The shipped version reads the
real model out of `crates/wcl_wskill`.

## What it showed

Three representations of one commit were built and compared:

**A · Changelog** — every one of the 30 new units landed **unpinned**. Not an artifact of the
extractor: the `language` index that houses them wasn't authored until `ec19b309`, three commits
later. An entire language reference was written with no home and nothing said so at the time.
This is ticket 09's place-before-write, visible as a defect.

**B · Health report** — 4 of 10 metrics worse (edges/unit 2.11 → 3.39, hubs 3 → 11, over-cap units
0 → 10, reasonless edges 92 → 208). Names no unit anywhere. It is the shape of ticket 07's gate,
not of a review.

**C · Graph overlay** — legible, and structurally blind: the graph renders the after-state, so the
commit's −5 units and −19 edges simply weren't on it.

## What was decided from it

- **C's blindness is fixed, not accepted**: the audit graph is the **union** (before ∪ after) with
  removals ghosted in red. The final artifact does this — the five deleted `builtins_*` units are
  visible.
- **B collapses into A's header.** One view, two representations (list · graph), health as a
  header strip.
- **Findings ride on the changed rows** as tags, scoped to the diff.
- **`wcl wskill audit [<rev>..<rev>]`** is a sixth subcommand; the editor tab is an adapter.

Full reasoning and the rest of the decisions: [ticket 10](../issues/10-editor-review.md).

## Files

| File | What |
|---|---|
| `audit-proto.html` | the built prototype — open this |
| `proto.template.html` | its source, with `__DATA__` / `__SUBJECT__` placeholders |
| `extract.py` | unit graph at a git rev → JSON |
| `audit.py` | two graphs → the audit model (diff + health metrics) |
| `shot-a.png` | the Changed pane, final shape |
| `shot-c.png` | the union graph, with removals ghosted |
