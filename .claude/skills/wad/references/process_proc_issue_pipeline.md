# The issue pipeline

## Purpose

Turn a change request against an existing codebase into a real wplan plan folder that build mode executes unchanged - six stages, gated like plan mode.

## Prerequisites

- An existing git repository with a clean-enough trunk
- wcl and just on PATH
- The user has described the bug or feature

## Flowchart

![diagram](../_wdoc/process_proc_issue_pipeline-diagram-1.svg)

## Steps

### Step 1: Intake

```console
$ <skill>/scripts/new-issue-plan.sh <repo-root> <slug>
$ cd <repo-root>/plans/<slug>/plan && just check
```

Classify the request: **bug** (existing behaviour is wrong) or **feature/change** (new behaviour wanted). Identify the repo root, confirm it is a git repo, and pick a kebab slug (`fix-login-timeout`, `feat-csv-export`). Scaffold with the bundled script - it runs `wcl init wplan` and performs the brownfield surgery (see the template-adaptation fact). `just check` must be green on the empty plan before any content goes in; if it is red, the surgery drifted - stop and investigate, never add content to a red plan. The script prints whether `.tree/` is gitignored - remember the answer for the spec-DAG stage.

### Step 2: Codebase recon

Start from what the repo already knows: the living WAD (`.wad/`) if one exists, `plans/project-context.md`, and `plans/capabilities/<capability>.md` for the capabilities this issue touches - verify the entries you rely on rather than re-deriving them, and treat contradictions as findings. Then follow the recon runbook: one `research` block per line of investigation in research.wcl, each pointing at a finding file `research/<id>.wcl` imported from plan.wcl. Minimum set for any issue: build/test/lint commands, affected-code map, conventions, and (bugs) a verbatim reproduction. Mark each `:done` only when its finding holds everything a spec author needs - implementation agents must never research.

### Step 3: External research (conditional)

Recon covers the codebase; it cannot cover technology the codebase doesn't use yet. If the change introduces a **new dependency, library, framework, external API, or pattern** the repo has no precedent for, add external research items alongside the recon findings - same mechanism, same bar: `:done` only when a spec can copy the finding in and a weak agent needs nothing else (exact package name and version, the API calls to use, integration gotchas, a minimal usage example). Skip this entirely when the change stays within the repo's existing technology - inventing filler research items is as bad as skipping needed ones. The test: would the implementing agent have to look something up outside this repo? If yes, research it now.

### Step 4: Targeted interview

Write `question` blocks in questions.wcl **only for what recon could not settle**: expected behaviour where the code and the report disagree, scope boundaries ("fix just this endpoint or the shared validator?"), acceptance criteria, compatibility constraints. Ask the user; record answers verbatim; never answer for them. A bug with a clean repro and obvious expected behaviour may legitimately have zero questions - but say so rather than inventing filler.

### Step 5: Mini-PRD

In prd.wcl: `goal` (usually one, the user-visible outcome); `non_goal` (the scope fences - brownfield lives or dies here: "no refactor of the auth module", "no dependency upgrades"); `requirement` (concrete, testable, `:must`/`:should` prioritised, EARS-phrased - pin behaviour the change must NOT break with `WHEN [condition] THE SYSTEM SHALL CONTINUE TO [existing behaviour]`); `rule` (keep the template's three defaults and add the brownfield rules from the template-adaptation fact: convention-matching, no reformatting untouched code, full suite green).

**Capability deltas.** For each capability the change touches, record what it ADDS/MODIFIES/REMOVES in `plans/<slug>/capability-deltas.md` (see the capability-specs concept) - the delta wording mirrors the prd.wcl requirements.

**Data models (conditional).** If the change touches stored or shared data, define or update the affected `model` blocks (recon captures the current shape; record it as it will be after the change) and set `defines_models`/`uses_models` on the specs. **Surfaces and scenarios (conditional).** If the change adds or alters anything user-facing, define or update the affected `surface` blocks (all elements, states, interactions; all four states for screens) and write at least one `scenario` exercising the changed behaviour end to end. A pure backend fix needs neither - the empty files pass the gates vacuously; do not define surfaces the change doesn't touch, or the exactly-one-implementing-spec gate forces phantom specs.

Present the mini-PRD, any surfaces/scenarios, the capability deltas, and the intended spec breakdown to the user together - one approval covers all. Do not proceed while any question is `:open`. When resolving signoffs later: `research` is `:done` when recon (and any external research) completed - `:not_applicable` only for the rare change needing neither.

### Step 6: Spec DAG

Author specs per the brownfield spec-shapes concept: one file per spec under specs/, an import line in plan.wcl and a status row in status.wcl each. Set `covers = [req_ids...]` on every spec - the `requirements_covered` gate blocks rendering while any `:must` requirement has no covering spec. If the scaffold script reported `.tree/` NOT gitignored (or no green baseline command exists), start the DAG with the conditional `spec_000_prep`; otherwise the first real spec has `depends_on = []`. A typical bug is one spec; features decompose along ownership lines exactly as in plan mode. If a spec implements a surface, list it in `implements`; pin any API the change exposes to other code in a `contract`; set a `harness` on specs with surfaces or walkthroughs. Resolve the six signoffs (most brownfield fixes: several `:not_applicable` with a one-line why - that is the point, the skip is recorded). `just check-full` green before rendering.

### Step 7: Analyze, render and hand off

```console
$ just render        # check + book + specs
```

Run the analyze pass first (the plan-mode analyze runbook applies unchanged) - gates verify structure, the pass verifies sense. Confirm `out/specs/index.md` shows the waves you expect and spot-read one brief for self-containedness. Then hand off to build mode against `<repo>/plans/<slug>/plan`. If build mode later blocks on a spec and the user amends the plan, edits happen here (the .wcl files), then `just check && just specs` before the build resumes - never edit out/ directly. When the build completes, its completion procedure folds knowledge back: capability deltas merge into `plans/capabilities/`, and the repo's living WAD gets its update sweep.

> [!TIP]
> **Verification**
>
> just check-full green; out/specs/ rendered; the user approved the mini-PRD and deltas; build mode can run the plan unchanged.

[← Back to SKILL.md](../SKILL.md)
