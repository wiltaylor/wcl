# The `wcl wskill` CLI surface

Type: grilling
Status: open
Blocked by: 07

## Question

Settled while charting: a real `wcl wskill` subcommand family, lifting `graph.rs`'s model into the
library and **consolidating** the scattered gates so they travel with the format rather than with
this repo.

The situation being replaced:

- **No `wcl wskill` subcommand exists at all.**
- `crates/wcl/src/editor/graph.rs` (639 lines) **already computes** the model — units, nested index
  trees, `related` edges, pin edges, per-unit block lists, `related_editable` flags, unindexed-unit
  detection — but only as an HTTP endpoint behind a running browser editor.
- Seven repo-root justfile recipes do the checking, as **grep/sed shell**: `wskill-check`,
  `wskill-coverage`, `wskill-crosstopic-check`, `wskill-artifact-check`, `wskill-template-check`,
  `wskill-schema-check`, `wskill-schema-sync`. `wskill-artifact-check` regexes `entry = "…"` out of
  the file instead of reading the parsed model; `wskill-crosstopic-check` diffs templates against
  hardcoded exempt-lists; `wskill-coverage` pipes a `.repl` script through `wcl repl` and strips
  quotes with `sed`. **None checks graph shape.**
- `docs/wskills/wskill/skill/` doesn't exist — the shipped AI skill bundles **zero** executables.

Decide:

- **The subcommand set.** `graph`? `lint`? `check`? `curate`? `sync`? What's the minimal set that
  serves the curator, the editor, CI, and an agent authoring a unit.
- **What lifts out of `graph.rs`** into the library, and what stays editor-specific. The editor
  becomes a *second consumer* of a shared model rather than its only home — establish where the seam
  goes so the editor isn't degraded.
- **Which of the seven recipes migrate, and which are WCL-repo housekeeping.** `wskill-schema-sync`
  and `wskill-crosstopic-check` exist to keep four in-repo wskills mirrored — arguably not something
  a standalone wskill needs. `wskill-coverage` and `wskill-artifact-check` clearly are.
- **The lint rule set.** This is the substance. Concretely: link-cap (3–5 on content units, uncapped
  on indexes), hub-note detection, orphan/unindexed units, kind misuse, index-mirroring in `related`,
  units with zero links, `audience` coverage gaps. Which are errors, which warnings, which advisory?
  Which are *mechanically decidable* versus needing an agent's judgement — because the undecidable
  ones belong to the curator, not the linter.
- **JSON vs human output**, and exit codes — an agent needs to parse it; CI needs it to fail.
- **Does the shipped AI skill bundle scripts now?** The `script` block exists in the wskill schema
  and nothing uses it.

Blocked by `07-curator-contract`: the CLI exists to serve the curator, so its contract comes first.
Feeds `10-editor-review`.
