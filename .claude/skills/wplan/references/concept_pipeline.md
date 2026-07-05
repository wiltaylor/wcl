# The gated pipeline

_Five phases - interview, research, PRD, spec breakdown, render - each blocked by checkable gates._

A plan moves through five phases: **interview** (questions.wcl), **research** (research.wcl plus one finding file per item), **PRD** (prd.wcl), **spec breakdown** (specs/\*.wcl), and **render** (out/). Progress is gated: `just check` runs `wcl check` plus every gate in gates.wcl, and a failing gate blocks rendering.

Gates catch what is recorded but unfinished (an open question, incomplete research, a dependency cycle). Process rules cover what gates cannot express - a gate cannot force an interview to have \*happened\*, only that recorded items are closed. Both matter: follow the workflow procedures in order and keep the gates green.

## Related

- [Gates are blocks, not lets](../references/concept_gates.md)

- [Running the interview](../references/process_proc_interview.md)

- [Doing the research](../references/process_proc_research.md)

- [Writing the PRD](../references/process_proc_write_prd.md)

- [Breaking down the specs](../references/process_proc_spec_breakdown.md)

- [Rendering and handoff](../references/process_proc_render_handoff.md)

[← All concepts](../references/concepts_ref.md) · [← Back to SKILL.md](../SKILL.md)
