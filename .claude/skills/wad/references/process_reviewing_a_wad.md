# Review a WAD

## Purpose

Get a human architecture review of the rendered book, and record the reviewed revision that anchors the change workflow.

## Prerequisites

- The WAD checks and renders
- A reviewer

## Flowchart

![diagram](../_wdoc/process_reviewing_a_wad-diagram-1.svg)

## Steps

### Step 1: Open the editor's preview

`wcl editor wdoc/book/main.wcl` — the reviewer rebuilds the preview and picks any block to
leave a note (notes land in a `comments.wcl` sidecar, no rebuild). An agent waiting on the
review blocks with `wcl wdoc review <entry>` until the reviewer hits “Send to agent”.
Mechanics are the wdoc wskill's — this process only adds the WAD-specific step below.


### Step 2: Fix from comments

Every comment resolves as a **data** edit (or a kinds/extension edit) — never a page edit.
`wcl wdoc comments <entry> resolve <id>` as each lands; loop until clean.


### Step 3: Record the reviewed revision

Commit, and record the revision — a tag (`wad-reviewed-2026-07`) or the sha noted in the
WAD's README. This is the `--from` baseline every future `wcl wad spec` diffs against; a
review that doesn't pin a revision anchors nothing.


> [!TIP]
> **Verification**
>
> No unresolved comments, and the reviewed revision (tag or sha) is recorded.

[← Back to SKILL.md](../SKILL.md)
