# The project context file

_Durable, repo-level knowledge - stack, commands, conventions, landmines - kept at plans/project-context.md, verified (not regenerated) by each plan, and copied into briefs like any finding._

Individual plans are islands; the repo's build commands, conventions and landmines are not.
`plans/project-context.md` is the one durable context artifact per repo: tech stack and
versions, exact build/test/lint commands with their observed baseline state, naming and
structure conventions, testing patterns, and known landmines (shared files many changes touch,
generated code, uncovered areas).


Discipline: the first plan against a repo writes it from recon or from the built result; every
subsequent plan starts by READING it and verifying the parts it relies on (run the commands,
spot-check a convention) - updating entries that drifted rather than regenerating the file.
Its content reaches implementation agents only by being copied into spec bodies, like any
research finding; briefs stay self-contained.


Keep it concise and factual - it is context for planners, not documentation. If an entry
cannot be verified quickly, mark it stale rather than deleting it.


## Related

- [Self-contained briefs](../references/concept_briefs.md)

[← Back to SKILL.md](../SKILL.md)
