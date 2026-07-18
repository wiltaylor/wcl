# Styling

_Class blocks, stylesheets, and themes — how a site looks._

Two layers control how a site looks: `class` blocks style individual elements, and a site `theme` sets the palette every built-in element draws from. Classes always win over theme defaults via the cascade, so you theme broadly and override locally.


## Classes

A `class <name> { … }` block declares a CSS class. Apply it by listing the name in any block's `class` field (or a span's). Fields cover text, box, and SVG paint properties; per-mode overrides go in `dark { }` / `light { }` sub-blocks.


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

## Field groups

| Group | Fields |
| --- | --- |
| Text | `color`, `background`, `bold`, `italic`, `underline`, `font_size`, `font_family`, `text_align`, `text_transform`, `letter_spacing`, `line_height` |
| Box | `padding`, `margin`, `border` |
| SVG | `fill`, `stroke`, `stroke_width`, `stroke_linejoin`, `stroke_linecap`, `opacity` |
| Callout | `accent` — sets a callout's accent colour (heading, border, icon) when the class is on a `callout` |
| Modes | `dark { … }`, `light { … }` for prefers-color-scheme overrides |

Hyphenated class names may be written bare — `class wdoc-series-1 { fill = "#88c0d0" }` — or quoted; both are equivalent. This is how you override built-in classes like the chart palette or callout styles. Set `sites = [:foo]` on a class to scope it to one site in a multi-site document; omit the field and the class applies everywhere.


## Themes

A theme is a complete colour palette plus the rules that map it onto every built-in element — page background, links, headings, code, charts, callouts, tables, inline emphasis. Set `theme = :<name>` on a `site` — a symbol naming a `theme` block. Seven built-in palettes ship (`forge` is the default, plus `nord`, `tokyonight`, `gruvbox`, `catppuccin`, `rose`, and `paper`), each with co-ordinated dark and light variants and its own typography. `theme_toggle = true` adds a light/dark toggle button.


```wcl
site mysite {
  default_template = :book
  theme            = :tokyonight
  accent           = :cyan
  theme_toggle     = true
}
```

Independently of the theme, `accent = :cyan` (or `:red`/`:orange`/`:yellow`/`:green`/`:blue`/`:purple`/`:pink`) picks the hue used for links and current-chapter highlights. Default is `:blue`. A custom theme is just a `theme` block holding a `dark` and a `light` `palette` sub-block.


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

## Block reference

A `class` block: a named CSS class with text, box, SVG, and callout-accent fields, plus per-mode `dark` / `light` overrides.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes |  |
| `sites` | `list<symbol>` | no |  |
| `color` | `utf8` | no |  |
| `background` | `utf8` | no |  |
| `bold` | `bool` | no |  |
| `italic` | `bool` | no |  |
| `underline` | `bool` | no |  |
| `font_weight` | `utf8` | no |  |
| `accent` | `utf8` | no |  |
| `font_size` | `utf8` | no |  |
| `line_height` | `utf8` | no |  |
| `font_family` | `utf8` | no |  |
| `text_align` | `utf8` | no |  |
| `text_transform` | `utf8` | no |  |
| `letter_spacing` | `utf8` | no |  |
| `padding` | `utf8` | no |  |
| `margin` | `utf8` | no |  |
| `border` | `utf8` | no |  |
| `fill` | `utf8` | no |  |
| `stroke` | `utf8` | no |  |
| `stroke_width` | `utf8` | no |  |
| `stroke_linejoin` | `utf8` | no |  |
| `stroke_linecap` | `utf8` | no |  |
| `opacity` | `utf8` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `dark` | `dark` | no |  |
| `light` | `light` | no |  |

The `dark` sub-block of a `class`: field overrides applied under a prefers-dark colour scheme.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `color` | `utf8` | no |  |
| `background` | `utf8` | no |  |
| `bold` | `bool` | no |  |
| `italic` | `bool` | no |  |
| `underline` | `bool` | no |  |
| `font_weight` | `utf8` | no |  |
| `accent` | `utf8` | no |  |
| `font_size` | `utf8` | no |  |
| `line_height` | `utf8` | no |  |
| `font_family` | `utf8` | no |  |
| `text_align` | `utf8` | no |  |
| `text_transform` | `utf8` | no |  |
| `letter_spacing` | `utf8` | no |  |
| `padding` | `utf8` | no |  |
| `margin` | `utf8` | no |  |
| `border` | `utf8` | no |  |
| `fill` | `utf8` | no |  |
| `stroke` | `utf8` | no |  |
| `stroke_width` | `utf8` | no |  |
| `stroke_linejoin` | `utf8` | no |  |
| `stroke_linecap` | `utf8` | no |  |
| `opacity` | `utf8` | no |  |

The `light` sub-block of a `class`: field overrides applied under a prefers-light colour scheme.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `color` | `utf8` | no |  |
| `background` | `utf8` | no |  |
| `bold` | `bool` | no |  |
| `italic` | `bool` | no |  |
| `underline` | `bool` | no |  |
| `font_weight` | `utf8` | no |  |
| `accent` | `utf8` | no |  |
| `font_size` | `utf8` | no |  |
| `line_height` | `utf8` | no |  |
| `font_family` | `utf8` | no |  |
| `text_align` | `utf8` | no |  |
| `text_transform` | `utf8` | no |  |
| `letter_spacing` | `utf8` | no |  |
| `padding` | `utf8` | no |  |
| `margin` | `utf8` | no |  |
| `border` | `utf8` | no |  |
| `fill` | `utf8` | no |  |
| `stroke` | `utf8` | no |  |
| `stroke_width` | `utf8` | no |  |
| `stroke_linejoin` | `utf8` | no |  |
| `stroke_linecap` | `utf8` | no |  |
| `opacity` | `utf8` | no |  |

A `stylesheet` block: raw CSS injected verbatim into the rendered site, for styling beyond the `class` field set.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes |  |
| `css` | `utf8` | yes |  |
| `sites` | `list<symbol>` | no |  |

A `theme` block: a named palette plus the `dark` / `light` `palette` sub-blocks that map colours onto every built-in element.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes |  |
| `font_head` | `utf8` | no |  |
| `font_body` | `utf8` | no |  |
| `font_mono` | `utf8` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `palettes` | `palette` | yes |  |

A `palette` sub-block of a `theme`: the colour roles (bg, fg, the named hues, …) for one colour scheme.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `mode` | `identifier` | yes |  |
| `bg` | `utf8` | no |  |
| `book_bg` | `utf8` | no |  |
| `bg_alt` | `utf8` | no |  |
| `bg_inset` | `utf8` | no |  |
| `overlay` | `utf8` | no |  |
| `border` | `utf8` | no |  |
| `border_strong` | `utf8` | no |  |
| `fg` | `utf8` | no |  |
| `fg_muted` | `utf8` | no |  |
| `fg_subtle` | `utf8` | no |  |
| `heading` | `utf8` | no |  |
| `selection` | `utf8` | no |  |
| `accent` | `utf8` | no |  |
| `accent_2` | `utf8` | no |  |
| `link` | `utf8` | no |  |
| `on_accent` | `utf8` | no |  |
| `syn_kw` | `utf8` | no |  |
| `syn_str` | `utf8` | no |  |
| `syn_num` | `utf8` | no |  |
| `syn_fn` | `utf8` | no |  |
| `syn_type` | `utf8` | no |  |
| `syn_comment` | `utf8` | no |  |
| `syn_punct` | `utf8` | no |  |
| `red` | `utf8` | no |  |
| `orange` | `utf8` | no |  |
| `yellow` | `utf8` | no |  |
| `green` | `utf8` | no |  |
| `cyan` | `utf8` | no |  |
| `blue` | `utf8` | no |  |
| `purple` | `utf8` | no |  |
| `pink` | `utf8` | no |  |

An `inline_pattern` block: a custom inline text pattern recognised in prose, mapping a delimiter to a class or rendering.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes |  |
| `pattern` | `utf8` | yes |  |
| `boundary` | `bool` | no |  |

## Related

- [Sites](../references/concept_sites.md)

- [Formatting](../references/concept_formatting.md)

[← Back to SKILL.md](../SKILL.md)
