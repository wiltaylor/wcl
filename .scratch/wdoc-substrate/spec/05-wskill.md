# 05 — The wskill format, CLI and curator

Source: tickets [06](../issues/06-unit-kinds.md), [07](../issues/07-curator-contract.md),
[08](../issues/08-wskill-cli.md), [09](../issues/09-research-flow.md).

This half of the map is about **how the agent authors** — the wskill format's data model, the tooling
that keeps its graph honest, and the two authoring processes that produce it.

## What the measurements corrected

The charting premise for this half was *"the wskill graph has collapsed and the authoring guidance is
ignored."* **Two of its three claims were measurement errors**, and the correction is the single most
important thing to carry forward:

- **The kind vocabulary is not collapsing.** `wcl`'s 45/2/18 is one topic shape, not the norm — wdoc is
  fact-dominant (34/13), wad procedure-heavy (22), wskill balanced. Reading the ids, all four apply the
  shipped guide correctly. WCL genuinely is 45 ideas.
- **The `related` cap is being respected.** The cited violations (19 and 12 ids) are **indexes**, which
  the cap explicitly exempts. Content units over cap: **wcl 0%, wskill 0%, wdoc 2%, wad 4%.**
- **Index membership is not mirrored into `related`.** 11–23% link density among co-pinned pairs vs a
  1–2.5% baseline — a 10× ratio that is topical adjacency, not mirroring (which would read 80–100%).

**So the guidance *is* being followed, measurably.** The map's standing preference "prose guidance is
not a mechanism" survives only in a narrower, better-evidenced form: **guidance fails where the schema
gives it no place to live.** The one rule that did not bind — *"say why the reader would follow this
link"* — is the one with nowhere to write it down, because `related` is a bare id list.

**What is real: 87–92% of `related` edges are bare** (WAD: 0 of 175). The graph has nodes and edges;
the edges carry no meaning.

---

## 5.1 The format changes

### 5.1.1 The kind vocabulary stands unchanged

`concept` / `entity` / `fact` / `procedure` / `index` / `example` / `research` all survive with their
current definitions. `fact` = *verifiable and indisputable*; `concept` = *a mental model, something less
concrete that has to be described*. **No mechanical test is needed and none should be invented** — the
prose definitions bind, as measured.

The `concept`/`fact` split is kept **despite having almost no rendering consequence today** (identical
fields; `Concept` adds `summary`, says `name` for `title`; their two page components diff to one
rendered italic line). It is real to a reader — "explain this to me" vs "let me look this up" — and the
corpus proves authors apply it reliably.

**`entity` at 3% is correct rarity, not underuse.** All 8 distinct entities across all four wskills are
external proper nouns or the tool itself. The historical failure was **over**-use — Wil: *"in the past
it was trying to log parts of the application as entities which is incorrect by my definition of it."*
**Reading 3% as a new failure would over-correct back into the original bug.**

*Rejected:* typed relations (`depends-on` / `see-also` / `part-of`). They would fix none of the four
real findings while adding a per-link judgement call to every unit — and `see-also` is exactly the kind
of soft category an agent would coin for everything, the way it once coined `entity` for everything.
Direction is *already* typed by render: "Related" means I chose you, "Referenced by" means you chose me.

### 5.1.2 Every `related` edge carries a **mandatory** `why`

`related` becomes a list of `{id, why}` (or a repeatable `link <id> "why"` child), rendering as
`- [Name](href) — <why>`.

Wil's framing settled it: *"it's a node graph that the reader can navigate around. If we link everything
to everything we lose this. The relationship between nodes also helps enrich the information like it
does in a zettelkasten."* In a zettelkasten the edge's value **is** its annotation.

Mandatory, not optional — Wil: *"more we think about it reason is important. lets make it mandatory."*
An optional annotation on a free-to-add field is precisely the guidance-without-mechanism pattern this
map rules out. What it buys:

- **Over-linking becomes self-limiting.** A bare id is free to add, which is why the pressure is always
  upward. A required clause makes the marginal link cost real. That is a *mechanism*.
- **`linking_discipline`'s own rule becomes enforceable** — "either say why the reader would follow it,
  or drop it" is currently unenforceable because there is nowhere to say it.
- **The curator gets something judgeable.** An edge whose `why` is vague or duplicated is visibly bad.

**Migration: 736 existing edges.** See §5.3.7 and part [07](07-migration.md) — it became mechanical.

### 5.1.3 Indexes become linkable nodes — iff they carry a `body`

Half the mechanism already exists: `book/main.wcl:113–130, 216–225` distinguishes a **content index**
(has a `body`) — which gets its own page `index_<id>` plus a chapter entry — from a **nav index**, a
pure sidebar heading with no page. `all_indexes = flatten([indexes, child_indexes])`, so nested
sub-headings already get pages on the same rule. Today: **53 indexes across the four wskills, 6 with
bodies** (wcl 1, wad 5, wdoc 0, wskill 0).

But indexes are **not** in `all_units`, so a `related` id naming one resolves to nothing and renders no
bullet, silently.

**The rule: an index is linkable iff it has a `body`.** Not arbitrary — it is the zettelkasten
structure-note distinction, and it is the only rule consistent with the hub-note prohibition:

- **A nav index has nothing to link to, by design.** Auto-generating pages for the 47 body-less indexes
  would produce pages that are lists of links to their own children with no content of their own —
  **the hub-note anti-pattern, machine-generated at scale**.
- **It makes the author's choice cost something real.** Want this area linkable? Write the paragraph
  saying what the area *is*.
- **It closes a loop `linking_discipline` leaves open.** Its prescribed hub fix is "delete the hub and
  pin its children into an index" — but nothing can point *at* that index today, so an author who wants
  to say "the rest of this area lives over there" has no target and is pushed back toward writing a hub.

What this commits the spec to:

1. Body-carrying indexes (including nested sub-indexes) join `all_units`. The `index_<id>` href already
   exists in the book.
2. **The skill projection needs matching pages for content indexes.** `skill/main.wcl:9` states *"There
   are NO per-kind index pages"* — they are inlined as bullet sections in SKILL.md. Without this, an
   index link resolves in the book and dies in the skill, violating the four-way projection rule
   ([03](03-templates.md) §3.6.6 made non-negotiable).
3. A `related` id naming a **body-less** index is a build error.
4. Index pages get "Referenced by" like any other node.

### 5.1.4 Reciprocal edges dedupe

`related` renders as two columns — "Related" (ids I chose) and "Referenced by" (computed by
`referenced_by(id)`) — with **no dedup**, and reciprocity is **32–48%** of all edges. With `why`
mandatory, a reciprocal pair would render once *with* a reason and once *bare* on the same page.
**Suppress from "Referenced by" anything already in "Related".**

### 5.1.5 `Comment` gains an object address

Add `object_kind: utf8?` + `object_id: utf8?`; make `page` optional; require at least one of the two.

The curator thinks only in ids, so its findings are id-addressed for the same reason its writes are —
and *"this unit shouldn't exist"* lands cleanly, as a comment about the unit rather than about any
rendering of it. Three things recommend it: the vocabulary already exists (`/api/object/locate` takes
`{kind, target}` and answers `{file, span}`, so an object-addressed comment resolves to a pin with no
new machinery); it keeps wskill knowledge out of `wcl_wdoc` (`object_kind` works for a WAD `component`
too); and it degrades well, since the editor's click path still supplies `page` + `loc`.

*Rejected:* pinning to the editor's synthetic-unit-page machinery — displays fine, but bakes a
generated page slug into a persisted sidecar, which rots the moment the slug scheme changes.

### 5.1.6 The duplication dies

`schema/base.wcl` plus **14 topic-agnostic wdoc templates** are copied verbatim into every wskill —
~56 copies — and two CI gates diff the copies. That is maintained to permit **five** documented
divergences, three of which are `wcl` and `wdoc` being reference-heavy.

They become `import <wskill.wcl>` from a registry **embedded in `wcl_wskill`** — exactly what
`wcl_wdoc` already does with `lib/*.wcl` and what every wskill already does with `import <wdoc.wcl>`.
`wcl init wskill` scaffolds a handful of entry files instead of ~20.

Wil's reason: *"we have seen a heap of problems with this by drift all over the place. My original
reason was I wanted to allow the author complete control to extend but never seen that in practice."*
The measurement agrees — 9% of copies diverge, most of it two topics.

**Overrides are import granularity — no shadowing mechanism.** Parts are separately importable
(`import <wskill/book.wcl>`, `<wskill/component/common.wcl>`, …) with `import <wskill.wcl>` as the
everything prelude — `wcl_wdoc`'s `wdoc.wcl → prelude.wcl → parts` shape. A topic wanting its own book
main **doesn't import that part and declares its own**; nothing imported a competing name, so nothing
shadows. The cost is an ergonomic cliff — overriding one part means swapping the one-line aggregate
import for enumerated part imports — **and that is the right shape**: the 95% case is one line, the 5%
case says out loud what it opts out of. This dissolved a queued language-level shadowing ticket.

The three legitimately-divergent files (`wcl`'s `common.wcl`, `wcl` + `wdoc`'s `book/main.wcl`) switch
to enumerated part imports.

*Rejected:* migrating the drift gates as a version-compatibility check ("your copies match the version
this binary ships") — it works, and outside this repo too, but it preserves the copies.

### 5.1.7 `question` gains an index-node field

From §5.4: a `question` block tagged `research_item` must name its index node. Smallest form is one
optional `identifier` field. Lands in the embedded registry, so it is **one edit in one place, not
four copies**.

---

## 5.2 `crates/wcl_wskill`

Layering becomes `wcl_lang → wcl_wdoc → wcl_wskill → wcl`. The model, the lint rules, the op vocabulary
and the embedded schema/templates live there; `wcl wskill` and the editor's `/api/graph` +
`/api/nav/op` both become **thin adapters** over it.

*Rejected:* inside `wcl_wdoc` (would push wskill's vocabulary *into* the wdoc crate in the same map
where [01](01-language.md) is pulling wdoc's vocabulary *out* of `wcl_lang` — same argument, opposite
direction); `crates/wcl/src/wskill/` (out of the editor but still in the binary, so the gates still
travel with this repo).

The chosen shape **forces the editor-degradation question to be answered at a crate boundary**: if
`/api/graph` cannot be rebuilt as an adapter, the seam is in the wrong place.

### 5.2.1 What moves

`crates/wcl/src/editor/graph.rs` (639 lines) already computes the model. Almost all of its `NodeInfo`
is **format-intrinsic, not editor-specific**: `id`, `kind`, `title`, `file`, `span`, `audience`,
`related`, `pinned`, `children`, `related_editable`. Even `visibility` is — §5.3.4's zero-projection
rule needs exactly that resolution.

**All of `NodeInfo` moves as typed Rust, spans included.** The editor keeps a thin adapter that adds
60-char block previews, calls `wcl_wdoc::layout_graph`, and serialises.

- **The model carries spans.** They are free — it is already walking the AST — and a span-free "pure
  semantic graph" would force the editor to re-parse to recover them, which is precisely the
  degradation to avoid. The curator ignores them; ops stay id-addressed.
- **`related_editable` is library-side.** It marks a `related` field that is a computed expression
  rather than a literal list — the difference between "I can write here" and "I must file a comment
  instead", which the curator needs as much as the editor does.

**`sidecar.rs`'s wskill knowledge generalises rather than moves.** Its callers are inside `wcl_wdoc`
(`sidecar.rs:38,60` walks up looking for `wskill.wcl`), so moving it would invert the dependency. The
concept is "the nearest owning document root"; the marker filename becomes a parameter and `wcl_wdoc`
stops naming the format.

### 5.2.2 Implementation constraint: gitspec plumbing

`wcl wskill audit` (§5.3.2) loads a wskill's model **at a git revision**, so the gitspec machinery that
today lives in `crates/wcl` (`src/gitspec.rs`, materialising a rev via `git archive | tar`) must be
reachable from the library. **It is not today.** This needs to be in scope from the start; it is not an
editor concern.

---

## 5.3 The CLI

### 5.3.1 Six subcommands

| | | |
|---|---|---|
| `wcl wskill graph` | the model as JSON | editor, curator, authoring agent |
| `wcl wskill lint` | **all** findings — `error` / `warn` / `candidate` | CI; curator phase 1 |
| `wcl wskill check` | projections build, artifacts resolve, coverage | CI |
| `wcl wskill op` | apply ops; `--dry-run` | curator |
| `wcl wskill install` | render the skill into `.claude/skills/` + `agents/` | anyone shipping a wskill |
| `wcl wskill audit [<rev>..<rev>]` | the before ∪ after graph diff | reviewing agent output |

**Nomination is a severity, not a seventh command.** The mechanically-decidable / needs-judgement line
is a split in *responsibility*, not in commands — both are the same operation (run rules over the
model, emit findings). One rule engine, one finding schema; CI ignores `candidate`, the curator asks
for nothing else.

**`lint` and `check` stay separate on cost, not on kind.** `lint` reads the model — cheap enough for an
authoring agent to call in a loop. `check` builds four projections through the template layer measured
at 36% of a build.

### 5.3.2 `audit` deliberately breaks the five-command precedent

Justification: **every other subcommand takes one graph; this one takes two.** A `--since` flag on
`graph` and `lint` was the alternative and was rejected — it would make every one of those commands'
outputs conditionally a diff. See [06](06-editor.md) for what it renders.

Range is an **arbitrary git range, defaulting to `HEAD~1`** — an authoring session is often several
commits and a branch is the natural review unit. Matches `wcl diff`, which already takes `<rev>:<path>`
specifiers.

### 5.3.3 One op vocabulary, id-addressed

`pin_unit` · `unpin_unit` · `reorder_children` · `create_index` · `delete_index` · `move_index` ·
`promote_index` · `demote_index` · `related_add` · `related_remove`

Every op is addressed by **`(kind, id)`** — `kind` optional in the JSON and inferred when unambiguous,
so an agent writing ops by hand isn't punished — and defined **once** in `wcl_wskill::ops`. The
editor's span-addressed `related_add` / `related_remove` become a **resolver in front of the same
function**: a drag resolves `span → (kind, id)` before calling it.

`/api/nav/op` was already id-addressed; only `related_*` sat on the span-addressed side, because in the
editor you drag an edge between rendered nodes. **The curator has never seen a rendered page**, so a
span-addressed op is a UI affordance, not a curation primitive. Consequence: the `--dry-run` op list is
the same JSON the editor would have sent.

`(kind, id)` rather than bare `id` because **unit ids are assumed unique across kinds and nothing
enforces it** — `related: list<identifier>` resolves against a flat `all_units`. An op that silently
hits the wrong kind is the failure mode.

`wcl wskill op` targets the **wskill root** (`wskill.wcl`), not a projection entry — the curator
operates on the format, not on one view of it.

### 5.3.4 The lint rule set

**Errors** (mechanically certain; CI fails):

- **dangling `related` id** — 6 exist today; renders nothing, warns nobody
- **duplicate unit id across kinds** — uniqueness is assumed by `all_units` and unenforced
- **`related` id naming a body-less index** — a dangling link wearing a valid id (§5.1.3)

**Warnings** (real signal, real exception rate; never fail CI):

- **over-cap `related` on a content unit** (3–5) — measured 0–4% violation; a cap honoured with genuine
  exceptions is guidance, not truth
- **unit pinned by no index** — orphan, reachable only by link
- **unit reaching zero projections** — an `audience` gap; authored and invisible everywhere

**Candidates** (nominations to the curator; fail nothing) — the judgement screens:
link-density-per-word · name-prefix and co-pin clustering · near-duplicate `why` across one unit's
edges · body-less index · co-pinned **and** the `why` adds nothing beyond co-membership.

**`why` is schema-required, so it is NOT a lint rule.** A mandatory field is caught by `wcl check` at
parse time. Carrying it in both places means two mechanisms for one rule and a corpus that can be
schema-valid yet lint-dirty.

### 5.3.5 Output and exit codes

`--format text|json`, **text by default**, matching `wcl wdoc comments`. A finding is
`{severity, rule, unit: {kind, id}, file, span?, message}` — the `span` is what lets the editor pin it
and an agent jump to it.

Exit **0** = clean · **1** = one or more errors · **2** = tool failure. Warnings never fail;
`--deny warn` escalates. `--severity <list>` filters, so the curator's phase 1 is
`wcl wskill lint --format json --severity candidate`.

**`lint` never writes.** Only the curator files `comments.wcl` entries, and only for judgement findings
it declines to fix.

### 5.3.6 Recipe disposition

| | |
|---|---|
| **die** | `wskill-schema-check`, `wskill-schema-sync`, `wskill-template-check`, `wskill-crosstopic-check` — they police copies that no longer exist |
| **→ `check`** | `wskill-artifact-check` (off the parsed model, not a regex over `entry = "…"`), `wskill-coverage` (report; its sharp case becomes a lint rule), `skills-check`'s projection-builds half |
| **→ `install --check`** | `skills-check`'s artifact drift, agent-name collisions, stale-generated detection |
| **stays** | `wcl-refcheck` — this repo's docs-vs-binary gate; no other wskill documents `wcl` |

**Note the original count was wrong**: there are **eight** recipes, not seven, and `wskill-check` does
not exist. No back-port gate survives — with the duplication gone there is nothing to back-port.
`docs/wskills/coverage.repl` and its `sed`-quote-stripping pipeline go too.

### 5.3.7 The 736-edge flip is `lint --fix`

The flip is a rule over the model emitting `related_remove` ops, and `wcl wskill op` already applies
ops with per-op rollback and `--dry-run`. So it is **`wcl wskill lint --fix`** (`--fix=<rule>` for one
rule; only rules declaring an autofix participate), not a bespoke `migrate-edges` subcommand that would
be a second path to the same write. The survivors arrive as ordinary findings for the author pass.

**The rule:**

> Delete every bare edge **except** where the target id or title already appears in the source unit's
> prose. Measured at 8–13%, so **~60–95 edges survive** across the four wskills and are flagged for an
> author pass. Everything else dies.

Mechanically computable; preserves exactly the edges with evidence of authorial intent and destroys
exactly the ones with none. **The schema flip becomes a filter, not a wipe.**

*Rejected:* deleting all 736 (loses the load-bearing spine with the noise); a one-off migration agent
writing all 736 (fabricates 640+ reasons nobody held); a throwaway script (reimplements edge-walking
and op-applying the library already has).

**Accepted cost:** the prose-mention test has false negatives — an author who linked for a good reason
but phrased it without naming the target loses that edge.

### 5.3.8 The shipped skill bundles no scripts — but the `script` block stays

The skill documents the CLI instead. A bundled script that shells out to `wcl wskill …` is strictly
worse than documenting the command: it adds a layer that drifts, hides the real invocation so the agent
cannot compose flags, and the skill is useless without `wcl` on PATH anyway. The curator cannot be a
script either — it is an agent in two phases and the judgement half is the point.

The `script` block is **kept** — Wil confirmed *other* wskills under way generate AI skills and will
use it, so it stops being a zero-user schema feature.

---

## 5.4 The curator

**One agent in two phases**, with a **prose-forward-only** write set, gated by **render + a baseline
lint diff**, backstopped by **git**.

### 5.4.1 Shape — mechanical screen, then targeted read

The CLI computes every screen that needs no judgement and emits a **candidate list**; the agent reads
only the flagged units and their neighbours, and rules on each.

*Rejected:* one agent holding the whole graph (works at 65 units, dies later); per-index curators with
bounded working sets (the sharpest defects live exactly at the boundaries a bounded set cannot see, and
it needs a conflict-merge step).

**The chosen shape is the only one where scale stops mattering** — the screens are thresholded, so a
500-unit wskill produces a candidate list of the same order as a 65-unit one. **Accepted cost:** a
thresholded screen has false negatives; a defect scoring below threshold is invisible permanently.

### 5.4.2 The write set

**May:** add a `related` edge, **authoring its `why`** · prune a `related` edge · re-kind a unit
(`concept` ⇄ `fact` ⇄ `entity`) · pin / unpin / reorder index membership · create / delete / nest an
index · file a comment.

**Never:** backfill a `why` onto a pre-existing bare edge · edit a body · merge, split or delete a unit.

**The line is forward-only prose, and Wil drew it** (the session had proposed the cruder "never writes
a sentence into the model", which would have made the curator prune-only and unable to repair a missing
link): *writing the reason for an edge you are creating is stating why you made it; writing one for
someone else's existing edge is fabricating it.* **This is self-enforcing in a way "structure-only,
bodies excluded" is not** — the curator cannot hold a reason it does not have.

**Structure-only writes, body-level reads.** It reads bodies — hub detection, atomicity and "is this
`why` honest" are unanswerable from the graph alone — but its edits are confined to kind, `related`,
`why`, and index membership. The reason the line sits there: structural edits are verifiable by the
render+validate gate; prose edits are not. Nothing in the pipeline can tell you a rewrite preserved a
unit's meaning, so a body-writing curator is an unbounded rewrite with no gate over content a human
already reviewed. It also keeps the contract specifiable — "reshape the graph" has a target shape,
"improve the prose" does not.

### 5.4.3 Hubs are a screen, not a test

**No hub notes currently exist in the corpus.** The lowest words-per-link units (30–56 words, 2–4
links) are legitimate atomic units, and `linking_discipline` records that the two real hubs (`strings`,
`cli`) were already found and deleted by hand. The one remaining candidate is `wdoc/fact themes`.

Link-density-per-word plus "targets share my name prefix / are all co-pinned under me" **flags
candidates**; it cannot separate a hub from a genuinely terse unit without reading the body. **Mandatory
`why` adds a strong new signal**: a hub's reasons come out near-identical, and near-duplicate `why`
clauses across one unit's edges *is* mechanically detectable.

On finding a hub the curator files the finding and at most performs the **structural half** of the fix
(pin the children into an index, delete the hub's edges), leaving "now write the real unit" to an
author pass.

Anti-mirroring is likewise a **screen, not a hard gate** — co-pinned units genuinely *are* the likeliest
to depend on each other. The check is **co-pinned AND the `why` adds nothing beyond co-membership**.

### 5.4.4 Findings go to `comments.wcl`

Everything the curator may not fix — hubs, non-atomic units, dishonest `why`s — is filed as a `comment`
block with `author = "curator"` in the wskill's existing sidecar.

**Zero new machinery:** `comments.wcl` already sits beside three of the four in-repo wskills, keyed by
page + block locator, carrying `body`/`author`/`status`; it surfaces as pins in the `wcl editor`
preview pane, is read back by `wcl wdoc comments`, resolved by `wcl wdoc comments resolve <id>`, and is
watcher-ignored so filing costs no rebuild. §5.1.5 adds the object address for page-less findings.

*Rejected:* JSON-to-stdout only (findings die with the terminal); a new first-class `finding` block
(new schema, renders into the book unless suppressed, duplicates `comment`).

### 5.4.5 When it runs

A **terminal phase of `adding_content` and `researching_a_topic`**, scoped to the units just touched
plus their 1-hop neighbourhood and the indexes they are pinned under — cheap, because those units are
already in the session's context. Whole-graph passes are available **on demand but never automatic** —
**except for `researching_a_topic`, where whole-graph is mandatory** (§5.5).

*Rejected:* on-demand-only (the graph drifts between runs); a commit/CI gate (makes an LLM judgement
call a merge blocker, and the authors aren't the problem).

**Separated out first:** the mechanically decidable checks are a **linter** and belong in CI regardless
(§5.3.4). The curator is only the judgement half.

**Accepted cost:** a 1-hop window is structurally blind to hub-ness, near-duplicate `why` clauses and
index bloat — no single addition visibly crosses those thresholds. That is what the whole-graph pass is
for.

### 5.4.6 The gate — two tiers

- **Per-op:** schema validation with per-op rollback, reusing the editor's existing `commit` pipeline
  (`crates/wcl/src/edit.rs`) — milliseconds, already implemented, already baseline-diffs
  `schema_errors` rather than checking absolutely.
- **Per-run:** all four projections build, **and lint violations must not increase against the pre-run
  baseline**. A run-level failure reverts to the run's starting state.

A full render costs seconds-to-a-minute, so it cannot run per op — hence the split. **The lint side must
be a baseline diff, not an absolute check**: the graph enters the world with 736 bare edges and 6
dangling ids, so an absolute gate could never pass on first run. The only workable rule is *you may not
make it worse*.

**The baseline is in-memory and per-run — there is no baseline file.** The comparison lives entirely
inside one `wcl wskill op` invocation: lint before, lint after, diff. Nothing to commit, nothing to
rot, nothing to rubber-stamp. CI's gate is the separate absolute one.

### 5.4.7 Backstop — git, plus `--dry-run`

Clean tree required at entry (which the revert path needs anyway); the run lands as **one commit**
summarising what it changed and why; a bad run is `git revert`. **No human gate on the pass itself** —
Wil's explicit call, on the grounds that a mandatory review of every incremental pass reintroduces the
human into the loop the curator exists to remove, and a diff of 40 `related` edits is precisely what a
human skims and approves.

`--dry-run` prints the op list without applying it — how you trust the thing on the first few runs, and
how it can be pointed at a wskill you don't own.

*Rejected:* an op journal with selective revert — machinery for a problem that only exists if ops are
expensive to redo, and every surviving op is small and independent.

---

## 5.5 The authoring processes

**Measured first: `researching_a_topic` has never been run.** All four in-repo wskills carry **zero**
`research` units and no `data/questions.wcl`; so do the installed skills under `~/.claude/skills`. The
only `research` block in the repo is the fill-out template. Two consequences: §5's "the graph is
healthy" finding measures wskills authored by hand through `adding_content` and says **nothing** about
the fan-out; and the sprawl this section prevents has never actually been observed. **Every decision
here is reasoned, not measured — don't cite them as evidence later.**

**The reframe: the fan-out was never the sprawl source.** Step 5 reads *"For each research unit, run the
[Adding content] loop"* — a **per-finding** loop. That is what turns N researchers into N overlapping
unit sets. Once distillation is a single pass over **all** findings filling a declared shape,
researcher blindness costs duplicated *effort* but not duplicated *units*.

### 5.5.1 `researching_a_topic`, rewritten

1. **Scope** — interview; **now also authors the provisional index tree, one `body` per node**
2. **Decompose** into research items, **each hung off an index node**
3. **Dispatch** researchers in parallel — **prompt now carries the whole tree**
4. **Gate** — coverage only (unchanged)
5. **Distil** — **pass A revise the tree, pass B fill it**
6. ~~Build the index~~ — **dissolved into 1 and 5A**
7. **Verify**
8. **Review** with the owner — **preceded by a whole-graph curator pass**

**The shape is a provisional index tree at step 1.** `data/indexes.wcl` is written from the scoping
interview. No new construct — `index` is already first-class and already nests one level. This kills the
"index retrofitted onto units that already exist" fault **by construction**.

**Every skeleton node carries a `body`, written at scoping time.** One artifact does three jobs: the
reader's area page (§5.1.3 made a bodied index linkable), the **researcher's brief**, and the
**distiller's fill contract**. Free sprawl check falls out: *a node nobody can write a scope for is a
node that shouldn't exist.* This also gives §5.1.3's rule a producer.

**The fan-out survives unchanged.** It is the reason research is fast, and its blindness now costs only
duplicated effort. Each researcher's prompt gains the whole index tree alongside its own node; the
merge-safe one-file-per-agent rule stands untouched. No reconciliation stage — skeleton-driven
distillation already performs that read.

**The gate stays coverage-only** — Wil's call, against the recommendation. *Recorded risk (raised,
overruled, proceed):* revision and fill become the same act, which is the pressure that made the index a
retrofit in the first place. **Mitigated entirely by the pass split, which is therefore load-bearing,
not a detail:**

- **Pass A — revise.** Read **all** findings, revise the tree against what came back. A **required act,
  not a permitted one**: research reliably discovers the shape was wrong, and a frozen skeleton would
  force findings into wrong boxes — the standard and legitimate charge against outline-first. Includes
  the cheap check that **every node is covered by some finding**; an uncovered node is dropped or
  yields a follow-up research item.
- **Pass B — fill.** Write units into the settled tree. **No unit is written against an unrevised node.**

**The terminal curator pass is whole-graph on this process, always.** Free on a from-scratch run
(everything is newly touched, so 1-hop already spans the graph); correct where it actually bites — a
research run **extending an existing wskill**, which is precisely the hub-ness / duplicate-`why` /
index-bloat class a 1-hop window cannot see.

### 5.5.2 `adding_content` reorders to place-before-write

`decompose → classify → **place** → write → link → render`. The old terminal `pin` step moves ahead of
authoring: pick the index node the unit belongs under (or argue for a new one) before the prose is
written.

Nearly free at single-unit scale, and *"no node wants this"* becomes information available **before**
the writing rather than after — which is what stops `related` being the author's after-the-fact
justification for a unit that already exists. Both processes now tell one shape-first story.

### 5.5.3 Consequences for the wskill's own content

- `data/process/researching_a_topic.wcl` rewritten per §5.5.1; its `verification` string changes with it
- `data/process/adding_content.wcl` reordered per §5.5.2
- `data/process/building_the_index.wcl` gains "author at scoping, one body per node" and is re-pointed
  as a step-1 reference
- the `wskill-researcher` agent contract (generated from the wskill's own `agent` block, rendered to
  `.claude/agents/wskill-researcher.md`) gains the index tree in its prompt
- **Nothing here needs a `wcl wskill` subcommand.** The node-coverage check in pass A is a distiller
  obligation, not a lint rule — though it is the obvious candidate if it later wants teeth.

---

## Checklist for this part

- [ ] `related` becomes `{id, why}` with `why` **optional at first** (see [07](07-migration.md) ordering)
- [ ] Bodied indexes join `all_units`; skill projection grows index pages; body-less-index link = error
- [ ] Reciprocal dedup in "Referenced by"
- [ ] `Comment` gains `object_kind` / `object_id`; `page` optional; ≥1 of the two required
- [ ] `question` gains its index-node field
- [ ] `crates/wcl_wskill` — `NodeInfo` with spans, ops, lint rules, embedded schema + 14 templates
- [ ] **gitspec plumbing reachable from the library**
- [ ] Six subcommands; three severities; exit codes; `--dry-run`; `--deny warn`
- [ ] Editor `/api/graph` + `/api/nav/op` rebuilt as adapters; span→`(kind,id)` resolver for `related_*`
- [ ] Four recipes deleted, three folded into `check`, one into `install --check`, `wcl-refcheck` stays
- [ ] Curator: two phases, forward-only `why`, two-tier gate, one commit per run
- [ ] Both authoring processes rewritten; researcher agent prompt updated
- [ ] `why` tightened to **required** — last, and easy to forget ([07](07-migration.md))
