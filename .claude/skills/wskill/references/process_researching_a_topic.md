# Researching a topic into a wskill

## Purpose

Author a whole wskill by research: scope the topic in an interview, fan the open questions out to parallel researcher agents as `research` units, then distill the findings into concept/entity/fact/procedure units and a curated index.

## Prerequisites

- A wskill folder for the topic that checks clean — scaffold one first via [Creating a new wskill](../references/process_creating_a_wskill.md) if none exists.

## Flowchart

![diagram](../_wdoc/process_researching_a_topic-diagram-1.svg)

## Steps

### Step 1: Scoping interview

Interview the topic owner in rounds until the shape is settled: what the topic covers and what it deliberately does NOT, which upstreams are authoritative (record each as a `source` block in wskill.wcl and pin `topic.version` when the subject has one), who each view is for, and which artifacts to ship. Decisions only the owner can make are captured as plain `question` blocks — those are for the owner, not for research.

### Step 2: Decompose into research items

```wcl
// data/questions.wcl (import it from data/main.wcl) — the run's worklist.
question r_<slug> {
  question = "<The single thing this item must settle.>"
  context  = "<Why it matters / which part of the topic it unblocks.>"
  tags     = ["research_item"]
}
```

Turn the scoped topic into a worklist of single-idea research items — one `question` block each, tagged `research_item`, in a `data/questions.wcl` created for the run. If an item needs an "and" to state, split it. The worklist is temporary metadata: question blocks never render as topic content and are deleted once folded.

### Step 3: Dispatch researchers in parallel

Dispatch one `wskill-researcher` subagent per `:open` research item, in parallel. Each prompt carries the item's id, question and context, the absolute wskill folder path, and today's date. Each agent writes ONLY its own `data/research/<id>.wcl` (finding + `source` blocks), its one import line in `data/research/main.wcl`, and its own question row — separate files are what make the fan-out merge-safe.

### Step 4: Completeness gate

```console
$ grep -n 'status = :open' data/questions.wcl    # must print nothing
$ just wskill-check                              # model + templates green
```

Every research_item question must be `:answered` with its `data/research/<id>.wcl` present, and the model must check clean. A blocked item comes back `:open`: settle it with the owner and re-dispatch, or mark it `:dropped` with the why in `answer`. Do not distill from an incomplete worklist.

### Step 5: Distill findings into units

For each research unit, run the [Adding content](../references/process_adding_content.md) loop: decompose its body into atomic notes, classify each with [the decision guide](../references/fact_unit_decision_guide.md), and write the concept/entity/fact/procedure/term units — each citing its research id in `related`, so unit and evidence link both ways. The `research` block stays as the dated evidence trail. Then fold and delete the answered research_item questions (and `data/questions.wcl` plus its import, once empty).

### Step 6: Build the index

Pin every unit into a topic-meaningful `index` tree in `data/indexes.wcl`, per [Building the index](../references/process_building_the_index.md) — the same curation shapes the book nav and SKILL.md's entry points.

### Step 7: Render and verify

```console
$ just wskill-check && just render
$ ls out/skill/references/research_*.md out/skill/references/index_research.md
```

Every research unit must ship at `references/research_<id>.md`, be listed in `references/index_research.md`, and appear under SKILL.md's `## Research` section; every distilled unit must have its own page and a home in an index.

### Step 8: Review with the owner

Hand off to [Reviewing a wskill](../references/process_reviewing_a_wskill.md): serve the book, walk it with the owner, and fold review comments back into the units.

> [!TIP]
> **Verification**
> No `:open` research_item questions remain; every research unit renders at references/research_<id>.md and is listed in index_research.md; every distilled unit has a page, cites its research id, and sits under an index; `just wskill-check` and `just render` are green.

## Related

- [Creating a new wskill](../references/process_creating_a_wskill.md)

- [Capturing research into a wskill](../references/process_capturing_research.md)

- [Adding content to a wskill](../references/process_adding_content.md)

- [Building the wskill index](../references/process_building_the_index.md)

- [Reviewing a wskill (human ⇄ agent loop)](../references/process_reviewing_a_wskill.md)

- [Which unit kind? — the decision guide](../references/fact_unit_decision_guide.md)

- [Decomposing information](../references/concept_decomposing_information.md)

[← Back to SKILL.md](../SKILL.md)
