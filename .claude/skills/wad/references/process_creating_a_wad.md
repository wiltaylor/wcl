# Create a new WAD

## Purpose

Go from nothing to a validated, rendering architecture document ready to populate.

## Prerequisites

- The `wcl` CLI installed
- A folder (usually a repo) the WAD will live in

## Flowchart

![diagram](../_wdoc/process_creating_a_wad-diagram-1.svg)

## Steps

### Step 1: Scaffold

Run `wcl init wad <dir>` and answer the prompts (or pass
`-D system_id=… -D system_name=… --defaults`). Everything arrives: schema, per-view data
hubs, book templates, extractor starter, justfile.


### Step 2: Orient

Read `wad.wcl` (the root + metadata block), skim `data/` (one folder per view — the folders
are the checklist) and `schema/kinds.wcl` (the vocabularies you may extend). Never touch
`schema/base.wcl`.


### Step 3: Validate and render

`just wad-check` then `just book-serve` — the empty book renders every chapter with “not
recorded yet” stubs. Keep the server running while populating; rebuild with Enter.


### Step 4: Choose a population strategy

Two strategies: designing something new →
[Design a new system (the interview)](../references/process_designing_new_system.md); documenting an existing system → [Document an existing system (the scan)](../references/process_documenting_existing_system.md).
Hybrid (some code exists) → scan first, interview the gaps.


> [!TIP]
> **Verification**
>
> `wcl check wad.wcl` is green and the twelve-chapter book renders with stubs.

## Related

- [wcl init wad](../references/entity_wcl_init_wad.md) — wcl init wad supports Create a new WAD: Scaffolds a complete WAD: schema, empty per-view data hubs, the book templates, extractor starter, justfile.

[← Back to SKILL.md](../SKILL.md)
