# Markdown output

_The `wcl wdoc markdown` render target, its block mapping, caveats, and frontmatter._

`wcl wdoc markdown <file> --out <dir>` (alias `md`) renders a document to a folder of Markdown files instead of HTML. The layout mirrors `wdoc build`: one `.md` per page, multi-site documents land under `<out>/<site>/`, and generated assets go in `_wdoc/`. The target is built for AI / text consumers, so it favours textual fidelity over visual polish.


```console
wcl wdoc markdown docs/main.wcl --out docs/_md
wcl wdoc md docs/main.wcl --out docs/_md --site reference
```

## What maps to what

Prose, headings, lists, tables, fenced code (with the language tag), callouts (as GitHub alert blockquotes) and images render as native Markdown. Inline emphasis, code spans and links are preserved — and an internal page link is rewritten to its `.md` sibling. Equations stay textual: a `math` block becomes a `$$ … $$` fence and inline math keeps its LaTeX.


Anything inherently visual — diagrams (and the charts, timelines, maps and tilemaps nested in them), terminals and wireframes — renders to a self-contained, **static** `.svg` file in `_wdoc/`, which the Markdown references with an image link. Interactivity (pan / zoom, controls, map popups) is dropped.


> [!NOTE]
> **Skipped blocks**
> Videos are skipped: an online video (YouTube / Vimeo) leaves a plain Markdown link, and a local video file is dropped (a static `.md` can't play it).

## Front matter

A page can carry YAML front matter via a `frontmatter` block — handy for tagging pages with model-facing metadata. The block is an open, schemaless kind: write any `key = value` entries you like. They serialize (in source order) to a `---`-fenced header at the top of the page's `.md`. The HTML and PDF targets ignore the block entirely.


```wcl
page intro { sites = [:demo]
  frontmatter {
    title    = "Intro"
    tags     = ["overview", "api"]
    audience = "llm"
    weight   = 3
  }
  h1 "Intro"
  p "Body text."
}
```

> [!NOTE]
> **Arbitrary keys are fine**
> The `frontmatter` type is declared `@schemaless` in the stdlib, so undeclared keys pass WCL's strict schema check — no per-instance marker is needed.

## Block reference

A `frontmatter` block: an open, schemaless set of `key = value` entries serialized as a `---`-fenced YAML header on the page's Markdown (ignored by the HTML and PDF targets).

| Property | Type | Required | Description |
| --- | --- | --- | --- |

## Related

- [Skill folders](../references/concept_skills.md)

- [Sites](../references/concept_sites.md)

[← Back to SKILL.md](../SKILL.md)
