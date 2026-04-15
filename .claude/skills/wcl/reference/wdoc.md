# wdoc Format

Documentation format built on top of WCL. Sources: `crates/wcl_wdoc/src/wdoc.wcl` (schemas + widgets), `crates/wcl_wdoc/src/model.rs` (data model), `crates/wcl/src/cli/wdoc.rs` (render pipeline).

For drawings, see `wdoc-drawings.md`.

## Top-Level Structure

```wcl
import <wdoc.wcl>
use wdoc::{doc, section, page, layout, heading, paragraph}

doc my_site {
  title   = "My Project"
  version = "0.1.0"          # optional
  author  = "Wil"            # optional

  section intro "Introduction" {}
  section guide "Guide" {
    section install "Installation" {}
  }
}

page welcome {
  section = "my_site.intro"   # qualified section ID
  title   = "Welcome"
  layout {
    heading   h1 { level = 1, content = "Welcome" }
    paragraph p1 { content = "This is my project." }
  }
}
```

A site = one `doc` block (title + section outline) + one or more `page` blocks (content bound to sections). Split across files freely; use `import "./*.wcl"` glob to assemble.

## Core Schemas

| Block | Purpose | Key attributes |
|-------|---------|----------------|
| `doc` | Root site | `title`, `version?`, `author?`, `section` children |
| `section` | Outline node | Inline ID, title (inline arg), nested sections |
| `page` | Content page | `section` (qualified ID), `title`, `layout` child |
| `layout` | Page content container | children: split groups or content |
| `style` | Named CSS rule set | children keyed by content kind |

## Content Elements

Require `use wdoc::{...}` for the blocks you use.

### Headings & Text

```wcl
heading h1_main { level = 1, content = "Welcome to WCL" }
h1 "Quick Start"                       # shorthand: inline arg as content
h2 "Advanced Topics"                   # h1 through h6 available
paragraph p1 { content = "Body text " + bold("with emphasis") + "." }
p "Shorthand paragraph."               # inline-arg shorthand
```

### Code

```wcl
code example {
  language = "wcl"
  content  = <<-'WCL'
    server api {
      host = "localhost"
      port = 8000
    }
  WCL
}
```

### Image

```wcl
image hero {
  src    = "images/banner.png"
  alt    = "Hero banner"
  width  = "100%"
  height = "auto"
}
```

Assets are auto-copied from input directories on build; extensions: `png`, `jpg`, `jpeg`, `gif`, `svg`, `webp`, `ico`.

### Data Table

```wcl
data_table features {
  caption = "Feature Comparison"
  rows = [
    { feature = "Typing", json = "No",  wcl = "Yes" },
    { feature = "Macros", json = "No",  wcl = "Yes" }
  ]
}
```

Renders as `<table>`; first list-of-maps attribute is treated as the rows.

### Callout

```wcl
callout info_box {
  icon   = "info"            # Bootstrap icon name
  header = "Important"
  color  = "#0d6efd"         # or "var(--color-link)"
  paragraph p1 { content = "This is a note." }
}
```

## Inline Formatting Functions

Return HTML strings; use inside `content = ...`.

```wcl
bold(text)                     # <strong>text</strong>
italic(text)                   # <em>text</em>
link(text, url)                # <a href="url">text</a>
icon(name, size?, color?)      # Bootstrap icon

paragraph p {
  content = "See " + bold("API docs") + " at " + link("example.com", "https://example.com")
}
```

## Layouts and Splits

```wcl
layout {
  vsplit cols {                # columns side-by-side
    split left  { size = 60
      heading   h1 { level = 1, content = "Left" }
      paragraph p  { content = "..." }
    }
    split right { size = 40
      heading   h1 { level = 1, content = "Right" }
    }
  }

  hsplit rows {                # rows stacked top-to-bottom
    split top    { size = 30, paragraph p { content = "Header" } }
    split bottom { size = 70, paragraph p { content = "Body"   } }
  }
}
```

- `vsplit` = row flex; `hsplit` = column flex.
- `size` is a percentage of the parent dimension.

## Styles

```wcl
style hero {
  heading   { color = "#FF0000", font_size = "2em" }
  paragraph { font_size = "1.2em" }
}

@style("hero")
heading big_title { level = 1, content = "Big Title" }
```

## Theme Variables

CSS custom properties available in any color-valued attribute (including inside drawings):

- `--color-bg`, `--color-text`
- `--color-link` (primary)
- `--color-code-bg`, `--color-code-text`
- `--color-nav-bg`, `--color-nav-border`

Use as `fill = "var(--color-link)"`.

## Rendering Pipeline

`wcl wdoc build` → parses WCL → runs the full pipeline (macros, imports, evaluation) → extracts a `WdocDocument` → calls `@template("html", "fn")` renderers per content block → emits `index.html`, per-page `.html`, `styles.css`, highlight assets, copied images.

See `reference/cli.md` for `wcl wdoc build/validate/serve/install-library`.

## Where to Find Examples

- `examples/wdoc/site.wcl` and `examples/wdoc/pages.wcl` — full site
- `docs/wdoc-overview.wcl` — quick start
- `docs/wdoc-content.wcl` — content element reference
- `docs/wdoc-styling.wcl` — styling guide
