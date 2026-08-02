# Usage scenarios

_End-to-end action/expect walks through the finished application - the system-level definition of done that survives all the per-spec merges._

Per-spec acceptance proves parts; scenarios prove the whole. Each `scenario` block captures a
user goal, the surfaces it touches, and ordered action/expect steps. Every surface must appear
in at least one scenario (scenario_coverage gate), which forces the planning conversation to
explain how each surface actually gets used.


Scenarios render into out/specs/index.md as the final acceptance section: after the last wave
merges, the verification agent executes every scenario end to end on trunk. The project is
done when all scenarios pass - not when the last spec merges.


## Related

- [Surfaces](../references/concept_surfaces.md) — Surfaces supports Usage scenarios: Every screen, command and endpoint is a typed contract - elements, required states, interactions - defined during the PRD and delivered by exactly one spec.

[← Back to SKILL.md](../SKILL.md)
