# Editing a wskill in the browser

## Purpose

Use `wcl editor` to modify units from the rendered book preview, writing real .wcl source.

## Prerequisites

- The wskill renders (`just wskill-check` passes).

## Flowchart

![diagram](../_wdoc/process_editing_via_serve-diagram-1.svg)

## Steps

### Step 1: Open the editor

```console
$ cd path/to/wskill && wcl editor wdoc/book/main.wcl
```

`wcl editor` serves a browser IDE for the directory: a file tree, tabbed CodeMirror editing with WCL language support, and a preview pane. Pick the book in the topbar's site selector and press **Rebuild** to render it (unsaved buffers are overlaid, so the preview always shows what you see in the tabs).

### Step 2: Jump from a rendered unit to its source

Every unit page carries an **Edit this concept/entity/fact/process** button. Clicking it opens the unit's declaring `.wcl` data file in an editor tab, selected at that exact block — no hunting through the tree. Edit the WCL directly; the LSP flags syntax and schema problems as you type.

### Step 3: Save through the validating pipeline

Saving (`Ctrl+S` or the status-bar **Save**) writes through a validate-then-write pipeline — a change that would introduce schema errors rolls back with the message shown as a toast, and concurrent on-disk changes surface as a reload-vs-overwrite conflict dialog.

### Step 4: Rebuild to verify

Press **Rebuild** again to re-render the book with your change and verify it on the reloaded preview page.

> [!TIP]
> **Verification**
> The edited value appears in the data file (git diff shows a clean, minimal change) and on the rebuilt preview page.

## Related

- [Reviewing a wskill (human ⇄ agent loop)](../references/process_reviewing_a_wskill.md)

- [Adding content to a wskill](../references/process_adding_content.md)

- [The wcl commands an author uses](../references/fact_authoring_cli.md)

[← Back to SKILL.md](../SKILL.md)
