# callout

A `callout` is an admonition: an icon, a coloured heading, and a body. Six built-in types are selected by `class`, each shipping a default colour and icon — `note`, `info`, `tip`, `warning`, `error`, `success`.

```wcl
callout "Note" {
  class = ["note"]
  body = "Background context the reader should remember."
}
callout "Info" {
  class = ["info"]
  body = "Neutral information worth surfacing."
}
callout "Tip" {
  class = ["tip"]
  body = "A helpful shortcut or best practice."
}
callout "Warning" {
  class = ["warning"]
  body = "Something to be careful about."
}
callout "Error" {
  class = ["error"]
  body = "A failure or hard constraint."
}
callout "Success" {
  class = ["success"]
  body = "Confirm an action completed."
}
```

> [!NOTE]
> **Note**
> Background context the reader should remember.

> [!NOTE]
> **Info**
> Neutral information worth surfacing.

> [!TIP]
> **Tip**
> A helpful shortcut or best practice.

> [!WARNING]
> **Warning**
> Something to be careful about.

> [!CAUTION]
> **Error**
> A failure or hard constraint.

> [!TIP]
> **Success**
> Confirm an action completed.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `heading` | `utf8` | yes | Inline label — the coloured heading at the top of the callout. |
| `body` | `utf8` | yes | The prose under the heading. Runs through the inline-pattern engine. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Selects the type: `["note"]` / `["tip"]` / etc. May also carry user classes. |
| `icon` | `utf8` | no | Override the default icon (any `pack.name` from a built-in or declared iconset). |

## The six classes

| Class | Use |
| --- | --- |
| note | Background context the reader should remember |
| info | Neutral information worth surfacing |
| tip | A helpful shortcut or best practice |
| warning | Something to be careful about |
| success | Confirm an action completed |
| error | A failure or hard constraint |

## Custom types

For a custom type, give a `class` an `accent` colour and list it in the callout's `class` — that sets the accent (heading, border, icon) with no CSS — and the `icon` field picks any glyph. See [styling](../references/concept_styling.md).

```wcl
callout "Deploying" {
  class = ["deploy"]
  icon = "lucide.rocket"
  body = "A **custom** type — the `deploy` class sets its accent colour, and `icon` picks the glyph."
}
```

> [!NOTE]
> **Deploying**
> A **custom** type — the `deploy` class sets its accent colour, and `icon` picks the glyph.

```wcl
class "deploy" { accent = "#b48ead" }
callout "Deploying" {
  class = ["deploy"]
  icon  = "lucide.rocket"
  body  = "A custom type — the class sets its accent colour."
}
```

## Examples

### The six callout types

Each built-in admonition type is selected by its class, shipping a default colour and icon.

```wcl
callout "Note"    { class = ["note"]    body = "Background context the reader should remember." }
callout "Tip"     { class = ["tip"]     body = "A helpful shortcut or best practice." }
callout "Warning" { class = ["warning"] body = "Something to be careful about." }
callout "Error"   { class = ["error"]   body = "A failure or hard constraint." }
```

**Expected:** Four admonitions, each with its type's icon and accent colour.

## Related

- [table](../references/fact_tables_block.md)

- [list / li](../references/fact_lists_block.md)

- [Formatting](../references/concept_formatting.md)

[← Back to SKILL.md](../SKILL.md)
