# Data views

A *data view* renders page content from a WCL **data structure** rather than from hand-written
blocks. Declare the data once; derive every card, table, chart and page from it.

Seven block kinds do this work. All are `@native` — their semantics (slot binding, body
expansion, iteration, reference resolution) live in Rust, not in a WCL `lower`.

| Block | What it does |
| --- | --- |
| `wdoc_component` | Declare a reusable fragment with named slots. |
| `wdoc_repeater` | Render a body once per element of a list. |
| `wdoc_instance` | Render the component named by a *value*. |
| `wdoc_content` | Mark where an instance's own nested blocks land. |
| `partial` / `collect` | Scatter tagged content; gather it somewhere else. |
| `body` / `project` | Attach content to a data record; render it by reference. |
| `type_table` / `block_reference` | Generate property tables from a schema. |

## Components

```wcl
wdoc_component metric_card {
  wdoc_slot label
  wdoc_slot value
  wdoc_slot status { default = "note" }     // a default makes the slot optional
  wdoc_body {
    callout $"${label}" { class = [status]  body = $"Currently at **${value}%**" }
  }
}

// Instantiate by the component's own name, anywhere a block is legal:
metric_card { label = "CPU"  value = 42  status = "warning" }
metric_card { label = "Memory"  value = 88 }        // status defaults to "note"
```

**Slot values reach text through `$"…${slot}…"` interpolated strings — note the `$` prefix.**
A plain `"…"` is literal. A bare reference in a field position (`class = [status]`) needs no
prefix.

### Typed slots

`slot name: Type` is the current spelling, and it preserves the type. `wdoc_slot name` is the
older untyped one. A component may use either — the derived kind schema reads both.

```wcl
wdoc_component badge {
  slot label: utf8
  slot tone:  utf8 = "note"
  slot loud:  bool = false
  wdoc_body { callout $"${label}" { class = [tone]  body = "…" } }
}
```

### Content slots — nested-block holes

A slot typed `content` is a **block hole**, not a scalar. The instance fills it with a bare
block of that name, and the body places it by writing the name:

```wcl
wdoc_component split_card {
  slot header: content
  slot body:   content
  wdoc_body {
    column {
      header {}                       // the header fill lands here
      body {}                         // the body fill lands here
    }
  }
}

split_card {
  header { h2 "Named header" }
  body   { p "Named body." }
}
```

`content?` makes the hole optional, `content*` repeatable, and `content<T>` restricts the
child kind or interface it accepts.

For a component that frames *whatever* the caller nests — one unnamed hole — use
`wdoc_content` instead:

```wcl
wdoc_component panel {
  wdoc_slot title
  wdoc_body {
    h3 $"${title}"
    wdoc_content                      // the instance's own nested blocks render here
  }
}

panel { title = "Notes"
  p "Anything nested in the instance renders at wdoc_content."
  list { li "including lists" }
}
```

### Why an instance validates like any other block

`wdoc_component` carries `@declares_kind`, the language feature meaning "instances of this
type declare block kinds of their own". So `metric_card { … }` gets a **derived schema**: an
undeclared slot is an unknown field, and an unfilled defaultless slot is a missing required
one. There is no component vocabulary inside the language — see
[`../language/lang_decorators.md`](../language/lang_decorators.md).

A `wdoc_body` is `@schemaless`, because a template's blocks only have meaning once expanded at
an instance site (a component used inside a diagram legitimately holds `SvgBlock`s).

## Repeating over data — `wdoc_repeater`

```wcl
wdoc_repeater { each = metrics  as = :m
  metric_card { label = m.name  value = m.pct  status = m.sev }
}
```

`each` is any list expression; `as` is a **symbol** naming the loop binding. `each` is read
dynamically, so the validator does not type-check its elements — a wrong field name on the
binding surfaces at build time, not at `wcl check`.

**A repeater is the one iteration concept at every level of a document:**

- Inside a page (or a `diagram` / `container`) its body is content blocks (or shapes).
- **At the document root** its body is `page` blocks — one rendered page per element.
- **Inside a `toc` or a `chapter`** its body is `chapter` blocks — one nav entry per element.

```wcl
wdoc_repeater { each = containers  as = :c
  page $"cont_${c.id}" { title = c.name  h1 $"${c.name}" }
}

site docbook {
  toc {
    chapter "Containers" {
      wdoc_repeater { each = containers  as = :c
        chapter $"${c.name}" { page = $"cont_${c.id}" }
      }
    }
  }
}
```

A generated page's **route is its interpolated label**, so it must be non-empty, contain only
`A-Za-z0-9_-`, and be unique within the site. Build a slug from prose with
`to_lower(replace(s, " ", "-"))`.

Components and repeaters compose in both directions: a repeater may sit in a component body
and iterate a list-valued slot, and a component may be instantiated in a repeater body.

## Render by reference — `wdoc_instance`

`wdoc_instance` renders the component named by the **value** of its `component` field, so one
repeater can emit a *different* component per element:

```wcl
wdoc_repeater { each = widgets  as = :row
  wdoc_instance { component = row.kind  label = row.label  value = row.value }
}
```

The instance's like-named fields fill the target's slots, falling back to each slot's
`default`. It is `@schemaless`, so the forwarded fields need no static schema — which also
means a typo in one is silently dropped rather than reported.

## Scatter and collect — `partial` / `collect`

```wcl
partial aside { callout "From section one" { body = "A point to collect later." } }
// … elsewhere, even in another imported file …
partial aside { callout "From section two" { body = "Another point." } }

page summary {
  collect aside                        // both bodies render here, in document order
}
```

A `partial` is **invisible where it is defined** unless `show_here = true`. The tag is a
symbol label on both blocks.

Two limits. First, collection is **document-global but import-shallow**: it reaches the root
document and every file a *top-level* `import` pulls in. It does not reach a file imported
lazily inside a block. Second, give a collected body no `id`s — it renders somewhere else.

## Content on data records — `body` / `project`

`partial` tags content by topic. `body` attaches content to **one data record**, as a
property, without that record being a renderable block:

```wcl
@block("server")
type Server {
  @inline(0) name: identifier
  region: utf8?
  @child("body") overview: WdocAddressableBody?     // content rides on the record
}

server web01 { region = "us-east"
  body { p $"Frontend in ${region}." }              // renders nothing here
}

page fleet {
  wdoc_repeater { each = servers  as = :s
    h2 $"${s.name}"
    project { from = s.overview }                   // render THIS record's body
  }
}
```

`body` is `@by_ref`: when a record is read as data, its body slot reifies to a resolvable
*reference* rather than to inlined content. That is what makes `from = s.overview` address the
right fragment. `${…}` inside the body resolves against the owning record.

Addressing rules:

- A single `@child("body")` slot is addressed by its slot, so the body needs no name.
- A body in a `@children("body")` **list**, or one declared at the document root, is addressed
  by its `@inline(0)` label — `body intro { … }`.
- A page-scope `project` names the full path from a root field
  (`from = tuts.t1.steps.1.body`). Note `steps.1` matches the step *labelled* `1` — label
  matching, not positional indexing.
- Records carrying a body may be nested arbitrarily.

A `body` never renders where it is declared, only where it is projected. `project` also
accepts a list of references (rendering each in order) and is cycle-guarded against a body
that projects itself.

## Schema-reflected tables — `type_table` and `block_reference`

These two are ordinary `wdoc_component`s shipped in the stdlib, built on the `type_fields`,
`child_types` and `decorator_arg` reflection builtins. The docs cannot drift from the schema
because they *are* the schema.

```wcl
type_table { type = Server }
```

This renders a **Properties** table: name, type, required, description. Own fields come first,
inherited (`extends`) fields after. A type with block slots also gets a **Child blocks** table,
naming each slot, the kind it accepts, and whether it holds a list. Two decorators drive it:

- `@doc("…")` on a field supplies the description column.
- `@hidden` drops a field from the table entirely.

Function-typed fields (the `lower` hook every block carries) are dropped automatically.
"Required" is `no` when the field is optional *or* has a default.

```wcl
block_reference { type = MyDoc }
```

Walks a type's `@child` / `@children` slots and emits an `h3` naming each block kind plus its
`type_table` — a whole document vocabulary documented from one line. This is how a project that
declares its own root `@document` alongside `import <wdoc.wcl>` documents its own top-level
tags.

## The gather-field collision hazard

A `@document` schema **composes per namespace**: the effective document schema is the merge of
every `@document` type visible there. That is what lets you declare your own root `@document`
beside `import <wdoc.wcl>` and add your own top-level blocks.

The consequence: **gather-field names share one space with wdoc's.** Give your document type a
gather field named like a wdoc one — `components`, `pages`, `sites`, `bodies` — and the name
resolves ambiguously. Template iteration then breaks silently. `each = components` fails as an
unresolved reference *at build time*, and nothing points you at the collision.

Prefix your gathers (`sw_components`, `my_pages`). See
[`../language/lang_schemas.md`](../language/lang_schemas.md) for the merge rules.

## Related

- Declaring your own block types and decorators:
  [`../language/lang_schemas.md`](../language/lang_schemas.md).
- The template-level `slot` mechanism, which shares this `content` syntax but belongs to
  layouts: [`wdoc_templates.md`](wdoc_templates.md) and [`wdoc_websites.md`](wdoc_websites.md).
- Feeding a `table`'s computed `rows` from a component slot:
  [`wdoc_tables.md`](wdoc_tables.md).
