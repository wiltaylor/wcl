# Does the unit-kind vocabulary survive, and what makes it discriminable to an agent?

Type: grilling
Status: resolved
Blocked by: —

## Question

> **PREMISE CORRECTED ON RESOLUTION.** Both framing claims below — "the vocabulary has collapsed"
> and "the guidance is ignored" — failed measurement. See *Resolution → What the measurements
> actually said*. The ticket's own guard ("45/2/18 is not evidence the kinds are wrong") turned out
> to be the right instinct. Read the resolution, not this framing.

The vocabulary has collapsed in practice. The `wcl` wskill — the most mature one in the repo —
contains **45 concepts, 2 entities, 18 facts**. Nearly everything became a `concept`.

`unit_decision_guide` exists, ships to the agent as `audience = :both`, and does not prevent this.

The curator (`07-curator-contract`) needs a **target shape to curate toward**. Until it's settled
what the kinds *are* and what makes one correct, the curator has no criterion — so this is upstream
of it.

Decide:

- **Do `concept` / `entity` / `fact` / `procedure` / `index` / `example` / `research` survive as the
  vocabulary?** Note that 45/2/18 is not evidence the kinds are wrong — it might be evidence the
  *guidance* is unenforceable, or that the WCL topic genuinely is mostly concepts. Establish which
  before redesigning anything.
- **What makes a kind discriminable at authoring time?** A definition an agent can apply without
  judgement, or a mechanical test. "An idea the reader must understand" vs "a concrete NAMED thing"
  vs "a container for values" are distinctions that read well and evidently don't bind.
- **Is `related` the right primitive at all?** Untyped, symmetric-in-effect (it renders both ways —
  the target page lists the source under "Referenced by"), and doing double duty as both dependency
  and see-also. Typed relations (`depends-on` / `see-also` / `part-of`) would give the curator
  something to judge; they'd also give an agent more to get wrong.
- **The hub-note problem specifically.** `linking_discipline` names it well: a unit whose body is a
  list of links to its own children — "a menu wearing a page's clothes". Is a hub structurally
  detectable, or only detectable by reading the body? This determines whether the curator needs to
  touch bodies (`07-curator-contract`).
- **`index` vs `related`.** The stated rule is "`related` is meaning; an index is navigation", with
  an explicit prohibition on mirroring index membership into members' `related`. Is that rule
  enforceable, and is the two-mechanism split right?

Keep this **bounded**. The charting decision was that the kinds are *not* the leverage point — the
authoring process is. This ticket exists because the curator needs a target, not to reopen the data
model. If the answer is "the vocabulary stands, here's the mechanical test", that's a good answer.

---

## Resolution

**The vocabulary stands unchanged. The one substantive change is that a `related` edge now carries a
mandatory reason, and indexes that carry a body become linkable nodes.**

`concept` / `entity` / `fact` / `procedure` / `index` / `example` / `research` all survive with their
current definitions. The kinds were never the problem — two of the three pieces of evidence that
built this ticket were measurement errors, and the third turned out to be a *rendering* defect rather
than a vocabulary one.

### What the measurements actually said

**1. The vocabulary is not collapsing — `wcl` is one topic shape, not the norm.** Content units, all
four in-repo wskills:

| | concept | fact | procedure | entity | index | example | lesson |
|---|---|---|---|---|---|---|---|
| wcl | **45** | 18 | 4 | 2 | 3 | 8 | 11 |
| wdoc | 13 | **34** | 5 | 1 | 4 | 7 | 12 |
| wad | 28 | 30 | **22** | 6 | 11 | 3 | — |
| wskill | 22 | 30 | 16 | 2 | 9 | 2 | 7 |

`wcl` and `wdoc` were created the **same day** (2026-06-21) and are inverted, which looks damning
until you read the ids. Both are internally coherent and both follow the shipped guide: wdoc's
`fact`s are one-per-block reference pages (`charts`, `flowcharts`, `theme_nord`), its `concept`s are
cross-cutting chapters (`pages`, `sites`, `visibility`); wcl's `fact`s are the lookup tables
(`dec_block`, `dec_child`, `operators`, `patterns`), its `concept`s are language features that each
need explaining. **WCL genuinely is 45 ideas.** The distributions differ because the *topics* differ.

**2. The `related` cap is being respected — the cited violations were the exempt kind.**
`linking_discipline` states *"An `index` may pin as many units as its area needs — the cap is for
content units"*. The map's headline "one unit carries 19 `related` ids, another 12" refers to
`index lang_schema` and `index lang_primitive_types`. Content units over the 3–5 cap: **wcl 0%,
wskill 0%, wdoc 2%, wad 4%.**

**3. Index membership is not being mirrored into `related`.** Link density among pairs co-pinned in
one index is **11–23%**, against a 1–2.5% baseline for non-co-pinned pairs. The 10× ratio is topical
adjacency — units filed under one index genuinely are likelier to depend on each other. Wholesale
mirroring would read 80–100%.

**4. What IS real: 87–92% of `related` edges are bare.** Only 8–13% of related ids appear anywhere in
the source unit's prose (WAD: **0 of 175**). The graph has nodes and edges, and the edges carry no
meaning. This is the finding the whole resolution turns on.

**5. Also real: `concept` and `fact` are the same type.** Identical fields
(`id`/`related`/`audience`/`tags`/`body`); `Concept` adds `summary` and says `name` where `Fact` says
`title`. `component/concept.wcl` and `component/fact.wcl` diff to **one rendered italic summary
line**. Wil confirmed the split stays anyway — see below.

**6. And: reciprocal edges render twice.** `related` renders as two columns, "Related" (ids I chose)
and "Referenced by" (computed by `referenced_by(id)`), with **no dedup**. Reciprocity is **32–48%**
of all edges, so a third to a half of edges put the same link in both columns of the same page.

### The decisions

**1. The kinds survive, with the definitions as shipped.** (Wil) `fact` = *verifiable and
indisputable* — constant values, API surface, things nobody argues with. `concept` = *a mental model,
something less concrete that has to be described*. That is the test already in
`unit_decision_guide`'s symptom table, and the corpus shows authors applying it correctly. **No
mechanical test is needed and none should be invented** — the prose definitions bind, as measured.

The `concept`/`fact` split is kept **despite** having almost no rendering consequence today. It is
real to a reader ("explain this to me" vs "let me look this up") and the corpus proves authors can
apply it reliably.

**2. `entity` at 3% is correct rarity, not underuse.** All 8 distinct entities across all four
wskills are **external proper nouns or the tool itself**: `wcl`, `uv`, `ripgrep` (`:tool`),
`wcl init wad`, `wcl diff` (`:command`), `C4 model`, `arc42` (`:standard`), `Wil Taylor` (`:person`,
triplicated). WAD has the most (6) because it is built on external methodologies it must name; wcl
and wdoc have 1–2 because a topic documenting *itself* has almost no proper nouns to point at.

Wil: *"in the past it was trying to log parts of the application as entities which is incorrect by my
definition of it."* The historical failure was **over**-use — the guide's own header says it exists so
agents stop defaulting everything to `entity`. That is fixed. **Reading 3% as a new failure would
over-correct back into the original bug.** Only 4 of `EntityKind`'s 13 members are ever used; that is
a consequence of correct rarity, not a defect to fix.

**3. `related` stays a single untyped list. It does NOT become typed relations.** The case for
`depends-on` / `see-also` / `part-of` was "the curator needs something to judge", and the
measurements say the curator already has plenty. Typed relations would fix none of the four real
findings above while adding a per-link judgement call to every unit — and `see-also` is exactly the
kind of soft category an agent would coin for everything, the way it once coined `entity` for
everything. Direction is also *already* typed by render: "Related" means I chose you, "Referenced by"
means you chose me.

**4. Every `related` edge carries a MANDATORY reason.** `related` becomes a list of `{id, why}` (or a
repeatable `link <id> "why"` child), rendering as `- [Name](href) — <why>`.

Wil's framing is what settled it: *"it's a node graph that the reader can navigate around. If we link
everything to everything we lose this. The relationship between nodes also helps enrich the
information like it does in a zettelkasten."* In a zettelkasten the edge's value **is** its
annotation; here 87–92% of edges have none.

Mandatory, not optional — Wil: *"more we think about it reason is important. lets make it
mandatory."* An optional annotation on a free-to-add field is precisely the
guidance-without-mechanism pattern this map's standing preferences rule out. What it buys:

- **Over-linking becomes self-limiting.** A bare id is free to add, which is why the pressure is
  always upward and the cap has to be policed by prose. A required clause makes the marginal link
  cost real — the author drops the ones they cannot finish the sentence for. That is a *mechanism*.
- **`linking_discipline`'s own rule becomes enforceable.** "Either say why the reader would follow
  it, or drop it" is currently unenforceable because there is nowhere to say it.
- **The reader gets a navigable graph instead of a name list** — the navigability Wil is protecting.
- **The curator gets something judgeable.** An edge whose `why` is vague or duplicated is visibly a
  bad edge. Untyped-plus-annotated gives 07 more to work with than typed-plus-bare would.

**Migration cost, stated honestly: 736 existing edges need a `why` written.** Agent-doable, not
mechanical — nobody can generate those reasons from the ids alone.

**5. A hub note is a SCREEN, not a test — and the curator therefore reads bodies but writes only
structure.** No hub notes currently exist in the corpus: the lowest words-per-link units (30–56
words, 2–4 links) are legitimate atomic units (`member_access`, `if_let`, `dec_block`), and
`linking_discipline` records that the two real hubs (`strings`, `cli`) were already found and deleted
by hand. The one remaining candidate is `wdoc/fact themes` — 108 words, 8 links, 7 of them inline,
all targeting units sharing its `theme_*` name prefix.

So link-density-per-word plus "targets share my name prefix / are all co-pinned under me" **flags
candidates**; it cannot separate `themes` from a genuinely terse unit without reading the body.
Mandatory `why` adds a strong new signal: a hub's reasons come out near-identical ("part of the
strings family"), and near-duplicate `why` clauses across one unit's edges *is* mechanically
detectable.

**The curator's contract, settled here for `07`: structure-only writes, body-level reads.** It reads
bodies — hub detection, atomicity and "is this `why` honest" are unanswerable from the graph alone —
but its *edits* are confined to kind, `related`, `why`, and index membership. On finding a hub it
files the finding and at most performs the structural half of the fix (pin the children into an
index, delete the hub's edges), leaving "now write the real unit" to an author pass.

The reason the line sits there: the charting decision was that the curator edits directly *gated on
render + validate*, and that gate is what makes direct editing safe. Structural edits are verifiable —
the render either resolves every id and validates, or it does not. Prose edits are not: nothing in
the pipeline can tell you a rewrite preserved the unit's meaning, so a body-writing curator is an
unbounded rewrite with no gate over content a human already reviewed. It also keeps 07 specifiable —
"reshape the graph" has a target shape; "improve the prose" does not.

**6. The `index` / `related` split stands; the anti-mirroring rule becomes a curator screen, not a
hard gate.** The two mechanisms are not substitutes: an index is *navigation* (a heading in the book
nav, uncapped, asserting only "these belong together"); `related` is *meaning* (capped, annotated,
making a claim an index structurally cannot). Collapsing them forces one to lie — either the nav gets
capped at 5, or `related` loses its cap and becomes the directory `linking_discipline` warns about.

A hard gate would be wrong because co-pinned units genuinely *are* the likeliest to depend on each
other; the 10× ratio is correct behaviour. What makes it screenable is the mandatory `why`: a
mirrored edge's reason can only restate the grouping the index already asserts. The check is
**co-pinned AND the `why` adds nothing beyond co-membership** — one structural signal, one requiring
a read, matching decision 5's shape exactly.

**7. Indexes become linkable nodes — exactly when they carry a `body`.** (Wil's question: *"Can we
make index and index sub headings be linked the same way we link to other nodes?"*)

Half the mechanism already exists. `book/main.wcl:113–130, 216–225` distinguishes a **content index**
(has a `body`) — which gets its own page `index_<id>` plus a chapter entry — from a **nav index** (no
`body`), a pure sidebar heading with no page. `all_indexes = flatten([indexes, child_indexes])`, so
nested sub-headings already get pages on the same rule. Today: **53 indexes across the four wskills,
6 with bodies** (wcl 1, wad 5, wdoc 0, wskill 0).

But indexes are **not** in `all_units` (built from concepts, entities, facts, procedures, research),
so a `related` id naming one resolves to nothing and renders no bullet, silently. Zero such edges
exist today.

**The rule: an index is linkable iff it has a `body`.** Not arbitrary — it is the zettelkasten
structure-note distinction, and it is the only rule consistent with decision 5:

- **A nav index has nothing to link to, by design.** Auto-generating pages for the 47 body-less
  indexes would produce pages that are lists of links to their own children with no content of their
  own — **the hub-note anti-pattern, machine-generated at scale**. The alternative defeats the rule.
- **It makes the author's choice cost something real.** Want this area linkable? Write the paragraph
  saying what the area *is*. A mechanism, not guidance.
- **It closes a loop `linking_discipline` currently leaves open.** Its prescribed hub fix is "delete
  the hub and pin its children into an index" — but nothing can point *at* that index today, so an
  author who legitimately wants to say "the rest of this area lives over there" has no target and is
  pushed back toward writing a hub.

What this commits the spec to:

1. Body-carrying indexes (including nested sub-indexes) join `all_units`, so `related` resolves them.
   The `index_<id>` href already exists in the book.
2. **The skill projection needs matching pages for content indexes.** `skill/main.wcl:9` states
   *"There are NO per-kind index pages"* — indexes are inlined as bullet sections in SKILL.md. Without
   this, an index link resolves in the book and dies in the skill, violating the four-way projection
   rule ticket 03 made non-negotiable.
3. A `related` id naming a body-less index is a **build error**.
4. Index pages get "Referenced by" like any other node.

**8. Reciprocal edges dedupe.** With `why` mandatory, a reciprocal pair would render once *with* a
reason (in "Related") and once *bare* (in "Referenced by") on the same page. Suppress from "Referenced
by" anything already present in "Related".

### The target shape handed to `07-curator-contract`

- Kinds as defined; `fact` = indisputable/verifiable, `concept` = describable model, `entity` =
  external proper nouns only (rare by nature — do NOT curate toward more of them).
- `related`: untyped, ≤5 on content units, **every edge annotated**, no dangling ids, no
  reciprocal duplicates.
- Indexes: uncapped, navigation-only, linkable iff they carry a body.
- Hubs: screened (link-density + name-prefix/co-pin + near-duplicate `why`), never auto-rewritten.
- Curator writes: kind, `related`, `why`, index membership. Curator reads: bodies. Curator never
  writes bodies.

### Incidental defects found (file separately — not part of this effort)

1. **6 `related` ids in the `wskill` wskill resolve to nothing and render silently** —
   `ripgrep → ignore_rules`, `exit_codes → validate_format`, `commands → git_add`, +3. A dangling id
   produces no bullet and no warning. Same missing check as decision 7.3.
2. **Reciprocal `related` edges render the same link twice on one page** — 32–48% of edges. Fixed by
   decision 8, but it is a live defect today.
3. **The `Index` schema doc-comment is stale.** `schema/base.wcl` claims that in the skill an index
   "renders as a link-collection page"; `skill/main.wcl:9` says *"There are NO per-kind index pages"* —
   they are inlined into SKILL.md.
4. **`entity wil_taylor` is triplicated** across wcl, wad and wskill with identical content — three
   copies of one unit, which the format's own `selfcontained` philosophy arguably requires but which
   nothing keeps in sync.

### Inherited from ticket 06 — do not re-litigate

- The kind vocabulary is **closed and settled**. Do not propose merging `concept`/`fact`, adding a
  mechanical classification test, or reviving `entity` usage.
- `related` is **untyped**. Typed relations were considered against the evidence and rejected.
- The curator **does not write prose**.
