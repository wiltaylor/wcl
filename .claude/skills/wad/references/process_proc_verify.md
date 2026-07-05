# The verification procedure

## Purpose

Judge an implemented spec against its brief from scratch - the verifier never trusts the implementer's self-report and is the only role recording verdicts.

## Prerequisites

- A spec in :implemented
- The brief, the spec's source .wcl, and the worktree at .tree/<branch>

## Flowchart

![diagram](../_wdoc/process_proc_verify-diagram-1.svg)

## Steps

### Step 1: Run the checks in order - fail fast, keep collecting findings

1. **Commits exist**: `git log <TRUNK>..<branch>` is non-empty. 2. **Ownership respected**: every path in `git diff --name-only <TRUNK>...<branch>` falls under the spec's owns (or an explicit allowed entry) - directory prefixes DO count here even though the plan-time gate is exact-string; any path outside fails, and ownership violations are the most important findings to feed lessons.wcl. 3. **not_allowed respected**: check manifest diffs (Cargo.toml, package.json, csproj) explicitly for forbidden dependency changes. 4. **Acceptance checks pass**: run each accept command in the worktree exactly as written; for accepts marked (manual walkthrough) or carrying numbered steps, launch the app using the brief's "How to run this spec's work" harness line and execute the steps in order, checking each stated expectation - a step whose expectation does not hold is a failure with the step quoted verbatim.

5. **Contracts honoured**: "Contract you MUST provide" signatures exist in the code verbatim - names, parameter types, return types, error shapes; consuming code calls through consumed contracts without reaching past them or reimplementing them. 6. **Surface contracts complete**: for every "Surface to implement" section, each listed element exists, each interaction produces its stated outcome, and each of the four screen states (empty/loading/error/populated) is demonstrably reachable and matches its shows text - exercise them (empty data dir, kill the backend for error); a missing state fails verification even when every test passes, which is precisely why this check exists. 7. **Done list holds**: verify each item concretely (run a command, read the file) - not by trusting AGENT_NOTES.md. 8. **Worktree hygiene**: `git status` clean.

### Step 2: Pass verdict

```wcl
status <spec_id> { state = :verified  by = "verifier"  note = "all accepts green" }
```

Edit the spec's row in plan/status.wcl. Also record an `asbuilt` row in plan/asbuilt.wcl for any deviation from the brief that downstream specs must know (renamed module, changed command, adjusted signature) - no deviations, no row; as-built rows render into dependent briefs on the next `just specs`. Then `just check` in the plan folder to confirm the ledger still validates.

### Step 2: Fail verdict

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

Write `.wbuild/reports/<spec>-attempt-<N>.md` and set the row back to `:in_progress` with by = "verifier" and a one-line note pointing at the report. The report must be self-contained and literal - the fixer may be a weak model and cannot investigate beyond what the report and brief say.

> [!TIP]
> **Verification**
> A verdict recorded in status.wcl (or a failure report written); just check green in the plan folder.

[← Back to SKILL.md](../SKILL.md)
