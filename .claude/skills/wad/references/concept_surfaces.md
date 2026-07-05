# Surfaces

_Every screen, command and endpoint is a typed contract - elements, required states, interactions - defined during the PRD and delivered by exactly one spec._

The under-specification failure mode: agents pass their tests with half the screens missing, because nothing ever enumerated the screens. A `surface` block closes that gap: purpose, entry, a textual layout, every element, every interaction with its outcome, and - for :screen kinds - all four standard states (empty/loading/error/populated, enforced by the surface_states gate). Kinds cover :screen, :command and :endpoint.

Coverage is exactly-one: each surface is implemented by exactly one spec via `implements`, and the brief automatically carries the full contract of every surface it implements, marked REQUIRED. Wireframes (wdoc wf_\* widgets) live in the surface body for the book only - they render as SVG images that weak agents cannot see, so the structured fields are the contract that reaches agents as text.

Draft surfaces before research where possible, refine them after - research often changes what a surface should look like. Walk every surface through with the user before spec breakdown.

## Related

- [Self-contained briefs](../references/concept_briefs.md)

- [Usage scenarios](../references/concept_scenarios.md)

- [The thirteen default gates](../references/fact_fact_default_gates.md)

- [Writing the PRD](../references/process_proc_write_prd.md)

[← Back to SKILL.md](../SKILL.md)
