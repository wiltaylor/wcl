# The wissue pipeline

Six stages, gated like wplan: the mini-PRD cannot close while questions are `:open` or research isn't `:done` (the template's `questions_closed` and `research_done` gates enforce this via `just check`).

## 1. Intake

Classify the request: **bug** (existing behaviour is wrong) or **feature/change** (new behaviour wanted). Identify the repo root, confirm it's a git repo with a clean-enough trunk, and pick a slug (`fix-login-timeout`, `feat-csv-export`). Then scaffold:

```console
$ ${CLAUDE_SKILL_DIR}/scripts/new-issue-plan.sh <repo-root> <slug>
$ cd <repo-root>/plans/<slug>/plan && just check
```

`just check` must be green on the empty plan before any content goes in. If it is red on the empty plan, stop — the template surgery drifted (see [template_adaptation.md](template_adaptation.md)); never add content to a red plan. The script prints whether `.tree/` is gitignored — remember the answer for stage 5.

## 2. Codebase recon

**Start from what previous plans learned.** Read `plans/project-context.md` (durable repo knowledge: commands, conventions, landmines) and `plans/capabilities/<capability>.md` for the capabilities this issue touches, if they exist — verify the entries you rely on rather than re-deriving them, and treat contradictions as findings. See [capabilities.md](capabilities.md) for the layout. Recon then fills only the gaps.

Follow [recon.md](recon.md). Create one `research` block per line of investigation in research.wcl, each pointing at a finding file `research/<id>.wcl`, imported from plan.wcl (add the literal line `import "./research/<id>.wcl"` alongside the existing imports). Minimum set for any issue: build/test/lint commands, affected-code map, conventions, and (bugs) a verbatim reproduction. Mark each `:done` only when its finding file holds everything a spec author needs — implementation agents must never research.

## 2b. External research (conditional)

Recon covers the codebase; it cannot cover technology the codebase doesn't use yet. If the change introduces a **new dependency, library, framework, external API, or pattern** the repo has no precedent for, add external research items alongside the recon findings — same mechanism (`research` block in research.wcl, one finding file per item in research/, imported from plan.wcl), same bar: `:done` only when a spec can copy the finding in and a weak agent needs nothing else (exact crate/package name and version, the API calls to use, integration gotchas, a minimal usage example). The `research_done` gate blocks the mini-PRD either way, so nothing changes downstream.

Skip this entirely when the change stays within the repo's existing technology — that's the common case, and inventing filler research items is as bad as skipping needed ones. The test: would the implementing agent have to look something up outside this repo? If yes, research it now.

## 3. Targeted interview

Write `question` blocks in questions.wcl **only for what recon could not settle**: expected behaviour where the code and the report disagree, scope boundaries ("fix just this endpoint or the shared validator?"), acceptance criteria, compatibility constraints. Ask the user; record answers verbatim; never answer for them. A bug with a clean repro and obvious expected behaviour may legitimately have zero questions — but say so rather than inventing filler.

## 4. Mini-PRD

In prd.wcl:

- `goal` — usually one, stating the user-visible outcome.
- `non_goal` — the scope fences; brownfield lives or dies here ("no refactor of the auth module", "no dependency upgrades").
- `requirement` — concrete, testable statements, `:must`/`:should` prioritised. Phrase them in EARS form — `WHEN [event] THE SYSTEM SHALL [behaviour]` — and pin behaviour the change must NOT break with the regression pattern `WHEN [condition] THE SYSTEM SHALL CONTINUE TO [existing behaviour]` (see [spec_shapes.md](spec_shapes.md)).
- `rule` — keep the template's three defaults and add the brownfield rules from [template_adaptation.md](template_adaptation.md) (match conventions, no reformatting untouched code, full suite green).

**Capability deltas.** For each capability the change touches, record what it ADDS/MODIFIES/REMOVES in `plans/<slug>/capability-deltas.md` per [capabilities.md](capabilities.md) — the delta wording mirrors the prd.wcl requirements. The user approves the deltas together with the mini-PRD.

When resolving the signoffs later: `research` is `:done` when recon (and any external research) completed — mark it `:not_applicable` only for the rare change needing neither.

**Data models (conditional, wplan 1.2.0+).** If the change touches stored or shared data, define or update the affected `model` blocks (fields, validation, persistence — recon captures the current shape; record it as it will be after the change) and set `defines_models`/`uses_models` on the specs. A change that touches no data needs none.

**Surfaces and scenarios (conditional, wplan 1.1.0+).** If the change adds or alters anything user-facing: define or update the affected `surface` blocks in surfaces.wcl (purpose, entry, layout, every element and interaction; all four states for screens — recon should have captured how the existing surface behaves, so an *alteration* records the surface as it will be after the change) and write at least one `scenario` in scenarios.wcl exercising the changed behaviour end to end. A pure backend fix needs neither — the empty files pass the gates vacuously. Do not define surfaces the change doesn't touch; the exactly-one-implementing-spec gate would then force phantom specs for them.

Present the mini-PRD, any surfaces/scenarios, and the intended spec breakdown to the user together — one approval covers all. Do not proceed while any question is `:open`.

## 5. Spec DAG

Author specs per [spec_shapes.md](spec_shapes.md): one file per spec under specs/, an import line in plan.wcl (`import "./specs/<spec_id>.wcl"`) and a status row in status.wcl (`status <spec_id> { state = :todo }`) each. Set `covers = [req_ids...]` on every spec — the `requirements_covered` gate (wplan 1.6.0+) blocks rendering while any `:must` requirement has no covering spec. If the scaffold script reported `.tree/` NOT gitignored (or no green baseline command exists), start the DAG with the conditional `spec_000_prep`; otherwise the first real spec has `depends_on = []`. A typical bug is one spec; features decompose along ownership lines exactly as in wplan. If a spec implements a surface, list it in `implements` (exactly one spec per surface); pin any API the change exposes to other code in a `contract`; set a `harness` on specs with surfaces or walkthroughs. Resolve the six signoffs (most brownfield fixes: several `:not_applicable` with a one-line why — that is the point, the skip is recorded). `just check-full` green before rendering.

## 6. Analyze, render and hand off

**Analyze before rendering** — gates verify structure, this pass verifies sense. Read the whole plan in one sitting and check: no unmeasurable adjectives in requirements or accept checks (fast/secure/robust without a metric — give each a number, an EARS clause, or a command); the same thing has the same name in PRD, findings and specs; no spec does significant work no requirement asked for; no requirement or non-goal contradicts a spec. Fix findings at the source (plan .wcl files), then `just check` again.

```console
$ just render        # check + book + specs
```

Confirm `out/specs/index.md` shows the waves you expect (and the final acceptance scenarios, when you defined any) and spot-read one brief for self-containedness (would a weak agent with ONLY this file succeed?). Then hand off: tell the user the plan is ready and that wbuild takes it from here —

> run wbuild against `<repo>/plans/<slug>/plan`

If wbuild later blocks on a spec and the user amends the plan, edits happen here (the .wcl files), then `just check && just specs` before wbuild resumes — never edit out/ directly.
