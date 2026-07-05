# wcl diff (WAD usage)

_command_

Diffs two WAD revisions over the evaluated views — the seam the change workflow builds on.

`wcl diff <rev>:wad.wcl wad.wcl` compares the \*evaluated\* documents: the git side materialises that revision's whole tree (so imports — including committed generated data — resolve from it), and formatting-only churn produces no diff. Entities key as `kind:label`; field edits report by path with old/new values; `--format json` for tooling.

For change management it is one of the delta sources the spec-writing LLM reads (alongside `git diff`, and seeded by `wcl wad spec --from <rev>`), and it is the fast answer to “what changed since the review?”. General diff semantics live in the wcl wskill.

## Related

- [Specs and the change workflow](../references/concept_spec_lifecycle.md)

- [WAD command table](../references/fact_wad_cli.md)

[← Back to SKILL.md](../SKILL.md)
