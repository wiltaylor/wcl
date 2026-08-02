# The thirteen default gates

| Gate | Asserts | Blocks |
| --- | --- | --- |
| questions_closed | No question has status :open | PRD while the interview is unfinished |
| research_done | Every research item is :done | PRD while research is incomplete |
| dag_acyclic | Kahn fixpoint covers every spec | Rendering a cyclic spec graph |
| owns_disjoint | No path string claimed by two specs | Parallel-unsafe ownership overlap (exact-string match only) |
| status_covered | Every spec has a status.wcl row | Specs invisible to the verification ledger |
| surface_coverage | Every surface implemented by exactly one spec | Surfaces nobody builds, or two specs building one surface |
| surface_states | Every :screen surface defines empty/loading/error/populated | Screens shipped without their standard states |
| scenario_coverage | Every surface touched by >=1 scenario | Surfaces whose real usage was never thought through |
| contract_order | Contract consumers transitively depend on providers | Building against an unmerged, imagined API |
| models_defined_once | Every model defined by exactly one spec | Two specs inventing incompatible representations |
| requirements_covered | Every :must requirement in some spec's covers list | Rendering a plan with an orphaned must-have requirement |
| harness_defined | Surface/walkthrough specs say how to run their work | Unexecutable walkthroughs mid-DAG |
| signoffs_complete | No phase signoff is :pending (check-full tier only) | Silently skipped planning phases |

Evaluate one gate: `wcl eval gates.wcl gates.<id>.ok` - exit 0 with `true` on pass, non-zero
with the gate message on failure. `just check` evaluates all of them.


## Related

- [Gates are blocks, not lets](../references/concept_gates.md)

- [File ownership](../references/concept_ownership.md)

[← Back to SKILL.md](../SKILL.md)
