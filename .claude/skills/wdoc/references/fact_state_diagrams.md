# state_diagram

A `state_diagram` draws an entity lifecycle coordinate-free: `state`s auto-rank by longest path from the transition graph (back-edges and self-loops route around), `initial = true` draws the filled entry dot, `final = true` the double border, and each `transition` carries a `trigger [guard]` edge label. It is a page-level block, not a `diagram` shape.

A page-level block drawing an entity lifecycle; `state`s auto-rank along `direction` and `transition`s carry `trigger [guard]` labels.

```wcl
state_diagram {
  width = 640
  direction = :left_to_right

  state "pending" {
    name = "Pending"
    initial = true
  }
  state "paid" {
    name = "Paid"
  }
  state "shipped" {
    name = "Shipped"
    final = true
  }
  state "cancelled" {
    name = "Cancelled"
    final = true
  }

  transition "t1" {
    from = "pending"
    to = "paid"
    trigger = "payment captured"
  }
  transition "t2" {
    from = "paid"
    to = "shipped"
    trigger = "dispatched"
    guard = "stock reserved"
  }
  transition "t3" {
    from = "pending"
    to = "cancelled"
    trigger = "customer cancels"
  }
  transition "t4" {
    from = "paid"
    to = "paid"
    trigger = "partial refund"
  }
}
```

![state_diagram](../_wdoc/fact_state_diagrams-state-diagram-1.svg)

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `width` | `f64` | no | Rendered width in pixels; the height follows the content. |
| `direction` | `symbol` | no | Flow direction: `:top_to_bottom` (default) / `:left_to_right`. |
| `layer_gap` | `f64` | no | Spacing between ranks (layers). |
| `node_gap` | `f64` | no | Spacing between states within a rank. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes on the `<svg>`. |
| `desc` | `utf8` | no | Accessible description (`aria-label` + `<title>`). |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `states` | `state` | yes | States; auto-ranked from the transition graph in declaration order. |
| `transitions` | `transition` | yes | Transitions between states (`trigger [guard]` edge labels). |

## States and transitions

A `transition` is a block (not an `a -> b` edge) because it carries payload: the `trigger` event and an optional `guard`. The same `from` and `to` renders a self-loop arc. States rank along `direction` (default `:top_to_bottom`); a state with explicit `x` **and** `y` opts out of auto-layout.

A lifecycle state; `initial = true` draws the entry dot, `final = true` the double border, and explicit `x` and `y` opt out of auto-layout.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `utf8` | yes | Stable id transitions reference via `from` / `to`. |
| `name` | `utf8` | no | Display name shown in the box (defaults to the id). |
| `initial` | `bool` | no | Entry state: draws the filled-dot pseudo-state with an arrow into the box. |
| `final` | `bool` | no | Final state: draws the double-border marker. |
| `link` | `utf8` | no | Link the box to an in-site page (bare page name, or `site:page`). |
| `x` | `f64` | no | Manual x placement (with `y`, opts this state out of auto-layout). |
| `y` | `f64` | no | Manual y placement (with `x`, opts this state out of auto-layout). |
| `width` | `f64` | no | Box width. |
| `height` | `f64` | no | Box height. |
| `class` | `list<utf8>` | no | Style classes for the box (replaces the theme defaults). |

An edge between two states carrying a `trigger` event and an optional `guard`; the same `from` and `to` renders a self-loop.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `utf8` | yes | Stable id. |
| `from` | `utf8` | yes | Source state id. |
| `to` | `utf8` | yes | Destination state id (same as `from` for a self-loop). |
| `trigger` | `utf8` | no | The event that fires the transition (the edge label). |
| `guard` | `utf8` | no | Guard condition, rendered as `trigger [guard]`. |

Like every `@children` slot, `states` / `transitions` accept computed splices, so a state-machine model can generate its figure. See [data views](../references/concept_data_views.md).

## Related

- [diagram](../references/fact_diagrams.md)

- [flowchart shapes](../references/fact_flowcharts.md)

- [sequence_diagram](../references/fact_sequence_diagrams.md)

[← Back to SKILL.md](../SKILL.md)
