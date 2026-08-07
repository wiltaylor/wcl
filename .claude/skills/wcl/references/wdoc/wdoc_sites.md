# Documents, pages and sites

A wdoc document is one `.wcl` file, plus the files it imports, that declares `page` blocks and
`site` blocks. `wcl wdoc build <entry> --out <dir>` renders it. This file covers the entry
document, the `site` block, the `page` block, the `toc` / `menu` / `sidebar_footer` navigation
blocks, and the directory the build writes.

## The entry document

An entry document opts in with one import. That import brings every wdoc block into scope:

```wcl
import <wdoc.wcl>

site handbook {
  title = "Handbook"
  toc {
    chapter "Getting started" { page = intro }
  }
}

page intro {
  start = true
  h1 "Getting started"
  p "wdoc renders this document to a static site."
}
```

`<wdoc.wcl>` is a **system import**. The angle brackets name the embedded standard library, not
a file on disk. Import it once in the entry document. An imported page file sees it too,
because name resolution runs across the whole document.

These tags are legal at the top level of a wdoc document:

| Tag | Holds |
| --- | --- |
| `page` | one rendered page |
| `site` | one output site |
| `template` | a custom layout |
| `theme`, `class`, `base`, `font_face`, `media`, `keyframes`, `style` | styling |
| `inline_pattern` | a custom prose pattern |
| `iconset`, `tileset` | asset registries |
| `wdoc_component`, `wdoc_repeater`, `partial`, `body` | reusable content |
| `agent` | a Claude Code subagent, written by the skill target only |
| `include` | other wdoc documents shipped into this output |

A document with **no** `site` block still builds. Each page renders bare: its blocks go
straight into `<body>`, with no template, no theme and no navigation.

## The `site` block

A `site` configures one output site. Its label is the site name:

```wcl
site docs {
  default_template = :book
  title            = "Project docs"
  theme            = :nord
  accent           = :cyan
  theme_toggle     = true
  search           = true
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| label | `identifier?` | The site name. Required once the document declares more than one site. It names the output subdirectory, and it is what `page.sites` references. |
| `root` | `bool?` | `true` renders this site at `/`; the others go under `/<name>/`. At most one site may set it. |
| `default_template` | `symbol?` | `:webpage`, `:book`, `:presentation`, `:website`, `:ai_skill`, or the name of your own `template`. |
| `title` | `utf8?` | Shown in the header or the sidebar, and in the browser tab as `<page title> — <site title>`. |
| `summary` | `utf8?` | A one-line description. No built-in template renders it. The `included_sites(...)` builtin surfaces it for a landing page. |
| `icon` | `utf8?` | A favicon path, resolved against the document and copied into `_wdoc/`. An `http(s)://` or `data:` URL passes through. Absent means a default WCL icon ships. |
| `stylesheets` | `list<utf8>?` | Each href becomes a `<link rel="stylesheet">` in every page head. |
| `scripts` | `list<utf8>?` | Each src becomes a deferred `<script>` in every page head. |
| `fonts` | `list<utf8>?` | The same as `stylesheets`, named for intent. |
| `assets` | `list<utf8>?` | Folders copied verbatim, and recursively, into the site output. |
| `theme_toggle` | `bool?` | Adds a light/dark toggle button. |
| `search` | `bool?` | Adds client-side full-text search. |
| `theme` | `symbol?` | Names a `theme` block. Unset means `:forge`. |
| `accent` | `symbol?` | One of `:red :orange :yellow :green :cyan :blue :purple :pink`. |
| `ui_theme`, `ui_accent`, `ui_mode` | `symbol?` | The theme for `wf_*` wireframe widgets — the look of the mocked application, separate from the document theme. Each one falls back to the document value. |

Child blocks: `toc`, `menu`, `sidebar_footer`, `deck`, `skill`. Each one is optional, and each
one belongs to a template. Loose content blocks inside a `site` fill a collection template's
site-level slots.

## The `page` block

A `page` declares one rendered page. Its label becomes the output filename:

```wcl
page setup {
  title = "Setting up"
  sites = [:docs]
  h1 "Setting up"
  p "Install the toolchain first."
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| label | `identifier` | The page name. Becomes `<name>.html` or `<name>.md`. |
| `id` | `identifier?` | An explicit HTML id on the page. |
| `title` | `utf8?` | The browser tab title. Falls back to the page name. |
| `template` | `symbol?` | Overrides the site's `default_template` for this page. |
| `sites` | `list<symbol>?` | The sites this page joins. |
| `start` | `bool?` | Marks the site's landing page. |
| `frontmatter` | child block | Free `key = value` pairs. The Markdown target emits them as a YAML header. HTML and PDF pass over them. |
| body | `ContentBlock`s | The page content. |

### Site membership

In a **single-site** document, omit `sites` everywhere.

Once a document declares **two or more** sites, every page must name its sites. wdoc refuses an
untagged page:

```console
$ wcl wdoc build main.wcl --out _site
page "orphan" declares no `sites` — in a document with more than one site every page
must name the sites it belongs to (declared: :a, :b)
```

The rule exists because a site chooses the page's template. An untagged page would belong to
every site, so a site added later would re-template it without the page changing. A page that
is genuinely shared says so: `sites = [:docs, :blog]`. A name that matches no `site` block is
also a build error (`page references unknown site "nope"`).

Page names are unique **per site**, so two sites may each hold a page named `index`.

### The start page

`start = true` marks the site's landing page. The build copies it to `index.html`, and the page
stays reachable at its own `<name>.html`. At most one page per site may set it. With no `start`
page, wdoc uses a page named `index`, else the first page.

### Cross-page links

Write a Markdown-style link in any prose. The URL is a bare **page name**, not a path:

```wcl
p "See [the setup page](setup), or jump to [the blog](blog:index)."
```

- `[text](page)` links within the current site.
- `[text](site:page)` links across sites.
- `[text](https://…)` is an external link.

A link to an unknown page is a build error, so a page rename cannot silently break navigation.

## Navigation blocks

Each navigation block sits inside `site` and feeds one template. A template ignores a block it
does not read.

### `toc` — the `book` sidebar

```wcl
site docs {
  default_template = :book
  title            = "Project docs"
  toc {
    chapter "Intro" { page = intro }
    chapter "Guide" {
      chapter "Setup"     { page = setup }
      chapter "First run" { page = first_run }
    }
  }
}
```

A `chapter` takes a display title as its label, an optional `page`, and nested `chapter`s to
any depth. A `chapter` with no `page` is a grouping heading, not a link. A `chapter` that names
an unknown page fails the build. With no `toc`, the `book` template falls back to a flat list
of every page.

The `toc` order is also the **reading order**. It drives the previous/next pagination at the
foot of each book page.

### `menu` — the `webpage` and `website` navbar

```wcl
menu {
  item "Home"   { page = index }
  item "Docs"   { href = "docs/" }
  item "More" {
    item "About"   { page = about }
    item "Contact" { page = contact }
  }
  item "Source" { href = "https://github.com/example/proj" }
}
```

An `item` takes a display label. It links with `page` (validated, rendered as `<page>.html`) or
with `href` (a raw URL). `page` wins when an item sets both. Nested `item`s become a
click-to-open dropdown. An item with no link and no children is a plain label. With no `menu`,
the `webpage` template falls back to one flat link per page.

### `sidebar_footer` — pinned `book` buttons

```wcl
sidebar_footer {
  button "Reference" { page = reference  icon = "lucide.chart-network" }
  button "Source"    { href = "https://example.com/repo" }
}
```

Each `button` takes a label, a `page` or an `href`, and an optional `icon` (`pack.name`). The
`book` template pins the buttons to the bottom of the sidebar and draws them icon-only. It uses
the label as the tooltip and the accessible name. A button with no icon shows its label as
text.

### `deck` and `skill`

`deck` groups the site's pages into a slide grid for the `presentation` template. `skill`
carries the front-matter metadata for the `:ai_skill` target.

## Search

`search = true` makes the build write a per-page text index to `_wdoc/search-index.json`. It
also makes the `book`, `webpage` and `website` templates render a search box. Typing ranks
pages by title and body matches, and lists the top hits with a snippet. Enter opens the first
hit. Escape clears the box.

The widget fetches the index over HTTP. Search therefore works when the site is served — a real
host, or `wcl wdoc serve` — not when a page is opened directly from disk.

## The output tree

### One site

A single site renders **flat** at the output root:

```console
$ wcl wdoc build main.wcl --out _site
wrote 2 pages
```

```
_site/
  index.html          # a copy of the `start` page
  intro.html
  setup.html
  _wdoc/              # fonts, favicon, pages.json, bundled scripts, icon sprite
```

`_wdoc/` holds everything the pages share: the bundled fonts, the favicon, the page manifest
(`pages.json`), the icon sprite, and the small player scripts a page asks for — search,
terminal replay, diagram pan and zoom, the deck player. A page references that folder by a
plain relative path, so a site directory is self-contained and can move anywhere.

### Several sites

Each site gets its own directory **and its own `_wdoc/`**. One site may claim the root:

```wcl
site docs { default_template = :book     title = "Docs" }
site blog { default_template = :webpage  title = "Blog"  root = true }
```

```
_site/
  index.html          # the root site's start page
  home.html
  _wdoc/
  docs/
    index.html
    intro.html
    _wdoc/
```

With **no** `root` site, the build generates a chooser page at the output root. It links to
each site directory:

```
_site/
  index.html          # generated chooser: <a href="docs/">Docs</a>, <a href="blog/">Blog</a>
  docs/…
  blog/…
```

A sub-site with neither a `start` page nor an `index` page still gets a redirect `index.html`,
so `/<site>/` always lands somewhere.

A template on a sub-site sees a back-link to the root site (`TemplateCtx.home_href` and
`home_title`). Both are empty on the root site and in a single-site build.

### Included documents

An `include` block names a folder of **other** wdoc documents. The build renders each one
independently and ships its whole output — pages and its own `_wdoc/` — into a subdirectory of
this output. It does **not** merge them into this document; that is what `import` does.

Pick exactly one discovery mode:

```wcl
include "projects" { pattern = "main.wcl" }          // recursive filename glob
include "members"  { entry   = "wdoc/book/main.wcl" } // a fixed path per immediate subfolder
```

| Field | Meaning |
| --- | --- |
| label | The folder to scan, resolved against this document. |
| `pattern` | A recursive filename glob (`*`, `?`). The sub-site name is the matching file's parent folder. |
| `entry` | A relative path checked inside each **immediate** subdirectory. No recursion. |
| `site` | Which named site of a multi-site member to build. Passed as `--site`. |
| `prefix` | Overrides the output subdirectory, so two includes over one folder can target different places. |

The `included_sites(options)` builtin returns one `{ name, href, title, summary }` record per
match, so a `wdoc_repeater` inside a `menu` or a `toc` can build navigation from it. Pass the
**same** options you gave the `include` block, or the hrefs will not line up. A record uses `:`
for its fields, not the `=` a block uses:

```wcl
wdoc_repeater { each = included_sites({ folder: "members", entry: "main.wcl", site: "book" })  as = :m
  item $"${m.title}" { href = m.href }
}
```

Sub-sites are embedded by the HTML build, the dev server and the skill target. The Markdown and
PDF targets define `included_sites` — so a document still parses — but embed nothing.

## Gotchas

- Put each block on **its own line**. A second block on the same line becomes a *child* of the
  first. `li "one"  li "two"` renders one item, and `h2 "Agenda"  list { … }` fails with
  `block kind 'li' is not allowed inside 'h2'`.
- `sites` on a `page` is required in a multi-site document. `sites` on a `class`, `base`,
  `font_face`, `media` or `keyframes` is always optional. An omitted list there means every
  site.
- `assets` copies a folder verbatim, so it also takes an externally-built bundle — a Vite or a
  webpack `dist/`. Reference the copied files by their output path.
- wdoc emits the `stylesheets` / `scripts` / `fonts` hrefs verbatim. It does not resolve or
  rewrite them, so a copied asset, a shipped `file` and a URL all behave the same way.

## See also

- [`wdoc_templates.md`](wdoc_templates.md) — what a template does with `toc`, `menu` and the
  page content, and how to write your own.
- [`wdoc_outputs.md`](wdoc_outputs.md) — the `build`, `serve`, `pdf`, `markdown` and `skill`
  targets.
- [`wdoc_styling.md`](wdoc_styling.md) — `theme`, `accent` and the `class` system.
