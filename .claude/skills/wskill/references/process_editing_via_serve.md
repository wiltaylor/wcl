# Editing a wskill in the browser

## Purpose

Use `wcl wdoc serve --edit` to modify units and blocks from the rendered book, writing real .wcl source.

## Prerequisites

- The wskill renders (`just wskill-check` passes).

## Flowchart

![diagram](../_wdoc/process_editing_via_serve-diagram-1.svg)

## Steps

### Step 1: Serve in edit mode

```console
$ wcl wdoc serve wdoc/book/main.wcl --edit --comment
```

`--edit` injects the WYSIWYG client (it composes with `--comment`). Every edit writes real `.wcl` source through a validate-then-write pipeline — a change that would introduce schema errors rolls back.

### Step 2: Edit units via the object editor

Every unit page carries an **Edit this concept/entity/fact/process** button that opens that exact object as raw WCL in the object editor. The editor also browses all schema objects by kind and namespace, and creates new ones from a schema template — new objects land in the right data file via the placement decorator, with the import wired automatically.

### Step 3: Edit page blocks in place

Select any rendered block to edit its fields in the side panel, double-click text to edit it inline, and add / move / delete blocks. This is for page-level material; unit content is better edited through the object editor so it stays one unit in one file.

### Step 4: Rebuild to apply

The server does not auto-rebuild: click the toolbar's **Rebuild** button (it rebuilds just the current page's sub-site and reloads) or press Enter in the server console for a full rebuild. Verify your change on the reloaded page.

> [!TIP]
> **Verification**
> The edited value appears in the data file (git diff shows a clean, minimal change) and on the reloaded page after rebuild.

## Related

- [Reviewing a wskill (human ⇄ agent loop)](../references/process_reviewing_a_wskill.md)

- [Adding content to a wskill](../references/process_adding_content.md)

- [The wcl commands an author uses](../references/fact_authoring_cli.md)

[← Back to SKILL.md](../SKILL.md)
