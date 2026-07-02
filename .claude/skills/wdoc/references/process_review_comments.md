# Review a site with comments

## Purpose

Run the click-to-comment review loop: a reviewer pins notes on the served site, and the notes are listed, worked through, and resolved from the CLI.

## Prerequisites

- A wdoc document that builds cleanly
- A `wcl` CLI on your PATH

## Flowchart

![diagram](../_wdoc/process_review_comments-diagram-1.svg)

## Steps

### Step 1: Serve with the review client

```console
$ wcl wdoc serve site.wcl --comment
Serving on http://127.0.0.1:8080
```

Start the dev server with `--comment` to inject the review client into every page. The comments live in a `comments.wcl` sidecar beside the document — writing one never triggers a rebuild.

### Step 2: Pin comments in the browser

> [!NOTE]
> **Click to comment**
> Click any rendered block to attach a note to it. Each comment is keyed by page and locator, so it stays pinned to that block across reloads.

In the browser, click the block the note is about and type the comment. Repeat on any page of the site; everything lands in the `comments.wcl` sidecar.

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

An agent working the loop runs `wcl wdoc review` instead: it blocks until the reviewer clicks "Send to agent" in the comment toolbar, then prints the round's comments exactly like `comments`. Re-running it re-shows the toolbar banner for the next round.

> [!TIP]
> **Verification**
> `wcl wdoc comments .` shows the notes pinned in the browser, and a resolved id no longer appears in the open list.

## Related

- [Render a site and live-preview it](../references/process_build_serve.md)

- [Sites](../references/concept_sites.md)

- [Pages](../references/concept_pages.md)

[← Back to SKILL.md](../SKILL.md)
