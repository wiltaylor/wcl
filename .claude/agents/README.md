# wcl-suite agent files

Claude Code subagent definitions for the wplan / wissue / wbuild pipeline.

Install: copy these .md files into the target repo's `.claude/agents/`
(project scope, committable) or `~/.claude/agents/` (all projects). Subagent
files load at session start — restart Claude Code after installing or editing.

| Agent | Dispatched by | Prompt payload it expects |
| --- | --- | --- |
| wbuild-implementer | wbuild wave dispatch | the assembled prompt: brief + worktree path |
| wbuild-fixer | wbuild fix loop | failure report + brief + worktree path |
| wbuild-verifier | wbuild verify stage | spec id, brief, spec .wcl, worktree, plan folder, trunk |
| wbuild-reviewer | wbuild review stage (post-verify, pre-merge) | spec id, brief, worktree, plan folder, trunk |
| wbuild-scenario-runner | wbuild post-merge / completion | one scenario + harness + repo root |
| wplan-researcher | wplan research phase | one research item + plan folder |
| wissue-recon | wissue stage 2 | repo root + plan folder + issue description |

Design: the agent file is the standing role constitution; the orchestrator's
prompt carries the per-task payload. Subagents receive ONLY their own system
prompt (not the session's), so each file is self-contained. The implementer
and fixer files deliberately never mention status.wcl — preserving wbuild's
invariant that implementation-side agents don't know the ledger exists.

Model fields default to `sonnet` for the implementer, verifier and scenario
runner, `opus` for the fixer (a failed attempt means the cheap model already
struggled - retrying with the same strength burns fix attempts) and the reviewer
(it exists precisely to be a stronger model than the implementer - do not
downgrade it to the implementing model), and `inherit` for research/recon;
edit per taste. These agents are the "Task-subagent fallback"
path in wbuild's runner contract made concrete — a run-agent.sh runner
remains the way to use non-Claude executors.
