# The build roles

The current session is the **orchestrator**: it never writes application code. It schedules
work to sub-agents in waves over the spec DAG, has a verification agent judge each implemented
spec, a strong-model reviewer judge each verified spec's code quality, dispatches a fix agent
when either fails, and merges reviewed branches back into trunk in dependency order.


| Role | Runs as | Sees | Never sees |
| --- | --- | --- | --- |
| Orchestrator | this session | everything | - |
| Implementer | sub-agent via runner | its own brief + its worktree | status.wcl, plan/, other briefs |
| Verifier | sub-agent (or orchestrator-run procedure) | plan/, worktrees, verification procedure | - |
| Reviewer | strong-model sub-agent via runner (role reviewer) | plan/, worktrees, diff, review procedure | - |
| Fixer | sub-agent via runner | its brief + failure report + its worktree | status.wcl, plan/, other briefs |
| Scenario runner | sub-agent | one scenario + harness + repo root on trunk | status.wcl, plan/, briefs |

Status writes are split by role - the invariant is that \*implementation-side agents never
touch or hear about status.wcl\*. The authoritative who-writes-which-transition table is the
states fact. The reviewer must be a strong model - never route review to the same cheap model
that implemented.


[← Back to SKILL.md](../SKILL.md)
