# The `wcl wskill` CLI surface

Type: grilling
Status: resolved
Blocked by: —

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

~~Blocked by `07-curator-contract`~~ — **07 is resolved; this ticket is now on the frontier.**
Feeds `10-editor-review`.

## Inherited from ticket 07 — do not re-litigate

07 settled the curator's contract, and in doing so **sharpened this ticket's brief and pre-answered
three of its six bullets**.

**The CLI's core job is now named: reduce the graph to what needs judgement.** The curator is one
agent in two phases — the CLI computes every screen requiring no judgement and emits a **candidate
list**; the agent then reads only the flagged units and their neighbours. This is what makes the
curator scale-independent, so it is the CLI's load-bearing responsibility, not an afterthought behind
"expose `graph.rs`".

**The mechanical/judgement split is drawn, and it maps onto the lint-rule bullet.** Mechanically
decidable — dangling id, over-cap, missing `why`, orphan unit, reciprocal duplicate — is a **linter,
in CI**. Requiring judgement — hub-ness, atomicity, "is this `why` honest" — is the curator's, and the
CLI's job there is only to **nominate**, never to decide. 07's named screens: link-density-per-word,
name-prefix and co-pin clustering, near-duplicate `why` text across one unit's edges, body-less
indexes. Note 06's anti-mirroring rule is a screen, not a gate: the check is co-pinned **and** the
`why` adds nothing beyond co-membership.

**The op layer comes with the model.** The curator issues the **same named ops with the same
semantics** as the editor (`pin_unit`, `unpin_unit`, `reorder_children`, `create_index`,
`delete_index`, `move_index`, `promote_index`, `demote_index`, `related_add`, `related_remove`) — one
definition per op, no second code path. **Wil ruled the transport is this ticket's call**, with one
hard constraint: a running `wcl editor` must **not** be required, since the curator runs headless at
the end of an agent session. Relevant asymmetry: `/api/nav/op` is already **id-addressed** and is
almost exactly the curator's write set, while `/api/block/ops` is **span-addressed** because a click
drives it — the curator thinks only in ids, so span-addressed block ops are a UI affordance, not a
curation primitive.

**The gate is specified, and it constrains the exit-code bullet.** Per-op schema validation reuses
`crates/wcl/src/edit.rs`'s pipeline; the per-run gate is *all four projections build* **and** *lint
violations must not increase against the pre-run baseline*. The lint side **must** be a baseline diff
— the graph enters with 736 bare edges and 6 dangling ids, so an absolute gate can never pass on first
run. Whatever the exit-code design, it has to express "did not get worse", not "is clean". A
`--dry-run` that prints the op list without applying is required.

**Findings have a home already: `comments.wcl`**, `author = "curator"`, using the existing sidecar +
`wcl wdoc comments` + editor-preview-pin machinery. Wil chose this over JSON-to-stdout, so the CLI's
human/JSON output question is narrower than the bullet assumes — **persisted findings are comments**;
whatever JSON this surface emits is for driving the agent, not for storing findings.

**One detail 07 left open for you:** a finding about a unit with **no rendered page** — a body-less
index, or a unit whose page a projection doesn't build — has no block to pin to. `comment.page_file`
plus a whole-page comment covers most of it; "this unit shouldn't exist" doesn't fit cleanly.

**A second, from the migration:** 07 committed to deleting every bare `related` edge **except** where
the target id or title already appears in the source unit's prose (~60–95 survivors of 736, flagged
for an author pass). That test is mechanically computable and needs a tool. Decide whether it lives
here as a one-shot `wcl wskill` subcommand or belongs to the migration effort.

---

## Resolution

Status: resolved

**Two of this ticket's own premises were wrong, and the correction reshaped the answer.**

The recipe list is misnamed and miscounted: there are **eight**, not seven, and `wskill-check` does
not exist. The real set is `wskill-schema-sync` / `wskill-schema-check`, `wskill-template-check`,
`wskill-crosstopic-check`, `wskill-artifact-check`, `wskill-coverage`, **`skills-check`** and
**`wcl-refcheck`** (the last is `wcl`-wskill-specific — it diffs the documented builtin and subcommand
lists against the binary).

Reading them in full changed the ticket. **Four of the eight exist solely to police duplicated
files**: `schema/base.wcl` plus **14 topic-agnostic wdoc templates** are copied verbatim into every
wskill, and two CI gates diff the copies — one against the scaffold heredoc, one against the reference
implementation. That is ~56 copies maintained to permit **five** documented divergences, three of
which are `wcl` and `wdoc` being reference-heavy. So the central question stopped being "which gates
migrate" and became "**do the gates migrate, or does the duplication die**".

Also: `wcl_wdoc` already knows the wskill format — `sidecar.rs:38,60` walks up looking for
`wskill.wcl`. And `related: list<identifier>` resolves against a flat `all_units`, so unit ids are
**assumed unique across kinds and nothing enforces it**.

### 1. A new `crates/wcl_wskill`

Layering becomes `wcl_lang → wcl_wdoc → wcl_wskill → wcl`. The model, the lint rules, the op
vocabulary and the embedded schema/templates live there; `wcl wskill` and the editor's `/api/graph` +
`/api/nav/op` both become thin adapters over it.

Rejected: **inside `wcl_wdoc`** (would push wskill's vocabulary *into* the wdoc crate in the same map
where ticket 14 is pulling wdoc's vocabulary *out* of `wcl_lang` — same argument, opposite direction)
and **`crates/wcl/src/wskill/`** (out of the editor but still in the binary, so the gates still travel
with this repo).

The chosen shape forces the editor-degradation question to be answered at a crate boundary rather than
left tangled: if `/api/graph` cannot be rebuilt as an adapter, the seam is in the wrong place.

`sidecar.rs`'s wskill knowledge **generalises rather than moves** — its callers are inside
`wcl_wdoc`, so moving it would invert the dependency. The concept is "the nearest owning document
root"; the marker filename becomes a parameter and `wcl_wdoc` stops naming the format.

### 2. Five subcommands

| | | |
|---|---|---|
| `wcl wskill graph` | the model as JSON | editor, curator, authoring agent |
| `wcl wskill lint` | **all** findings — `error` / `warn` / `candidate` | CI; curator phase 1 |
| `wcl wskill check` | projections build, artifacts resolve, coverage | CI |
| `wcl wskill op` | apply ops; `--dry-run` | curator |
| `wcl wskill install` | render the skill into `.claude/skills/` + `agents/` | anyone shipping a wskill |

**Nomination is a severity, not a sixth command.** 07 draws a hard line between mechanically-decidable
and needs-judgement, but that is a split in *responsibility*, not in commands — both are the same
operation (run rules over the model, emit findings), and the only difference is what the consumer
does. One rule engine, one finding schema; CI ignores `candidate`, the curator asks for nothing else.
Splitting them would duplicate the model-walk and the output format to encode what a `severity` field
already carries.

**`lint` and `check` stay separate on cost, not on kind.** `lint` reads the model — cheap enough for
an authoring agent to call in a loop. `check` builds four projections through the template layer that
ticket 11 measured at 36% of a build. Merging them makes the cheap thing expensive, and CI is the only
caller that wants both.

### 3. One op vocabulary, id-addressed

Every curator op is addressed by **`(kind, id)`** — `kind` optional in the JSON and inferred when
unambiguous, so an agent writing ops by hand isn't punished — and defined **once** in
`wcl_wskill::ops`. The editor's span-addressed `related_add` / `related_remove` become a **resolver in
front of the same function**: a drag resolves `span → (kind, id)` before calling it.

This closes 07's noted asymmetry. `/api/nav/op` was already id-addressed; only `related_*` sat on the
span-addressed side, because in the editor you drag an edge between rendered nodes. The curator has
never seen a rendered page, so a span-addressed op is a UI affordance, not a curation primitive.
Consequence: the `--dry-run` op list is the same JSON the editor would have sent.

`(kind, id)` rather than bare `id` because ids are unique **in practice but unenforced** (see the
`all_units` finding above); an op that silently hits the wrong kind is the failure mode.

`wcl wskill op` targets the **wskill root** (`wskill.wcl`), not a projection entry — the curator
operates on the format, not on one view of it.

### 4. The duplication dies; four recipes die with it

`schema/base.wcl` and the 14 topic-agnostic wdoc templates stop being files on disk and become
`import <wskill.wcl>` from a registry embedded in `wcl_wskill` — exactly what `wcl_wdoc` already does
with `lib/*.wcl` and what every wskill already does with `import <wdoc.wcl>`. `wcl init wskill`
scaffolds a handful of entry files instead of ~20.

**Wil's ruling and his reason:** *"we have seen a heap of problems with this by drift all over the
place. My original reason was I wanted to allow the author complete control to extend but never seen
that in practice."* The measurement agrees — 9% of copies diverge, and most of that is two topics.

Rejected: migrating the drift gates as a version-compatibility check ("your copies match the version
this binary ships"). It works, and it works outside this repo too, but it preserves the copies.

**Overrides are import granularity — no shadowing mechanism, and no new ticket.** Parts are separately
importable (`import <wskill/book.wcl>`, `<wskill/component/common.wcl>`, …) with `import <wskill.wcl>`
as the everything prelude — `wcl_wdoc`'s `wdoc.wcl → prelude.wcl → parts` shape. A topic wanting its
own book main **doesn't import that part and declares its own**; nothing imported a competing name, so
nothing shadows. The cost is an ergonomic cliff — overriding one part means swapping the one-line
aggregate import for enumerated part imports — and that is the right shape: the 95% case is one line,
the 5% case says out loud what it opts out of. This is what dissolved the language-level shadowing
question the grill had queued as a new ticket.

**Recipe disposition:**

| | |
|---|---|
| **die** | `wskill-schema-check`, `wskill-schema-sync`, `wskill-template-check`, `wskill-crosstopic-check` — they police copies that no longer exist |
| **→ `check`** | `wskill-artifact-check` (off the parsed model, not a regex over `entry = "…"`), `wskill-coverage` (report; its sharp case becomes a lint rule), `skills-check`'s projection-builds half |
| **→ `install --check`** | `skills-check`'s artifact drift, agent-name collisions, stale-generated detection — all real for anyone installing more than one wskill |
| **stays** | `wcl-refcheck` — this repo's docs-vs-binary gate; no other wskill documents `wcl` |

No back-port gate survives: with the reference-implementation/scaffold duplication gone there is
nothing to back-port. `docs/wskills/coverage.repl` and its `sed`-quote-stripping pipeline go too.

### 5. The lint rule set

**Errors** (mechanically certain; CI fails):

- **dangling `related` id** — 6 exist today; renders nothing, warns nobody
- **duplicate unit id across kinds** — uniqueness is assumed by `all_units` and unenforced
- **`related` id naming a body-less index** — 06 made indexes linkable *iff* they carry a `body` (53
  indexes, 6 bodies), so this is a dangling link wearing a valid id

**Warnings** (real signal, real exception rate):

- **over-cap `related` on a content unit** (3–5) — measured 0–4% violation; a cap being honoured with
  genuine exceptions is guidance, not truth
- **unit pinned by no index** — orphan, reachable only by link
- **unit reaching zero projections** — an `audience` gap; authored and invisible everywhere

**Candidates** (nominations to the curator; fail nothing) — 07's screens: link-density-per-word ·
name-prefix and co-pin clustering · near-duplicate `why` across one unit's edges · body-less index ·
co-pinned **and** the `why` adds nothing beyond co-membership (06's anti-mirroring rule).

**`why` is schema-required, so it is not a lint rule.** 07 listed "missing `why`" as mechanically
decidable, but 06 made the field mandatory and a mandatory field is caught by `wcl check` at parse
time. Carrying it in both places means two mechanisms for one rule and a corpus that can be
schema-valid yet lint-dirty. The 736-edge flip (§7) is what makes strictness safe from cutover.

**The gate baseline is in-memory and per-run — there is no baseline file.** 07's gate is "lint must not
get worse than the pre-run baseline", and that comparison lives entirely inside one `wcl wskill op`
invocation: lint before the ops, lint after, diff. Nothing to commit, nothing to rot, nothing to
rubber-stamp. CI's gate is the separate absolute one — errors fail, warnings don't.

### 6. Output, exit codes, and object-addressed comments

`--format text|json`, **text by default**, matching `wcl wdoc comments`. A finding is
`{severity, rule, unit: {kind, id}, file, span?, message}` — the `span` is what lets the editor pin it
and an agent jump to it. Exit **0** = clean · **1** = one or more errors · **2** = tool failure;
warnings never fail, with `--deny warn` to escalate. `--severity <list>` filters, so the curator's
phase 1 is `wcl wskill lint --format json --severity candidate`. **`lint` never writes** — only the
curator files `comments.wcl` entries, and only for judgement findings it declines to fix.

**`Comment` gains an object address**, resolving 07's explicit leftover. Add `object_kind: utf8?` +
`object_id: utf8?`, make `page` optional, require at least one of the two. The curator thinks only in
ids (§3), so its findings are id-addressed for the same reason its writes are — and *"this unit
shouldn't exist"* lands cleanly, as a comment about the unit rather than about any rendering of it.
Three things recommend it: the vocabulary already exists (`/api/object/locate` takes `{kind, target}`
and answers `{file, span}`, so an object-addressed comment resolves to a pin with no new machinery);
it keeps wskill knowledge out of `wcl_wdoc` (`object_kind` works for a WAD `component` too); and it
degrades well, since the editor's click path still supplies `page` + `loc`.

Rejected: pinning to the editor's existing synthetic-unit-page machinery. It displays fine but bakes a
generated page slug into a persisted sidecar, which rots the moment the slug scheme changes.

### 7. The 736-edge flip is `lint --fix`

The flip is a rule over the model that emits `related_remove` ops, and `wcl wskill op` already applies
ops with per-op rollback and `--dry-run`. So it is **`wcl wskill lint --fix`** (`--fix=<rule>` for one
rule; only rules declaring an autofix participate), not a bespoke `migrate-edges` subcommand that
would be a second path to the same write. The ~60–95 survivors then arrive as ordinary findings for
the author pass rather than in an ad-hoc report format.

Rejected: a throwaway script in the migration effort — cheaper now, but it reimplements edge-walking
and op-applying the library already has.

**This creates a sequencing constraint, from §5's ruling that `why` is schema-required**: a bare edge
will not *parse* under the new schema, so the flip cannot run after the schema tightens. The order is

1. ship `wcl_wskill` with `why` **optional**
2. run the flip (`lint --fix`) — 736 → ~60–95
3. author the survivors' reasons
4. **tighten `why` to required** — a one-line schema change, gated on 3

Step 4 is trivially cheap and easy to forget, and forgetting it leaves 06's whole finding
("guidance fails where the schema gives it no place to live") unenforced. Carried onto the map's
**Migration sequencing** patch.

### 8. Where the seam falls

Almost all of `graph.rs`'s `NodeInfo` is format-intrinsic, not editor-specific: `id`, `kind`, `title`,
`file`, `span`, `audience`, `related`, `pinned`, `children`, `related_editable`. Even `visibility` is —
§5's zero-projection rule needs exactly that resolution. Genuinely editor-only: the 60-char block
previews, the force-layout seeding (already `wcl_wdoc::layout_graph`), and the `serde_json::Value` wire
shape.

**All of `NodeInfo` moves to `wcl_wskill` as typed Rust, spans included**; the editor keeps a thin
adapter. Two specifics:

- **The model carries spans.** They are free — it is already walking the AST — and a span-free "pure
  semantic graph" would force the editor to re-parse to recover them, which is precisely the
  degradation this bullet warns against. The curator ignores them; ops stay id-addressed.
- **`related_editable` is library-side.** It marks a `related` field that is a computed expression
  rather than a literal list, which the curator needs as much as the editor does — it is the difference
  between "I can write here" and "I must file a comment instead".

Wil expects a fuller editor rework later; the adapter is deliberately the interim shape. Carried onto
ticket 10.

### 9. The shipped skill bundles no scripts — but the block stays

This skill documents the CLI instead. A bundled script that shells out to `wcl wskill …` is strictly
worse than documenting the command: it adds a layer that drifts, hides the real invocation so the
agent cannot compose flags, and the skill is useless without `wcl` on PATH anyway. The curator cannot
be a script either — 07 made it an agent in two phases and the judgement half is the point.

The `script` block is **kept**, and not merely on generality grounds: Wil confirmed *other* wskills
under way generate AI skills and will use it. So it stops being a zero-user schema feature.
