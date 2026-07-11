# Forge Tokens — Colour Scheme, Type, Spacing, Motion

Everything here is defined as a CSS custom property in `${CLAUDE_SKILL_DIR}/assets/colors_and_type.css`.
Always reference the variable (`var(--accent)`), never the raw value — the raw values below exist so you
can reason about contrast and pick the right token, and so you can extend the palette consistently.

## Theme model

- **Dark is the default** (`:root`). Light activates via `@media (prefers-color-scheme: light)`.
- Manual override: `data-theme="light"` or `data-theme="dark"` on **any** element (usually `<html>`)
  beats the media query. Never ship light-only.
- The overall character: near-neutral with a slight cool cast (~5° blue hue shift in the grays).
  Pure `#000`/`#FFF` are never used.

## Neutrals

Backgrounds rise from 0 (page) to 4 (popover). Elevation = moving up this ramp, **not** shadows.

| Token | Role | Dark | Light |
|---|---|---|---|
| `--bg-0` | page background | `#0B0D10` | `#FAFAFA` |
| `--bg-1` | card / resting surface | `#11141A` | `#FFFFFF` |
| `--bg-2` | hover / nested card | `#171B22` | `#F4F5F7` |
| `--bg-3` | pressed / active row | `#1E232C` | `#EAECEF` |
| `--bg-4` | popover, dropdown, menu | `#252B36` | `#FFFFFF` |

Foregrounds descend in contrast:

| Token | Role | Dark | Light |
|---|---|---|---|
| `--fg-0` | primary text, values | `#ECEEF2` | `#0C0F14` |
| `--fg-1` | secondary text, labels | `#B7BDC8` | `#3D4654` |
| `--fg-2` | tertiary, captions, eyebrows | `#7C8593` | `#6B7383` |
| `--fg-3` | disabled, placeholder | `#4E5664` | `#A0A6B2` |

Borders carry the structure (shadows are `none` in this system):

| Token | Role | Dark | Light |
|---|---|---|---|
| `--border-subtle` | in-card dividers, sidebar edges | `#1A1F27` | `#EEF0F3` |
| `--border` | card/input outlines | `#262C36` | `#DCDFE4` |
| `--border-strong` | hover/focus border step | `#3A4250` | `#B6BBC4` |

## Accent

One accent: a desaturated blue. Used for primary actions, focus rings, selection, active nav.
**Never** as a large background fill.

| Token | Role | Dark | Light |
|---|---|---|---|
| `--accent` | solid fills (primary button), focus outline | `oklch(0.62 0.16 250)` | `oklch(0.52 0.18 250)` |
| `--accent-hover` | hover on solid accent | `oklch(0.66 0.17 250)` | `oklch(0.46 0.19 250)` |
| `--accent-press` | active/pressed | `oklch(0.56 0.16 250)` | `oklch(0.40 0.19 250)` |
| `--accent-bg` | subtle tint (selection, focus glow, badges) | `oklch(0.62 0.16 250 / 0.14)` | `oklch(0.55 0.17 250 / 0.14)` |
| `--accent-fg` | accent-coloured text/links, text on `--accent-bg` | `oklch(0.82 0.13 250)` | `oklch(0.38 0.19 250)` |
| `--accent-contrast` | text on solid `--accent` | `#FFFFFF` | `#FFFFFF` |

## Semantic colours

Four tones, each in three forms — the **triple pattern** every status treatment uses:

- `--<tone>` — the saturated solid (dots, solid danger buttons, progress bars)
- `--<tone>-bg` — a ~14% alpha tint for backgrounds (badges, toasts, row highlights)
- `--<tone>-fg` — text that holds contrast **on top of** the tint (and for coloured text on normal surfaces)

| Tone | Meaning | Dark solid | Dark fg | Light solid | Light fg |
|---|---|---|---|---|---|
| `--success` | healthy, passed, deployed | `oklch(0.68 0.14 150)` | `oklch(0.82 0.16 150)` | `oklch(0.50 0.15 150)` | `oklch(0.36 0.14 150)` |
| `--warning` | degraded, pending, caution | `oklch(0.78 0.14 75)` | `oklch(0.86 0.13 80)` | `oklch(0.62 0.16 70)` | `oklch(0.40 0.14 60)` |
| `--danger` | failed, error, destructive | `oklch(0.65 0.20 25)` | `oklch(0.78 0.16 25)` | `oklch(0.52 0.22 25)` | `oklch(0.42 0.20 25)` |
| `--info` | neutral notice | `oklch(0.68 0.13 230)` | `oklch(0.82 0.12 230)` | `oklch(0.50 0.14 230)` | `oklch(0.36 0.13 230)` |

The `-bg` variants are the solid at `/ 0.14` alpha in dark (`0.14`–`0.20` in light). If you need a
tinted border (like toasts do), use `color-mix(in oklab, var(--<tone>) 30%, transparent)`.

## Typography

- `--font-sans`: **IBM Plex Sans** (Google Fonts, imported by the token CSS). All UI text.
- `--font-mono`: **JetBrains Mono**. Code, IDs, log lines, metric deltas, counts.
- No display face. Headings are the same sans, larger and tighter.
- Scale (1.2 ratio, anchored at 14px): `--fs-xs` 11 · `--fs-sm` 12 · `--fs-base` 14 · `--fs-md` 16 ·
  `--fs-lg` 18 · `--fs-xl` 22 · `--fs-2xl` 28 · `--fs-3xl` 34.
- Line heights: `--lh-tight` 1.2 (headings) · `--lh-snug` 1.4 · `--lh-normal` 1.5 (body) · `--lh-relaxed` 1.65 (prose).
- Weights: 400 / 500 / 600 / 700 (`--fw-regular/medium/semibold/bold`). UI labels are 500, headings 600.
- Tracking: `--tracking-tight` -0.01em (headings) · `--tracking-eyebrow` 0.08em (uppercase micro-labels).
- **Sentence case everywhere** — titles, buttons, menu items. ALL CAPS only for eyebrows/table headers at `--fg-2`.
- **Tabular numerals** (`font-variant-numeric: tabular-nums`) in every table and metric.

## Spacing, radii, sizes

- Spacing: 4px base. `--sp-1..16` = 4, 8, 12, 16, 20, 24, 32, 40, 48, 64.
- Radii: `--r-sm` 4px (buttons, inputs, badges) · `--r-md` 6px (cards) · `--r-lg` 8px (modals, large panels) ·
  `--r-pill` only for status dots, avatars, **toggle-switch tracks, slider thumbs and graph
  port dots** — **never on buttons**.
- Gradients stay banned decoratively; the two sanctioned **functional fills** are the
  skeleton shimmer sweep and the slider's hard-stop track fill.
- Heights: `--h-sm` 28 · `--h-md` 32 (default control + table row) · `--h-lg` 36 · `--h-xl` 40.
- Shell: `--topbar-h` 48px · `--sidebar-w` 240px — the app-shell grid and the mobile drawer share these.
- Density is the point: 32px table rows, 14px body, 48px topbar, 240px sidebar.

## Breakpoints & responsive rules

CSS variables can't be used inside `@media` conditions, so the breakpoints are **documented
constants** — use the literal values (they're also listed in the token CSS header comment):

- **compact** — `@media (max-width: 1024px)`: the sidebar leaves the shell grid and becomes an
  off-canvas drawer (`.is-sidebar-open` on `.app-shell`, hamburger `.fsidebar-toggle`, backdrop
  `.fscrim`); search shrinks; settings stack.
- **mobile** — `@media (max-width: 768px)`: single-column stacking — `.page-head` wraps,
  breadcrumbs hide (the page `<h1>` carries location), tables scroll horizontally inside
  `.ftable-wrap`, `.settings-row` goes one column.
- **touch** — `@media (pointer: coarse)`: the `--h-*` control heights bump to 32/40/44/48 and
  tap targets get minimums. This is the only thing that relaxes density.

**Density is the desktop truth** — only a coarse pointer relaxes control heights; viewport
width never does. Size interactive components with `var(--h-*)` so touch sizing applies
automatically. Grids of tiles use `.fgrid` (auto-fit, `minmax(180px, 1fr)`) — intrinsically
responsive, no media query needed.

## Motion

- `--ease-out: cubic-bezier(0.2, 0, 0, 1)` — the only easing for enter/exit/state changes.
  No spring, no bounce.
- **Continuous infinite loops use `linear`** (marching-ants edges, spinner, indeterminate
  progress) — easing a seamless loop reads as pulsing.
- `--dur-1` 80ms (hover/press) · `--dur-2` 160ms (panels) · `--dur-3` 240ms (modals, routes).
- Reduced motion is honored globally by the token CSS. For **infinite** animations whose
  frozen frame would be misleading (a flash stuck at low opacity), put the animated
  treatment inside `@media (prefers-reduced-motion: no-preference)` with a static, legible
  fallback outside it — see the `.fgraph-edge.is-active/.is-broken` pattern in console.css.

## Layers (z-index ladder)

| z | Layer | Classes |
|---|---|---|
| 10 | topbar | `.ftopbar` |
| 20 | drawer scrim | `.fscrim` |
| 30 | sidebar drawer | `.fsidebar` (≤1024px) |
| 40 | sheet | `.fsheet` — above the drawer, below modals |
| 50 | modal, command palette | `.fmodal`, `.fcmd` (don't open both) |
| 60 | anchored popovers | `.fpop`, `.fselect-pop` — beat modals so they work inside them |
| 70 | toaster, editor context menu | `.ftoaster`, `.fcode-menu` |
| 80 | tooltip | `.ftip::after` |

## Chart colours

Categorical series use a **fixed, validated order** (dataviz six-checks validator; min
adjacent CVD ΔE **17.8 dark / 16.9 light**, target ≥ 12; light mode passes all checks,
dark mode's warning/success/info sit slightly above the lightness band — a conscious
brand-tokens-only tradeoff mitigated by always-on legends, direct labels and 2px surface
gaps). Never reorder, never cycle:

| Slot | Token | Slot | Token |
|---|---|---|---|
| 1 | `--accent` | 4 | `--warning` |
| 2 | `--danger` | 5 | `--info` |
| 3 | `--success` | Other (fold, not a slot) | `--fg-2` |

When the data is **semantic** (statuses, health), use `tone:` props — the status meaning
wins and the categorical ramp is not used. Text in charts always wears text tokens
(`--fg-1`/`--fg-2`), never the series colour.

## Syntax highlighting (code.jsx)

| Lezer tag group | Token | Lezer tag group | Token |
|---|---|---|---|
| keywords | `--accent-fg` | types, classes, regex | `--warning-fg` |
| strings | `--success-fg` | properties, attributes | `--info-fg` |
| numbers, bools | `--info-fg` | punctuation, operators | `--fg-2` |
| comments | `--fg-3` | invalid | `--danger-fg` |
| variables | `--fg-0` | HTML tags, links | `--accent-fg` |

`-fg` variants so contrast holds in both themes; no italics — Forge has no italic voice.

## Graph port colours

Node-graph connection ports (and their edges) colour by data type — the one sanctioned
data-driven colour use (applied as inline `var(--…)`, still token-pure):

| Type | Token | Type | Token |
|---|---|---|---|
| `trigger` | `--fg-0` | `object` | `--accent` |
| `string` | `--success` | `array` | `--warning` |
| `number` | `--info` | `any` | `--fg-3` |
| `boolean` | `--danger` | | |

## Interaction states (apply to every new *interactive* component)

Display-only components (meters, charts, read-only indicators) take **no** hover/press/focus
treatment — for them, only both-theme legibility and reduced-motion safety apply.

- **Hover**: background moves one step up the ramp (`--bg-1` → `--bg-2`) *or* border goes
  `--border` → `--border-strong`. Text colour may rise `--fg-1` → `--fg-0`. Hue never changes.
- **Press**: one more bg step (`--bg-3`), or `translateY(0.5px)` on buttons. No scale-down.
- **Focus**: `outline: 2px solid var(--accent); outline-offset: 2px` (the token CSS applies this via
  `:focus-visible`). Inputs use `border-color: var(--accent)` + `box-shadow: 0 0 0 3px var(--accent-bg)`.
- **Active/selected nav**: `--bg-2` fill + `box-shadow: inset 2px 0 0 var(--accent)` left rail.
- **Disabled**: `opacity: 0.4; pointer-events/cursor off`. No special background.
- **Loading**: the thin `Progress` bar (or a 1px top bar / inline shimmer) is the default;
  the `Spinner` component is sanctioned only for inline and button-adjacent waits.

## New-component checklist

Every new component must pass all of these before it ships:

- [ ] Colours only via `var(--token)` — no hardcoded hex/oklch anywhere in component code
- [ ] Surface = `--bg-1` + `1px solid var(--border)`; floating surface = `--bg-4` + `--border-strong`
- [ ] No `box-shadow` for elevation (the `.fmodal` backdrop — `rgb(0 0 0 / 0.5)` + `blur(4px)` — is the one exception)
- [ ] No gradients, no frosted glass on cards/buttons, no emoji, no unicode-as-icon (`→`, `✓`)
- [ ] Status shown with the tone triple: `-bg` tint + `-fg` text, or a solid `--<tone>` dot
- [ ] Hover/press/focus/disabled states follow the ladder above (interactive components only)
- [ ] Radius 4px for controls, 6px for cards, 8px for modals
- [ ] Numbers get units and `tabular-nums`; transitions use `--dur-*` + `--ease-out`
- [ ] Renders correctly in **both** themes — check by toggling `data-theme` on `<html>`
- [ ] Usable at 375px wide — no page-level horizontal scroll (wide tables scroll inside `.ftable-wrap`)
- [ ] Interactive targets sized with `var(--h-*)` so `pointer: coarse` touch sizing applies
