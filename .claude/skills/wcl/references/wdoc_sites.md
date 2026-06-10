# Sites & Templates

A `site` block configures one output site — its template, title, theme, and navigation. A document can declare several sites; each page joins one or more via its `sites` field.

## Fields



## webpage template

Hugo-style site header, sticky top navbar built from `menu`, and a reading column.

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

Menu items use `page = <name>` for in-site links (validated against pages in this site) or `href = "…"` for external or cross-site URLs. Nested `item`s become dropdown groups.

## book template

mdBook-style fixed left sidebar with nested chapters and current-chapter highlight; reading column on the right.

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

Chapters nest to any depth. A `chapter` with no `page =` is a grouping heading. A `chapter` pointing at an unknown page is a build error.

## Search

Set `search = true` on a site to add client-side full-text search. The build writes a per-page text index to `_wdoc/search-index.json` (page title from its first `h1`, plus the page body text) and the `book` and `webpage` templates render a search box — in the sidebar and the nav respectively — backed by a small bundled script. Typing ranks pages by title and body matches and shows the top hits with a context snippet; Enter opens the first hit, Escape clears.

> [!NOTE]
> **Served, not opened**
> The widget fetches the index over HTTP, so search works when the site is hosted (or under `wcl wdoc serve`), not when a page is opened directly from disk.

## presentation template

A reveal.js-style slide deck: the whole site renders into a single `index.html`, navigated with the keyboard. The `deck` block lays out the 2-D grid — each `section` is a column, its `slide`s are rows — and each `slide` names a page that belongs to this site.

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

> [!NOTE]
> **Keyboard navigation**
> **← →** move between sections, **↑ ↓** between the slides within a section, **Space / PageDown** step forward (revealing fragments, then advancing), **s** toggles speaker notes, **f** fullscreen. A progress bar, slide counter, and nav-hint arrows update as you go.

Each `slide` must sit on its own line (like `li` / `chapter`). A `slide` pointing at an unknown page is a build error. Two in-slide blocks are deck-specific:

| Block | Renders |
| --- | --- |
| `fragment { … }` | A step-reveal group — its content stays hidden until the presenter advances with Space |
| `notes { … }` | Speaker notes — hidden in the deck, shown in the overlay toggled with `s` |

```wcl
page topic {
  h2 "Key points"
  fragment { p "Revealed on the first Space" }
  fragment { p "…then this one" }
  notes { p "Reminder: mention the benchmark numbers." }
}
```
