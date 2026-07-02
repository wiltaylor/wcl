# Render a slide deck

## Purpose

Turn a site into a keyboard-navigated slide deck with the `:presentation` template and a `deck` grid.

## Prerequisites

- A WCL file declaring the pages that will become slides

## Flowchart

![diagram](../_wdoc/process_render_presentation-diagram-1.svg)

## Steps

### Step 1: Opt the site into the presentation template

```wcl
site talk {
  default_template = :presentation
  title            = "My talk"
  theme            = :nord
}
```

Set `default_template = :presentation` on the `site`. The whole site renders into a single `index.html` navigated with the keyboard — ← → move between sections, ↑ ↓ between the slides within one.

### Step 2: Lay out the deck grid

```wcl
site talk {
  default_template = :presentation
  title            = "My talk"
  deck {
    section "Intro" {
      slide title_slide
      slide agenda
    }
    section "Main" { slide topic }
  }
}
```

Declare a `deck` block on the site: each `section` is a column of the 2-D grid and each `slide` names a page belonging to this site. Each `slide` must sit on its own line.

### Step 3: Add reveals and speaker notes

```wcl
page topic {
  h2 "Key points"
  fragment { p "Revealed on the first Space" }
  fragment { p "…then this one" }
  notes { p "Reminder: mention the benchmark numbers." }
}
```

Inside a slide page, a `fragment { … }` is a step-reveal group (hidden until the presenter advances with Space) and a `notes { … }` holds speaker notes (hidden in the deck, shown in the overlay toggled with **s**).

### Step 4: Build and present

```console
$ wcl wdoc build talk.wcl --out out/talk
```

Render with the ordinary `build` target (or iterate under `wcl wdoc serve`). Open `out/talk/index.html` and drive the deck with the arrow keys.

> [!TIP]
> **Verification**
> `out/talk/index.html` opens as a deck: arrow keys move between slides, Space reveals fragments, and **s** toggles the notes overlay.

## Related

- [Templates](../references/concept_templates.md)

- [Built-in site templates](../references/fact_template_kinds.md)

- [Sites](../references/concept_sites.md)

[← Back to SKILL.md](../SKILL.md)
