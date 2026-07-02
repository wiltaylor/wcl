# Attaching a wskill to a registry

## Purpose

Add a wskill to a collection so its views are discovered, built, and linked from the landing page.

## Prerequisites

- A registry/collection repo (scaffold one with `wcl init wskill-registry`).

## Flowchart

![diagram](../_wdoc/process_attaching_to_registry-diagram-1.svg)

## Steps

### Step 1: Place the wskill under wskills/

```console
$ mv <topic>/ <registry>/wskills/<topic>/
# or scaffold in place:
$ wcl init wskill <registry>/wskills/<topic>
```

Discovery is file-based: any immediate subdirectory of `wskills/` with a `wdoc/book/main.wcl` becomes a member. There is no member list to edit.

### Step 2: Build the registry site

```console
$ wcl wdoc build wdoc/page/main.wcl --out out/site
```

The landing's `include` blocks build every member's book under `wskills/<name>/` — plus `decks/<name>/` and `training/<name>/` for members that ship those views — and the card grid picks the new member up automatically.

### Step 3: Check the landing entry

Open the built landing: the new card shows the member's title and summary (read from its book site block and topic), links to the book, and shows deck/course buttons only if the member ships those views. A missing summary means the member's `topic.summary` is empty; a missing card means the entry file isn't at `wskills/<name>/wdoc/book/main.wcl`.

> [!TIP]
> **Verification**
> The rebuilt landing lists the new wskill with working links to each of its shipped views.

## Related

- [Collections and registries](../references/concept_collections_registries.md)

- [Creating a new wskill](../references/process_creating_a_wskill.md)

- [The view family](../references/concept_views.md)

[← Back to SKILL.md](../SKILL.md)
