# Data Views

_Render content from WCL data: wdoc_component / slot / repeater / instance, body & project, partial & collect._

A \*data view\* renders document content — cards, tables, charts, diagrams — from a WCL **data structure** rather than hand-authored blocks. Declare the data once, then derive every view from it. The primary tool is a **component**: a reusable fragment of ordinary wdoc markup with named **slots**.


## Components

Declare a `wdoc_component` with `wdoc_slot`s and a `wdoc_body` of ordinary markup. Reference slots in any `$"…${slot}…"` interpolated string or as a bare identifier in a field (`class = [status]`). A slot with a `default` is optional. Instantiate the component by its own name.


```wcl
wdoc_component dv_metric {
  wdoc_slot label
  wdoc_slot value
  wdoc_slot status { default = "note" }
  wdoc_body {
    callout $"${label}" { class = [status]  body = $"Currently at **${value}%**" }
  }
}

// ... then, anywhere a block is allowed:
dv_metric { label = "CPU" value = 42 status = "warning" }
dv_metric { label = "Memory" value = 88 }    // status defaults to "note"
```

Live — each call fills the slots and renders the component's `wdoc_body`:

```wcl
dv_metric {
  label = "CPU"
  value = 42
  status = "warning"
}
dv_metric {
  label = "Memory"
  value = 88
}
```

> [!WARNING]
> **CPU**
> Currently at **42%**

> [!NOTE]
> **Memory**
> Currently at **88%**

> [!TIP]
> **Interpolating slots**
> Slot values land in text via WCL's `$"…"` interpolated strings — note the `$` prefix. A plain `"…"` string is literal. Bare references in a field (like `class = [status]`) need no prefix.

## Repeating over data

`wdoc_repeater` renders its body once per element of `each`, binding the element to the symbol named by `as`. Combined with a component it stamps one card per data row; with no component its body is just markup with the loop variable in scope. A slot can hold a whole list, and a repeater inside a component body can iterate it.


```wcl
wdoc_repeater { each = inventory  as = :h
  dv_metric { label = h.name  value = h.cpu  status = h.status }
}
```

Live — one card per element of the data list, the loop variable filling each slot:

```wcl
wdoc_repeater {
  each = dv_demo_metrics
  as = :m
  dv_metric {
    label = m.name
    value = m.pct
    status = m.sev
  }
}
```

> [!WARNING]
> **CPU**
> Currently at **42%**

> [!CAUTION]
> **Memory**
> Currently at **88%**

> [!TIP]
> **Disk**
> Currently at **31%**

## Generating pages and navigation

A `wdoc_repeater` is the single iteration concept at every level. At the **document root**, give it a `page` block and it emits one rendered page per element — the page's interpolated label becomes the route. Inside a `toc` (or a `chapter`), give it a `chapter` block and it emits one navigation entry per element.


```wcl
wdoc_repeater { each = containers  as = :c
  page $"cont_${c.id}" {
    sites = [:docbook]
    title = c.name
    h1 $"${c.name}"
  }
}

site docbook {
  default_template = :book
  toc {
    chapter "Containers" {
      wdoc_repeater { each = containers  as = :c
        chapter $"${c.name}" { page = $"cont_${c.id}" }
      }
    }
  }
}
```

> [!NOTE]
> **Routes must be slug-safe and unique**
> A generated route is its interpolated label, so it must be non-empty, contain only `A-Za-z0-9_-`, and be unique within its site. Build a slug from prose with `to_lower(replace(s, " ", "-"))`.

## Render by reference

A `wdoc_instance` renders the component named by the **value** of its `component` field — so a repeater can emit a \*different\* component per element. The instance's like-named fields fill the target's slots (falling back to each slot's `default`).


```wcl
wdoc_repeater { each = widgets  as = :row
  // `component` is data, so each element picks its own component.
  wdoc_instance { component = row.kind  label = row.label  value = row.value  status = row.status }
}
```

## Content slots (layout wrappers)

A `wdoc_content` block in a component body marks where the instance's \*own\* nested blocks render — so a component can frame arbitrary content.


```wcl
wdoc_component dv_panel {
  wdoc_slot title
  wdoc_body {
    h3 $"${title}"
    wdoc_content          // the caller's nested blocks render here
  }
}

dv_panel { title = "Notes"
  p "Anything nested in the instance renders at wdoc_content."
  list { li "including lists" li "and more" }
}
```

Live — the component frames the caller's own nested blocks:

```wcl
dv_panel {
  title = "Notes"
  p "Anything nested in the instance renders at the content slot."
  list {
    li "including lists" li "and more"
  }
}
```

### Notes

Anything nested in the instance renders at the content slot.

- including lists

## Partials (scatter and collect)

A `partial` tags a body of blocks; a `collect` with the same tag gathers every matching partial — across the whole document and its imported files — and renders their bodies, in document order, at the collect site. A partial is **invisible where it's defined** unless you set `show_here = true`. It's the appendix / glossary / collected-sidebars pattern.


```wcl
// Scatter tagged deposits anywhere — different blocks, even imported files:
partial aside { callout "From section one" { body = "A point to collect later." } }
// ... prose, other blocks ...
partial aside { callout "From section two" { body = "Another point." } }

// Gather every `aside` partial here, in document order:
collect aside
```

> [!NOTE]
> **Scope and limits**
> Collection is **document-global**: a `collect` gathers matching partials from the root document and every file pulled in by a top-level `import`. Partials in block-scoped (lazily imported) files aren't reached, and a collected body should avoid `id`s.

## Content fragments on data (body and project)

A `body` attaches a chunk of renderable content to a **data record** as a \*property\* — without that record being a renderable block — and a `project` renders it elsewhere by **reference**. Declare your own block type with a `@child("body")` slot, author the content inside each record, then `project` it from a repeater. Because `body` is `@by_ref`, `from = s.overview` resolves to \*that\* record's fragment, and `${…}` inside the body resolves against the record.


```wcl
@block("server")
type Server {
  @inline(0) name: identifier
  region: utf8?
  @child("body") overview: WdocAddressableBody?   // content rides on the record
}

server web01 { region = "us-east"
  body { p $"Frontend in ${region}." }            // NOT a renderable block here
}

page fleet {
  wdoc_repeater { each = servers  as = :s
    h2 $"${s.name}"
    project { from = s.overview }                 // render THIS record's body
  }
}
```

> [!NOTE]
> **Addressing**
> A single `@child("body")` slot is addressed by its slot, so the body needs no name. A body in a `@children("body")` list, or one declared at the document root, is addressed by its `@inline(0)` name. The record carrying a body may be nested (a step inside a tutorial). A `body` never renders where it's declared, only where projected.

## Documenting schema types

The built-in `type_table` component documents a schema type by reflecting it — `type_table { type = Image }` renders a table of the type's properties (name, type, required, description), including inherited fields. Descriptions and visibility are authored on the schema with `@doc("…")` and `@hidden`. `block_reference { type = MyDoc }` walks a document's `@child` / `@children` slots and emits an `h3` plus a `type_table` for each.


## Block reference

A `wdoc_component` block: a reusable fragment of wdoc markup with named slots, instantiated by its own name.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `slots` | `wdoc_slot` | yes |  |
| `body` | `wdoc_body` | no |  |

A `wdoc_slot` inside a component: a named, optionally-defaulted parameter filled at instantiation.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes |  |
| `default` | `utf8` | no |  |

A `wdoc_body` inside a component: the markup template that renders, with the slots in scope.

| Property | Type | Required | Description |
| --- | --- | --- | --- |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `WdocBlock` | yes |  |

A `wdoc_content` marker in a component body: where the instance's own nested blocks render, framing arbitrary content.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `identifier` | no |  |

A `wdoc_repeater` block: renders its body once per element of `each`, binding the element to the symbol named by `as`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `each` | `list<WdocItem>` | yes |  |
| `as` | `symbol` | yes |  |
| `id` | `identifier` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `WdocBlock` | yes |  |
| `pages` | `page` | yes |  |
| `chapters` | `chapter` | yes |  |
| `classes` | `class` | yes |  |
| `sheets` | `stylesheet` | yes |  |

A `wdoc_instance` block: renders the component named by the value of its `component` field, filling slots from its like-named fields.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `component` | `utf8` | yes |  |
| `id` | `identifier` | no |  |

A `partial` block: tags a body of blocks for later collection — invisible where defined unless `show_here = true`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `tag` | `symbol` | yes |  |
| `show_here` | `bool` | no |  |
| `id` | `identifier` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `WdocBlock` | yes |  |

A `collect` block: gathers every matching `partial` across the document and renders their bodies in document order at the collect site.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `tag` | `symbol` | yes |  |
| `id` | `identifier` | no |  |

A `body` block: a chunk of renderable content attached to a data record as a property, rendered elsewhere by `project`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | no |  |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `WdocBlock` | yes |  |

A `project` block: renders an addressable `body` by reference (`from = …`), resolving `${…}` against that record.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `from` | `utf8` | yes |  |
| `id` | `identifier` | no |  |

## Examples

### One card per data row

A wdoc_repeater renders its body once per element of `each`, binding the element to the symbol named by `as`. Combined with a component, it stamps one card per row of data.

```wcl
wdoc_repeater { each = inventory  as = :h
  dv_metric { label = h.name  value = h.cpu  status = h.status }
}
```

**Expected:** One metric card per inventory entry, each reading its label, value, and status from the data row.

## Related

- [Including sub-sites](../references/concept_includes.md)

- [Connections](../references/concept_connections.md)

[← Back to SKILL.md](../SKILL.md)
