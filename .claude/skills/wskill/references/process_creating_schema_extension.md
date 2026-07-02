# Creating a schema extension

## Purpose

Model topic-specific recurring data as its own typed blocks and project it into generated pages.

## Prerequisites

- The recurring shape genuinely doesn't fit concept/entity/fact/process — check the decision guide first.

## Flowchart

![diagram](../_wdoc/process_creating_schema_extension-diagram-1.svg)

## Steps

### Step 1: Declare the blocks + a merging @document

```wcl
// schema/extensions.wcl (or a dedicated module file)
namespace wcl.wskill

@block("keybinding")
type Keybinding {
  @inline(0) id: identifier
  keys:    utf8
  action:  utf8
  context: utf8?
}

@document
type KeybindingDoc {
  @children("keybinding") keybindings: list<Keybinding>
}
```

Declare an `@block` per shape (fields typed, `@inline(0) id` first) plus an `@document` gathering it. Imported `@document`s merge with the base, so the new list appears alongside `concepts`/`facts`. Keep it in `schema/extensions.wcl`, or in its own `schema/<area>.wcl` module for a large surface (the wcl wskill's `builtins.wcl` / `cli.wcl` pattern).

### Step 2: Author instances in data/

```wcl
// data/reference/keybindings.wcl
keybinding save   { keys = "Ctrl+S"  action = "Save the buffer" }
keybinding search { keys = "Ctrl+F"  action = "Find in file"    context = "editor" }
```

Write instances like any other unit; import the file from `wskill.wcl` (or the data aggregator). `wcl check` now validates them against your schema.

### Step 3: Add a render to every template set

```wcl
// wdoc/book/main.wcl and wdoc/skill/main.wcl (and pages/ where it fits)
wdoc_repeater { each = keybindings  as = :k
  p $"`${k.keys}` — ${k.action}"
}
```

Project the gathered list in BOTH the book and the skill templates — a table, a page per instance, or a `wdoc_component` for a full house-style page body (see [Components](../references/concept_components_look_feel.md)). Extension data that renders in only one view drifts.

> [!TIP]
> **Verification**
> `just wskill-check` passes with instances present, and both the book and the skill show the new section on the next render.

## Related

- [Custom projections (schema extension modules)](../references/concept_custom_projection.md)

- [Structured data](../references/concept_structured_data.md)

- [Components: one look for every unit](../references/concept_components_look_feel.md)

- [Which unit kind? — the decision guide](../references/fact_unit_decision_guide.md)

[← Back to SKILL.md](../SKILL.md)
