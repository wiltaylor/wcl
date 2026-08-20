# Sequence and state diagrams

`sequence_diagram` and `state_diagram` are **page-level blocks**, not `diagram` shapes. Write
each one in a page body beside `p` and `code`. Neither takes an `x` / `y`.

Both are coordinate-free. You declare the parts and the renderer computes every position.

## sequence_diagram

Participants rank left to right in declaration order. Messages stack top to bottom in
declaration order. Lifelines run past the last message, and the height follows the content. You
declare only the width.

```wcl
sequence_diagram {
  width = 720
  desc = "customer checkout"

  participant "customer" { name = "Customer"        kind = :actor }
  participant "web"      { name = "Web App" }
  participant "api"      { name = "API Application" }
  participant "stripe"   { name = "Stripe"          kind = :external }

  message "m1" { from = "customer" to = "web"    text = "Submit payment form" }
  message "m2" { from = "web"      to = "api"    text = "POST /orders" }
  message "m3" { from = "api"      to = "stripe" text = "Capture charge" }
  message "m4" { from = "stripe"   to = "api"    text = "charge id"    kind = :reply }
  message "m5" { from = "api"      to = "api"    text = "persist order" }
  message "m6" { from = "api"      to = "web"    text = "201 Created"  kind = :reply }
  note    "n1" { at = "m3"  text = "Retries reuse the idempotency key." }
}
```

### The block

| Field | Default | Meaning |
| --- | --- | --- |
| `width` | `760.0` | Rendered width. The height follows the content. |
| `col_width` | `140.0` | Horizontal distance between lifelines. |
| `row_height` | `44.0` | Vertical distance between message rows. |
| `header_height` | `64.0` | Y of the first message row, below the heads. |
| `id` / `class` | — | HTML id and style classes on the SVG. |
| `desc` | — | Accessible description. |

Here `width` is an `f64` with a decimal default, unlike a `diagram`'s `i64` canvas. `width = 720`
and `width = 720.0` both work.

### participant

The inline label is the **id** that messages reference. Order of declaration is column order.

| Field | Meaning |
| --- | --- |
| inline label | The id used by `from` / `to`. |
| `name` | Display name in the head. Defaults to the id. |
| `kind` | `:box` (default), `:actor` (stick figure), `:external` (dashed box). |
| `link` | Make the head a link to an in-site page. |
| `class` | Style classes, replacing the theme defaults. |

### message

The inline label is the id a `note` anchors to. Order of declaration is row order.

| Field | Meaning |
| --- | --- |
| inline label | The message id. |
| `from` / `to` | Participant ids. Required. |
| `text` | The arrow label. |
| `kind` | `:sync` (solid line, filled head — the default), `:async` (solid, open head), `:reply` (dashed, open head). |

`from` equal to `to` draws the standard self-message loop out of and back into the one lifeline.

**A typo in `from` or `to` is a build error**, not a stray arrow. The lowering raises
`sequence message 'm2' references unknown participant 'wbe'` and the build stops.

### note

A margin annotation drawn to the right of the last lifeline, level with the message it names.

| Field | Meaning |
| --- | --- |
| inline label | The note id. |
| `at` | The id of the message the note sits beside. Required. |
| `text` | The note text. Required. |

An `at` naming no message is a build error, on the same rule as a message endpoint.

## state_diagram

States rank automatically from the transition graph. `initial` draws the filled entry dot,
`final` the double border, and each transition carries a `trigger [guard]` label.

```wcl
state_diagram {
  width = 640
  direction = :left_to_right
  desc = "order lifecycle"

  state "pending"   { name = "Pending"   initial = true }
  state "paid"      { name = "Paid" }
  state "shipped"   { name = "Shipped"   final = true }
  state "cancelled" { name = "Cancelled" final = true }

  transition "t1" { from = "pending" to = "paid"      trigger = "payment captured" }
  transition "t2" { from = "paid"    to = "shipped"   trigger = "dispatched"  guard = "stock reserved" }
  transition "t3" { from = "pending" to = "cancelled" trigger = "customer cancels" }
  transition "t4" { from = "paid"    to = "paid"      trigger = "partial refund" }
}
```

### The block

| Field | Default | Meaning |
| --- | --- | --- |
| `width` | `640.0` | Rendered width. The height follows the content. |
| `direction` | `:top_to_bottom` | Flow axis. `:left_to_right` is the other. |
| `layer_gap` | `64.0` | Spacing between ranks. |
| `node_gap` | `48.0` | Spacing between states within a rank. |
| `id` / `class` / `desc` | — | As for a sequence diagram. |

### state

| Field | Meaning |
| --- | --- |
| inline label | The id transitions reference. |
| `name` | Display name in the box. Defaults to the id. |
| `initial` | `true` draws the filled entry dot and an arrow into the box. |
| `final` | `true` draws the double border. |
| `x` / `y` | Set **both** to opt this state out of auto-layout. |
| `width` / `height` | `110.0` / `44.0`. |
| `link` | Make the box a link to an in-site page. |
| `class` | Style classes, replacing the theme defaults. |

### transition

A transition is a block, not an `a -> b` connection, because it carries payload.

| Field | Meaning |
| --- | --- |
| inline label | The transition id. |
| `from` / `to` | State ids. Required. Equal ids draw a self-loop arc. |
| `trigger` | The event, drawn as the edge label. |
| `guard` | A condition, rendered as `trigger [guard]`. |

### How states are ranked

Rank is breadth-first distance from a seed set. The renderer picks the seeds in this order:

1. Every state marked `initial = true`.
2. Failing that, every state with no incoming transition.
3. Failing that — a pure cycle — the first state declared.

States sharing a rank stack along the cross axis in declaration order. A state no seed reaches
sits at rank 0 beside the roots. A transition that points back to an equal or lower rank is a
back-edge. The renderer routes it around the far side of the diagram, not through it.

## What both share

**One geometry lowering, every backend.** Neither block is native. Each computes its shapes in
WCL and returns them as a single `Content::Drawing` — a typed payload of SVG shapes plus a
width. HTML, PDF and Markdown all fit the same drawing to the same viewBox. There is no second
backend-specific geometry pass, and nothing in either renderer names `sequence_diagram` or
`state_diagram`.

**Both child lists accept computed splices.** `participants`, `messages`, `notes`, `states` and
`transitions` are ordinary list fields, so a model can generate its own figure:

```wcl
state_diagram {
  width = 520
  states = [
    { id: "draft",     name: "Draft", initial: true },
    { id: "review",    name: "In review" },
    { id: "published", name: "Published", final: true },
  ]
  transitions = [
    { id: "t1", from: "draft",  to: "review",    trigger: "submit" },
    { id: "t2", from: "review", to: "published", trigger: "approve" },
    { id: "t3", from: "review", to: "draft",     trigger: "reject" },
  ]
}
```

The same works with `map` over your own data:

```wcl
transitions = map(machine.edges, fn(e: SmEdge) -> Transition {
  { id: e.key, from: e.src, to: e.dst, trigger: e.event }
})
```

Mixing the two is fine: authored child blocks and a computed list on the same slot both land in
the collection.

## Styling

Both diagrams paint through theme classes you can redeclare:

| Class | Painted part |
| --- | --- |
| `wdoc-participant` | The participant head box. |
| `wdoc-participant-line` | The stroke-only heads — the actor stick figure, the dashed external box. |
| `wdoc-lifeline` | The dashed vertical lifeline. |
| `wdoc-seq-message` | The message line and the open arrowhead. |
| `wdoc-seq-arrow` | The filled `:sync` arrowhead. |
| `wdoc-seq-text` | Message labels. |
| `wdoc-note` / `wdoc-note-text` | The margin note box and its text. |
| `wdoc-shape-text` | Head labels. |

A `class` on a participant or a state **replaces** those defaults for that one shape.

## Gotchas

- Neither block goes inside a `diagram`. They are page blocks in their own right.
- The inline label is an **id**, not a display name. Set `name` for what the reader sees.
- Ids are quoted strings here (`participant "web"`, `from = "web"`), unlike a diagram's bare
  identifiers.
- You declare only `width`; the content decides the height. There is no `height` field.
- An unknown participant, message or state id stops the build. That is deliberate — an off-grid
  arrow would otherwise silently distort the drawing.
- A state pins itself only when it sets **both** `x` and `y`.
