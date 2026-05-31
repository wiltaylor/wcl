# Sites & Templates

A `site` block configures one output site — its template, title, theme, and navigation. A document can declare several sites; each page joins one or more via its `sites` field.

## Fields

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | no | Optional site name (the label slot). Required with more than one site; names the output subdirectory referenced by `Page.sites`. |
| `root` | `bool` | no | When `true`, this site renders at `/` (others go under `/<site>/`). |
| `default_template` | `symbol` | no | `:webpage` (header + top nav), `:book` (sidebar + TOC), `:presentation` (slide deck), or `:ai_skill` (skill folder, Markdown-only — build with `wcl wdoc skill`). |
| `title` | `utf8` | no | Site title shown in the header / sidebar. |
| `icon` | `utf8` | no | Path to a favicon image (svg/png/ico), resolved relative to the document and copied into `_wdoc/`. Absent ⇒ a default WCL icon is used. |
| `theme_toggle` | `bool` | no | When `true`, adds a light/dark toggle button. |
| `theme` | `symbol` | no | Symbol naming a colour `theme` block (`:nord` …) — see Styling. |
| `accent` | `symbol` | no | Symbol naming the accent hue (`:red`..`:pink`); default `:blue`. |
| `ui_theme` | `symbol` | no | UI theme for `wf_*` wireframe elements (the mocked app's theme), separate from the document `theme`. Falls back to `theme`. |
| `ui_accent` | `symbol` | no | Accent hue for `wf_*` wireframe elements; falls back to `accent`. |
| `ui_mode` | `symbol` | no | Mode for `wf_*` wireframe elements — `:dark` (default) or `:light`. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `toc` | `toc` | no | Chapter tree for the `book` template. |
| `menu` | `menu` | no | Top navbar entries for the `webpage` template. |
| `deck` | `deck` | no | Slide grid for the `presentation` template. |
| `skill` | `skill` | no | Skill metadata for the `:ai_skill` target — populates SKILL.md's front matter. |

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
