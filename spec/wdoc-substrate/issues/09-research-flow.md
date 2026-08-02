# How do research findings become units without manufacturing sprawl?

Type: grilling
Status: resolved
Blocked by: 06

## Question

`researching_a_topic` is a machine for producing exactly the problem this map exists to fix. Its
steps: scope → decompose into research items → **dispatch researchers in parallel** → completeness
gate → distill findings into units → **build the index** → verify → review.

Two structural faults:

1. **N researchers write blind to each other.** Each produces findings with no view of what the
   others found, so overlapping units, inconsistent granularity and duplicate coverage are the
   expected output, not the failure case. (`.claude/agents/wskill-researcher.md` is generated from
   the wskill's own `agent` block — the parallelism is a designed feature.)
2. **The index is built after the units exist.** Structure is retrofitted onto nodes already written,
   which is when `related` starts carrying navigation weight it shouldn't.

`adding_content` — the incremental loop — has the same ordering: decompose → classify → write →
**link** → **pin**. The unit exists before anyone asked where it belongs.

Decide how the research path changes. The charting decision was a **curator pass** rather than
outline-first authoring, so the question is how much of this the curator absorbs versus how much the
process must stop generating:

- **Does the parallel fan-out survive?** It's the reason research is fast. Options: keep it and let
  the curator clean up; keep it but give each researcher a bounded slice of an agreed skeleton; add a
  reconciliation stage between research and unit-writing; serialise.
- **Where does the index get built?** Before distillation, after, or continuously?
- **Do research findings become units at all?** `research` blocks are already first-class, already
  render to `references/research_<id>.md`, and default to `audience = :ai`. Maybe findings stay
  findings and unit-writing is a separate deliberate act.
- **What does the completeness gate check?** It currently gates coverage. Should it also gate
  *structure* — that findings map onto a shape — before any unit is written?
- **Same question for `adding_content`.** Does the incremental loop reorder, or does the curator make
  its ordering harmless?

Blocked by `06-unit-kinds`: what a unit *is* determines what distillation is aiming at.

## Inherited from ticket 06 — do not re-litigate

**What a unit is, is settled**, so distillation has a target: the kind vocabulary is unchanged
(`fact` = indisputable/verifiable, `concept` = a describable model, `entity` = external proper nouns
only), `related` stays untyped, and **every edge a researcher or distiller creates must carry a
`why`**.

Two things this changes for the research flow specifically:

- **The "N researchers write blind" fault is real but is NOT evidenced by graph collapse.** Ticket 06
  measured the output of this very process and found the vocabulary applied correctly, the 3–5 cap
  respected (0–4% over), and index-mirroring not happening. Whatever the parallel fan-out costs, it
  is not producing a degraded graph today — so argue the change on its own merits, not on cleanup.
- **Mandatory `why` raises the cost of the retrofitted index.** A researcher who cannot state why an
  edge exists cannot create it, which is a natural brake on "link it into the web after the fact".
  Consider whether that alone addresses the ordering fault before restructuring the pipeline.

`research` blocks remain first-class and unchanged in the vocabulary.

## Inherited from ticket 07 (resolved) — do not re-litigate

07 settled the curator's contract, and **both** of this ticket's processes now have a fixed terminal
phase, which partly pre-answers the last bullet (*"does the incremental loop reorder, or does the
curator make its ordering harmless?"*).

**A curator pass is now the terminal phase of `adding_content` and `researching_a_topic` alike**,
scoped to the units just touched plus their 1-hop neighbourhood and the indexes they're pinned under.
Cheap, because those units are already in the session's context. Whole-graph passes exist but are
**on demand only, never automatic**.

**So the ordering fault is mitigated but not cured, and the residue is precisely this ticket's.** A
1-hop window is structurally blind to the whole-graph defects — hub-ness, near-duplicate `why` clauses
across a unit's edges, index bloat — because no single addition visibly crosses a threshold. A
research run that fans out N researchers produces exactly that kind of defect, all at once, and the
terminal 1-hop pass will not see it. Decide whether `researching_a_topic` therefore ends with a
**whole-graph** pass rather than the incremental one, which is the natural place for the on-demand
mode to become mandatory for this one process.

**The `why` brake is real but weaker than 06 hoped.** 07 confirmed the curator **never backfills** a
`why` — it authors one only for edges it creates itself. So a researcher or distiller who writes a
bare edge has no downstream rescue: at the schema flip the rule is *delete every bare edge unless the
target is already named in the source prose*. That makes "state the reason at creation time" a hard
requirement on the research path, not a nicety — and it means an index retrofitted after the fact
cannot be silently justified later.

**Anything the curator can't fix is filed, not fixed.** Hubs, non-atomic units and dishonest `why`s
become `comments.wcl` entries with `author = "curator"`. If the parallel fan-out's real output is
overlapping units of inconsistent granularity, the curator will **report** that (merge and split are
explicitly outside its write set) — so the fan-out question cannot be resolved by "the curator cleans
up afterwards". It can't.

---

## Resolution

Status: resolved

**Measured first: this process has never been run.** All four in-repo wskills carry **zero**
`research` units and no `data/questions.wcl`; so do the installed skills under `~/.claude/skills`. The
only `research` block in the repo is the fill-out template (`data/fact/template_research.wcl`). Two
consequences: ticket 06's finding that the graph is healthy measures wskills authored by hand through
`adding_content` and says **nothing** about the fan-out, and the sprawl this ticket exists to prevent
has never actually been observed. Every decision below is reasoned, not measured — don't cite them as
evidence later.

**The reframe that carried the ticket: the fan-out was never the sprawl source.** Step 5 reads *"For
each research unit, run the [Adding content] loop"* — a **per-finding** loop. That is what turns N
researchers into N overlapping unit sets: each finding is walked into units in isolation, so two
researchers who found the same thing produce two units. Once distillation becomes a single pass over
**all** findings filling a declared shape, researcher blindness costs duplicated *effort* but not
duplicated *units*. So the answer changes distillation, not the fan-out.

### 1. Findings still become units — but against a shape declared before research

Not "findings stay findings" (a wskill nobody can read as a topic until a second pass that never
comes) and not today's unbounded per-finding distillation. **Sprawl is bounded by the shape, not by
researcher output volume.**

### 2. The shape is a provisional index tree, authored at step 1

`data/indexes.wcl` is written from the scoping interview — the topic's areas as `index` blocks — and
each research item hangs off an index node. No new construct: `index` is already first-class and
already nests one level. This kills the "index retrofitted onto units that already exist" fault by
construction — the index exists **first** and units land into it.

**Consequence: step 6 "Build the index" disappears as a step.** `building_the_index` remains as a
referenced procedure, invoked at step 1 instead of step 6.

### 3. Every skeleton node carries a `body`, written at scoping time

The node's scope statement — what the area covers and what it deliberately does not — is exactly what
the scoping interview produces. One artifact does three jobs: the reader's area page (ticket 06 made
an index linkable **iff** it carries a body), the **researcher's brief**, and the **distiller's fill
contract**. Free sprawl check falls out: *a node nobody can write a scope for is a node that shouldn't
exist.*

This also gives ticket 06's "bodied index" rule a producer — today **53 indexes across the four
wskills carry only 6 bodies**.

### 4. The fan-out survives unchanged

It is the reason research is fast, and per the reframe its blindness now costs only duplicated effort.
Each researcher's prompt gains **the whole index tree** for context alongside its own node; the
merge-safe one-file-per-agent rule (`data/research/<id>.wcl` + one import line + its own question row)
stands untouched. No reconciliation stage — skeleton-driven distillation already performs that read.

### 5. The gate stays coverage-only — Wil's call, against the recommendation

The completeness gate is unchanged: no `:open` research_item questions, every `data/research/<id>.wcl`
present, model checks clean. It is **not** the skeleton's revision point.

*Recorded risk (raised, overruled, proceed):* revision and fill become the same act, which is the
pressure that made the index a retrofit in the first place. Mitigated entirely by decision 6 — which
is therefore load-bearing, not a detail.

### 6. Distillation splits in two, ordered: revise, then fill

Since the gate doesn't guard the shape, the guard lives inside the step.

- **Pass A — revise.** Read **all** findings, revise the tree against what came back. This is a
  **required act, not a permitted one**: research reliably discovers the shape was wrong (an area that
  isn't real, a missing one, a node that is actually two), and a frozen skeleton would force findings
  into wrong boxes — the standard and legitimate charge against outline-first. Pass A includes the
  cheap mechanical check that **every node is covered by some finding**; an uncovered node is dropped
  or yields a follow-up research item.
- **Pass B — fill.** Write units into the settled tree. **No unit is written against an unrevised
  node.**

Every edge either pass creates carries a mandatory `why` (tickets 06/07) — and per 07 the curator
**never backfills**, so a bare edge written here has no downstream rescue.

### 7. The terminal curator pass is whole-graph on this process, always

Ticket 07 made a 1-hop pass terminal for both processes and left whole-graph on demand. For
`researching_a_topic` the on-demand mode becomes **mandatory**. Free on a from-scratch run (everything
is newly touched, so 1-hop already spans the graph); correct where it actually bites — a research run
**extending an existing wskill**, where new material can quietly unbalance what is already there,
which is precisely the hub-ness / duplicate-`why` / index-bloat class a 1-hop window cannot see.

### 8. `adding_content` reorders to place-before-write

`decompose → classify → **place** → write → link → render`. The old terminal `pin` step moves ahead of
authoring: pick the index node the unit belongs under (or argue for a new one) before the prose is
written. Nearly free at single-unit scale, and *"no node wants this"* becomes information available
**before** the writing rather than after — which is what stops `related` being the author's after-the-
fact justification for a unit that already exists. Both processes now tell one shape-first story.

### The resulting process

1. **Scope** — interview; *now also authors the provisional index tree, one `body` per node*
2. **Decompose** into research items, *each hung off an index node*
3. **Dispatch** researchers in parallel — *prompt now carries the whole tree*
4. **Gate** — coverage only (unchanged)
5. **Distil** — *pass A revise the tree, pass B fill it*
6. ~~Build the index~~ — *dissolved into 1 and 5A*
7. **Verify**
8. **Review** with the owner — *preceded by a whole-graph curator pass*

### Consequences for the spec

- `data/process/researching_a_topic.wcl` rewritten: step 1 gains tree authoring, step 2 gains node
  attachment, step 3's prompt contract gains the tree, step 5 splits in two, step 6 is deleted, the
  terminal phase is a whole-graph curator pass. Its `verification` string changes with it.
- `data/process/adding_content.wcl` reordered per decision 8.
- `data/process/building_the_index.wcl` gains the "author at scoping, one body per node" guidance and
  is re-pointed as a step-1 reference.
- The `wskill-researcher` agent contract (generated from the wskill's own `agent` block, rendered to
  `.claude/agents/wskill-researcher.md`) gains the index tree in its prompt.
- **Schema touch:** a `question` block tagged `research_item` must name its index node. Smallest form
  is one optional `identifier` field on `question`. Lands in the schema that **ticket 08** moved into
  the `wcl_wskill` embedded registry — so it is one edit in one place, not four copies.
- Nothing here needs a `wcl wskill` subcommand. The node-coverage check in pass A is a distiller
  obligation, not a lint rule — though it is the obvious candidate if it later wants teeth.
