# Adding content to a wskill

## Purpose

The core capture loop: decompose a piece of knowledge, classify it, write the unit, link it, pin it, render.

## Prerequisites

- An existing wskill folder that checks clean (`just wskill-check`).

## Flowchart

![diagram](../_wdoc/process_adding_content-diagram-1.svg)

## Steps

### Step 1: Decompose to atomic notes

Break the knowledge into single-idea pieces — one idea, thing, value or task per note. If a draft note needs an "and" in its summary, split it. See [Decomposing information](../references/concept_decomposing_information.md).

### Step 2: Classify with the decision guide

Run each note through [the decision guide](../references/fact_unit_decision_guide.md): a repeatable task → `procedure`; a concrete NAMED thing (person, software, place, …) → `entity` with a `kind` from `schema/kinds.wcl`; an indisputable value/table → `fact`; a word needing a definition → `term`; otherwise → `concept`. Never default to entity.

### Step 3: Write the unit file

```wcl
// data/reference/<id>.wcl (or the per-kind folder)
concept <id> {
  name     = "<Headline>"
  summary  = "<One-liner the indexes show.>"
  audience = :both              // opt into the skill when the agent needs it
  related  = [<other_ids>]
  body { p "The substance — capture it here, never defer to an external source." }
}
```

Write the block instance in `data/` (one file per unit as the wskill grows; add the import line to the folder's `main.wcl`). Give it a stable `id`, a headline, a summary, and a self-contained body. Attach worked examples as `example` blocks with `unit = <id>`.

### Step 4: Link it into the web

Fill `related` with the ids of the units this one leans on or explains — links resolve both ways (each page also lists what references it). A unit with no links is usually either misfiled or not atomic.

### Step 5: Pin it into an index

Add the id to the `related` list of the `index` that owns its area (or create a new index branch — see [Building the wskill index](../references/process_building_the_index.md)). Unpinned units stay reachable via links but invisible in the nav.

### Step 6: Check and render

```console
$ just wskill-check && just render
```

`wcl check` catches schema violations (a wrong entity kind, a missing required field) with file/line errors; the render makes the new pages. Fix anything it reports before moving to the next note.

> [!TIP]
> **Verification**
> The new unit has its own page in the rendered book, appears under its index in the nav, and its related links resolve in both directions.

## Related

- [Decomposing information](../references/concept_decomposing_information.md)

- [Which unit kind? — the decision guide](../references/fact_unit_decision_guide.md)

- [Atomic Note](../references/concept_atomic_note.md)

- [Building the wskill index](../references/process_building_the_index.md)

- [Self-Contained Content](../references/concept_selfcontained.md)

[← Back to SKILL.md](../SKILL.md)
