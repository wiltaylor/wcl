# Themes and styling

Two layers decide how a site looks.

- A **theme** is a colour palette. A site names one, and everything the renderer emits
  recolours. That covers the page, the text, the links, the headings and the code highlighting.
  It also covers the chart series, the callouts, the tables and the template chrome.
- A **class** (and its neighbours `base`, `nest`, `media`, `font_face`, `keyframes`, `style`) is
  a structured CSS rule. A class always wins over a theme default through the cascade.

Theme broadly. Override locally.

## Themes

Set `theme` on the `site`. The symbol names a `theme` block:

```wcl
site docs {
  default_template = :book
  theme            = :tokyonight
  accent           = :cyan
  theme_toggle     = true
}
```

A site with no `theme` gets `:forge`. An unknown name also falls back to `:forge`. A document
with no `site` block renders unthemed.

### The built-in themes

Seven ship. Each one carries a co-ordinated `dark` and `light` palette.

| Theme | Look |
| --- | --- |
| `forge` | The Forge control palette. High contrast, compact. **The default.** |
| `nord` | Cool blue-grey. |
| `tokyonight` | A vivid night palette. |
| `gruvbox` | Warm retro. |
| `catppuccin` | Soft pastel. Mocha for dark, Latte for light. |
| `rose` | Muted rosé. Main for dark, Dawn for light. |
| `paper` | A warm print look, with serif headings. |

### Accent

`accent` picks the hue that drives links, active navigation and highlights. It is independent
of the theme:

```wcl
site docs { theme = :nord  accent = :green }
```

The eight hues: `:red :orange :yellow :green :cyan :blue :purple :pink`.

Leave `accent` unset and the theme's own `accent` role drives it, so a theme looks designed out
of the box.

### The `theme_toggle` flag

`theme_toggle = true` adds a light/dark button. The `book`, `webpage` and `website` templates
render it. Without it, a reader still gets the light palette through
`@media (prefers-color-scheme: light)`. The button overrides that preference and persists the
choice.

## Writing a theme

A `theme` block holds a `dark` palette, a `light` palette, three optional font stacks, an
optional `metrics` block carrying the type scale, and an optional `extends` naming a theme to
inherit from:

```wcl
theme sunset {
  font_head = "'IBM Plex Sans', system-ui, sans-serif"
  metrics { body_size = "16px"  measure = "46rem" }
  palette dark {
    bg = "#1a1a2e"  fg = "#e0e0e0"
    blue = "#5e81ac"  green = "#a3be8c"
  }
  palette light {
    bg = "#fdf6e3"  fg = "#073642"
  }
}

site docs { theme = :sunset  accent = :green }
```

The `palette` label is `dark` or `light`. **Every role is optional.** An omitted role emits no
variable — unless the theme `extends` another, which is almost always what you want for a
partial theme. See [Inheriting a theme](#inheriting-a-theme) below.

### Inheriting a theme

`extends = :<theme>` takes everything this theme does not restate from the named theme —
fonts, individual metrics, individual palette roles, whole palettes. Anything it does state
wins. So changing one number is a three-line theme, not a copy of 31 roles across two
palettes:

```wcl
theme narrow {
  extends = :forge
  metrics { measure = "46rem" }
}

site book { theme = :narrow }
```

That renders exactly as `:forge` — all 31 roles, both palettes, all three fonts — with a
46rem measure.

Resolution is **per role, not per palette.** A theme that extends `forge` and declares
`palette dark { bg = "#000" }` overrides `bg` and inherits the other 30 dark roles plus the
entire light palette. The same holds for a single `metrics` field.

Chains are followed to any depth, so a theme may extend a theme that extends a built-in.
Two rules keep a chain terminating:

- An `extends` naming **no** theme ends the chain. It does not fall back to `forge` — the
  theme's own roles are all it gets.
- A **cycle** ends the chain at the repeat. Each theme in the cycle still contributes what
  it states.

Unlike `site.theme`, an unknown `extends` is not silently redirected to the default. `site`
falls back to `forge`; `extends` does not.

### The 31 colour roles

| Group | Roles |
| --- | --- |
| Surfaces | `bg` (the page gutter), `book_bg` (the reading column or card), `bg_alt` (a raised surface — sidebar, code card, table header, inline code), `bg_inset` (a sunken surface), `overlay` (a subtle fill or divider) |
| Borders | `border`, `border_strong` |
| Text | `fg`, `fg_muted` (nav, captions), `fg_subtle` (comments, metadata), `heading`, `selection` |
| Accent | `accent`, `accent_2`, `link`, `on_accent` (text drawn on an accent fill) |
| Syntax | `syn_kw`, `syn_str`, `syn_num`, `syn_fn`, `syn_type`, `syn_comment`, `syn_punct` |
| The hue ring | `red`, `orange`, `yellow`, `green`, `cyan`, `blue`, `purple`, `pink` |

The hue ring does double duty. It supplies chart series 1–8 in that order, the six callout
accents, and the default diagram-shape strokes. The built-in themes derive it from their own
semantic colours. The callout mapping is one hue per kind, all six distinct: `blue` is note,
`cyan` is info, `purple` is tip, `green` is success, `yellow` is warn, `red` is error.

### Fonts

| Field | Drives | Default |
| --- | --- | --- |
| `font_head` | `--wdoc-font-head` — headings, nav, chrome | `'IBM Plex Sans', system-ui, sans-serif` |
| `font_body` | `--wdoc-font-body` — body copy | `'Source Serif 4', Georgia, serif` |
| `font_mono` | `--wdoc-font-mono` — code | `'JetBrains Mono', ui-monospace, monospace` |

wdoc ships all three faces in `_wdoc/`, so a site needs no web-font request.

### Typography

A `metrics` block is `palette`'s counterpart for everything about type that is a number
rather than a font name. One per theme, mode-independent, every field optional.

| Field | Drives | Default |
| --- | --- | --- |
| `body_size` | `--wdoc-body-size` — body copy size | `17px` |
| `line_height` | `--wdoc-line-height` — body leading, unitless | `1.7` |
| `measure` | `--wdoc-measure` — the book template's reading-column width | `60rem` |
| `h1` … `h6` | `--wdoc-h1` … `--wdoc-h6` — the six heading steps | `2.6rem` `1.85rem` `1.35rem` `1.12rem` `1rem` `0.85rem` |

```wcl
theme narrow {
  metrics {
    body_size   = "16px"
    line_height = "1.65"
    measure     = "46rem"
    h1 = "2.2rem"  h2 = "1.7rem"  h3 = "1.3rem"
    h4 = "1.1rem"  h5 = "1rem"    h6 = "0.85rem"
  }
  palette dark  { bg = "#101014"  fg = "#e6e6ea" }
  palette light { bg = "#fbfbfd"  fg = "#16161a" }
}
```

The scale is **six explicit sizes, not a base plus a ratio.** The shipped scale is not
modular (the steps run 1.41, 1.37, 1.21, 1.12, 1.18), so no single ratio reproduces it.

Every bundled rule reads its metric as `var(--wdoc-…, <default>)`, and the fallback is the
constant wdoc has always shipped. So an omitted field, or a theme with no `metrics` block at
all, renders exactly as before — nothing reflows. All seven built-in themes declare the
default metrics explicitly, which is why they render identically to each other.

The measure is the one worth a second look: `60rem` less the book template's `3.5rem` of
horizontal padding is about 53rem of text, roughly 130 characters per line at 17px.
Conventional long-form measure is 60–75. `metrics { measure = "46rem" }` is the fix, and it
is now one line rather than a `class "book-measure"` CSS override. Pair it with
`extends = :forge` unless the theme also declares its own palettes — `metrics` lives inside a
`theme`, and a theme with no palette and no `extends` renders colourless.

`metrics` reaches the web output only. The PDF backend has its own print type scale, and
reads a theme for colour but not for type.

### How a theme reaches the page

The renderer emits the palette as CSS custom properties named `--wdoc-<role>`, in four places:

```css
:root { … }                                        /* the dark palette, the default */
@media (prefers-color-scheme: light) { :root { … } } /* the light palette */
:root[data-theme="dark"]  { … }                    /* the toggle */
:root[data-theme="light"] { … }
:root { --wdoc-accent: var(--wdoc-green); }        /* the chosen accent hue */
```

Everything else — the structured rules, the `class` system, the templates — paints with
`var(--wdoc-*)`. A new template therefore needs no theme work. Reference the variables and it
themes for free.

### Scoped palettes

Two extra selectors re-declare the whole palette for a **subtree**:

```css
.wdoc-theme-dark  { --wdoc-bg:#1a1a2e; --wdoc-fg:#e0e0e0; … }
.wdoc-theme-light { --wdoc-bg:#fdf6e3; … }
```

Custom properties inherit, and a closer ancestor wins. Wrapping content in an element that
carries one of those classes paints it in that palette whatever the reader's global choice is.
This is how the `demo` block shows the same content under both palettes side by side.

## Classes and structured rules

A `class <name> { … }` block declares one CSS class. Apply it by listing the name in a block's
`class` field:

```wcl
class accent {
  css = "font-weight:700;"
  dark  { css = "color:#88c0d0;" }
  light { css = "color:#5e81ac;" }
}

p "A highlighted paragraph." { class = ["accent"] }
```

| Field group | Fields |
| --- | --- |
| Declarations | `css` — raw CSS declarations for this rule |
| SVG paint | `fill`, `stroke`, `stroke_width`, `opacity` — so a class can theme a chart series or a diagram shape without a declaration string |
| Callout | `accent` — sets `--callout-accent`, which colours a callout's heading, border and icon |
| Nesting | `nest "selector" { css = … }` |
| Modes | `dark { … }`, `light { … }` |
| Scope | `sites = [:docs]` |

A `dark` or `light` sub-block takes the same fields as the class, minus the name. The class
above emits as:

```css
.accent { font-weight:700;color:#88c0d0; }
@media (prefers-color-scheme: light) { .accent { color:#5e81ac; } }
:root[data-theme="dark"]  .accent { font-weight:700;color:#88c0d0; }
:root[data-theme="light"] .accent { font-weight:700;color:#5e81ac; }
```

Dark is the base rule, light arrives through the media query, and the two `data-theme`
selectors let the toggle override the system preference.

### The rule vocabulary

| Block | Declares |
| --- | --- |
| `class "name" { css = … }` | `.name { … }` |
| `nest "frag" { css = … }` inside a class | A fragment with no `&` is a **descendant** (`.card .title`). A fragment with `&` attaches to the class itself (`.card:hover`). |
| `base "selector" { css = … }` | Any rule whose root is not one bare class: an element, a reset, a selector list. |
| `font_face "Family" { src = … }` | An `@font-face`, with typed `weight` / `style` / `display` descriptors. |
| `media "(query)" { … }` | An `@media` wrapping `class` and `base` children. |
| `keyframes "name" { base "from" {…} base "to" {…} }` | An `@keyframes`, whose frames are `base` blocks. |
| `style "name" { … }` | A **named bundle** of structured rules. It emits nothing on its own; a template renders it in place with `css_style(:name)`. |

```wcl
class card {
  css = "display:grid;"
  nest ".title" { css = "font-weight:700;" }   // .card .title
  nest "&:hover" { css = "opacity:0.8;" }      // .card:hover
}

base "body,.wdoc-body" { css = "margin:0;" }

font_face "Inter" {
  src     = "url('inter.woff2') format('woff2')"
  weight  = "400"
  display = "swap"
}

media "(max-width: 40rem)" {
  class card { css = "display:block;" }
}

keyframes "pulse" {
  base "from" { css = "opacity:0;" }
  base "to"   { css = "opacity:1;" }
}
```

A `style` bundle keeps template-local CSS beside the template instead of making it global. The
built-in templates use it: `css_style(:book_css)`, `css_style(:webpage_css)`,
`css_style(:website_css)`, `css_style(:presentation_css)`.

### Hyphenated names

A hyphenated class name may be written bare or quoted. Both forms are equivalent:

```wcl
class wdoc-series-1 { fill = "#88c0d0" }
class "wdoc-series-1" { fill = "#88c0d0" }
```

A name with other non-identifier characters, such as a space, still needs the quotes.

Bare hyphenated names matter, because that is how you **override a built-in class**. wdoc's own
rules use ordinary class blocks. A redeclaration therefore wins through the cascade. Useful
names:

| Class | Paints |
| --- | --- |
| `wdoc-series-1` … `wdoc-series-8` | The chart series palette. |
| `callout` | The callout box. `nest "&.note"` and friends set each type's accent. |
| `code-card`, `code-filename`, `code-lang` | The code block chrome. |
| `wdoc-table` | The table. |
| `heading-1` … `heading-6` | The six heading levels. |
| `wdoc-node`, `wdoc-process`, `wdoc-decision`, `wdoc-terminator` | Diagram shapes. |
| `book-sidebar`, `book-chapter`, `book-section`, `site-nav`, `ws-header` | Template chrome. |

### Site scoping

`class`, `base`, `font_face`, `media` and `keyframes` each take an optional `sites` list:

```wcl
class hero { sites = [:marketing]  css = "font-size:3rem;" }
```

An omitted or empty list means **every** site. This is the opposite of `page.sites`, which a
multi-site document requires.

## What the build checks

### A line comment is an error

`//` is not CSS. A browser throws away the rest of the declaration, so one stray line comment
silently deletes the rules after it. A `css` value containing one is rejected at build time,
like a schema violation. Use `/* … */`. A `//` inside a quoted string or a `url(…)` is a URL
and passes.

```wcl
class card { css = "color:red; // muted" }        // rejected
class card { css = "color:red; /* muted */" }     // fine
```

### The class lint

After rendering, wdoc compares the class names its pages carry with the class names its rules
select, both directions, and **warns** (it never fails the build):

- A name in the markup that no rule selects — a misspelled class name, or a hook nothing styles.
- A rule this document authors that no page carries — a misspelled selector, or a leftover rule.

It reads the rendered output, not your source, because a class reaches markup three ways: a
`class` field, a raw HTML string in a template, and the renderer's own markup. The finished page
sees all three, and sees a computed name (`format("level-{}", h.level)`) already resolved.

Two exemptions, both structural:

- **The bundled rules.** wdoc's stylesheet ships the whole built-in vocabulary; an unused
  library rule is another document's rule, not dead code. Only rules the document itself
  authors are judged unused.
- **Generator vocabularies.** Syntax highlighting mints one class per grammar scope (`tok-…`)
  and one per language (`language-…`) — open-ended sets no stylesheet can declare.

For a class you emit on purpose and style nowhere (a hook for a script, or a name a reader may
restyle), say so: **an empty `class` block declares the name and emits no CSS.**

```wcl
class ws-main {}
```

The lint runs over **every site of the document at once**, so a rule scoped to one site does not
read as dead while another site renders. `wcl wdoc build --site one` and the dev server's
targeted page rebuild produce partial output and skip it. A class a wireframe or terminal
resolves counts as used, even though those renderers bake the colour into their SVG instead of
carrying the class into an element.

## Gotchas

- A `class` field is a **list**: `class = ["accent"]`, not `class = "accent"`.
- The `css` field is opaque text. wdoc does not parse it, so a typo inside a declaration fails
  silently in the browser rather than at build time (a `//` line comment is the one exception —
  see above). Keep the selector in WCL blocks. Put only the declarations in `css`, so a mistake
  can affect at most its own rule.
- A `theme` recolours what wdoc emits. It does not restyle raw HTML you inject with `raw(...)`.
  Paint that from `var(--wdoc-*)` yourself.
- A palette role you omit emits **no** variable at all, unless the theme `extends` one that
  states it. Without `extends` it falls through the CSS cascade to whatever declared it last.
- A theme that states **no palette at all** and extends nothing emits no colour variables, and
  most bundled rules read colour as a bare `var(--wdoc-…)` with no fallback — so the page
  renders as browser defaults. The build warns per mode and still exits 0. Add `extends`, or a
  `palette`.

## See also

- [`wdoc_sites.md`](wdoc_sites.md) — the `theme`, `accent`, `theme_toggle` and `assets` fields
  on `site`.
- [`wdoc_templates.md`](wdoc_templates.md) — `css_style(:name)` and the built-in layouts.
- [`wdoc_visibility.md`](wdoc_visibility.md) — hiding a block per backend or per site.
