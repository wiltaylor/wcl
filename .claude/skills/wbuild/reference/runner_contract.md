# Runner contract

wbuild is agnostic about *what* executes a brief — headless Claude Code, OpenCode, a local model behind a wrapper, anything. The orchestrator talks to it through one script with a fixed contract, or falls back to its own Task subagents when no runner is configured.

## `.wbuild/` in the target repo

```text
.wbuild/
  config            # KEY=VALUE, shell-sourceable
  run-agent.sh      # the runner (optional — Task fallback if absent)
  prompts/          # orchestrator writes one prompt file per dispatch
  reports/          # verifier writes failure reports here
  logs/             # runner stdout/stderr per dispatch
```

Add `.wbuild/prompts/`, `.wbuild/reports/`, `.wbuild/logs/` to .gitignore (init script does this); `config` and `run-agent.sh` may be committed.

## config keys and defaults

| Key | Default | Meaning |
| --- | --- | --- |
| TRUNK | main | Branch specs merge into |
| RUNNER | .wbuild/run-agent.sh | Runner script; if the file doesn't exist, use Task subagents |
| MAX_PARALLEL | 4 | Max concurrently running implementer/fixer agents |
| MAX_FIX_ATTEMPTS | 3 | Fix cycles per spec before `:blocked` |
| POST_MERGE_CHECK | (empty) | Command run in repo root on trunk after each merge; empty → re-run the merged spec's accept commands on trunk |
| KEEP_BRANCHES | false | Keep `spec/*` branches after merge |

## The script contract

```console
$ .wbuild/run-agent.sh <role> <worktree-path> <prompt-file>
```

- `role` — `implementer` or `fixer` (a runner may pick different models per role; verification normally runs in-session, but a runner MAY be used for it too, with role `verifier`; review dispatches with role `reviewer` and MUST route to a strong model).
- `worktree-path` — absolute path; the agent must treat this as its working directory and touch nothing outside it.
- `prompt-file` — absolute path to the fully assembled prompt (brief + wrapper, and for fixers the failure report). The runner passes its *contents* to the agent.

Semantics: the script **blocks** until the agent finishes and exits `0` when the agent ran to completion, non-zero when the agent itself crashed/timed out. Exit 0 does **not** mean the work is correct — only verification decides that. The orchestrator backgrounds one runner invocation per spec (up to MAX_PARALLEL), captures output to `.wbuild/logs/<spec>-<attempt>.log`, and waits.

Example runners ship in this skill's `assets/`:

- `run-agent.example.sh` — annotated template to adapt.
- `run-agent.claude.sh` — headless Claude Code sketch built around `claude -p` with permissions relaxed and cwd set to the worktree. **Verify the exact CLI flags against the installed Claude Code version before relying on it** — flags here may be stale.

## Task-subagent fallback

When no runner script exists, dispatch each implementer/fixer as a Task subagent instead. If the wcl-suite agent files are installed in `.claude/agents/` (wbuild-implementer, wbuild-fixer, wbuild-verifier, wbuild-reviewer, wbuild-scenario-runner), dispatch by those names — their system prompts already carry the role rules, so the per-dispatch prompt only needs the payload (brief, worktree path, and for fixers the failure report). Without them:

- Prompt = the same assembled prompt file's contents.
- Instruct it that its working directory is the worktree and it must not read or write outside it (subagents are not filesystem-jailed — the prompt is the fence, which is exactly why the brief's `owns`/`not_allowed` lists get re-checked by the verifier).
- Launch the wave's subagents in parallel; each one reporting back = that runner "exiting 0".

The fallback burns orchestrator-session context and uses the session's own model for every role; prefer a runner script for long plans or when you want weak/cheap models implementing.
