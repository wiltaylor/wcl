# Capturing research into a wskill

## Purpose

Turn the durable output of an investigation into `research` blocks other agents — and external pipelines like a planner's researcher — can reuse instead of re-investigating.

## Prerequisites

- An existing wskill folder that checks clean (`just wskill-check`).

## Flowchart

![diagram](../_wdoc/process_capturing_research-diagram-1.svg)

## Steps

### Step 1: Investigate and keep the evidence

Do the research (web, docs, source-diving) and keep every locator you actually used — URLs, paths, versions. The bar for a finding: exact names and versions, specific calls or commands, the gotcha that cost time. "Check the docs" is not a finding.

### Step 2: Distill one finding per research block

```wcl
// data/research/<id>.wcl (import it from data/research/main.wcl)
research <id> {
  topic    = "<Headline of the finding>"
  question = "<What the investigation set out to answer.>"
  summary  = "<One-line finding — shown in the research index.>"
  checked  = "<YYYY-MM-DD>"
  applies_to = "<subject/version it holds for, e.g. bevy 0.18>"
  source_ids = [<source block ids>]
  locators   = ["<ad-hoc evidence URLs/paths>"]
  body { p "The full findings — exact names, versions, calls, gotchas." }
}
```

One finding per block, dated with `checked` and scoped with `applies_to`. Project-specific conclusions stay in the project that researched them; only durable, topic-level findings belong in the wskill.

### Step 3: Link the evidence

Register durable upstreams as `source` blocks and reference them via `source_ids`; one-off URLs go straight in `locators`. Fill `related` with the unit ids the finding touches.

### Step 4: Fold what's settled into real units

If part of the finding is settled, reusable knowledge (a value, a behaviour, a runbook step), ALSO capture it as the proper unit kind via [the decision guide](../references/fact_unit_decision_guide.md) — the research block keeps the dated evidence trail; the unit carries the knowledge.

### Step 5: Check, render, verify the contract

```console
$ just wskill-check && just render
$ ls out/skill/references/research_*.md out/skill/references/index_research.md
```

The rendered skill must contain `references/research_<id>.md` for the new finding, list it in `references/index_research.md`, and show it under SKILL.md's `## Research` section — that fixed layout is what external consumers glob.

> [!TIP]
> **Verification**
> `references/research_<id>.md` exists in the rendered skill, `index_research.md` lists it, and SKILL.md's Research section links it with its checked date.

## Related

- [Adding content to a wskill](../references/process_adding_content.md)

- [Updating a wskill when its source changes](../references/process_updating_a_wskill.md)

- [Which unit kind? — the decision guide](../references/fact_unit_decision_guide.md)

- [Researching a topic into a wskill](../references/process_researching_a_topic.md)

[← Back to SKILL.md](../SKILL.md)
