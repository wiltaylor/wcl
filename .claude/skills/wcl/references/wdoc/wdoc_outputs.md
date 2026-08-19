# Output targets

One document, three outputs, one command. `wcl wdoc build --type html` renders a website,
`--type pdf` a paginated PDF, and `--type markdown` a folder of `.md` files (`md` is an alias).
`html` is the default, so a bare `wcl wdoc build` is the website. `wcl wdoc serve` is that
first target with a watcher and a browser attached, not a fourth thing.

This file covers each target: what it writes, what it can carry and what it cannot. It then
covers the mechanism that keeps the three honest: one closed content vocabulary every backend
must match exhaustively. Two dev-loop devices are built on that mechanism, and this file covers
them too — `wcl wdoc serve` and the `markdown_source` block.

## One document, three renderers

The three targets are not three exporters bolted onto an HTML generator. Each reads the same
evaluated document and the same **content vocabulary**. Each then decides for itself what a
heading, a code listing or a callout looks like.

A block reaches a backend by one of two routes. Most blocks **lower**. A WCL function on the
block's type returns a node of the content vocabulary: a `Heading`, a `Paragraph`, a `Code`, a
`Callout`. Each of the three renderers reads that node. A minority are **native**: wdoc renders
them in Rust, because their output is not expressible in WCL.

So a block that lowers reaches all three targets by construction. The only blocks that can be
missing from a target are the natives that say so.

**A new content node is a compile error in three places.** The content vocabulary is a closed
union with no generic container and no raw-markup escape hatch. Every backend matches it
exhaustively — no catch-all arm in the HTML walker, the PDF walker or the Markdown walker. A
variant added to the union does not silently render as nothing in two of three outputs. It
fails to compile until all three say what to do with it.

## `--type html` — the website

```console
$ wcl wdoc build main.wcl --out _site
wrote 2 pages
```

```
_site/
  index.html          # a copy of the site's start page
  intro.html
  ref.html
  _wdoc/              # fonts, favicon, icon sprite, page manifest, player scripts
```

`_wdoc/` holds everything the pages share, and every page references it by a plain relative
path. That is what makes an output tree **relocatable**: move `_site/` anywhere, serve it from
any prefix, and the links still resolve.

### `build` never wipes `--out`

The output directory is created if missing and written into. It is never emptied first.

```console
$ wcl wdoc build main.wcl --out _site
wrote 1 page
$ touch _site/leftover.html
$ wcl wdoc build main.wcl --out _site
wrote 1 page
$ ls _site
index.html
leftover.html
one.html
_wdoc
```

This cuts both ways: a build can write into a directory that already holds hand-authored
files, and a **renamed or deleted page leaves its old file behind**. Delete the directory
yourself when you want a clean tree.

Search fetches its index over HTTP, and so do the player scripts. The site works when a host
serves it, or when `wcl wdoc serve` does. Opening a page directly from disk gives you the
prose and loses the search box.

## `wcl wdoc serve` — the dev loop

`serve` runs a build, then serves the result over HTTP with a watcher and a live-reload script
in every page.

```console
$ wcl wdoc serve main.wcl --out _serve
rendered 2 pages
serving http://127.0.0.1:8199  (source: main.wcl, out: _serve)
auto-rebuild is off — press Enter here to rebuild after edits
```

That third line is the design, not a missing feature. The watcher sees every `.wcl` change and
**accumulates** it:

```console
2 file changes pending — press Enter to rebuild
```

There are exactly two ways to ask for a rebuild: press **Enter** in the console where `serve`
is running, or send `POST /__wdoc_rebuild`.

```console
$ curl -s -X POST http://127.0.0.1:8199/__wdoc_rebuild
{"ok":true,"summary":"2 pages (intro, ref)"}
```

The HTTP form **waits** for the build and reports what it did. An editor hook, or a file-watcher
of your own, can therefore trigger a rebuild and know whether it succeeded. The browser reloads on its
own: each page long-polls `/__wdoc_reload` and reloads when the build generation changes. The
generation moves on a failed build too, so a browser parked on the error page picks up the fix.

Omitting `--out` gives a temporary directory, removed on shutdown.

### What a rebuild re-renders

A rebuild drains **every** pending change at once and hands the whole set to the incremental
builder. There is no scoping below that: it is the changed set, or a full build.

The incremental builder maps each changed file onto the top-level blocks that came from it.
Say every changed file contributed only `page` blocks. It then re-renders those pages in place
and leaves the shared site-wide artifacts untouched — icon sprite, search index, the CSS
embedded in each page:

```console
rebuilt: 2 pages (intro, ref)
```

Anything else falls back to a full rebuild, and says so:

```console
rebuilt: 2 pages (full)
```

The fallback is deliberately eager, because the failure mode of being clever here is a stale
page that looks right. Six kinds of change force a full build: an imported library, the page
set, the CSS, an asset declaration, a repeater, and one that pulls in an icon the sprite does
not have yet. So does a change to a file holding a `site` block, which is why editing your
entry document usually rebuilds everything.

**Scoping is by file, not by block.** Two pages in one `pages.wcl` are one unit: edit either
and both re-render. The saving is in skipping the *other* fifty pages and the aggregate
writes; the parse happens either way, because imports force it.

## `--type pdf` — print

`--type pdf` renders each site to one paginated PDF. Pure Rust: no browser, no headless Chrome,
no external binary.

```console
$ wcl wdoc build main.wcl --out _pdf --type pdf
wrote 1 pdf
$ ls _pdf
handbook.pdf
```

The file is named after the **site**, not after the source document. With no `site` block, the
source file stem names it. `--page-size letter` switches from A4; that flag applies to this
type only, and passing it with `--type html` or `--type markdown` is an error rather than
being ignored. Note the count: `pdf` reports **sites**, not pages, so a three-page two-site
document says `wrote 2 pdfs`.

Prose, headings, lists, tables, code listings, callouts, footnotes, chapter headers, images
and maths all paginate. Diagrams, sequence and state diagrams, terminals and wireframes are
painted from the same shape vocabulary the other targets draw. A diagram in a PDF is therefore
the same drawing at the same viewBox. It is not a screenshot and it is not a fallback.

What a PDF cannot do is anything interactive, or anything that lives beside the document:

- **Video** shows its poster image, with a link only when the source is an online URL. A link
  to a local path in a distributed PDF is worse than no link.
- **`file`** is not covered at all. There is no output folder beside a PDF to ship a copy into.
  A `file` on a page you build to PDF is therefore refused, until you waive it with
  `@except(backends = [:pdf])`.
- **Search, the theme toggle, diagram pan-and-zoom, terminal replay and the deck player** are
  HTML devices. They are simply absent.

## `--type markdown` — text

`--type markdown` writes one `.md` per page, plus the standalone `.svg` files that Markdown
references. `--type md` is an alias.

```console
$ wcl wdoc build main.wcl --out _md --type markdown
wrote 1 page
$ find _md -type f
_md/index.md
_md/one.md
_md/_wdoc/one-diagram-1.svg
_md/_wdoc/video-clip-b97542c2.mp4
```

The target is aimed at readers that want text: a model, a search index, a repository someone
browses on a code host. It keeps meaning and drops chrome. Here is a page carrying
frontmatter, prose, an equation, a diagram and a video, and the whole file it produced:

```markdown
---
author: platform
status: draft
---

# One

Prose with **bold**, `code` and a [link](one.md).

$$
E = mc^2
$$

![diagram](_wdoc/one-diagram-1.svg)

[_wdoc/video-clip-b97542c2.mp4](_wdoc/video-clip-b97542c2.mp4)
```

| In the document | In the Markdown |
| --- | --- |
| a `frontmatter` block | a YAML header — the other two targets pass over it |
| `[link](one)` | `[link](one.md)` — a page name becomes a file name |
| a `math` block | `$$ … $$`, still LaTeX, for the reader to typeset |
| a `diagram` | a standalone `.svg` under `_wdoc/`, referenced by an image |
| a `video` | a link; a local source is copied out first |

A `code` block becomes a fence under its filename rather than a styled card. A `callout`
becomes a GitHub alert, keyed on the callout's kind:

```markdown
> [!NOTE]
> **One source**
>
> Every target reads the same document.
```

Diagrams, terminals and wireframes render as **plain SVG** rather than as an approximation in
text, which is why a zoomable diagram survives the trip. Equations stay LaTeX for the same
reason: converting either to ASCII would lose the thing worth keeping.

## `markdown_source` — previewing the Markdown

`markdown_source` shows a page's generated Markdown inside an HTML build, next to the rendered
version.

```wcl
markdown_source {
  id = "pv"
  h2 "A section"
  p "Prose with **bold** and `code`."
  list {
    li "one"
    li "two"
  }
}
```

The HTML build lowers the body through the Markdown emitter and shows the result as a
highlighted `code markdown` listing:

```markdown
## A section

Prose with **bold** and `code`.

- one
- two
```

Set `id` to the previewed page's name. It is the filename stem for any diagram or terminal SVGs the body's Markdown writes. The `![](…)` references then line up with what a real `wcl wdoc build --type markdown` run would produce.

The block is native on `:html` and only `:html`, because it taps the Markdown emitter from
inside the HTML build. Using it on another target is refused rather than rendering nothing.

## Choosing a target

| `--type` | Reach for it when | Give up |
| --- | --- | --- |
| `html` | People will read it in a browser | nothing — this is the full-fidelity target, and the default |
| `pdf` | It has to print, or travel as one file | interactivity, `file` assets, playable video |
| `markdown` | A model, a diff or a code host will read it | layout, theming, search; keeps the meaning |

Building more than one from the same source is the normal case, and it is what the `backends`
visibility axis is for: the paragraph that only makes sense in a browser carries
`@only(backends = [:html])`, and everything else is written once.

## See also

- [`../language/lang_cli.md`](../language/lang_cli.md) — every flag of `build` and `serve`, and the exit codes.
- [`wdoc_visibility.md`](wdoc_visibility.md) — `@only` / `@except`, and waiving a native block on a target that cannot render it.
- [`wdoc_sites.md`](wdoc_sites.md) — the output tree, multi-site routing, the start page and search.
- [`wdoc_extending.md`](wdoc_extending.md) — lowering versus `@native`, and how a block declares the targets it covers.
- [`wdoc_code.md`](wdoc_code.md) — the chrome each backend draws around a `code` block.
- [`wdoc_media.md`](wdoc_media.md) — the `file` block the PDF target does not cover.
