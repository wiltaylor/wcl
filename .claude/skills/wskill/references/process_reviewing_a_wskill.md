# Reviewing a wskill (human ⇄ agent loop)

## Purpose

Run the review loop: a human pins comments on the rendered book; an agent collects, fixes, and resolves them.

## Prerequisites

- The wskill renders (`just wskill-check` passes).

## Flowchart

![diagram](../_wdoc/process_reviewing_a_wskill-diagram-1.svg)

## Steps

### Step 1: Open the editor's preview

```console
$ wcl editor wdoc/book/main.wcl
```

The editor's preview pane hosts the review client: rebuild the book, pick any block from the comment menu, type a note, save. Comments land in a `comments.wcl` sidecar beside `wskill.wcl` — no rebuild, they re-appear on reload.

### Step 2: Agent: wait for the reviewer

```console
$ wcl wdoc review .        # blocks until the reviewer clicks "Send to agent"
```

When an agent has just made changes and wants review, it runs `wcl wdoc review <root>` — this registers it as waiting and blocks. The reviewer's toolbar shows a banner: rebuild, look at the changes, leave comments, then click **Send to agent**. The command unblocks and prints the comments.

### Step 3: Work through the comments

```console
$ wcl wdoc comments . --format json     # list (id, page, target, body)
$ wcl wdoc comments . resolve <id>      # delete one once addressed
```

Address each comment in the data files, resolve it by id, and rebuild. Comments pin to a page + block locator, so open the named page to see exactly what the note refers to.

### Step 4: Iterate until clean

More comments after the next Send? Fix and resolve again. The loop ends when `wcl wdoc comments .` lists nothing unresolved.

> [!TIP]
> **Verification**
> `wcl wdoc comments .` reports no open comments and the reviewer signs off on the served book.

## Related

- [Editing a wskill in the browser](../references/process_editing_via_serve.md)

- [Updating a wskill when its source changes](../references/process_updating_a_wskill.md)

- [The wcl commands an author uses](../references/fact_authoring_cli.md)

[← Back to SKILL.md](../SKILL.md)
