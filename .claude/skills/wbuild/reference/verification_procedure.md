# Verification procedure

Run once per `:implemented` spec. The verifier is the only role allowed to record verdicts, and it never trusts the implementer's self-report — every check is re-run from scratch. By default the orchestrator executes this procedure itself (it is judgement + commands, not code-writing, so it doesn't violate the no-coding boundary); for extra independence it may instead be dispatched through the runner with role `verifier`, giving the agent this procedure, the spec's brief, the worktree path, and the plan folder.

## Inputs

- The spec's brief (`plan/out/specs/<spec>.md`) — the contract being judged.
- The spec's source block (`plan/specs/<spec>.wcl`) — authoritative `owns`, `allowed`, `not_allowed`, `done`, `accept`.
- The worktree at `.tree/<branch>`.

## Checks, in order — fail fast, but keep collecting findings for the report

1. **Commits exist.** `git log <TRUNK>..<branch>` is non-empty.
2. **Ownership respected.** `git diff --name-only <TRUNK>...<branch>` — every changed path must fall under the spec's `owns` (or an explicit `allowed` entry). Remember the owns gate is exact-string at plan time; here you check real paths, so directory prefixes DO count. Any path outside → fail, and note it as an ownership violation (these are the most important findings to feed back into lessons.wcl).
3. **not_allowed respected.** Diff shows none of the forbidden changes (e.g. new dependencies beyond those permitted — check manifest diffs like Cargo.toml/csproj/package.json explicitly).
4. **Acceptance checks pass.** For each `accept` in the brief: if it has a command, run it in the worktree exactly as written, capturing output — all must succeed. If it is marked `(manual walkthrough)` (no command) or carries numbered walkthrough steps, launch the application using the brief's "How to run this spec's work" harness line (command, stubs, seed data — wplan 1.2.0+; if absent on an older plan, ask the orchestrator/user how to run it rather than improvising) and execute the steps in order, checking each stated expectation against what actually happens. A walkthrough step whose expectation does not hold is a failure with the step quoted verbatim in the report.
5. **Interface contracts honoured (wplan 1.2.0+).** If the brief has "Contract you MUST provide" sections, the exact signatures exist in the code — names, parameter types, return types, error shapes verbatim. If it has "Contract you consume" sections, the diff calls through those signatures and does not reach past them into the providing spec's internals, and does not reimplement them.
6. **Surface contracts complete.** For every "Surface to implement" section in the brief (wplan 1.1.0+): each listed element exists, each interaction produces its stated outcome, and — for screens — each of the four states (empty/loading/error/populated) is demonstrably reachable and matches its `shows` text. Exercise them (empty data dir, kill the backend for error, etc.); do not accept the implementer's word. A missing state or element fails verification even when every test passes — this check exists precisely because tests pass on half-finished screens.
7. **Done list holds.** For each `done` item, verify it concretely (run a command, read the file) — not by trusting AGENT_NOTES.md.
8. **Worktree hygiene.** `git status` clean in the worktree (everything committed).

## Verdict

**Pass** — edit the spec's row in `plan/status.wcl`:

```wcl
status <spec_id> { state = :verified  by = "verifier"  note = "all accepts green" }
```

Also record an as-built row in `plan/asbuilt.wcl` (wplan 1.2.0+) for any deviation from the brief that downstream specs must know — renamed module, changed command, adjusted signature. No deviations → no row. As-built rows are rendered into dependent briefs on the next `just specs`, which the orchestrator runs before each dispatch.

Then `just check` in the plan folder to confirm the ledger still validates.

**Fail** — write `.wbuild/reports/<spec>-attempt-<N>.md`:

```markdown
# Verification failure: <spec_id> (attempt N)

## What failed
- <check>: <exact command run> → <exact error/output excerpt>
- <ownership violation: path X changed but is owned by spec_Y>

## What passed
- <so the fixer doesn't re-litigate working parts>

## Required to pass
- <concrete, minimal statements of what must change>
```

Set the row back to `:in_progress` with `by = "verifier"` and a one-line note pointing at the report. The report must be self-contained and literal — the fixer may be a weak model and cannot investigate beyond what the report and brief say.
