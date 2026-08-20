# Templates and layouts

A template wraps a page's content in a surrounding layout. It is a WCL **function** from a
`TemplateCtx` to a list of HTML fundamentals, and the result becomes the page `<body>`. The
renderer still emits the `<head>` and the stylesheet.

There is no template language, no front matter and no second syntax. A template is a block that
holds a function.

## Selecting a template

A site sets `default_template`. A page may override it with `template`:

```wcl
site docs { default_template = :book  title = "Docs" }

page intro { h1 "Intro" }                     // book
page bare  { template = :webpage  h1 "Bare" } // webpage
```

With neither field set, a page renders bare: its blocks go straight into `<body>`.

## The built-in templates

| Symbol | Shape | Reads |
| --- | --- | --- |
| `:webpage` | A site header, a sticky top navbar, and a reading card. | `menu` |
| `:book` | A fixed left sidebar with a nested chapter tree, a reading column, an "on this page" rail, and previous/next pagination. | `toc`, `sidebar_footer` |
| `:website` | A slot-driven marketing shell: sticky header, `banner`, `hero`, content plus `sidebar`, `footer`. | `menu` |
| `:presentation` | A slide deck rendered as one `index.html`. | `deck` |

All four are ordinary `template` blocks in the standard library, each a composition of the
public parts listed below, so you can call, extend or rebuild any of them.

## Page templates and collection templates

A template's **slot declarations** decide which of two shapes it has.

- A **page template** declares no repeated slot. The renderer calls it once per page, and each
  page becomes its own `<name>.html`. `:webpage`, `:book` and `:website` are page templates.
- A **collection template** declares at least one **repeated** slot (`content*`). The renderer
  calls it once for the whole site and writes one `index.html`. `:presentation` is the only
  built-in one.

Collection-ness comes from the slot contract, never from the template's name.

Two rules follow from that:

- A collection template must be selected by the **site** (`default_template`). A page that
  names one in its own `template` field fails the build.
- The `site` fills a collection template's **non-repeated** slots, from loose content blocks
  inside the `site` block. Each member page fills the **repeated** slots.

## Slots

A `slot` declaration names a content hole the layout can place:

```wcl
template site_main {
  slot content: content                                       // required
  slot hero:    content?                                      // optional
  slot shapes:  content<SvgBlock>                             // restricted to a block type
  slot footer:  content = fn(c: SlotOwner) -> list<Html> [raw(c.title)]   // with a fallback
  render = fn(c: TemplateCtx) -> list<Html> …
}
```

| Form | Meaning |
| --- | --- |
| `slot n: content` | Required. Every page must fill it. |
| `slot n: content?` | Optional. An unfilled slot renders nothing. |
| `slot n: content*` | Repeated. Declaring one makes the template a collection template. |
| `slot n: content<Kind>` | Only blocks of `Kind` may fill it. Anything else is a build error. |
| `slot n: content = fn(c: SlotOwner) -> list<Html> …` | A fallback, used when the slot is unfilled. |

A page that leaves a required slot unfilled stops the build:

```console
page `index`: required slot `lede` is unfilled for template `shell`
```

A page fills a named slot with a **bare block of the same name**:

```wcl
page index {
  hero {
    h1 "Build something great"
  }
  h2 "About"                 // loose blocks fill the reserved `content` slot
  p "Everything outside a named fill lands in `content`."
}
```

`content` is the **reserved** slot name. Every loose page block goes there implicitly, and the
slot must be declared with a `content` type.

Place a slot from the layout with `slot(c, :name)`. Ask whether a page filled it with
`slot_blocks(c, :name)`, which returns the raw handles:

```wcl
let aside = slot_blocks(c, :sidebar);
el("div", ["layout", if len(aside) > 0 { "has-aside" }], …)
```

Filling a slot that no layout on this site declares is a build error:

```console
page `topic` fills slot `notes`, but no layout used by this site declares it
```

The conditional form `name? { … }` drops the fill in one case only. **Some** layout used by
this site must declare the slot, while the **selected** template does not. That is the case
where a page overrides `template` and loses a slot. It does not silence a slot that no layout
on the site declares.

## The template context

`render` receives one `TemplateCtx`:

| Field | Type | Holds |
| --- | --- | --- |
| `content` | `list<BlockHandle>` | The page's authored block tree, read-only and typed. |
| `slots` | `list<TemplateSlot>` | The resolved slots. Read them through `slot` / `slot_blocks`. |
| `title` | `utf8` | The site title. |
| `owner` | `utf8?` | The owning member, for a collection template. |
| `page_name` | `utf8` | The current page's name. |
| `pages` | `list<PageRef>` | Every page in this site, in source order. Each has `name` and `href`. |
| `members` | `list<PageHandle>` | The lazily rendered member pages. Non-empty only for a collection template. |
| `toc` | `list<TocEntry>` | The chapter tree. Flat, one entry per page, when the site declares no `toc`. |
| `menu` | `list<MenuEntry>` | The navbar tree. Empty when the site declares no `menu`. |
| `footer` | `list<FooterButton>` | The pinned sidebar buttons. Each `icon` arrives as rendered SVG markup. |
| `deck` | `list<DeckSection>` | The slide grid. Empty when the site declares no `deck`. |
| `theme_toggle` | `bool` | The site's `theme_toggle` flag. |
| `search` | `bool` | The site's `search` flag. |
| `home_href` | `utf8` | A relative href back to the root site. Empty on the root and in a single-site build. |
| `home_title` | `utf8` | The root site's title, for the back-link text. |

`TocEntry` is `{ title, href, children }`. `href` is empty for a grouping heading.
`MenuEntry` is `{ label, href, children }`. `FooterButton` is `{ label, href, icon }`.

### Authored blocks are handles, not HTML

`c.content` is **not** pre-rendered markup. Each `BlockHandle` carries:

| Field | Holds |
| --- | --- |
| `kind` | The block kind, as a string (`"h2"`, `"callout"`, …). |
| `block` | The concrete authored value. An `h1` stays an `H1`. |
| `children` | The same shape, recursively. |
| `handle` | Opaque renderer identity. |

A template may query, filter and reorder handles, then place them with `wdoc_blocks(handles)`
(or the long form `Html::Blocks { blocks: handles }`). The renderer resolves a placement only
**after** the template function returns. A template never renders or mutates a block itself.

### `page_metadata(c)`

`page_metadata(c)` returns a `PageMetadata` for the current page:

| Field | Holds |
| --- | --- |
| `reading_order` | The TOC flattened to linked entries. |
| `previous`, `current`, `next` | The neighbouring `TocEntry`s. Absent at either end, and when the page is outside the TOC. |
| `current_href` | The current page's href. |
| `active_path` | The TOC entries from the root down to `current`. |
| `headings` | The page's own h2/h3 headings as `OnPageHeading { level, id, title, number }`. |

The builtin indexes the shared site TOC **once** and memoises it. It never evaluates another
page's body, so calling it in several parts costs nothing extra.

## The public parts

You rarely build a layout from raw fundamentals. The standard library exposes its own chrome as
composable **parts**. Each one returns `list<Html>`. Compose them with `flatten([…])`.

Shared:

| Part | Returns |
| --- | --- |
| `wdoc_part_search_box(enabled)` | The search box and its `<style>`. Empty when `enabled` is false. |
| `wdoc_part_theme_toggle(enabled)` | The light/dark toggle button and its one-time script. |
| `wdoc_part_home_link(c, cls)` | The back-to-root link for a sub-site. Empty otherwise. |

Webpage:

| Part | Returns |
| --- | --- |
| `wdoc_webpage_layout(c)` | The whole `webpage` body. The template is exactly this. |
| `wdoc_part_webpage_css()` | The header / nav / content-card `<style>`. |
| `wdoc_part_header(c)` | The site-title `<header class="site-header">`. |
| `wdoc_part_navbar(c)` | The `<nav class="site-nav">`: back-link, search box, and the menu. |
| `wdoc_part_content(c)` | The `content` slot inside `<main class="site-main">`. |
| `wdoc_part_menu_tree(c)` | The nested `<ul class="menu">` alone. |
| `wdoc_part_menu_script(c.menu)` | The one-time dropdown-toggle script. Empty with no menu. |

Book:

| Part | Returns |
| --- | --- |
| `wdoc_book_layout(c)` | The whole `book` body. |
| `wdoc_part_book_css()` | The sidebar / TOC / reading-column `<style>`. |
| `wdoc_part_sidebar(c)` | The fixed `<nav class="book-sidebar">`: title, back-link, search, toggle, TOC, footer buttons, scroll script. |
| `wdoc_part_sidebar_footer(c)` | The pinned footer buttons alone. Empty with no `sidebar_footer`. |
| `wdoc_part_book_content(c)` | The reading column `<main>` plus the previous/next pagination. |
| `wdoc_part_book_rail(c)` | The right-hand "on this page" rail. Empty on a page with no h2/h3. |
| `wdoc_part_toc_tree(c)` | The nested `<ul class="book-toc">` alone. |
| `wdoc_part_pagenav(c)` | The previous/next links alone. |

Three of those parts also have a `_with_metadata` twin: `wdoc_part_sidebar_with_metadata(c, m)`,
`wdoc_part_book_content_with_metadata(c, m)` and `wdoc_part_book_rail_with_metadata(m)`. Call
`page_metadata(c)` once. Then pass the result to each twin you use.

Website and presentation parts have their own references:
[`wdoc_websites.md`](wdoc_websites.md), [`wdoc_presentations.md`](wdoc_presentations.md).

## Two ways to reuse a built-in

**Extend it.** Call the layout wholesale and append your own chrome:

```wcl
template blog {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      wdoc_webpage_layout(c),
      [ el("footer", ["site-footer"], [raw("© 2026 — built with wdoc")]) ],
    ])
}

site myblog { default_template = :blog  title = "My blog" }
```

**Rebuild it from parts.** Copy the layout's body and swap one part. Omit a part to drop that
region:

```wcl
template app_home {
  slot content: content
  slot hero: content?
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      wdoc_part_webpage_css(),                  // keep the stdlib styling
      [ el("header", ["hero"], slot(c, :hero)) ],  // a hero instead of wdoc_part_header
      wdoc_part_navbar(c),                      // reuse the stdlib nav
      wdoc_part_content(c),                     // the standard content <main>
    ])
}
```

## The `el` constructor family

Where no part fits, build the markup yourself. The `el` family is a set of shorthands for the
HTML element vocabulary. Each one is exactly its long form with the field names dropped:

```wcl
el("div", ["book-toc-row"], kids)
Html::Element { tag: "div", class: ["book-toc-row"], children: kids }
```

| Constructor | Builds |
| --- | --- |
| `el(tag, cls, kids)` | `<tag class>children</tag>` — the common shape. |
| `ela(tag, cls, attrs, kids)` | The same, with attributes: a list of `[name, value]` pairs. |
| `eli(tag, id, cls, kids)` | The same, with an explicit HTML id. |
| `raw(html)` | Verbatim, pre-rendered HTML. **Not escaped.** |
| `css_style(:name)` | A named top-level `style` block, rendered in place as `<style>`. |
| `inl(text)` | An inline prose run, fed through the inline-pattern engine. |
| `icon(name, cls)` | An icon from a declared `iconset` or a built-in pack. |
| `para(cls, spans)` | A `<p>` of inline-patterned spans. |
| `wdoc_blocks(handles)` | Typed placement of authored block handles. |

There are three element constructors rather than one because a WCL parameter list is fixed at
declaration. The language has neither default nor named arguments, so every call fills every
parameter, and `id` and `attrs` each need a name of their own.

An empty `class` or `attrs` list emits no attribute at all. `el("li", [], kids)` renders `<li>`,
so passing `[]` costs nothing over omitting the field. An optional field goes straight in:
`el("p", t.class, kids)` with `t.class` unset renders as an omitted `class`.

**`ela` and `eli` take the same number of arguments.** Arity is the one positional mistake WCL
catches, and it cannot separate these two. Calling one where you meant the other drops the id
or the attributes silently. Pick by what you are passing.

The family covers the HTML **element** vocabulary only. `Svg` shapes and the semantic content
IR keep the named-field record literal, because they are field-shaped. WCL checks argument
arity but never argument types. Transposing two of a shape's interchangeable `f64`s would
therefore render silently wrong, where a record raises a shape mismatch.

Write the long form for anything the family does not name. That covers an element carrying both
an id and attrs, a `Paragraph` with an id, a `Head`, a `Table`, a `Highlighted` and a `Math`.

## Gotchas

- **Parts resolve by bare name.** The parts, the `el` family and `page_metadata` are plain
  names reached through `import <wdoc.wcl>`. They are not `wdoc::`-qualified. Name your own
  `let`s clear of those families — a `let` called `wdoc_part_*`, `wdoc_*_layout`, `el`, `ela`,
  `eli`, `raw`, `inl`, `icon` or `para` shadows the standard-library one.
- A raw heredoc's closing delimiter must sit **alone on its line**. `HTML ) ],` on one line is
  an unterminated heredoc.
- Keep `content*` for a layout you mean to be a collection template. A repeated slot anywhere
  else converts a page layout into one, which changes how the whole site builds.
- `render` must return `list<Html>`, not `list<Content>`. The two vocabularies both declare a
  `Paragraph` and a `Table`; a template speaks the HTML one.

## See also

- [`wdoc_websites.md`](wdoc_websites.md) — named slots, head assets and the `:website` layout.
- [`wdoc_presentations.md`](wdoc_presentations.md) — the one built-in collection template.
- [`wdoc_sites.md`](wdoc_sites.md) — the `toc`, `menu` and `sidebar_footer` blocks a template
  reads.
- [`wdoc_styling.md`](wdoc_styling.md) — the `style` blocks `css_style(:name)` renders.
