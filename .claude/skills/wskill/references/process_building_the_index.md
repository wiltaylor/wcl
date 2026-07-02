# Building the wskill index

## Purpose

Curate the wskill's navigation by writing `index` blocks that group units into a meaningful tree.

## Flowchart

![diagram](../_wdoc/process_building_the_index-diagram-1.svg)

## Steps

### Step 1: Add an index block

```wcl
// data/indexes.wcl
index commands {
  name    = "Commands"
  summary = "The everyday command set."
  related = [git_add, git_commit, status_fact]
}
```

Add an `index` block (in `data/indexes.wcl` or its own file). Give it a `name` — the sidebar heading — and a `summary`, then list the unit ids it groups in `related`. Each id resolves to a link to that unit's page; an id that matches no unit is simply dropped.

### Step 2: Nest sub-indexes

```wcl
index reference {
  name = "Reference"
  index commands { name = "Commands"  related = [git_add, git_commit] }
  index config   { name = "Config"    related = [core_settings] }
}
```

An index may hold child `index` blocks one level deep — write them inside the parent. The book renders them nested under the parent chapter. Document gathering is direct-only, so a nested index is not also listed at the top level.

### Step 3: Choose the audience

Indexes default to `:book`. Set `audience = :ai` (or `:both`) on an index meant to steer the AI skill — `:ai`/`:both` indexes drive `SKILL.md`, while `:book` indexes shape only the book sidebar. See \*Setting up AI skill generation\*.

> [!TIP]
> **Verification**
> Each `index` appears as a chapter in the book sidebar (and, when `:ai`/`:both`, as a section in `SKILL.md`), listing links to the units it pins.

## Related

- [Index](../references/concept_index.md)

- [Structured data](../references/concept_structured_data.md)

- [Creating a new wskill](../references/process_creating_a_wskill.md)

[← Back to SKILL.md](../SKILL.md)
