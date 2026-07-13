# sequence_diagram

A `sequence_diagram` draws a runtime interaction coordinate-free: `participant`s rank left-to-right and `message`s top-to-bottom **in declaration order**, lifelines extend past the last message, and the height follows the content (only `width` is declared). It is a page-level block, not a `diagram` shape.

A page-level block drawing a runtime interaction; `participant`s rank left-to-right and `message`s flow top-to-bottom in declaration order.

```wcl
sequence_diagram {
  width = 720

  participant "customer" {
    name = "Customer"
    kind = :actor
  }
  participant "web" {
    name = "Web App"
  }
  participant "api" {
    name = "API Application"
  }
  participant "stripe" {
    name = "Stripe"
    kind = :external
  }

  message "m1" {
    from = "customer"
    to = "web"
    text = "Submit payment form"
  }
  message "m2" {
    from = "web"
    to = "api"
    text = "POST /orders"
  }
  message "m3" {
    from = "api"
    to = "stripe"
    text = "Capture charge"
  }
  message "m4" {
    from = "stripe"
    to = "api"
    text = "charge id"
    kind = :reply
  }
  message "m5" {
    from = "api"
    to = "api"
    text = "persist order"
  }
  message "m6" {
    from = "api"
    to = "web"
    text = "201 Created"
    kind = :reply
  }
  note "n1" {
    at = "m3"
    text = "Retries reuse the idempotency key."
  }
}
```

![sequence_diagram](../_wdoc/fact_sequence_diagrams-sequence-diagram-1.svg)

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `width` | `f64` | no | Rendered width in pixels; the height follows the content. |
| `col_width` | `f64` | no | Horizontal distance between participant lifelines. |
| `row_height` | `f64` | no | Vertical distance between message rows. |
| `header_height` | `f64` | no | Y of the first message row (below the participant heads). |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes on the `<svg>`. |
| `desc` | `utf8` | no | Accessible description (`aria-label` + `<title>`). |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `participants` | `participant` | yes | Participants, left-to-right in declaration order. |
| `messages` | `message` | yes | Messages, top-to-bottom in declaration order. |
| `notes` | `note` | yes | Margin notes anchored to messages. |

## Participants, messages, notes

A `participant`'s `kind` picks the head: `:box` (default), `:actor` (stick figure), `:external` (dashed box); `link` makes it clickable. A `message`'s `kind` picks the arrow: `:sync` (solid line, filled head, default), `:async` (solid, open head), `:reply` (dashed, open head); the same `from` and `to` renders a self-message loop. A `note` is a margin annotation drawn at the row of the message named by `at`.

One figure exercising all three participant kinds, the three arrow kinds, a self-message, and a note:

```wcl
sequence_diagram {
  width = 600
  participant "user" {
    name = "User"
    kind = :actor
  }
  participant "svc" {
    name = "Service"
  }
  participant "ext" {
    name = "Provider"
    kind = :external
  }
  message "a" {
    from = "user"
    to = "svc"
    text = "request"
  }
  message "b" {
    from = "svc"
    to = "ext"
    text = "fetch"
    kind = :async
  }
  message "c" {
    from = "ext"
    to = "svc"
    text = "data"
    kind = :reply
  }
  message "d" {
    from = "svc"
    to = "svc"
    text = "cache result"
  }
  message "e" {
    from = "svc"
    to = "user"
    text = "response"
    kind = :reply
  }
  note "n1" {
    at = "b"
    text = "async: solid line, open head"
  }
}
```

![sequence_diagram](../_wdoc/fact_sequence_diagrams-sequence-diagram-2.svg)

A lifeline; `kind` picks the head (`:box` / `:actor` / `:external`) and `link` makes it clickable.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `utf8` | yes | Stable id messages reference via `from` / `to`. Column order is declaration order. |
| `name` | `utf8` | no | Display name shown in the head (defaults to the id). |
| `kind` | `ParticipantKind` | no | Head style: `:box` (default) / `:actor` / `:external`. |
| `link` | `utf8` | no | Link the head to an in-site page (bare page name, or `site:page`). |
| `class` | `list<utf8>` | no | Style classes for the head shapes (replaces the theme defaults). |

An arrow between participants; `kind` picks the style (`:sync` / `:async` / `:reply`) and the same `from` and `to` renders a self-message loop.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `utf8` | yes | Stable id (notes reference it via `at`). Row order is declaration order. |
| `from` | `utf8` | yes | Sending participant id. |
| `to` | `utf8` | yes | Receiving participant id (same as `from` for a self-message loop). |
| `text` | `utf8` | no | Arrow label. |
| `kind` | `MessageKind` | no | Arrow style: `:sync` (default) / `:async` / `:reply`. |

A margin annotation drawn at the row of the message named by `at`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `utf8` | yes | Stable id. |
| `at` | `utf8` | yes | Id of the message this note is anchored beside. |
| `text` | `utf8` | yes | The note text. |

Like every `@children` slot, the `participants` / `messages` lists accept computed splices, so a repeated scenario model can generate its figure. See [data views](../references/concept_data_views.md).

## Related

- [diagram](../references/fact_diagrams.md)

- [flowchart shapes](../references/fact_flowcharts.md)

- [state_diagram](../references/fact_state_diagrams.md)

[← Back to SKILL.md](../SKILL.md)
