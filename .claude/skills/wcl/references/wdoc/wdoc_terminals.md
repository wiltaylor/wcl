# Terminals and TUI

A `terminal` is a monospace character grid rendered as inline SVG, drawn with a bundled
JetBrains Mono Nerd Font. Box-drawing glyphs, powerline symbols and Nerd Font icons all render.

`terminal` `extends ContentBlock`, so it is a **page block**. Write it in a `page` body, a
`card` body, a `column`, or anywhere else a `ContentBlock` is accepted. It is not a diagram
shape.

It is a **native** block — wdoc owns the grid model, the ANSI handling and the replay player in
Rust — so it declares no `lower`.

Populate the grid three ways:

| Way | How | Use it for |
| --- | --- | --- |
| Authored primitives | `term_text` and the widgets below, at 1-based `(row, col)` | Hand-drawn mockups, TUI designs |
| Inline text | `text = "…"` | Quick output samples, ANSI escape demos |
| Recording | `source = "rec.cast"` | Replaying a real session |

## The `terminal` block

```wcl
terminal {
  cols = 46  rows = 7  title = "demo"
  term_text "Colours" { row = 1  col = 2  bold = true  underline = true }
  term_text "red"     { row = 2  col = 2   fg = "red" }
  term_text "green"   { row = 2  col = 8   fg = "green" }
  term_text "blue"    { row = 2  col = 16  fg = "blue" }
  term_text "#ff5fd2" { row = 2  col = 23  fg = "#ff5fd2" }
  term_box { row = 4  col = 2  width = 32  height = 3  border = :rounded  fg = "cyan"  title = "box" }
  term_text "rounded border" { row = 5  col = 4 }
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `cols`, `rows` | `i64` | Grid size (defaults `80` × `24`). A `.cast` header overrides both. |
| `font_size`, `line_height` | `f64` | Cell metrics. |
| `palette` | `symbol` | Seed colours: `:default` (dark) or `:light`. |
| `fg`, `bg` | `utf8` | Explicit default foreground / background. |
| `chrome` | `bool` | Draw the window title bar (default `true`). Set `false` for a bare grid. |
| `title` | `utf8` | Caption in the chrome bar. |
| `text` | `utf8` | Inline content fed to the virtual terminal, ANSI included. |
| `source` | `utf8` | Path to an asciinema `.cast` recording. |
| `autoplay`, `loop` | `bool` | Replay controls. |
| `speed` | `f64` | Replay speed multiplier. |
| `id`, `class` | | HTML id; the `class` themes the window `<div>`. |
| `children` | `@children(TermPrimitive)` | The placeable primitives and widgets. |

## `term_text` — the one primitive

Everything drawn on the grid is a run of styled text. `term_text` is that run:

| Field | Type | Meaning |
| --- | --- | --- |
| `content` | `utf8` | The text. `@inline(0)`, so you write `term_text "hi" { … }`. May contain `\n`. |
| `row`, `col` | `i64` | **1-based** grid position. Required. |
| `fg`, `bg` | `utf8` | Colours — see below. |
| `bold`, `dim`, `italic`, `underline`, `strike`, `blink`, `inverse`, `conceal` | `bool` | Style flags. |

**Colours are strings**, in three forms:

- an ANSI name — `"red"`, `"green"`, `"blue"`, `"cyan"`, `"magenta"`, `"bright_black"`,
  `"bright_white"`, and the rest of the sixteen;
- a 256-palette index as text — `"208"`;
- a hex value — `"#ff5fd2"`.

An ANSI name resolves through the terminal's palette, so it follows a site theme. A hex value
does not.

## Boxes, fills and glyphs

Three helpers save you writing the runs by hand. Each one lowers to styled-text runs. The
renderer paints glyphs, never vector chrome.

| Block | Fields beyond `row` / `col` / `fg` / `bg` / `bold` | Draws |
| --- | --- | --- |
| `term_box` | `width`, `height` (both required), `border`, `title` | A bordered box, optional title in the top edge. |
| `term_glyph` | `glyph` (`@inline(0)`, required) | One styled run — the ergonomic single-run form. |
| `term_fill` | `ch` (`@inline(0)`), `width`, `height` (all required) | A rectangle of one repeated character. |

`border` takes `:single` (default), `:double`, `:rounded`, `:heavy` or `:ascii`.

## Inline text and recordings

Set `text` and a bundled virtual terminal evaluates it. ANSI sequences, cursor movement and
styling all apply; a bare `\n` is a newline:

```wcl
terminal {
  cols = 30  rows = 3  title = "inline"
  text = "first line\nsecond line"
}
```

Set `source` for asciinema replay. Frames are coalesced and replayed at the recording's own
pace. Playback **stops at the end** unless you set `loop = true`:

```wcl
terminal {
  cols = 80  rows = 24  chrome = true
  source = "casts/demo.cast"
  loop   = true
}
```

The `.cast` header supplies `cols` / `rows`, so any values you write are overridden.

## TUI widgets

Colour-forward, single-line controls for mocking up a text user interface. Every one is a
`TermPrimitive`, so it is a legal `terminal` child and takes `row` / `col`.

```wcl
terminal {
  cols = 44  rows = 8  title = "controls"
  term_text "Progress" { row = 1  col = 2  bold = true  underline = true }
  tui_progress "Upload" { row = 2  col = 2  value = 78 }
  tui_progress "Sync"   { row = 3  col = 2  value = 40  accent = "cyan" }
  term_text "Buttons" { row = 5  col = 2  bold = true  underline = true }
  tui_button "Save"    { row = 6  col = 2   accent = "green" }
  tui_button "Discard" { row = 6  col = 11  accent = "red" }
}
```

**Leaf widgets:**

| Block | Inline label | Own fields |
| --- | --- | --- |
| `tui_progress` | optional caption | `value` (required), `max` (100), `width` (24), `show_value` (true), `accent` (`green`), `muted` (`bright_black`) |
| `tui_button` | the caption | `width` (label + 4), `accent` (`blue`), `fg` (`bright_white`) |
| `tui_spinner` | optional caption | `frame` (0), `kind` (`:dots` / `:braille` default, `:circle`, `:line`), `accent` (`cyan`) |
| `tui_input` | the placeholder | `value`, `focused`, `accent` (`blue`) |
| `tui_dropdown` | the selected label | `items`, `selected`, `width`, `open`, `accent` (`blue`) |
| `tui_checkbox` | the caption | `checked`, `accent` (`green`), `muted` (`bright_black`) |
| `tui_radio` | the caption | `selected`, `accent` (`green`), `muted` (`bright_black`) |

Notes:

- `tui_spinner` draws **one static frame**. A rendered document does not animate, so pick the
  frame you want with `frame`.
- `tui_input` shows the placeholder muted when `value` is unset, and the value solid when it is
  set. `focused = true` adds a trailing cursor.
- `tui_dropdown` shows just its `text` and a `▾` caret when closed. With `open = true` and an
  `items` list, the options drop below the field and the one matching `text` — or the explicit
  `selected` index — is highlighted.
- `tui_radio` marks one option. Laying a group out is your job; nothing groups them.

**Container widgets** — each takes `@children(TermPrimitive)`, so widgets nest:

| Block | Own fields | Content origin |
| --- | --- | --- |
| `tui_panel` | `title`, `width`, `height`, `border` (`:rounded`), `accent` (`bright_black`) | Inside the border, at local row 2, col 2. |
| `tui_group` | `title` | Row 1 with no title; row 2 under one. |

```wcl
terminal { cols = 30  rows = 6  chrome = false
  tui_panel { row = 1  col = 1  width = 30  height = 6  title = "Status"
    tui_progress "Load" { row = 1  col = 1  value = 50  width = 16 }
    tui_button "Go" { row = 3  col = 1  accent = "green" }
  }
}
```

**Child coordinates are relative to the container.** The `tui_progress` above is at the panel's
own row 1, not the terminal's. The renderer adds the container's content origin.

## Writing your own control

Declare a `@block` type that extends `TermPrimitive` and give it a `lower` returning
`list<TermFundamental>`. It then plugs in exactly like a built-in widget:

```wcl
// A keycap pill: a coloured background run with the key label on top.
@block("kbd")
type Kbd extends TermPrimitive {
  @inline(0) key: utf8
  accent: utf8?
  row: i64  col: i64
  lower = fn(k: Kbd) -> list<TermFundamental> {
    let acc = if k.accent == none { "magenta" } else { k.accent };
    let w = len(k.key) + 2;
    [
      term_run(term_repeat(" ", w), 1, 1, none, acc, none),
      term_run(k.key, 1, 2, "bright_white", acc, true),
    ]
  }
}

terminal { cols = 30  rows = 1  chrome = false
  kbd "Ctrl" { row = 1  col = 1 }
  kbd "K"    { row = 1  col = 8  accent = "blue" }
}
```

`TermFundamental` is a union with exactly two variants:

```wcl
union TermFundamental {
  Text     { content: utf8  row: i64  col: i64  fg: utf8?  bg: utf8?  bold: bool? }
  Children { row: i64  col: i64 }
}
```

- `Text` is a styled run. `Children` marks the **content origin** of a container: the renderer
  draws the block's child widgets there, and recurses.
- Shared helpers save you building records by hand. `term_run(content, row, col, fg, bg, bold)`
  builds a `Text`. `term_repeat(ch, n)` repeats a character, and gives `""` when `n < 1`.
  `term_fill_runs(...)` builds a filled rectangle, and `term_box_runs(...)` a bordered box.
- **Lay your widget out from its own `(1, 1)`.** Never read your own `row` / `col`. The renderer
  owns placement and offsets the runs it gets back. That is what lets containers nest widgets.
- Prefer ANSI colour names for accents, so an instance follows the site theme. Leave label text
  uncoloured and it inherits the terminal's foreground.

## Gotchas

- **Coordinates are 1-based.** `row = 0` is off the grid.
- **A widget emits at `(1, 1)`.** Its own `row` / `col` are the renderer's to read; the renderer
  offsets the runs it gets back.
- **Colour, not chrome.** Box-drawing characters are glyphs on the same grid as everything else.
  There are no vector borders to style with CSS.
- **The grid does not grow.** Content past `cols` / `rows` is clipped. Size the terminal for
  what you drew.
- **`text` and `source` are alternatives to authored children.** Pick one way to fill a grid.
- **A `.cast` recording sets its own `cols` / `rows`.**
- **`terminal` is page content, not a shape.** To place one inside a drawing, put it in a `card`
  body — the card is the diagram shape.
- **The fonts ship in the output.** wdoc writes the Nerd Font faces into `_wdoc/` only when a
  document uses a terminal, so they need the output to be served.
