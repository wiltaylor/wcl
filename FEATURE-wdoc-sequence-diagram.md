# Feature request: sequence-diagram blocks for wdoc

Requested 2026-06-10 from the `wad` example project (`~/dev/wad`), which is growing a
"Scenarios" chapter — one sequence diagram per runtime interaction (e.g. "customer checks
out": web → api → stripe → db, with async publishes to a topic). Design docs lean on
sequence diagrams more than any other figure type, and wdoc currently has no primitive
for them.

## What exists today

- `diagram-core.wcl`: `rect` / `circle` / `line` / `label` / `polygon` fundamentals,
  `container`, layouts `:free` / `:grid` / `:layered` / `:force` / `:radial`.
- `flowchart.wcl`: the custom-shape pattern — `@block` types extending `SvgBlock` whose
  `lower` fn emits `SvgFundamental`s, connectable via `a -> b` with `connect_points`.

None of the existing layouts fit a sequence diagram: it is a fixed two-axis grid
(participants on x in declaration order, time on y in message order) with per-edge
labels, dashed reply arrows, and lifelines that must extend to cover the last message.
Hand-rolling it from fundamentals means computing every coordinate in template code:
lifeline x = `i * column_width`, message y = `header + j * row_height`, plus arrowheads,
self-message loops and label fitting — all O(participants × messages) `let` math per
diagram, in templates that are already the slow path (see PERF-wdoc-build-scaling.md).

## Proposed authoring surface

A `sequence.wcl` stdlib module in the spirit of `flowchart.wcl`:

```wcl
sequence_diagram {
  width = 760

  participant "customer" { name = "Customer"        kind = :actor }
  participant "web"      { name = "Web App"         link = "cont_web" }
  participant "api"      { name = "API Application" link = "cont_api" }
  participant "stripe"   { name = "Stripe"          kind = :external }

  message "m1" { from = "customer" to = "web"    text = "Submit payment form" }
  message "m2" { from = "web"      to = "api"    text = "POST /orders" }
  message "m3" { from = "api"      to = "stripe" text = "Capture charge" }
  message "m4" { from = "stripe"   to = "api"    text = "charge id"  kind = :reply }
  message "m5" { from = "api"      to = "api"    text = "persist order" }      // self-message
  message "m6" { from = "api"      to = "web"    text = "201 Created" kind = :reply }
  note    "n1" { at = "m3"  text = "Retries reuse the same idempotency key." }
}
```

- `participant`: ordered left-to-right by declaration; `kind` picks the head shape
  (`:box` default, `:actor` stick figure, `:external` dashed box); `link` reuses the
  existing in-site page linking from flowchart shapes.
- `message`: ordered top-to-bottom by declaration; `kind = :sync` (solid, filled
  arrowhead, default) / `:async` (solid, open arrowhead) / `:reply` (dashed). Same-id
  `from`/`to` renders the standard self-message loop.
- `note`: margin annotation anchored to a message.
- Nice-to-have, not blocking: `activation` bars (`from_msg` / `to_msg`) and `fragment`
  boxes (`alt` / `loop` / `opt` spanning a message range).

Layout is fully determined by declaration order — no solver needed, so this can
plausibly be pure stdlib: a `lower` on `sequence_diagram` that walks its children and
emits fundamentals, the way flowchart shapes lower themselves. If child-walking inside
`lower` isn't expressible today, a renderer-side `layout = :sequence` on `diagram` that
ranks participants on x and `@connections`-style edges on y would do as well.

## Why stdlib rather than per-project templates

Coordinate-free authoring is the whole value: the wad book generates these diagrams from
data (`scenario` blocks with `flow_step` children — `from` / `to` / `action` / mode map
1:1 onto `message` fields above), so the template just repeats over steps. Every wdoc
user documenting a service interaction needs the same figure; today each would re-derive
the same brittle geometry math, multiplied across pages by the superlinear build cost.

## Naming note

`wad` deliberately avoided claiming generic block kinds (`event`, `state`, `transition`,
`message`-adjacent names) precisely so the stdlib stays free to take the natural names
here — `participant` / `message` / `note` have no collision from our side. (`note`:
check against existing stdlib kinds; `seq_note` is a fine fallback.)
