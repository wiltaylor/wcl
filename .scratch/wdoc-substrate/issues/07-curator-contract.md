# The curator contract — what may it change, and what gates it?

Type: grilling
Status: resolved
Blocked by: 06

## Question

Settled while charting: authors write freely and messily; a **dedicated curator pass** owns the
graph, **edits directly**, and is gated on render + validate. An advisory curator would just be
`linking_discipline` again — guidance with no teeth.

What's open is its contract.

- **Structure only, or bodies too?** This is the sharp one. A hub note's defect is its *content* — it
  carries no fact of its own. A purely structural curator can flag "this unit's body is 80% links"
  but the fix is a rewrite. Options: structure-only and hubs get reported; bodies in scope with the
  render gate as the guard; or a split where the curator restructures and dispatches body rewrites.
- **What ops does it have?** Candidates: merge two units, split one, demote a hub to an `index`,
  promote an index to a unit, prune a link, re-pin, re-kind (concept → fact), delete an orphan, move
  between files. Which are safe to automate, which need a human? Note the editor already has
  span-addressed AST mutation ops (`/api/block/ops`, `/api/nav/op`, `replace_block_by_span`,
  `remove_field`, `set_or_remove_decorator`, `ensure_import`) — establish whether the curator reuses
  that op vocabulary or needs its own.
- **When does it run?** After every authoring session, on demand, on a schedule, as a pre-commit
  gate, or as a phase of the authoring process itself?
- **What exactly gates it?** "Render + validate" needs teeth. Does it mean the four projections all
  still build, the graph lint passes, a diff review, or a before/after structural comparison? A
  curator that can delete a unit needs a stronger gate than one that only prunes links.
- **How is a bad curation caught?** It edits directly, so a wrong merge is a data loss. Git is the
  backstop — is that enough, or does it need a dry-run / journal / revert path?
- **Is it one agent or several?** One curator holding the whole graph, or per-index curators with
  bounded working sets? Scale matters: the `wcl` wskill is 65 units and 69 data files.

Blocked by `06-unit-kinds`: the curator needs a target shape to curate toward.
Feeds `08-wskill-cli`: the CLI surface exists to serve this contract.

## Inherited from ticket 06 — do not re-litigate

Ticket 06 answered the first bullet outright and supplied the target shape.

**Structure only, or bodies too? — SETTLED: structure-only writes, body-level reads.** The curator
reads bodies (hub detection, atomicity and "is this `why` honest" are unanswerable from the graph
alone) but its *edits* are confined to kind, `related`, `why`, and index membership. On finding a hub
it files the finding and at most does the structural half of the fix — pin the children into an
index, delete the hub's edges — leaving "now write the real unit" to an author pass. The reason the
line sits there: structural edits are verifiable by the render+validate gate; prose edits are not, so
a body-writing curator is an unbounded rewrite with no gate over content a human already reviewed.

**The target shape to curate toward:**

- Kinds unchanged: `fact` = indisputable/verifiable, `concept` = a describable model, `entity` =
  external proper nouns only — **rare by nature at ~3%, do NOT curate toward more of them**.
- `related`: untyped (typed relations were considered and rejected), ≤5 on content units, **every
  edge carries a mandatory `why`**, no dangling ids, no reciprocal duplicates.
- Indexes: uncapped, navigation-only, and **linkable iff they carry a `body`**.
- Hubs: a **screen, not a test** — link-density-per-word, plus targets sharing the unit's name prefix
  or all co-pinned under it, plus near-duplicate `why` clauses across one unit's edges.
- Anti-mirroring is a **screen, not a hard gate**: co-pinned units genuinely are the likeliest to
  depend on each other (11–23% density vs a 1–2.5% baseline). The check is co-pinned **and** the
  `why` adds nothing beyond co-membership.

**Also inherited:** the guidance in `linking_discipline` / `unit_decision_guide` **is** being followed
— measurably. Do not design the curator around "authors ignore the rules". Design it around the one
failure mode measurement found: rules with nowhere in the schema to live.

## Resolution

The curator is **one agent running in two phases**, with a **prose-forward-only** write set, gated by
**render + a baseline lint diff**, backstopped by **git**.

### 1. Shape — mechanical screen, then targeted read

The CLI computes every screen that needs no judgement — link-density-per-word, name-prefix and
co-pin clustering, near-duplicate `why` text across one unit's edges, cap violations, dangling ids,
orphans, body-less indexes — and emits a **candidate list**. The agent reads only the flagged units
and their neighbours, and rules on each.

Rejected: one agent holding the whole graph (works at 65 units, dies later), and per-index curators
with bounded working sets (the sharpest defects live exactly at the boundaries a bounded set can't
see, and it needs a conflict-merge step). The chosen shape is the only one where **scale stops
mattering** — the screens are thresholded, so a 500-unit wskill produces a candidate list of the same
order as a 65-unit one. It also gives `08-wskill-cli` a much sharper brief than "expose `graph.rs`":
**reduce the graph to what needs judgement.**

Accepted cost: a thresholded screen has false negatives. A defect scoring below threshold is
invisible to the curator permanently, where a whole-graph read might have caught it.

### 2. The write set

- **add** a `related` edge, **authoring its `why`**
- **prune** a `related` edge
- **re-kind** a unit (`concept` ⇄ `fact` ⇄ `entity`)
- **pin / unpin / reorder** index membership; **create / delete / nest** an index
- **file a comment** into `comments.wcl` (see §3)

**Never**: backfills a `why` onto a pre-existing bare edge; edits a body; merges, splits or deletes a
unit.

**The line is forward-only prose, and Wil drew it** (the grill had proposed the cruder "never writes a
sentence into the model", which would have made the curator prune-only and unable to repair a missing
link): *writing the reason for an edge you are creating is stating why you made it; writing one for
someone else's existing edge is fabricating it.* So the curator authors `why` **forward, never
backwards**. This is self-enforcing in a way "structure-only, bodies excluded" is not — the curator
cannot hold a reason it doesn't have.

### 3. Findings go to `comments.wcl`

Everything the curator may not fix — hubs, non-atomic units, dishonest `why`s — is filed as a
`comment` block with `author = "curator"` in the wskill's existing sidecar.

Zero new machinery: `comments.wcl` already sits beside three of the four in-repo wskills, keyed by
page + block locator, carrying `body`/`author`/`status` (`:open`/`:resolved`); it surfaces as pins in
the `wcl editor` preview pane — **the human loop the charting grill chose** — is read back by
`wcl wdoc comments` for an author-pass agent, resolved by `wcl wdoc comments resolve <id>`, and is
watcher-ignored so filing costs no rebuild.

Rejected: JSON-to-stdout only (findings die with the terminal) and a new first-class `finding` block
(new schema, renders into the book unless suppressed, duplicates `comment`).

**Open detail for `08`:** a finding about a unit with no rendered page — a body-less index, or a unit
whose page a projection doesn't build — has no block to pin to. `page_file` + a whole-page comment
covers most cases; "this unit shouldn't exist" doesn't fit cleanly.

### 4. Ops — shared vocabulary, transport is 08's call

The curator issues the **same named ops with the same semantics** as the editor (`pin_unit`,
`unpin_unit`, `reorder_children`, `create_index`, `delete_index`, `move_index`, `promote_index`,
`demote_index`, `related_add`, `related_remove`). One definition of what each op means; no second code
path to drift the way the scaffold WAD templates did.

Whether they arrive over HTTP or a direct library call is **ticket 08's decision**. A running
`wcl editor` is **not** required — the curator runs headless at the end of an agent session.

Note the editor's two op layers are addressed differently: `/api/nav/op` is **id-addressed** and is
already almost exactly the curator's write set (not a coincidence — both edit the same structure),
while `/api/block/ops` is **span-addressed** because it's driven by clicking a rendered block. The
curator thinks in ids and never in spans, so span-addressed block ops are a UI affordance, not a
curation primitive.

### 5. When it runs

A **terminal phase of `adding_content` and `researching_a_topic`**, scoped to the units just touched
plus their 1-hop neighbourhood and the indexes they're pinned under — cheap, because those units are
already in the authoring session's context. Whole-graph passes are available **on demand but never
automatic**.

Rejected: on-demand-only (the graph drifts between runs) and a commit/CI gate (it makes an LLM
judgement call a merge blocker, and 06 established the authors aren't the problem).

**Separated out first:** the mechanically decidable checks — dangling id, over-cap, missing `why`,
orphan unit, reciprocal duplicate — are a **linter**, and belong in CI regardless of what the curator
does. The curator is only the judgement half. (Rule-set ownership is `08`'s.)

Accepted cost: a 1-hop window is structurally blind to hub-ness, near-duplicate `why` clauses and
index bloat — no single addition visibly crosses those thresholds. That is what the on-demand
whole-graph pass is for.

### 6. The gate — two tiers

- **Per-op**: schema validation with per-op rollback, reusing the editor's existing `commit` pipeline
  (`crates/wcl/src/edit.rs`) — milliseconds, already implemented, already baseline-diffs
  `schema_errors` rather than checking absolutely.
- **Per-run**: all four projections (book / skill / training / presentation) build, **and lint
  violations must not increase against the pre-run baseline**. A run-level failure reverts to the
  run's starting state.

A full render costs seconds-to-a-minute (WAD's HTML build is 55s debug), so it cannot run per op —
hence the split. The lint side **must** be a baseline diff, not an absolute check: the graph enters
the world with 736 bare edges and 6 dangling ids, so an absolute gate could never pass on first run.
The only workable rule is *you may not make it worse*.

### 7. Backstop — git, plus `--dry-run`

Clean tree required at entry (which §6's revert path needs anyway); the run lands as **one commit**
summarising what it changed and why; a bad run is `git revert`. **No human gate on the pass itself** —
a mandatory review of every incremental pass reintroduces the human into the loop the curator exists
to remove, and a diff of 40 `related` edits is precisely what a human skims and approves.

`--dry-run` prints the op list without applying it — how you trust the thing on the first few runs,
and how it can be pointed at a wskill you don't own.

Rejected: an op journal with selective revert. That's machinery for a problem that only exists if ops
are expensive to redo, and 06 made sure they aren't — every surviving op is small and independent.

### 8. Consequence for the migration (feeds the map's migration fog)

`why` is a **required** schema field under 06 decision 4, so at the schema flip all **736** bare edges
stop validating — the migration is forced, not optional. Combined with the curator's forward-only
rule, which puts backfilling out of its reach:

> **Delete every bare edge except where the target id or title already appears in the source unit's
> prose.** 06 measured that at 8–13%, so **~60–95 edges survive** across the four wskills and are
> flagged for an **author pass** to write their `why`. Everything else dies.

The test is mechanically computable, and it preserves exactly the edges with evidence of authorial
intent while destroying exactly the ones with none. **The schema flip becomes a filter, not a wipe.**

Rejected: deleting all 736 (loses the load-bearing spine along with the noise) and a one-off
migration agent writing all 736 (fabricates 640+ reasons nobody held).

Accepted cost: the prose-mention test has false negatives — an author who linked for a good reason but
phrased it without naming the target loses that edge. It also needs a migration tool nobody has
specified yet.
