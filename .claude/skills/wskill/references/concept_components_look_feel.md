# Components: one look for every unit

_The wdoc components under wdoc/component/ render every unit of a kind identically across projections — page structure lives in exactly one place._

Every unit page in every wskill looks the same because its structure is rendered by ONE
`wdoc_component`, shared by all projections. `wdoc/component/` holds one component per
unit kind — `concept_body`, `entity_body`, `fact_body`, `process_body` — plus shared
helpers (`unit_examples`, `related_section`, backlinks) and `skill_md.wcl`, the canonical
SKILL.md body both the skill build and the book's skill preview render (so the preview
can never drift from the shipped file).


A component declares `wdoc_slot`s and a `wdoc_body`; the projection templates invoke it
per unit (`concept_body { unit = c  examples = examples … }`). The book and the skill
pass different slot values — the book adds backlinks and two-column layout, the skill a
back-to-index footer — but the page \*structure\* lives in exactly one place. Change the
component and every page of that kind changes everywhere, in every view.


The same mechanism gives [custom projections](../references/concept_custom_projection.md) a native look:
a new domain kind gets its own `<kind>_body` component next to the built-in ones, and its
generated pages inherit the house style — heading, summary line, projected body,
examples, related links — for free.


## Examples

### One component, every projection

The book and the skill invoke the same component with different slot values — page structure lives once.

```wcl
// wdoc/book/main.wcl
concept_body { unit = c  examples = examples  all_units = all_units
               backlinks = referenced_by(c.id)  layout = "columns" }

// wdoc/skill/main.wcl — same component, skill-flavoured slots
concept_body { unit = c  examples = examples  all_units = all_units  backlinks = []
  p "[← All concepts](concepts_ref) · [← Back to SKILL.md](index)"
}
```

## Related

- [Separation of Data and Presentation](../references/concept_datapresentation.md)

- [Custom projections (schema extension modules)](../references/concept_custom_projection.md)

- [The view family](../references/concept_views.md)

[← Back to SKILL.md](../SKILL.md)
