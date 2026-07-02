# list / li

A `list` holds `li` items. It's a bullet list by default; set `style = :numbered` for a numbered one. Each `li`'s text runs through the inline-pattern engine, so **bold**, `code`, links, and icons all work inside items. See [formatting](../references/concept_formatting.md).

```wcl
list {
  li "Plain item"
  li "With **bold** and a [link](concept_overview)"
}
list {
  style = :numbered
  li "First step"
  li "Second step"
}
```

- Plain item
- With **bold** and a [link](../references/concept_overview.md)

1. First step
2. Second step

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |
| `style` | `ListStyle` | no | `:bullet` (default → `<ul>`) or `:numbered` (→ `<ol>`). |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `items` | `li` | yes | The list items. |

An `li` is one list item. Nest an `li` inside an `li` for a sublist, or drop a whole `list` block inside it for a sublist with a different style.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `text` | `utf8` | yes | Item text (the inline label); inline patterns apply. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `ListNode` | yes | Nested `li`s (a sublist), or a whole `list` block for a different style. |

## Nesting

Nest an `li` inside an `li` for a sublist — it inherits the parent list's style, so a numbered list numbers sublists hierarchically (`2.1`, `2.2`). For a sublist with a **different** style, drop a whole `list` block inside the `li` instead.

```wcl
list {
  style = :numbered
  li "Setup"
  li "Build" {
    li "Compile"  # renders 2.1
    li "Link"  # renders 2.2
  }
  li "Run" {
    list {
      # a bulleted sublist inside a numbered list
      li "Foreground"
      li "Background"
    }
  }
}
```

1. Setup
2. Build
  1. Compile
  2. Link
3. Run
  - Foreground
  - Background

## Related

- [table](../references/fact_tables_block.md)

- [callout](../references/fact_callouts.md)

[← Back to SKILL.md](../SKILL.md)
