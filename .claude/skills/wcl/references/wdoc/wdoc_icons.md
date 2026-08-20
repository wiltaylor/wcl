# Icons

Two full SVG icon packs are compiled into the `wcl` binary. Nothing is downloaded, nothing is
read from disk at build time, and only the icons a document actually uses reach the output.

| Pack | `pack` value | Icons | Style | Licence |
| --- | --- | --- | --- | --- |
| Lucide | `lucide` | ~1700 | Stroke-based (`stroke="currentColor"`) | ISC |
| Bootstrap Icons | `bootstrap` | ~2000 | Fill-based (`fill="currentColor"`) | MIT |

Both licences are permissive and travel with the vendored files. Both packs honour
`currentColor`, so **`color` is the one foreground knob** and it recolours either.

## Three ways to draw an icon

### 1. Inline in prose — `:name:`

The common case. An inline pattern inside any patterned string (`p`, `li`, a `callout` body, a
table cell):

```wcl
p "Status: :lucide.check: ready — see the :lucide.compass: navigation guide."
```

Write `:set.name:` or a bare `:name:`. A bare name resolves against the first declared iconset
whose pack has it, root-document sets before the library defaults. Prefer the qualified
`:lucide.check:` when a bare word could also read as prose.

**An unresolved `:name:` renders as the literal text `:name:`.** That is deliberate: it keeps a
chance match in prose harmless. A misspelt icon name is therefore a silent no-op, never a
build error. If an icon does not appear, check the spelling first.

### 2. As a diagram shape — the `icon` block

```wcl
diagram {
  width  = 80
  height = 80
  icon "lucide.compass" { x = 10.0  y = 10.0  width = 60.0  height = 60.0 }
}
```

**`icon` extends `SvgBlock` only.** It is a diagram shape, not a page block: put it inside a
`diagram` or `container`, never directly under a `page`. Use the inline `:name:` pattern for
prose.

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | `identifier` | yes | The label slot; optionally `set.name`. |
| `set` | `identifier?` | no | Which declared iconset to read from. |
| `size`, `color`, `fill`, `background` | `utf8?` | no | Styling, as on an iconset. |
| `class`, `id` | | no | Classes and an explicit id. |
| `x`, `y` | `f64` | no | Placement, default `0.0`. |
| `width`, `height` | `f64` | no | Size in diagram units, default `24.0`. |
| `scale` | `f64?` | no | Extra multiplier on the size. |
| `anchor_*`, `connect_points` | | no | Anchors and edge-attach sides, like any shape. |

`icon` is `@native`: the glyph is resolved in Rust against a bundled pack and emitted as a
`<use>` of the shared sprite, which WCL cannot express.

### 3. As a badge on a box-like shape

Four badge fields ride on any box-like shape: `rect`, `circle`, `container`, `process`,
`decision`, `terminator`, and any custom `extends SvgBlock` type that declares them.

```wcl
process "Validate" {
  id = v  x = 20.0  y = 15.0  width = 120.0  height = 50.0
  icon     = "lucide.shield-check"
  icon_pos = :top_right
}
```

`icon` (a `pack.name` string), `icon_size` (`f64`), `icon_class` (`list<utf8>`), and
`icon_pos`, one of `:center`, `:top_left` (the default), `:top_right`, `:bottom_left`,
`:bottom_right`, `:left`, `:right`.

## `iconset` and `icon_def`

The stdlib already declares `iconset lucide {}` and `iconset bootstrap {}`, so both packs work
with **no declaration at all**. Declare your own only to rename a pack or to set defaults:

```wcl
iconset ui {
  pack  = "lucide"
  size  = "1.15em"
  color = "var(--wdoc-accent)"

  icon_def "heart" { color = "#bf616a" }
}

p "We :ui.heart: WCL."
```

`iconset` fields: `name`, `pack`, `size`, `color`, `fill`, `background`, `class`, and
`@children("icon_def") icons`. The label `name` is the reference name that `set = ui` and
`:ui.name:` use. `pack` defaults to the set's own name.

`icon_def` takes the icon name as its label and the same five styling fields, overriding the
set default for that one glyph.

Styles merge in one order, each layer winning over the last: **set defaults → the `icon_def`
override → the field on the individual `icon` block or inline span.**

An `iconset` is a document-root declaration, not a page block. A user set of the same name as
a library one wins.

## Finding an icon name

**An icon name is the SVG file's stem in the upstream pack.** `house.svg` is `house`.
Both packs use kebab-case throughout. Lucide spells concepts out (`triangle-alert`,
`circle-check`); Bootstrap abbreviates (`exclamation-triangle`, `check2`).

No `wcl` subcommand lists a pack. This working set of Lucide names covers most technical
documentation, and every one of them resolves:

| Purpose | Names |
| --- | --- |
| Status | `info` `lightbulb` `triangle-alert` `circle-alert` `circle-x` `circle-check` `circle-question-mark` |
| Actions | `check` `x` `plus` `minus` `search` `settings` `pencil` `copy` `trash-2` `download` `upload` `refresh-cw` |
| Files | `file` `file-text` `folder` `folder-open` `code` `image` `video` `package` |
| Systems | `terminal` `database` `server` `cloud` `zap` `shield` `lock` `key` `rocket` |
| Version control | `git-branch` `git-fork` `git-merge` `git-pull-request` |
| People | `user` `users` `mail` `bell` |
| Navigation | `arrow-right` `arrow-left` `arrow-up` `arrow-down` `chevron-right` `external-link` `link` |
| Documents | `book` `book-open` `bookmark` `list` `layout-grid` `star` `heart` `clock` `calendar` |

For a name outside that set, browse the upstream index — Lucide at `lucide.dev/icons`,
Bootstrap Icons at `icons.getbootstrap.com` — then render the page to confirm it, because the
failure is silent.

## Output and styling

Every reference — inline pattern, `icon` block, shape badge, callout default — resolves
through one registry, the renderer's `patterns.icons()`. That registry reads the document's
`iconset` blocks once, then records each icon it hands out.

The build writes **one `_wdoc/icons.svg` sprite** from that record, holding a `<symbol>` per
icon actually used, and each occurrence emits a tiny
`<use href="_wdoc/icons.svg#…">`. `currentColor` propagates through `<use>`, so the theme and
the `class` system recolour icons exactly like text. Because the sprite is a relative URL, an
icon resolves when the site is served or hosted, not in a page opened directly from disk.

Inline icons default to `1em` — the surrounding font size — with `vertical-align: -0.125em`.
Diagram icons are sized by their `width` / `height` instead, and the default inline rule
leaves them alone.

The PDF backend cannot reference a sprite, so it embeds each icon as a standalone `<svg>`
instead. The same names work; nothing changes in what you author.

## Related

- The inline pattern vocabulary `:name:` belongs to:
  [`wdoc_formatting.md`](wdoc_formatting.md).
- Callout default icons and the `icon` override: [`wdoc_callouts.md`](wdoc_callouts.md).
- Shapes, anchors and edges: [`wdoc_diagrams.md`](wdoc_diagrams.md).
- `color`, `accent` and the theme variables: [`wdoc_styling.md`](wdoc_styling.md).
