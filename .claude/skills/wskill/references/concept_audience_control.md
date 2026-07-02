# Audience control

_Every unit opts into projections via `audience`: `:book` (default), `:ai`, or `:both` — the skill is curated up, never filtered down._

Every content unit carries an `audience` field saying which projections render it:
`:book` (human docs only — the default), `:ai` (skill/agent only), or `:both`. The
default is deliberate: content stays OUT of the AI skill until opted in, so the skill is
\*curated up\* rather than filtered down — an agent gets a lean, intentional reference
instead of everything.


In practice: reference material an agent should cite gets `:both`; agent-only working
notes (task heuristics, guardrail detail) get `:ai`; background prose for human readers
keeps the `:book` default. The book renders `:book` + `:both`; the skill renders `:ai` +
`:both`. Indexes carry an audience too — an `:ai` index shapes SKILL.md's Reference
section without touching the book nav.


Audience selects whole units. To vary \*content inside one body\* per view, use
[visibility scoping](../references/concept_visibility_scoping.md) instead.


## Examples

### Opting units into the skill

The default keeps content out of the AI skill; tag what the agent needs.

```wcl
concept fast_forward { audience = :both  ... }   // book + skill
fact   port_table    { audience = :ai    ... }   // skill only
concept history      { ... }                     // :book default — book only
```

**Expected:** The skill's references/ contains fast_forward and port_table but not history; the book shows fast_forward and history.

## Related

- [The view family](../references/concept_views.md)

- [Anatomy of the AI skill](../references/concept_skill_anatomy.md)

- [Visibility scoping (@only / @except)](../references/concept_visibility_scoping.md)

[← Back to SKILL.md](../SKILL.md)
