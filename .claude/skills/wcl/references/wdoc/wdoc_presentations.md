# Presentations

A presentation is a reveal.js-style slide deck. The whole site renders to **one**
`index.html`, and a bundled player navigates it by keyboard.

Slides are the site's ordinary `page` blocks. A `deck` block on the `site` arranges them into a
two-dimensional grid: each `section` is a column, and its `slide`s are the rows.

```wcl
import <wdoc.wcl>

site talk {
  default_template = :presentation
  title            = "My talk"
  theme            = :catppuccin
  deck {
    section "Intro" {
      slide title
      slide agenda
    }
    section "Main" { slide topic }
  }
}

page title  { h1 "My talk" }

page agenda {
  h2 "Agenda"
  list {
    li "Why"
    li "How"
  }
}

page topic {
  h2 "Key points"
  fragment { p "Revealed on the first Space" }
  fragment { p "…then this one" }
  notes { p "Mention the benchmark numbers." }
}
```

```console
$ wcl wdoc build main.wcl --out _site
wrote 1 page
```

The build writes `_site/index.html` and `_site/_wdoc/presentation.js`. "1 page" is the deck, not
the slide count.

## The deck grid

| Block | Fields | Holds |
| --- | --- | --- |
| `deck` | — | `section` blocks, in order. Declared inside `site`. |
| `section` | label: the display title | `slide` blocks, in order. One column. |
| `slide` | label: a `page` name | Nothing. It names the page whose content the slide shows. |

The `deck` mirrors the `book` template's `toc { chapter … }` shape. A `slide` that names an
unknown page is a build error, exactly as a `chapter` is.

Each `slide` must sit on **its own line**. A second block on the same line becomes a child of
the first.

## Keyboard navigation

The bundled `presentation.js` player drives the grid:

| Key | Does |
| --- | --- |
| ← / → | Move between sections. It lands on the section's top slide. |
| ↑ / ↓ | Move between the slides in the current section. |
| Space / PageDown | Step forward: reveal the next `fragment`, then advance to the next slide. |
| ⇧Space / PageUp | Step backward. |
| Home / End | The first / the last slide. |
| `s` | Toggle the speaker-notes overlay. |
| `f` | Toggle fullscreen. |

An arrow key shows the target slide with every fragment already revealed. Only the Space path
reveals fragments one at a time. The player also keeps the URL hash in step
(`#/<section>/<slide>`, both zero-based), so a slide is directly addressable.

The layout draws a progress bar, a slide counter and nav-hint arrows. The player updates all
three as you move.

## In-slide blocks

Two content blocks exist for a deck.

### `fragment` — step reveal

```wcl
fragment { p "Revealed on the first Space" }
fragment { p "…then this one" }
```

A `fragment` holds any content blocks: a paragraph, a list item, an image, a diagram. Its
children stay hidden until the presenter steps forward.

`fragment` is legal on **any** page, not only a deck. Outside a deck it is a transparent
wrapper: HTML wraps the children in `<div class="wdoc-fragment">` and shows them, and the
Markdown and PDF targets render the children in place.

### `notes` — speaker notes

```wcl
notes { p "Mention the benchmark numbers." }
```

On the presentation build, the renderer takes the notes out of the slide's visible content. It
puts them into the hidden overlay that `s` toggles.

**A `notes` block only works on a presentation site.** It is a named fill for the
`presentation` template's `notes` slot. A site whose layout declares no such slot refuses it:

```console
page `topic` fills slot `notes`, but no layout used by this site declares it
```

The `notes? { … }` conditional form does not help, because no layout used by that site declares
the slot at all. Keep deck pages on a deck site.

## How a deck behaves on the other targets

| Target | Result |
| --- | --- |
| `wcl wdoc build` | One `index.html` holding the whole deck, plus `_wdoc/presentation.js`. |
| `wcl wdoc serve` | The same build behind the watching dev server. |
| `wcl wdoc markdown` | One `.md` **per page**. The deck grid disappears; a `fragment`'s children and a `notes` block's children render in place. |
| `wcl wdoc pdf` | One flowed document. A `fragment`'s children render in place. |

The Markdown output of the `topic` page above:

```markdown
## Key points

Revealed on the first Space

…then this one

Mention the benchmark numbers.
```

Note that the speaker notes are **visible** there. They are hidden only by the deck's own
overlay. Do not put anything private in `notes` if you also ship the Markdown or the PDF.

## The `presentation` template

`presentation` is the one built-in **collection** template. Its slot contract is what makes it
one:

```wcl
template presentation {
  slot content: content*
  slot notes:   content* = fn(c: SlotOwner) -> list<Html> []
  render = fn(c: TemplateCtx) -> list<Html>
    wdoc_presentation_layout(c)
}
```

A repeated slot (`content*`) means the renderer calls the template **once for the whole site**
rather than once per page. Two rules follow:

- A collection template must be selected by the **site**, through `default_template`. A page
  that names one in its own `template` field fails the build.
- `TemplateCtx.deck` carries the resolved grid: a `list<DeckSection>`, each
  `{ title, slides: list<PageHandle> }`. Placing a member handle's slot is what forces that
  page's body through the renderer, so an unplaced slide costs nothing.

A deck site with no `deck` block is an error: `a presentation site needs a deck block`.

### The parts

| Part | Returns |
| --- | --- |
| `wdoc_presentation_layout(c)` | The whole deck body. The template is exactly this. |
| `wdoc_part_presentation_css()` | The deck `<style>`: full-viewport slides, fragment fades, progress bar, counter, nav hints, notes overlay. |
| `wdoc_part_deck(c)` | The `<div class="deck">` slide grid. |
| `wdoc_part_deck_chrome()` | The progress bar, the counter and the nav-hint arrows. |
| `wdoc_part_presentation_player()` | The request for the bundled `presentation.js`. |

Write your own deck layout by composing them:

```wcl
template my_deck {
  slot content: content*
  slot notes:   content* = fn(c: SlotOwner) -> list<Html> []
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      wdoc_part_presentation_css(),
      wdoc_part_deck(c),
      wdoc_part_presentation_player(),   // drop the chrome part
    ])
}
```

Drop `wdoc_part_presentation_player()` and you lose the keyboard navigation. The layout owns
its player, exactly as it owns its CSS.

## Gotchas

- The build reports `wrote 1 page` for a deck of any size. That is the deck file, not a bug.
- The deck CSS sets `overflow: hidden` on the body. Long slide content scrolls inside its own
  slide.
- `notes` outside a presentation site is a build error, not a silent no-op.
- Each `slide`, `li` and `chapter` needs its own line.
- Fragments reveal in **source order** within a slide.

## See also

- [`wdoc_templates.md`](wdoc_templates.md) — collection templates, `TemplateCtx`, the parts.
- [`wdoc_sites.md`](wdoc_sites.md) — the `site` block and page membership.
- [`wdoc_outputs.md`](wdoc_outputs.md) — the build, Markdown and PDF targets.
