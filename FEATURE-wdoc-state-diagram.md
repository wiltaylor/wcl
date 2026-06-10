# Feature request: state-diagram blocks for wdoc

Requested 2026-06-10 from the `wad` example project (`~/dev/wad`), which is growing a
"Lifecycles" chapter — one state diagram per domain entity lifecycle (e.g. order:
pending → paid → shipped / cancelled). Companion request to
FEATURE-wdoc-sequence-diagram.md; of the two, sequence diagrams are the higher
priority for wad.

## What exists today

- `diagram-core.wcl` fundamentals plus `:layered` / `:force` layouts that *almost* fit:
  a state graph is exactly the shape `:layered` ranks well (DAG-ish with back-edges).
- `flowchart.wcl` shapes (`process` / `decision` / `terminator` / `node`) are close
  cousins but carry flowchart semantics; a statechart needs different node chrome
  (rounded state boxes, the filled initial dot, the double-ring final marker) and
  *labelled* edges (trigger / guard), which `a -> b` connections don't carry today —
  edge labels are the real gap, more than the shapes.

Hand-rolling from fundamentals means manual x/y for every state (no `:layered` reuse,
because labelled edges need midpoint label placement the layout doesn't expose),
polygon arrowheads, and curved self-transition loops — the same geometry-in-template
cost called out in PERF-wdoc-build-scaling.md.

## Proposed authoring surface

A `statechart.wcl` stdlib module in the spirit of `flowchart.wcl`:

```wcl
state_diagram {
  width = 640
  direction = :left_to_right        // reuse :layered's direction knob

  state "pending"   { name = "Pending"   initial = true }
  state "paid"      { name = "Paid" }
  state "shipped"   { name = "Shipped"   final = true }
  state "cancelled" { name = "Cancelled" final = true }

  go "t1" { from = "pending" to = "paid"      trigger = "payment captured" }
  go "t2" { from = "paid"    to = "shipped"   trigger = "dispatched"  guard = "stock reserved" }
  go "t3" { from = "pending" to = "cancelled" trigger = "customer cancels" }
  go "t4" { from = "paid"    to = "paid"      trigger = "partial refund" }   // self-loop
}
```

- `state`: rounded box; `initial = true` draws the filled-dot entry pseudo-state with an
  arrow into it; `final = true` draws the double-ring marker (or double-border box).
  `link` for in-site page links as on flowchart shapes.
- Transition block (named `go` above only to dodge the `transition` ↔ CSS-ish ambiguity —
  any name works; it's a block, not an `->` connection, because it must carry payload:
  `trigger`, optional `guard`, rendered as the standard `trigger [guard]` edge label).
  Self-loops (`from == to`) render as the usual arc.
- Layout: auto by default — `:layered` ranking from initial state(s) is the right
  default; back-edges and self-loops route around. Optional per-state x/y override for
  the rare manual case.
- Nice-to-have, not blocking: composite/nested states, entry/exit action lines inside
  the state box.

## Why stdlib rather than per-project templates

Same argument as the sequence-diagram request: wad generates these from data
(`state_machine` blocks with `sm_state` / `sm_transition` children that map 1:1 onto
the sketch above), so the figure must be authorable coordinate-free under a repeater.
Labelled edges + auto-layout are renderer-adjacent capabilities a project template
can't fake cleanly — the edge-label gap in particular would also benefit `:layered`
flowcharts generally, and may be worth solving at the `diagram` level (label on the
connection/edge) with statechart blocks consuming it.

## Naming note

`wad` deliberately renamed its own kinds to `sm_state` / `sm_transition` (precedent:
`decision` → `adr` after colliding with flowchart's `decision`) specifically to leave
the bare names `state` / `transition` free for this stdlib module — no collision from
our side whatever names upstream picks.
