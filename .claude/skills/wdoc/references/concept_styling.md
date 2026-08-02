# Styling

_Structured CSS blocks and themes — how a site looks._

Two layers control how a site looks: `class` blocks style individual elements, and a site
`theme` sets the palette every built-in element draws from. Classes always win over theme
defaults via the cascade, so you theme broadly and override locally.


## Classes

A `class <name> { … }` block declares a CSS class. Apply it by listing the name in any block's
`class` field (or a span's). Declarations stay CSS text in `css`; SVG paint and callout accents
retain short fields. Per-mode overrides go in `dark { }` / `light { }` sub-blocks.


```wcl
class accent {
  css = "color:var(--wdoc-accent);font-weight:bold;"
  dark  { css = "color:#88c0d0;" }
  light { css = "color:#5e81ac;" }
}

// Use the class on a block:
p "highlighted paragraph" { class = ["accent"] }
```

## Field groups

| Group | Fields |
| --- | --- |
| Declarations | `css` — raw CSS declarations for this rule |
| SVG shorthand | `fill`, `stroke`, `stroke_width`, `opacity` |
| Callout | `accent` — sets a callout's accent colour (heading, border, icon) when the class is on a `callout` |
| Nesting | `nest "selector" { css = … }`; `&` explicitly references the parent class |
| Modes | `dark { … }`, `light { … }` for prefers-color-scheme overrides |

Hyphenated class names may be written bare — `class wdoc-series-1 { fill = "#88c0d0" }` — or
quoted; both are equivalent. This is how you override built-in classes like the chart palette
or callout styles. Set `sites = [:foo]` on a class to scope it to one site in a multi-site
document; omit the field and the class applies everywhere.


## Selectors and at-rules

Use `nest` for descendant or compound selectors rooted at a class, `base` for element,
reset, and selector-list rules, `font_face` for typed font descriptors, `media` for grouped
responsive rules, and `keyframes` for animations. A named `style` groups structured rules
that a template emits in place through `css_style(:name)`.


## Themes

A theme is a complete colour palette plus the rules that map it onto every built-in element —
page background, links, headings, code, charts, callouts, tables, inline emphasis. Set
`theme = :<name>` on a `site` — a symbol naming a `theme` block. Seven built-in palettes ship
(`forge` is the default, plus `nord`, `tokyonight`, `gruvbox`, `catppuccin`, `rose`, and
`paper`), each with co-ordinated dark and light variants and its own typography.
`theme_toggle = true` adds a light/dark toggle button.


```wcl
site mysite {
  default_template = :book
  theme            = :tokyonight
  accent           = :cyan
  theme_toggle     = true
}
```

Independently of the theme, `accent = :cyan` (or
`:red`/`:orange`/`:yellow`/`:green`/`:blue`/`:purple`/`:pink`) picks the hue used for links
and current-chapter highlights. Default is `:blue`. A custom theme is just a `theme` block
holding a `dark` and a `light` `palette` sub-block.


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
| `css` | `utf8` | no |  |
| `accent` | `utf8` | no |  |
| `fill` | `utf8` | no |  |
| `stroke` | `utf8` | no |  |
| `stroke_width` | `utf8` | no |  |
| `opacity` | `utf8` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `nests` | `nest` | yes |  |
| `dark` | `dark` | no |  |
| `light` | `light` | no |  |

The `dark` sub-block of a `class`: field overrides applied under a prefers-dark colour scheme.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `css` | `utf8` | no |  |
| `accent` | `utf8` | no |  |
| `fill` | `utf8` | no |  |
| `stroke` | `utf8` | no |  |
| `stroke_width` | `utf8` | no |  |
| `opacity` | `utf8` | no |  |

The `light` sub-block of a `class`: field overrides applied under a prefers-light colour scheme.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `css` | `utf8` | no |  |
| `accent` | `utf8` | no |  |
| `fill` | `utf8` | no |  |
| `stroke` | `utf8` | no |  |
| `stroke_width` | `utf8` | no |  |
| `opacity` | `utf8` | no |  |

A `base` block: an element, reset, selector-list, or other rule whose root is not one bare class.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `selector` | `utf8` | yes |  |
| `css` | `utf8` | yes |  |
| `sites` | `list<symbol>` | no |  |

A `font_face` block: typed family, source, weight, style, and display descriptors.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `family` | `utf8` | yes |  |
| `src` | `utf8` | yes |  |
| `weight` | `utf8` | no |  |
| `style` | `utf8` | no |  |
| `display` | `utf8` | no |  |
| `sites` | `list<symbol>` | no |  |

A `media` block containing structured class and base rules.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `query` | `utf8` | yes |  |
| `sites` | `list<symbol>` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `classes` | `class` | yes |  |
| `bases` | `base` | yes |  |

A `keyframes` block containing `base` frame selectors.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `utf8` | yes |  |
| `sites` | `list<symbol>` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `frames` | `base` | yes |  |

A named `style` bundle containing structured rules that a template can render in place with `css_style(:name)`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `classes` | `class` | yes |  |
| `bases` | `base` | yes |  |
| `font_faces` | `font_face` | yes |  |
| `media` | `media` | yes |  |
| `keyframes` | `keyframes` | yes |  |

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

- [Sites](../references/concept_sites.md) — Sites supports Styling: The `site` block: one output target — template, title, theme, multi-site routing, and full-text search.

[← Back to SKILL.md](../SKILL.md)
