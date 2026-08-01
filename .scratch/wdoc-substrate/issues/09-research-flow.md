# How do research findings become units without manufacturing sprawl?

Type: grilling
Status: open
Blocked by: 06

## Question

`researching_a_topic` is a machine for producing exactly the problem this map exists to fix. Its
steps: scope → decompose into research items → **dispatch researchers in parallel** → completeness
gate → distill findings into units → **build the index** → verify → review.

Two structural faults:

1. **N researchers write blind to each other.** Each produces findings with no view of what the
   others found, so overlapping units, inconsistent granularity and duplicate coverage are the
   expected output, not the failure case. (`.claude/agents/wskill-researcher.md` is generated from
   the wskill's own `agent` block — the parallelism is a designed feature.)
2. **The index is built after the units exist.** Structure is retrofitted onto nodes already written,
   which is when `related` starts carrying navigation weight it shouldn't.

`adding_content` — the incremental loop — has the same ordering: decompose → classify → write →
**link** → **pin**. The unit exists before anyone asked where it belongs.

Decide how the research path changes. The charting decision was a **curator pass** rather than
outline-first authoring, so the question is how much of this the curator absorbs versus how much the
process must stop generating:

- **Does the parallel fan-out survive?** It's the reason research is fast. Options: keep it and let
  the curator clean up; keep it but give each researcher a bounded slice of an agreed skeleton; add a
  reconciliation stage between research and unit-writing; serialise.
- **Where does the index get built?** Before distillation, after, or continuously?
- **Do research findings become units at all?** `research` blocks are already first-class, already
  render to `references/research_<id>.md`, and default to `audience = :ai`. Maybe findings stay
  findings and unit-writing is a separate deliberate act.
- **What does the completeness gate check?** It currently gates coverage. Should it also gate
  *structure* — that findings map onto a shape — before any unit is written?
- **Same question for `adding_content`.** Does the incremental loop reorder, or does the curator make
  its ordering harmless?

Blocked by `06-unit-kinds`: what a unit *is* determines what distillation is aiming at.
