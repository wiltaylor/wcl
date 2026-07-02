# Render a site and live-preview it

## Purpose

Get a finished HTML build of a wdoc document, and a watch-rebuild server while you iterate on it.

## Prerequisites

- A WCL file declaring at least one `page` block
- A `wcl` CLI on your PATH

## Flowchart

![diagram](../_wdoc/process_build_serve-diagram-1.svg)

## Steps

### Step 1: Render to a folder

```console
$ wcl wdoc build site.wcl --out out
```

Render every `page` block in `site.wcl` to `out/`. Each page lands at `out/<name>.html`; with several sites, each renders into its own `out/<name>/` subdirectory with a chooser index.

### Step 2: Open the output

```console
$ ls out
index.html  usage.html
```

Open `out/index.html` in a browser to check the rendered result.

### Step 3: Switch to a live preview

```console
$ wcl wdoc serve site.wcl
Serving on http://127.0.0.1:8080
```

Run `wcl wdoc serve` to start a dev server that watches the source for `.wcl` changes and re-renders on each save. Refresh the browser to see updates; pass `--addr` to change the bind address.

> [!TIP]
> **Verification**
> `out/` holds one `.html` per page, and editing the source under `wcl wdoc serve` updates the served page after a refresh.

## Related

- [Sites](../references/concept_sites.md)

- [Pages](../references/concept_pages.md)

[← Back to SKILL.md](../SKILL.md)
