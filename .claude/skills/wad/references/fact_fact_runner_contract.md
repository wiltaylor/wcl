# The runner contract and .wbuild/

Build mode is agnostic about \*what\* executes a brief - headless Claude Code, OpenCode, a local model behind a wrapper, anything. The orchestrator talks to it through one script with a fixed contract, or falls back to its own Task subagents when no runner is configured.

```text
.wbuild/
  config            # KEY=VALUE, shell-sourceable
  run-agent.sh      # the runner (optional — Task fallback if absent)
  prompts/          # orchestrator writes one prompt file per dispatch
  reports/          # verifier/reviewer write reports here
  logs/             # runner stdout/stderr per dispatch
```

Add `.wbuild/prompts/`, `.wbuild/reports/`, `.wbuild/logs/` to .gitignore (the bundled init script does this); config and run-agent.sh may be committed.

| Key | Default | Meaning |
| --- | --- | --- |
| TRUNK | main | Branch specs merge into |
| RUNNER | .wbuild/run-agent.sh | Runner script; if the file doesn't exist, use Task subagents |
| MAX_PARALLEL | 4 | Max concurrently running implementer/fixer agents |
| MAX_FIX_ATTEMPTS | 3 | Fix cycles per spec before :blocked |
| POST_MERGE_CHECK | (empty) | Command run in repo root on trunk after each merge; empty = re-run the merged spec's accept commands on trunk |
| KEEP_BRANCHES | false | Keep spec/\* branches after merge |

**The script contract**: `.wbuild/run-agent.sh <role> <worktree-path> <prompt-file>`. `role` is implementer or fixer (a runner may pick different models per role; verification normally runs in-session but MAY use role verifier; review dispatches with role reviewer and MUST route to a strong model). `worktree-path` is absolute; the agent must treat it as its whole world. `prompt-file` is absolute; the runner passes its \*contents\* to the agent. Semantics: the script **blocks** until the agent finishes and exits 0 when the agent ran to completion, non-zero when the agent itself crashed or timed out - exit 0 does NOT mean the work is correct; only verification decides that. The orchestrator backgrounds one runner invocation per spec (up to MAX_PARALLEL), captures output to `.wbuild/logs/<spec>-<attempt>.log`, and waits. Example runners ship in this skill's assets: `run-agent.example.sh` (annotated template) and `run-agent.claude.sh` (headless Claude Code sketch - verify its CLI flags against the installed version before relying on it).

**Task-subagent fallback**: when no runner script exists, dispatch each implementer/fixer as a Task subagent. If the suite's agent files are installed in .claude/agents/ (wbuild-implementer, wbuild-fixer, wbuild-verifier, wbuild-reviewer, wbuild-scenario-runner), dispatch by those names - their system prompts already carry the role rules, so the per-dispatch prompt only needs the payload (brief, worktree path, and for fixers the failure report). Without them: prompt = the assembled prompt file's contents, and instruct the subagent that its working directory is the worktree and it must not read or write outside it (subagents are not filesystem-jailed - the prompt is the fence, which is exactly why the verifier re-checks owns/not_allowed). The fallback burns orchestrator-session context and uses the session's own model for every role; prefer a runner script for long plans or when you want weak/cheap models implementing.

## Related

- [The build roles](../references/fact_fact_roles.md)

- [Dispatching implementation](../references/process_proc_dispatch.md)

- [The orchestrator loop](../references/process_proc_orchestrator_loop.md)

[← Back to SKILL.md](../SKILL.md)
