# Creating a new wskill

## Purpose

Start a new wskill for a topic from the wcl template, set its topic, and prove the empty model renders.

## Prerequisites

- WCL installed — see the [WCL wskill](../wcl/) to install it.

## Flowchart

![diagram](../_wdoc/process_creating_a_wskill-diagram-1.svg)

## Steps

### Step 1: Scaffold from the template

```console
$ wcl init wskill docs/wskills/<topic>
$ cd docs/wskills/<topic>
```

Run `wcl init wskill <path>` to scaffold the folder — `wskill.wcl`, the `schema/`, the `wdoc/` templates and an empty `data/`. Don't hand-create it. Answer the prompts (topic id, name, summary, whether to ship the optional presentation/training views), or pass `-D key=value` / `--defaults`.

### Step 2: Set the topic

```wcl
topic <id> {
  name    = "<Topic name>"
  summary = "<One-line description of the topic.>"
  version = "1.0.0"
  created = "<YYYY-MM-DD>"
}
```

Edit `wskill.wcl` and fill in the single `topic` block — its `id`, `name` and `summary` describe the whole wskill. Pin `version` to the upstream version when the topic has one, and add `source` blocks for where authoritative information lives (they drive the update workflow).

### Step 3: Check and render the empty model

```console
$ just wskill-check     # wcl check on the model + every template
$ just render           # out/book + out/skill
$ just book-serve       # live preview
```

Before adding any content, prove the pipeline: the scaffolded model must check clean and render. If this fails now, fix it now — every later error will then be about your content.

### Step 4: Capture the first units and curate the nav

Follow [Adding content](../references/process_adding_content.md) for the capture loop — decompose, classify with the decision guide, write, link, and pin each unit into an `index`. Once a handful of units exist, [build the index](../references/process_building_the_index.md) so the book nav and SKILL.md reflect the topic's real shape. To author a whole topic by research instead of hand-capture, follow [Researching a topic](../references/process_researching_a_topic.md).

> [!TIP]
> **Verification**
> `just wskill-check` passes and `just render` writes out/book and out/skill; the served book shows your topic name, and each unit you add appears as its own page on the next render.

## Related

- [What is it?](../references/concept_wskill_concept.md)

- [Separation of Data and Presentation](../references/concept_datapresentation.md)

- [Self-Contained Content](../references/concept_selfcontained.md)

- [Adding content to a wskill](../references/process_adding_content.md)

- [The wskill folder layout](../references/fact_folder_layout.md)

- [Researching a topic into a wskill](../references/process_researching_a_topic.md)

[← Back to SKILL.md](../SKILL.md)
