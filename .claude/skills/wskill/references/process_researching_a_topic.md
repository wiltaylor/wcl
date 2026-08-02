# Researching a topic into a wskill

## Purpose

Author a whole wskill shape-first: scope a provisional bodied index tree, research each area in parallel, revise the tree against all findings, then fill it with distilled units.

## Prerequisites

- A wskill folder for the topic that checks clean — scaffold one first via [Creating a new wskill](../references/process_creating_a_wskill.md) if none exists.

## Flowchart

![diagram](../_wdoc/process_researching_a_topic-diagram-1.svg)

## Steps

### Step 1: Scope the topic and author its shape

Interview the topic owner in rounds until the scope is settled: what the topic covers and
deliberately does NOT, which upstreams are authoritative (record each as a `source` block in
wskill.wcl and pin `topic.version` when the subject has one), who each view is for, and which
artifacts to ship. Decisions only the owner can make are captured as plain `question` blocks
— those are for the owner, not for research.


During the same interview, author a provisional `index` tree in `data/indexes.wcl` using
[Building the index](../references/process_building_the_index.md). Every node MUST carry a `body` that states
what its area includes and excludes. That body is simultaneously the reader's area page, the
research brief, and the contract distillation will fill. If no honest scope can be written
for a node, remove it now.


### Step 2: Decompose into research items

```wcl
// data/questions.wcl (import it from data/main.wcl) — the run's worklist.
question r_<slug> {
  question = "<The single thing this item must settle.>"
  context  = "<Why it matters / which part of the topic it unblocks.>"
  index    = <index_id>
  tags     = ["research_item"]
}
```

Turn the scoped tree into a worklist of single-idea research items — one `question` block
each, tagged `research_item`, in a `data/questions.wcl` created for the run. Set `index` to
the id of the provisional node whose scope the item investigates. If an item needs an "and"
to state, split it. The worklist is temporary metadata: question blocks never render as topic
content and are deleted once folded.


### Step 3: Dispatch researchers in parallel

Dispatch one `wskill-researcher` subagent per `:open` research item, in parallel. Each
prompt carries the item's id, question, context and assigned index node; the WHOLE provisional
index tree (ids plus every node body); the absolute wskill folder path; and today's date. The
whole tree lets a researcher respect neighbouring scopes without changing the merge-safe
fan-out. Each agent writes ONLY its own `data/research/<id>.wcl` (finding + `source` blocks),
its one import line in `data/research/main.wcl`, and its own question row.


### Step 4: Completeness gate

```console
$ grep -n 'status = :open' data/questions.wcl    # must print nothing
$ just wskill-check                              # model + templates green
```

Every research_item question must be `:answered` with its `data/research/<id>.wcl` present,
and the model must check clean. A blocked item comes back `:open`: settle it with the owner
and re-dispatch, or mark it `:dropped` with the why in `answer`. Do not distill from an
incomplete worklist.


### Step 5: Revise the shape, then fill it

**Pass A — revise.** Read ALL findings as one set, then revise the provisional index tree
against what research discovered. This pass is required: split, merge, add or drop nodes
whose scopes proved wrong instead of forcing findings into the original boxes. Check that
every surviving node is covered by at least one finding; drop an uncovered node or create and
dispatch a follow-up research item before continuing.


**Pass B — fill.** Only after every node has been revised, distill all findings into atomic
notes, classify them with [the decision guide](../references/fact_unit_decision_guide.md), and write each
concept/entity/fact/procedure under its settled index node. No unit is written against an
unrevised node. Each unit cites its research id in `related`, so unit and evidence link both
ways; the `research` block remains as the dated evidence trail. Then fold and delete answered
research_item questions (and `data/questions.wcl` plus its import, once empty).


### Step 7: Render and verify

```console
$ just wskill-check && just render
$ ls out/skill/references/research_*.md out/skill/references/index_research.md
```

Every research unit must ship at `references/research_<id>.md`, be listed in
`references/index_research.md`, and appear under SKILL.md's `## Research` section; every
distilled unit must have its own page and a home in an index.


### Step 8: Curate the whole graph, then review

Before owner review, dispatch `wskill-curator` with the absolute wskill folder and explicit
`whole graph` scope. This is mandatory even when the run extended an existing wskill: a 1-hop
pass cannot see whole-graph hubs, duplicated reasons or index bloat. After the curator's gated
run succeeds (an empty candidate list is success), serve the book, walk it with the owner,
and fold review comments back into the units.


> [!TIP]
> **Verification**
>
> The provisional tree was revised against all findings before any unit was written; every surviving bodied index node is covered; no `:open` research_item questions remain; every research unit renders at references/research_<id>.md and is listed in index_research.md; every distilled unit has a page, cites its research id, and sits under its declared node; the whole-graph curator pass, `just wskill-check`, and `just render` are green.

## Related

- [Creating a new wskill](../references/process_creating_a_wskill.md)

- [Capturing research into a wskill](../references/process_capturing_research.md)

- [Adding content to a wskill](../references/process_adding_content.md)

[← Back to SKILL.md](../SKILL.md)
