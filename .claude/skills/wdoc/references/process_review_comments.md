# Review a site with comments

## Purpose

Run the click-to-comment review loop: a reviewer pins notes on the served site, and the notes are listed, worked through, and resolved from the CLI.

## Prerequisites

- A wdoc document that builds cleanly
- A `wcl` CLI on your PATH

## Flowchart

![diagram](../_wdoc/process_review_comments-diagram-1.svg)

## Steps

### Step 1: Open the editor's preview

```console
$ wcl editor site.wcl
wcl editor at http://127.0.0.1:8080
```

Run `wcl editor` and press **Rebuild** to render the site in the preview pane — the pane hosts the comment UI. The comments live in a `comments.wcl` sidecar beside the document — writing one never triggers a rebuild.

### Step 2: Pin comments in the preview

> [!NOTE]
> **Click to comment**
> Pick any rendered block to attach a note to it. Each comment is keyed by page and locator, so it stays pinned to that block across reloads.

In the preview header's comment menu, choose **Comment on a block**, click the block the note is about, and type the comment (or **Comment on this page** for a page-level note). Repeat on any page of the site; everything lands in the `comments.wcl` sidecar.

### Step 3: List and resolve from the CLI

```console
$ wcl wdoc comments .
$ wcl wdoc comments . resolve <id>
```

Run `wcl wdoc comments` to list every open note with its id, page, and text. Work through them, then `resolve <id>` marks a note done and drops it from the open list.

### Step 4: Hand off with the review handshake

```console
$ wcl wdoc review site.wcl
waiting for the reviewer…
```

An agent working the loop runs `wcl wdoc review` instead: it blocks until the reviewer clicks "Send to agent" in the editor's preview pane, then prints the round's comments exactly like `comments`. Re-running it re-shows the banner for the next round.

> [!TIP]
> **Verification**
> `wcl wdoc comments .` shows the notes pinned in the browser, and a resolved id no longer appears in the open list.

## Related

- [Render a site and live-preview it](../references/process_build_serve.md)

- [Sites](../references/concept_sites.md)

- [Pages](../references/concept_pages.md)

[← Back to SKILL.md](../SKILL.md)
