# Visibility

One document renders to three outputs: a website, a PDF and a folder of Markdown. It can also
declare several sites, and lay each one out with a different template. A block that belongs in
one of those and not in another says so with a **decorator** — `@only` to include it,
`@except` to exclude it — rather than with a duplicate document.

This file covers both decorators, their three axes, the rule that joins the axes, and the one
place where visibility is not a preference but a requirement: waiving a native block on a
target that cannot render it.

## Two decorators, three axes

`@only` and `@except` attach to any block instance. Each takes up to three optional
`list<symbol>` arguments.

```wcl
@only(sites = [:handbook], templates = [:book], backends = [:html])
callout "Browse the search index" { class = ["tip"]  body = "Type in the box." }

@except(backends = [:pdf])
video "assets/tour.mp4" { }
```

| Axis | Values | The current value is |
| --- | --- | --- |
| `sites` | the label on a `site` block | the name of the site being rendered |
| `templates` | `:webpage`, `:book`, `:presentation` | the site's `default_template` |
| `backends` | `:html`, `:pdf`, `:markdown` | the output target this build produces |

Three backends, not four. `wcl wdoc serve` is an HTML build, so it matches `:html`.

A block with neither decorator renders everywhere. That is the default and what almost every
block should be.

## The rule

- **Within an axis, the values are OR'd.** `backends = [:pdf, :markdown]` matches a PDF build
  or a Markdown build.
- **Across axes, they are AND'd.** `@only(sites = [:docs], backends = [:html])` matches only
  when the site is `docs` *and* the target is HTML.

An axis you do not write does not constrain. The two decorators combine like this:

```text
render  =  (no @only   or  @only matches)
       and (no @except or  @except does NOT match)
```

Write both on one block and `@except` wins where they overlap, because it is a veto.

### An axis whose value is unknown never matches

A constrained axis with no current value fails to match rather than matching vacuously. It
shows up in two places: a document with one unnamed `site` has no site name, and a `site` that
declares no `default_template` has no template kind.

The effect differs by decorator, and follows from the rule rather than from a special case. On
an `@only` an unknown axis **hides** the block (`@only` failed to match). On an `@except` it
**shows** the block (`@except` failed to match either).

## The backends axis

```wcl
import <wdoc.wcl>

site docs {
  default_template = :book
  title            = "Docs"
  toc { chapter "Notes" { page = notes } }
}

page notes {
  start = true
  h1 "Notes"

  @only(backends = [:html])
  p "HTML-ONLY"

  @only(backends = [:markdown])
  p "MARKDOWN-ONLY"

  @except(backends = [:markdown, :pdf])
  p "NOT-MD-NOT-PDF"
}
```

The Markdown target keeps one paragraph:

```console
$ wcl wdoc markdown main.wcl --out _md
wrote 1 page
$ cat _md/notes.md
# Notes

MARKDOWN-ONLY
```

The HTML target keeps the other two:

```console
$ wcl wdoc build main.wcl --out _site
wrote 1 page
$ grep -o 'HTML-ONLY\|MARKDOWN-ONLY\|NOT-MD-NOT-PDF' _site/notes.html
HTML-ONLY
NOT-MD-NOT-PDF
```

`@except(backends = [:markdown, :pdf])` and `@only(backends = [:html])` select the same set
today, because there are exactly three targets. They do not *say* the same thing. Write
`@only` when the block is **for** that target, and `@except` when the block is **wrong on**
that one — a live search box, a pan-and-zoom control, an embedded player. The `@except` form
keeps working when the block starts rendering somewhere new; the `@only` form silently keeps
hiding it.

## The sites axis

In a multi-site document, `sites` scopes a block to some of them. The values are `site` block
labels.

```wcl
site docs { default_template = :book     title = "Docs" }
site blog { default_template = :webpage  title = "Blog"  root = true }

page notes {
  sites = [:docs, :blog]
  h1 "Notes"

  p "This paragraph is in both sites."

  @only(sites = [:docs])
  callout "Internal detail" { class = ["note"]  body = "Only the docs site shows this." }
}
```

Two ways to keep content out of a site, at different scales. A `page` names its sites with the
ordinary `sites` **field** — a page belongs to sites, so that is data. `@only(sites = …)`
scopes **one block inside a page** that several sites share. The styling blocks (`class`,
`base`, `media`, `keyframes`, `font_face`) carry their own `sites` field for the same reason.

## The templates axis

`templates` scopes a block to the kind of layout it renders into — `:webpage`, `:book` or
`:presentation`. Use it for content that only makes sense in one shape: a "press Space for the
next slide" note in a deck, a "use the sidebar" note in a book.

**The axis reads the site, not the page.** The current template kind is the site's
`default_template`. A `page` that overrides its own layout with `template = :book` inside a
`:webpage` site still matches `templates = [:webpage]`, because the axis never looks at the
page. Check that before concluding a decorator is broken.

## Waiving a native block on a target that cannot render it

Everything above is preference. This is the one case where the `backends` axis is
load-bearing, and it is why the axis exists.

Some blocks are **native**: wdoc renders them in Rust rather than through a WCL `lower`
function, because their output is not expressible in WCL. A native block declares which
targets implement it, and not every one covers all three.

`file` is the clear example. It ships a file into the output tree and optionally links to it.
A PDF is one self-contained document: there is no output folder beside it to copy into, so a
rendered link would point at something never shipped. `file` therefore declares `:html` and
`:markdown` and stops there. Put one on a page and build to PDF, and the build **refuses**:

```console
$ wcl wdoc pdf main.wcl --out _pdf
wcl::eval::user_error

  × error: `file` has no :pdf implementation (it is native
  │ on :html, :markdown); remove the block or waive it here with
  │ `@except(backends = [:pdf])`
    ╭─[main.wcl:12:3]
 11 │   h1 "Downloads"
 12 │   file "notes.txt" { as = "the notes" }
    ·   ──────────────────┬──────────────────
    ·                     ╰── error raised here
 13 │ }
    ╰────
```

It refuses rather than rendering nothing, and that is the design: a block that silently
vanished from one of your outputs is a bug you find months later, from a reader. The message
names the fix — add the waiver to that instance:

```wcl
page dl {
  h1 "Downloads"

  @except(backends = [:pdf])
  file "notes.txt" { as = "the notes" }
}
```

```console
$ wcl wdoc pdf main.wcl --out _pdf
wrote 1 pdf
$ wcl wdoc markdown main.wcl --out _md
wrote 1 page
$ cat _md/dl.md
# Downloads

[the notes](_wdoc/notes.txt)
```

**Capability says *can't*; intent says *don't want to*.** The block's declaration states what
the renderers can do; `@except` states what you want. The build refuses until the two agree.
The waiver is per instance on purpose: the next `file` block makes the same decision for
itself.

### Two targets have to cover the block

A block is checked against two backends, because two are involved: the target the build is
producing, and the renderer actually running. Those are not always the same — a `card` in a
diagram draws its body as HTML in whichever target embeds the SVG.

So a `file` inside a card must not reach a PDF just because the card body renders as HTML, and
`markdown_source` — which taps the Markdown emitter from inside the HTML build — is a
rendering question rather than an output one. On an ordinary page the two are the same backend
and this is one check. When they differ, the error says so.

## Where a decorator goes

`@only` and `@except` sit on the line above the block instance they apply to.

```wcl
@except(backends = [:pdf])
video "assets/tour.mp4" { poster = "assets/tour.jpg" }
```

They apply to **one instance** and take its whole subtree with it. Hide a `callout` and its
body goes; hide a `diagram` and every shape inside it goes.

There is no way to hide a field, and no way to hide a page — a page belongs to sites through
its `sites` field, and a page you do not want built is a page you do not declare.

One decorator of each kind per block. Keep the axes in one decorator, where the AND rule reads
them together.

## See also

- [`wdoc_outputs.md`](wdoc_outputs.md) — the three targets the `backends` axis names.
- [`wdoc_sites.md`](wdoc_sites.md) — the `site` block, the `sites` field on a page, multi-site output.
- [`wdoc_extending.md`](wdoc_extending.md) — what makes a block native, and how it declares the targets it covers.
- [`wdoc_styling.md`](wdoc_styling.md) — the parallel `sites` field the styling blocks carry.
- [`../language/lang_decorators.md`](../language/lang_decorators.md) — how a decorator is declared and read back.
