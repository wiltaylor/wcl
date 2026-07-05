# The plan/ justfile recipes

| Command | Does |
| --- | --- |
| just check | Structural tier: wcl check + every gate except signoffs; green on a fresh template |
| just answer | Walk pending interview questions interactively (wcl answer plan.wcl) |
| just check-full | check + the signoffs gate; render requires this |
| just book | HTML book to out/book |
| just specs | Agent briefs to out/specs (one .md per spec + index.md) |
| just render | check-full + book + specs |
| just serve | Live book preview |
| just status <spec> | Query one spec's state (wcl eval plan.wcl statuses.<spec>.state) |

## Related

- [The gated pipeline](../references/concept_pipeline.md)

- [Rendering and handoff](../references/process_proc_render_handoff.md)

[← All facts](../references/facts_ref.md) · [← Back to SKILL.md](../SKILL.md)
