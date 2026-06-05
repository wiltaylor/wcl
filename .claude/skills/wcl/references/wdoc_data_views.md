# Data Views

A \*data view\* renders document content — cards, tables, charts, diagrams — from a WCL **data structure** rather than hand-authored blocks. It's how you present data gathered about systems: declare the data once, then derive every view from it. The primary tool is a **component**: a reusable fragment of ordinary wdoc markup with named **slots**.

Every example draws from one inventory:

```wcl
let inventory = [
  { name: "web-1", role: "frontend", cpu: 42.0, mem: 60.0, up: true,  status: "note"    },
  { name: "db-1",  role: "database", cpu: 88.0, mem: 75.0, up: true,  status: "warning" },
  { name: "cache", role: "cache",    cpu: 25.0, mem: 40.0, up: false, status: "error"   },
]
```

## Components

Declare a `wdoc_component` with `wdoc_slot`s and a `wdoc_body` of ordinary markup. Reference slots in any `$"…${slot}…"` interpolated string or as a bare identifier in a field (`class = [status]`). A slot with a `default` is optional. Instantiate the component by its own name:

> [!WARNING]
> **CPU**
> Currently at **42%**

> [!NOTE]
> **Memory**
> Currently at **88%**

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

> [!TIP]
> **Interpolating slots into text**
> Slot values land in text via WCL's `$"…"` interpolated strings — note the `$` prefix. A plain `"…"` string is literal. Bare references in a field (like `class = [status]`) need no prefix.

## Repeating over data

`wdoc_repeater` renders its body once per element of `each`, binding the element to the symbol named by `as`. Combined with a component, it stamps one card per data row:

> [!NOTE]
> **web-1**
> Currently at **42%**

> [!WARNING]
> **db-1**
> Currently at **88%**

> [!CAUTION]
> **cache**
> Currently at **25%**

```wcl
wdoc_repeater { each = inventory  as = :h
  dv_metric { label = h.name  value = h.cpu  status = h.status }
}
```

A repeater needs no component — its body is just markup with the loop variable in scope:

```wcl
wdoc_repeater { each = inventory  as = :h
  h3 $"${h.name}"
  p  $"CPU ${h.cpu}% · Mem ${h.mem}%"
}
```

## Repeaters inside components

A slot can hold a whole list, and a `wdoc_repeater` inside the body can iterate it. Bindings stack, so the loop body sees both the loop variable and the component's other slots:

### Fleet

> [!NOTE]
> **web-1**
> Currently at **42%**

> [!WARNING]
> **db-1**
> Currently at **88%**

> [!CAUTION]
> **cache**
> Currently at **25%**

```wcl
wdoc_component dv_hosts {
  wdoc_slot heading
  wdoc_slot rows                       // a list-typed slot
  wdoc_body {
    h3 $"${heading}"
    wdoc_repeater { each = rows  as = :h
      dv_metric { label = h.name  value = h.cpu  status = h.status }
    }
  }
}

dv_hosts { heading = "Fleet"  rows = inventory }
```

## Content slots (layout wrappers)

A `wdoc_content` block in a component body marks where the instance's \*own\* nested blocks render — so a component can frame arbitrary content:

### Notes

Anything nested in the instance renders at `wdoc_content`.

- including lists

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

## Partials (scatter & collect)

Components and repeaters compose content **at one spot**. A **partial** does the opposite: it lets you \*scatter\* tagged content throughout a document — even across imported files — and have one place **gather** it. It's the appendix / glossary / collected-sidebars pattern.

A `partial` tags a body of blocks; a `collect` with the same tag gathers every matching partial — across the whole document and its imported files — and renders their bodies, in document order, at the collect site. A partial is **invisible where it's defined** unless you set `show_here = true`.

Some prose lives between the deposits…

…and the `collect` below gathers both of the `dv_aside` deposits above, in order:

> [!NOTE]
> **From section one**
> A point worth collecting later.

> [!TIP]
> **From section two**
> Another collected point.

```wcl
// Scatter tagged deposits anywhere — different blocks, even imported files:
partial aside { callout "From section one" { body = "A point to collect later." } }
// ... prose, other blocks ...
partial aside { callout "From section two" { body = "Another point." } }

// Gather every `aside` partial here, in document order:
collect aside
```

Set `show_here = true` to render a partial both where it's defined \*and\* where it's collected:

```wcl
partial aside { show_here = true  p "Pinned in place and also collected." }
```

> [!NOTE]
> **Scope & limits**
> Collection is **document-global**: a `collect` gathers matching partials from the root document and every file pulled in by a top-level `import` — so tag deposits distinctly to avoid cross-page bleed. Partials in **block-scoped** (lazily imported) files aren't reached, nested partials inside a partial body aren't separately collected, and a collected body should avoid `id`s (per-page id checks run before collection). A `collect` whose collected content contains another `collect` of the same tag is a no-op (the cycle is broken).

## Tables from data

Set a `table`'s `rows` to a list of cell-lists — `map`ped from your data — with an optional `header` row. utf8 cells run through the inline-pattern engine (so `**bold**`, `:icons:`, links work); other scalars stringify:

| Host | Role | CPU % |
| --- | --- | --- |
| web-1 | frontend | 42% |
| db-1 | database | 88% |
| cache | cache | 25% |

```wcl
table {
  header = ["Host", "Role", "CPU %"]
  rows = map(inventory, fn(h: DvHost) -> list<utf8> { [h.name, h.role, $"${h.cpu}%"] })
}
```

Wrap a table in a component to reuse the layout, feeding the rows through a slot. Name the slot apart from the table's own `rows` field (a field that references its own name is a cycle, and renders nothing):

```wcl
wdoc_component host_table {
  wdoc_slot data
  wdoc_body { table { header = ["Host", "Role", "CPU %"]  rows = data } }
}

host_table { data = map(inventory, fn(h: DvHost) -> list<utf8> { [h.name, h.role, $"${h.cpu}%"] }) }
```

## Charts from data

Charts read their data from value fields, so you `map` the inventory straight into them — no wrapper needed:

![diagram](../_wdoc/wdoc_data_views-diagram-1.svg)

```wcl
bar_chart {
  categories = map(inventory, fn(h: DvHost) -> utf8 { h.name })
  series = [
    { name: "cpu", values: map(inventory, fn(h: DvHost) -> f64 { h.cpu }) },
    { name: "mem", values: map(inventory, fn(h: DvHost) -> f64 { h.mem }) },
  ]
}
```

## Connections from data

A `wdoc_repeater` inside a `diagram` generates one shape per data element; give each a data-derived `id`. Then set the diagram's `edges` to a computed list — `map`ped from the data's own relationships — and `layout = :layered` (or `:force`) positions the graph automatically. Connect by id: each edge's `source`/`destination` names a node's `id`.

![diagram](../_wdoc/wdoc_data_views-diagram-2.svg)

```wcl
type DvSvc { key: utf8  name: utf8  deps: list<utf8> }

let services = [
  { key: "web",   name: "Web",   deps: ["api"] },
  { key: "api",   name: "API",   deps: ["db", "cache"] },
  { key: "db",    name: "DB",    deps: [] },
  { key: "cache", name: "Cache", deps: [] },
]

// One edge per (service → dependency) pair, derived from the data.
let service_links = flatten(map(services, fn(s: DvSvc) -> list<Edge> {
  map(s.deps, fn(d: utf8) -> Edge { { source: s.key, destination: d } })
}))

diagram { width = 460  height = 300  layout = :layered
  wdoc_repeater { each = services  as = :s
    process $"${s.name}" { id = s.key }   // node label from data, id = key
  }
  edges = service_links
}
```

> [!WARNING]
> **Name the binding apart from `edges`**
> Assign the computed edges from a differently-named binding (`edges = service_links`), not `edges = edges` — a field that references its own name is a cycle and renders nothing.

## Documenting types

The built-in `type_table` component documents a schema type by reflecting it — no hand-maintained field lists. `type_table { type = Image }` renders a table of the type's properties (name, type, whether it's required, and a description), including fields inherited via `extends`; a second "Child blocks" table lists its `@child` / `@children` slots. It's built on the `type_fields` reflection builtin.

Descriptions and visibility are authored on the schema itself:

| Decorator | Effect |
| --- | --- |
| `@doc("…")` | Sets the description shown for the field. |
| `@hidden` | Drops the field from the generated table. |

Function-typed fields (every block's `lower` hook) are skipped automatically. This very page's [Pages](../references/wdoc_pages.md) and [Images](../references/wdoc_images.md) field tables are generated this way.

## Documenting your own schema

`type_table` reflects \*any\* type, so it documents the blocks **you** declare just as well as wdoc's built-ins. Declare your own root `@document` alongside `import <wdoc.wcl>` (the two schemas [merge](../references/schema.md)), give each block type a `@block("…")` kind and `@doc` descriptions, then reflect them:

```wcl
import <wdoc.wcl>

@document
type MyDoc {
  @children("project_meta") metas: list<ProjectMeta>
  @child("settings") settings: Settings
}
@block("project_meta") type ProjectMeta { @inline(0) id: identifier  @doc("the owner") owner: utf8 }
@block("settings")     type Settings     { @doc("UI theme") theme: utf8 }

page reference {
  h2 "Top-level blocks"
  block_reference { type = MyDoc }    // one heading + property table per block
}
```

`block_reference { type = MyDoc }` walks the document's `@child` / `@children` slots and emits an `h3` (the block's `@block` kind) plus a `type_table` for each — no hand-maintained list, and the reference can't drift from the schema. It's a thin wrapper over the `child_types` reflection builtin, which returns the element type references of a type's block slots. Drop to the repeater directly when you want a different layout:

```wcl
wdoc_repeater { each = child_types(MyDoc)  as = :b
  type_table { type = b }
}
```

> [!NOTE]
> **Union and interface slots**
> `child_types` resolves a `@child` / `@children` slot to its declared element type. A slot that accepts a **union** or **interface** resolves to that type's name; since `type_table` documents a single concrete `type` / `interface`, such slots aren't expanded into a table per variant — document those members individually.

## The pattern

| Tool | Use it for |
| --- | --- |
| **`wdoc_component`** | A reusable fragment of wdoc markup with slots — cards, panels, sections. The everyday tool. |
| **`wdoc_repeater`** | Rendering a body (or a component) once per element of a data list. |
| **`wdoc_content`** | A component that wraps arbitrary caller-provided content. |
| **`partial` + `collect`** | Scattering tagged content across a document (or imported files) and gathering it by tag at one site — appendices, glossaries, collected sidebars. |
| **`table` computed `rows`** | A data-driven `<table>` — `table { header = […] rows = map(data, …) }`. |
| **`wdoc_repeater` + computed `edges`** | A data-driven diagram — repeater-generated nodes (id from data) wired by a computed `edges` list, auto-positioned by `:layered`/`:force`. |
| **Chart value fields** | Feeding `map`ped data straight into `bar_chart` / `line_chart` / `pie_chart`. |
| **`type_table`** | Auto-documenting a schema type's fields — reflected via `type_fields`, described with `@doc` / `@hidden`. |
| **`block_reference`** | Auto-documenting every top-level block your `@document` declares — a heading + `type_table` per block, via `child_types`. |
