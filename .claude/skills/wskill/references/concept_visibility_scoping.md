# Visibility scoping (@only / @except)

_Gate individual blocks inside a body per site, template, or backend — finer-grained than the unit-level audience field._

Where `audience` selects whole units, wdoc's `@only` / `@except` decorators gate a single
block \*inside\* a body. Any block can carry them, with three optional axes — `sites`,
`templates` (`:book`, `:webpage`, `:presentation`, `:ai_skill`), and `backends` (`:html`,
`:markdown`, `:pdf`). Values within an axis OR together; axes AND together.


```wcl
body {
  p "Rendered everywhere."
  @only(templates=[:ai_skill]) callout "Agent note" { body = "Extra instruction only the skill sees." }
  @except(backends=[:pdf]) video { src = "../../assets/demo.mp4" }
}
```

Use it sparingly: a body that needs heavy per-view gating is usually two units in
disguise (one `:book`, one `:ai`). The common legitimate cases are an agent-only callout
inside shared reference material, and media that one backend can't carry.


## Related

- [Audience control](../references/concept_audience_control.md)

- [The view family](../references/concept_views.md)

[← Back to SKILL.md](../SKILL.md)
