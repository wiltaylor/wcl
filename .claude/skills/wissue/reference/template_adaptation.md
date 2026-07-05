# Template adaptation

`scripts/new-issue-plan.sh` extracts wplan's verified template (via the wplan skill's own `scripts/new-plan.sh`) and performs deterministic brownfield surgery. This page documents what it does — so you can sanity-check the result — and the additions you make by hand.

## What the script does

1. Locates the wplan skill (in order: `--wplan-dir` flag, `$WPLAN_SKILL_DIR`, `~/.claude/skills/wplan`) and runs its `new-plan.sh` into `<repo>/plans/<slug>/`, yielding `plans/<slug>/plan/`.
2. Deletes the greenfield bootstraps: `specs/spec_000_repo.wcl` and `specs/spec_010_build.wcl`.
3. Removes exactly their import lines from plan.wcl and their status rows from status.wcl. The script greps for each expected line **before** removing it and aborts loudly if any is missing — that means the upstream template has drifted from what this skill was written against; re-verify the surgery by hand and update this skill.
4. Reports whether `.tree/x` is git-ignored in the target repo (`git check-ignore`), which decides the conditional prep spec (see spec_shapes.md).

It never touches schema/, gates.wcl, the justfile, or the wdoc projections — the brief renderer derives waves from whatever specs exist, so removing specs is safe.

After the script: `just check` in the new plan folder must pass with zero specs. If it doesn't, stop and investigate before adding content.

## What you add by hand

**Brownfield rules** in prd.wcl, alongside the template's three defaults (rule_commits, rule_no_deps, rule_scope) — these are copied verbatim into every brief:

```wcl
rule rule_conventions { text = "Match the surrounding code's existing conventions exactly — naming, error handling, test structure. Do not introduce new patterns." }
rule rule_no_reformat { text = "Never reformat, reorder, or 'clean up' code you are not functionally changing." }
rule rule_suite_green { text = "The project's full existing test suite must pass before you finish. If a pre-existing test fails for reasons unrelated to your change, STOP and report it in AGENT_NOTES.md rather than fixing it." }
```

Adjust wording to the repo (e.g. name the actual suite command) but keep the three intents: convention-matching, no drive-by changes, whole-suite green.

**Requirement style**: write prd.wcl requirements in EARS form (`WHEN [event] THE SYSTEM SHALL [behaviour]`), and pin must-not-break behaviour with `WHEN [condition] THE SYSTEM SHALL CONTINUE TO [existing behaviour]` — see spec_shapes.md.

**Everything else** — questions, research + finding files, goals/non-goals/requirements, specs, status rows — is normal wplan authoring, covered by the pipeline and spec_shapes pages. One import line in plan.wcl per new spec file and per finding file; one status row per spec.

## Verification note

The surgery in this skill was written and tested against wplan's template as shipped in wplan skill version 1.0.0 (template contents inspected directly). It was **not** validated with `wcl check` at authoring time — the authoring environment had no wcl binary — so the first `just check` on a freshly scaffolded plan is the real test. If wplan's template moves, expect step 3's drift guard to fire first.
