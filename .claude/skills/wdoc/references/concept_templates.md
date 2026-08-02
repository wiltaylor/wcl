# Templates

_The built-in templates webpage / book / presentation / ai_skill, their navigation blocks, and writing a custom template._

A **template** turns a site's pages into a particular shape of output. A site selects one with
`default_template` (a page may override with `template`). Four are built in — `:webpage`,
`:book`, `:presentation`, and `:ai_skill` — and each reads its own navigation block off the
`site`.


## webpage template

A Hugo-style site header, a sticky top navbar built from `menu`, and a reading column. Menu
items use `page = <name>` for in-site links (validated against pages in this site) or
`href = "…"` for external or cross-site URLs. Nested `item`s become dropdown groups.


```wcl
site marketing {
  default_template = :webpage
  title            = "My project"
  root             = true
  theme            = :nord
  menu {
    item "Home"     { page = index }
    item "Docs"     { href = "docs/" }
    item "More" {
      item "About"   { page = about }
      item "Contact" { page = contact }
    }
    item "Source"   { href = "https://github.com/example/proj" }
  }
}
```

## book template

An mdBook-style fixed left sidebar with nested chapters and current-chapter highlight; reading
column on the right. Chapters nest to any depth. A `chapter` with no `page =` is a grouping
heading. A `chapter` pointing at an unknown page is a build error.


```wcl
site docs {
  default_template = :book
  title            = "Project Docs"
  theme            = :nord
  theme_toggle     = true
  toc {
    chapter "Intro"        { page = index }
    chapter "Guide" {
      chapter "Setup"      { page = setup }
      chapter "First run"  { page = first_run }
    }
  }
}
```

## presentation template

A reveal.js-style slide deck: the whole site renders into a single `index.html`, navigated
with the keyboard. The `deck` block lays out the 2-D grid — each `section` is a column, its
`slide`s are rows — and each `slide` names a page that belongs to this site.


```wcl
site talk {
  default_template = :presentation
  title            = "My talk"
  theme            = :catppuccin
  deck {
    section "Intro" {
      slide title
      slide agenda
    }
    section "Main" {
      slide topic
    }
  }
}
```

Each `slide` must sit on its own line. Two in-slide blocks are deck-specific: `fragment { … }`
is a step-reveal group (hidden until the presenter advances with Space), and `notes { … }`
holds speaker notes (hidden in the deck, shown in the overlay toggled with **s**).


```wcl
page topic {
  h2 "Key points"
  fragment { p "Revealed on the first Space" }
  fragment { p "…then this one" }
  notes { p "Reminder: mention the benchmark numbers." }
}
```

## ai_skill template

A fourth built-in: `default_template = :ai_skill` makes the site a Claude / agent skill
folder, built by `wcl wdoc skill` (not `wcl wdoc build`). See the **skills** concept for the
`skill { }` block and `file` blocks.


## Custom templates

The built-ins are not special: a template is just a function from a `TemplateCtx` to a list of
HTML fundamentals. Declare a
`template <name> { render = fn(c: TemplateCtx) -> list<Html> … }` and select it
with a site's `default_template` (e.g. `:blog`) or a page's `template` field. The stdlib
exposes its chrome as composable parts (`wdoc_part_*`) plus one `wdoc_*_layout` per built-in,
all resolved by bare name once you `import <wdoc.wcl>`.


The page arrives as read-only authored handles in `c.content`, not pre-rendered HTML. Query
`BlockHandle.kind`, its concrete `block` fields, or recursive `children`, then place the
selected handles with `wdoc_blocks(handles)`; resolution happens after the template returns.
For cross-page navigation, `page_metadata(c)` returns memoised reading order, neighbours and
active path without evaluating another page body; it also derives the current page's h2/h3
metadata from those authored handles.


Where no part fits, build the markup with the `el` constructor family — shorthands for the
HTML element vocabulary, each exactly the long form with the field names dropped.
`el(tag, cls, kids)` is the common shape, `ela(tag, cls, attrs, kids)` adds attributes (a list
of `[name, value]` pairs) and `eli(tag, id, cls, kids)` an explicit id; the leaves are
`raw(html)` (verbatim, unescaped), `inl(text)` (an inline-patterned prose run),
`icon(name, cls)` and `para(cls, spans)`. There are three element constructors rather than one
because a WCL parameter list is fixed at declaration — the language has neither default nor
named arguments, so every call fills every parameter and `id` / `attrs` each need a name of
their own. An empty `class` / `attrs` list emits no attribute, so `[]` costs nothing over
omitting the field. Note that `ela` and `eli` take the SAME number of arguments: arity is the
one positional mistake WCL catches, and it can't tell those two apart, so calling one where
you meant the other drops the id or the attributes silently.


```wcl
template blog {
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      wdoc_webpage_layout(c),
      [ el("footer", ["site-footer"], [raw("© 2026 — built with wdoc")]) ],
    ])
}
```

The family covers the HTML **element** vocabulary only. `Svg` and the semantic
content IR keep the named-field literal: they are field-shaped, and WCL checks argument arity
but never argument types, so transposing two of a shape's interchangeable `f64`s would render
silently wrong where the record raises a shape mismatch. Write the record for anything the
family doesn't name, too — an element with both an id and attrs, a `Paragraph` with an id, a
`Head`, a `Table`.


> [!NOTE]
> **Parts resolve by bare name**
>
> Template parts and the `el` family are plain functions reached by name through `import <wdoc.wcl>` — don't define a `let` of your own named `wdoc_part_*`, `wdoc_*_layout`, `el`/`ela`/`eli`, `raw`, `inl`, `icon` or `para`, or it will shadow the stdlib one.

## Block reference

A `template` block: a custom output shape — a `render` function from a `TemplateCtx` to a list of HTML fundamentals, selectable by `default_template`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes |  |

A `menu` block: the top navbar navigation for the `webpage` template, holding `item` entries.

| Property | Type | Required | Description |
| --- | --- | --- | --- |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `items` | `item` | yes |  |
| `generators` | `wdoc_repeater` | yes |  |

An `item` in a `menu`: a label with a `page` (in-site link) or `href` (external / cross-site), nesting more `item`s as a dropdown group.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `label` | `utf8` | yes |  |
| `page` | `identifier` | no |  |
| `href` | `utf8` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `item` | yes |  |
| `generators` | `wdoc_repeater` | yes |  |

A `toc` block: the left-sidebar table of contents for the `book` template, holding nested `chapter` entries.

| Property | Type | Required | Description |
| --- | --- | --- | --- |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `chapters` | `chapter` | yes |  |
| `generators` | `wdoc_repeater` | yes |  |

A `chapter` in a `toc`: a label with an optional `page` (omit it for a grouping heading), nesting more chapters to any depth.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `title` | `utf8` | yes |  |
| `page` | `identifier` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `chapter` | yes |  |
| `generators` | `wdoc_repeater` | yes |  |

A `deck` block: the 2-D slide grid for the `presentation` template — `section`s are columns, their `slide`s are rows.

| Property | Type | Required | Description |
| --- | --- | --- | --- |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `sections` | `section` | yes |  |

A `section` in a `deck`: one column of the slide grid, holding `slide` references.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `title` | `utf8` | yes |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `slides` | `slide` | yes |  |

A `slide` in a deck `section`: a reference to a page that belongs to the presentation site.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `page` | `identifier` | yes |  |

A `fragment` block: a step-reveal group inside a slide, hidden until the presenter advances.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `identifier` | no |  |
| `class` | `list<utf8>` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `body` | `WdocBlock` | yes |  |

A `notes` block: speaker notes inside a slide — hidden in the deck, shown in the presenter overlay.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `identifier` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `body` | `WdocBlock` | yes |  |

## Related

- [Pages](../references/concept_pages.md)

- [Skill folders](../references/concept_skills.md)

[← Back to SKILL.md](../SKILL.md)
