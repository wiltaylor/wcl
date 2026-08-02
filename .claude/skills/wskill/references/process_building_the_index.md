# Building the wskill index

## Purpose

Declare the wskill's shape during scoping by writing a bodied `index` tree, then place units under the node whose scope owns them.

## Flowchart

![diagram](../_wdoc/process_building_the_index-diagram-1.svg)

## Steps

### Step 1: Author the scoped index tree

```wcl
// data/indexes.wcl
index commands {
  name    = "Commands"
  summary = "The everyday command set."
  body { p "Tasks performed directly with the CLI; excludes configuration concepts and file-format reference." }
  related = [git_add, git_commit, status_fact]
}
```

Use this procedure while scoping, before units are written. Add an `index` block (in
`data/indexes.wcl` or its own file) for each real topic area. Give every node a `name`, a
`summary`, and exactly one `body` stating what belongs in the area and what does not. The body
makes the node a reader-facing area page and supplies the research brief and distillation
contract. If you cannot write that scope, the node should not exist. As units are authored,
list their ids in the owning node's `related`; each id resolves to its page.


### Step 2: Nest sub-indexes

```wcl
index reference {
  name = "Reference"
  index commands { name = "Commands"  related = [git_add, git_commit] }
  index config   { name = "Config"    related = [core_settings] }
}
```

An index may hold child `index` blocks one level deep — write them inside the parent, and give
every child its own body too. The book renders them nested under the parent chapter. Document
gathering is direct-only, so a nested index is not also listed at the top level.


### Step 3: Choose the audience

Indexes default to `:book`. Set `audience = :ai` (or `:both`) on an index meant to steer the
AI skill — `:ai`/`:both` indexes drive `SKILL.md`, while `:book` indexes shape only the book
sidebar. See \*Setting up AI skill generation\*.


> [!TIP]
> **Verification**
>
> Every index node has one scope body; each appears as an area page and a chapter in the book sidebar (and, when `:ai`/`:both`, in the skill projection), listing links to the units it owns.

## Related

- [Structured data](../references/concept_structured_data.md)

- [Creating a new wskill](../references/process_creating_a_wskill.md)

- [Researching a topic into a wskill](../references/process_researching_a_topic.md)

- [Adding content to a wskill](../references/process_adding_content.md)

[← Back to SKILL.md](../SKILL.md)
