# Markdown output

`wcl wdoc markdown <file> --out <dir>` (alias `md`) renders a document to a folder of Markdown files instead of HTML. The layout mirrors `wdoc build`: one `.md` per page, multi-site documents land under `<out>/<site>/`, and generated assets go in `_wdoc/`. The target is built for AI / text consumers, so it favours textual fidelity over visual polish.

```bash
wcl wdoc markdown docs/main.wcl --out docs/_md
wcl wdoc md docs/main.wcl --out docs/_md --site reference
```

## What maps to what

Prose, headings, lists, tables, fenced code (with the language tag), callouts (as GitHub alert blockquotes) and images render as native Markdown. Inline emphasis, code spans and links are preserved — and an internal page link is rewritten to its `.md` sibling. Equations stay textual: a `math` block becomes a `$$ … $$` fence and inline math keeps its LaTeX.

Anything whose output is inherently visual — diagrams (and the charts, timelines, maps and tilemaps nested in them), terminals and wireframes — renders to a self-contained, **static** `.svg` file in `_wdoc/`, which the Markdown references with an image link. Interactivity (pan / zoom, controls, map popups) is dropped; a zoomable diagram degrades to its fully-fitted view.

> [!NOTE]
> **Skipped blocks**
> Videos are skipped: an online video (YouTube / Vimeo) leaves a plain Markdown link, and a local video file is dropped (a static `.md` can't play it).

## Front matter

A page can carry YAML front matter via a `frontmatter` block — handy for tagging pages with model-facing metadata. The block is schemaless: mark the instance `@schemaless` and write any `key = value` entries you like. They're serialized (in source order) to a `---`-fenced header at the top of the page's `.md`. The HTML and PDF targets ignore the block entirely.

```wcl
page intro { sites = [:demo]
  @schemaless frontmatter {
    title    = "Intro"
    tags     = ["overview", "api"]
    audience = "llm"
    weight   = 3
  }
  h1 "Intro"
  p "Body text."
}
```

Renders the page with this header:

```text
---
title: Intro
tags:
  - overview
  - api
audience: llm
weight: 3
---

# Intro

Body text.
```

> [!WARNING]
> **Omitting @schemaless**
> Without the `@schemaless` marker, WCL's strict schema check rejects the undeclared keys, so the build fails with a message pointing at the fix rather than silently dropping the front matter.
