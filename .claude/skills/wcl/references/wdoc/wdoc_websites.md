# Websites

The **website** workflow builds a marketing or product site. You bring the design: the HTML and
CSS from a Figma export, a Claude artifact, a hand-built theme, or an external bundle. You then
slot wdoc-rendered content into it. The split is deliberate — HTML and CSS for the shell, wdoc
for the words.

Start with `default_template = :website` for the built-in shell, or author your own `template`
for a real design. `wcl init website ./my-site` scaffolds a complete project.

Two mechanisms carry the workflow, on top of the ordinary template machinery: **named slots**
and **head assets**.

## Named slots

A layout declares a slot. A page fills it with a bare block of the same name. The layout then
places the fill wherever the design needs it.

```wcl
template site_main {
  slot content: content
  slot hero:    content?
  slot footer:  content?
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      wdoc_head_meta("viewport", "width=device-width, initial-scale=1"),
      [
        raw($<<HTML
          <header class="nav"><a class="brand" href="index.html">${c.title}</a></header>
        HTML
        ),
        el("section", ["hero"],    slot(c, :hero)),
        el("main",    ["content"], slot(c, :content)),
        el("footer",  ["foot"],    slot(c, :footer)),
      ],
    ])
}
```

The page fills them by name. Everything **outside** a named fill lands in the reserved
`content` slot:

```wcl
page index { start = true
  hero {
    h1 "Build something great"
  }

  h2 "About"
  p "Everything outside a named fill lands in the reserved content slot."

  footer { p "© Built with WCL + wdoc." }
}
```

That renders:

```html
<header class="nav"><a class="brand" href="index.html">Acme</a></header>
<section class="hero"><h1 class="heading-1" id="build-something-great">Build something great</h1>
</section><main class="content"><h2 class="heading-2" id="about">About</h2>
<p>Everything outside a named fill lands in the reserved content slot.</p>
</main><footer class="foot"><p>© Built with WCL + wdoc.</p></footer>
```

### The slot contract

| Declaration | Meaning |
| --- | --- |
| `slot n: content` | Required. |
| `slot n: content?` | Optional. An unfilled slot renders nothing. |
| `slot n: content*` | Repeated. Declaring one silently makes the template a **collection** template — the whole site renders to one page. A website layout keeps every slot unrepeated. |
| `slot n: content<Kind>` | Only blocks of `Kind` may fill the slot. |
| `slot n: content = fn(c: SlotOwner) -> list<Html> …` | A fallback used when the slot is unfilled. |

Placement and inspection:

- `slot(c, :name)` places the fill, and falls back to the declaration's default.
- `slot_blocks(c, :name)` returns the raw handles. Use it to branch on whether a page filled a
  slot at all:

  ```wcl
  let aside = slot_blocks(c, :sidebar);
  el("div", ["ws-layout", if len(aside) > 0 { "has-aside" }], …)
  ```

- `content` is the **reserved** slot name. Every loose page block fills it implicitly, and it
  must be declared with a `content` type.

### When a fill has no slot

Filling a slot that no layout used by this site declares is a build error:

```console
page `topic` fills slot `notes`, but no layout used by this site declares it
```

The `name? { … }` conditional form is narrower than it looks. It drops the fill in one case
only: **some** layout used by this site declares the slot, but the **selected** template does
not. That is the case where a page overrides `template` and loses a slot. It does not silence a
slot that no layout on the site declares:

```console
page `topic` conditionally fills slot `notes`, but no layout used by this site declares it
```

Other refusals worth recognising:

```console
template `X`: reserved slot `content` must have a `content` type
site fills slot `n` more than once
site: slot `n` accepts `Kind`, but found `other_kind`
site: required slot `n` is unfilled for collection template `X`
```

## Head assets

Two routes add `<link>`, `<script>` and `<meta>` tags to every page `<head>`.

### From the `site` block

```wcl
site web {
  default_template = :site_main
  title       = "Acme"
  assets      = ["assets"]                       // copies ./assets/ → _site/assets/
  stylesheets = ["assets/site.css"]              // <link rel="stylesheet"> in every head
  scripts     = ["assets/app.js"]                // deferred <script> in every head
  fonts       = ["https://fonts.example/Inter.css"]
}
```

The build emits each href **verbatim**. It never resolves or rewrites one. A copied asset, a
shipped `file` and an absolute URL therefore all behave the same way.

`assets` copies a folder verbatim and recursively, resolved against the document. It also takes
an externally-built bundle — point it at a Vite or webpack `dist/` and reference the hashed
files it emits.

The result:

```html
<link rel="icon" type="image/svg+xml" href="_wdoc/favicon.svg">
<link rel="stylesheet" href="assets/site.css">
<script src="assets/app.js" defer></script>
<meta name="viewport" content="width=device-width, initial-scale=1"></meta>
```

### From the layout

A layout emits head content itself by returning an `Html::Head` fundamental. The `wdoc_head_*`
helpers wrap the common cases:

| Helper | Adds |
| --- | --- |
| `wdoc_head_stylesheet(href)` | `<link rel="stylesheet" href>` |
| `wdoc_head_script(src)` | a deferred `<script src>` |
| `wdoc_head_font(href)` | a web-font `<link rel="stylesheet">` |
| `wdoc_head_meta(name, content)` | `<meta name content>` |
| `wdoc_head_raw(html)` | verbatim head HTML |

**wdoc hoists a `Head` from the layout's top level only.** Return it in the list `render` gives
back, or in a part's list that `flatten` merges into it. A `Head` nested inside the body renders
to nothing. It never leaks into `<body>`.

## The built-in `:website` layout

`default_template = :website` gives a theme-aware shell. It paints from the same `--wdoc-*`
variables as everything else, so it follows the site `theme` and `accent`.

Its slots:

| Slot | Renders as |
| --- | --- |
| `content` | `<main class="ws-main">` inside the layout grid. Required. |
| `banner` | `<div class="ws-banner">` above the hero. Optional. |
| `hero` | `<section class="ws-hero">`. Optional. |
| `sidebar` | `<aside class="ws-aside">` beside the content. Optional; filling it switches the grid to two columns. |
| `footer` | `<footer class="ws-footer">`. Falls back to the site title. |

The header is sticky. It shows the site title as a home link, then the curated `menu`, then a
controls cluster. With no `menu` it falls back to a flat link per page. The controls appear
only when the site sets `search = true` or `theme_toggle = true`.

```wcl
site web {
  default_template = :website
  title            = "Acme"
  search           = true
  theme_toggle     = true
  menu { item "Home" { page = index } }
}

page index { start = true
  banner { p "Beta." }
  hero {
    h1 "Acme"
    p "Ship faster."
  }
  h2 "Features"
  sidebar { p "Links." }
  footer { p "© Acme" }
}
```

Copy it as a starting point. `wdoc_website_layout(c)` is the template's whole body, and
`wdoc_part_website_css()` is its `<style>`. Drop that one part to supply your own CSS.

## The scaffold

`wcl init website ./my-site` writes a complete project:

| File | Holds |
| --- | --- |
| `main.wcl` | the `site` block and its head assets |
| `theme.wcl` | a custom slot-declaring layout |
| `components.wcl` | starter landing components — hero, feature cards, steps, footer |
| `content.wcl` | the page, built from those components plus named fills |
| `assets/app.js` | the design's script, shipped verbatim |

The generated `site` sets `assets = ["assets"]` and `scripts = ["assets/app.js"]`. Add your own
stylesheet to that folder. Then list it in `stylesheets`. Build the project with:

```console
$ wcl wdoc build my-site/main.wcl --out my-site/_site
```

## Gotchas

- **The heredoc terminator sits alone on its line.** `HTML ) ],` on one line is an unterminated
  heredoc. Put the closing bracket on the next line.
- Use the interpolating heredoc form `$<<TAG` when the fragment carries `${c.title}`. Plain
  `<<TAG` is literal.
- `raw(...)` is **not** escaped, so build one only from markup you author yourself.
- A `Head` returned below the top level renders to nothing, with no warning.

## See also

- [`wdoc_templates.md`](wdoc_templates.md) — `TemplateCtx`, the parts, the `el` family.
- [`wdoc_sites.md`](wdoc_sites.md) — the `site` fields and the `menu` block.
- [`wdoc_styling.md`](wdoc_styling.md) — the `--wdoc-*` variables the shell paints from.
