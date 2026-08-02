# Specs and the change workflow

_A spec is implementation detail for an AI agent to build part of the system; WAD edits are decomposed into specs by an LLM._

A **spec** is not a diff — it is the worked-out detail an AI coding agent needs to implement
\*part of the system\*: scope, instructions, acceptance criteria. The WAD stays the source of
truth for what the system \*is\*; specs are the work packages that get it built or changed.


The change workflow: edit the WAD data to describe the target state and get it reviewed. Then
an LLM (with a human shaping the cut — decomposition is hard to get right) reads the delta
between the reviewed baseline and the new state — `git diff`,
`wcl diff <rev>:wad.wcl wad.wcl`, or a `wcl wad spec` skeleton, which pins the baseline sha
and the exact entity/field change list — and writes one or more specs sized for the
implementing model.


How much decomposition depends on the implementer: a strong model can often implement directly
from the WAD and the diff; smaller models need the change broken into narrow, explicit specs.
Status tracks the work honestly: `:planning → :in_progress → :complete` (or `:abandoned`); the
merged revision becomes the next baseline.


[← Back to SKILL.md](../SKILL.md)
