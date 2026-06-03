# Styling

Two layers control how a site looks: `class` blocks style individual elements, and a site `theme` sets the palette every built-in element draws from. Classes always win over theme defaults via the cascade, so you theme broadly and override locally.

## Classes

A `class <name> { … }` block declares a CSS class. Apply it by listing the name in any block's `class` field (or a span's). Fields cover text, box, and SVG paint properties; per-mode overrides go in `dark { }` / `light { }` sub-blocks.

### Authoring

```wcl
class accent {
  color = "var(--wdoc-accent)"
  bold  = true
  dark  { color = "#88c0d0" }
  light { color = "#5e81ac" }
}

// Use the class on a span:
text {
  span "highlighted segment" { class = ["accent"] }
}
```

Live — the `sty-accent` class declared at the top of this page, applied to a span (and following the light/dark mode):

![diagram](../_wdoc/wdoc_styling-diagram-1.svg)

### Field groups

| Group | Fields |
| --- | --- |
| Text | `color`, `background`, `bold`, `italic`, `underline`, `font_size`, `font_family`, `text_align`, `text_transform`, `letter_spacing`, `line_height` |
| Box | `padding`, `margin`, `border` |
| SVG | `fill`, `stroke`, `stroke_width`, `stroke_linejoin`, `stroke_linecap`, `opacity` |
| Callout | `accent` — sets a callout's accent colour (heading, border, icon) when the class is on a `callout` |
| Modes | `dark { … }`, `light { … }` for prefers-color-scheme overrides |

### Hyphenated names

Hyphenated class names may be written bare — `class wdoc-series-1 { fill = "#88c0d0" }` — or quoted; both are equivalent. (Names with other non-identifier characters, like spaces, still need quoting.) This is how you override built-in classes like the chart palette or callout styles.

> [!NOTE]
> **Site scope**
> Set sites = \[:foo\] on a class to scope it to one site in a multi-site document. Omit the field and the class applies everywhere.

## Themes

A theme is a complete colour palette plus the rules that map it onto every built-in element — page background, links, headings, code, charts, callouts, tables, inline emphasis. Selecting a theme in one line recolours the whole site.

### Selecting a built-in theme

Set `theme = :<name>` on a `site` — a symbol naming a `theme` block. Seven built-in palettes ship, each with co-ordinated dark and light variants. `theme_toggle = true` adds a light/dark toggle button to the chrome.

```wcl
site mysite {
  default_template = :book
  theme            = :tokyonight
  accent           = :cyan
  theme_toggle     = true
}
```

### Built-in themes

| Theme | Notes |
| --- | --- |
| `nord` | Cool blue-grey, the default |
| `gruvbox` | Warm retro |
| `solarized` | Classic Solarized |
| `catppuccin` | Soft pastel |
| `tokyonight` | Vivid night palette |
| `kanagawa` | Muted with hints of indigo |
| `everforest` | Green forest tones |

### Accent colour

Independently of the theme, `accent = :cyan` (or `:red`/`:orange`/`:yellow`/`:green`/`:blue`/`:purple`/`:pink`) picks the hue used for links and current-chapter highlights. Default is `:blue`.

### Custom themes

A theme is just a `theme` block holding a `dark` and a `light` `palette` sub-block; select it with `theme = :name`. Each palette sets any of the 18 colour roles (documented in `lib/theme.wcl`); omitted roles fall back through the cascade.

```wcl
theme sunset {
  palette dark {
    bg = "#1a1a2e"  fg = "#e0e0e0"
    blue = "#5e81ac"  green = "#a3be8c"
    // … any of the 18 roles
  }
  palette light {
    bg = "#fdf6e3"  fg = "#073642"
  }
}

site mysite { theme = :sunset  accent = :blue }
```
