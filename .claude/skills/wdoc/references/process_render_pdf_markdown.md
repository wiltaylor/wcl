# Render a document to PDF or Markdown

## Purpose

Produce a paginated PDF or a folder of Markdown files (one per page) from the same wdoc source.

## Prerequisites

- A WCL file declaring one or more `page` blocks

## Flowchart

![diagram](../_wdoc/process_render_pdf_markdown-diagram-1.svg)

## Steps

### Step 1: Render a PDF

```console
$ wcl wdoc pdf site.wcl --out out
```

Render each `site` to `out/<name>.pdf` — a pure-Rust PDF, no browser or external tools. Prose, headings and more paginate onto A4 by default; pass `--page-size letter` for US Letter.

### Step 2: Render Markdown

```console
$ wcl wdoc markdown site.wcl --out out/md
```

Render every page to one `.md` file per page under `out/md`. Diagrams, terminals and wireframes are written as standalone `.svg` files the Markdown references; equations stay as LaTeX and videos are skipped — aimed at AI / text consumers.

### Step 3: Pick a single site

```console
$ wcl wdoc markdown site.wcl --out out/md --site book
```

When a document declares several sites, pass `--site <name>` to render just one; otherwise each site renders into its own subdirectory.

> [!TIP]
> **Verification**
> `out/<name>.pdf` opens as a paginated document, and `out/md/` holds one `.md` per page with referenced `.svg` assets alongside.

## Related

- [Pages](../references/concept_pages.md)

- [Sites](../references/concept_sites.md)

[← Back to SKILL.md](../SKILL.md)
