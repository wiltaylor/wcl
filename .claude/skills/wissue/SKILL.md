---
name: wissue
description: "Raise a bug or feature against an EXISTING project and break it down into a wplan-compatible plan — codebase recon, targeted interview, mini-PRD, and a spec DAG with self-contained briefs — ready for wbuild to execute. Use this skill whenever the user reports a bug to fix, requests a feature or change on an existing codebase, says 'raise an issue/ticket', or wants specs/briefs generated for wbuild from a change request — even if they just say 'fix X in project Y' or 'add support for Z'. For planning a NEW project from scratch, use wplan instead."
user-invocable: true
argument-hint: "<bug/feature description> [repo-path]"
allowed-tools:
  # Bash is deliberately unscoped: recon runs the target repo's own
  # build/test/lint commands, which are discovered at runtime.
  - Bash
  - Read
  - Write
  - Edit
  - Glob
  - Grep
---

# wissue

<overview>
Turn a change request against an existing codebase into a real wplan plan folder that wbuild can execute unchanged. This is wplan's pipeline compressed for brownfield work: open-ended research becomes **codebase reconnaissance** (plus conditional external research when the change introduces technology the repo doesn't already use), the interview shrinks to only what recon can't answer, the PRD becomes a mini-PRD dominated by scope fences, and the greenfield bootstrap specs (spec_000_repo, spec_010_build) are removed — the repo and build already exist.

Everything downstream is unchanged: same schema, same gates, same `just check` / `just specs`, same `spec/NNN-name` branches, same status.wcl ledger. wbuild cannot tell a wissue plan from a wplan plan.

**When NOT to use this skill.** A trivial change — single file, no surface/model/contract impact, an obvious failing-test-first regression test — should just be fixed directly (test first, then the smallest fix, full suite green). wissue starts paying for itself at multi-file or multi-spec scope, when ownership needs splitting, or when the change touches anything user-facing. Ceremony disproportionate to the change is how planning pipelines lose users; say so and fix the trivial thing.
</overview>

<requirements>
- The **wplan skill installed** (this skill extracts wplan's verified template via its `scripts/new-plan.sh` — never reconstruct template files from memory, per wplan's own boundary).
- The `wcl` binary and `just` on PATH (the plan's justfile drives validation and rendering).
</requirements>

<variables>
- `${CLAUDE_SKILL_DIR}`: path to this skill's directory (its `reference/` and `scripts/` live here).
- `$ARGUMENTS`: the bug/feature description, and the target repo. Description from the user's request; repo defaults to the current directory. If the description is missing, ask what the issue is.

Plans live at `plans/<slug>/plan/` inside the target repo (slug like `fix-login-timeout`), so multiple change plans coexist; hand wbuild the specific path.
</variables>

<workflow>
Read in this order:

- [Pipeline](${CLAUDE_SKILL_DIR}/reference/pipeline.md) — the six stages from intake to wbuild handoff. **Start here.**

- [Codebase recon](${CLAUDE_SKILL_DIR}/reference/recon.md) — what to investigate, how to reproduce bugs, and how to capture findings.

- [Spec shapes](${CLAUDE_SKILL_DIR}/reference/spec_shapes.md) — brownfield spec patterns: the bug-fix spec, feature decomposition, the conditional prep spec, ownership in existing trees.

- [Template adaptation](${CLAUDE_SKILL_DIR}/reference/template_adaptation.md) — exactly what the scaffold script changes and the brownfield rules/PRD additions to make by hand.

- [Living capability specs](${CLAUDE_SKILL_DIR}/reference/capabilities.md) — the cross-plan memory layer: plans/capabilities/, capability deltas, and the merge-on-completion discipline.
</workflow>

<boundaries>
<always>
- Scaffold with `${CLAUDE_SKILL_DIR}/scripts/new-issue-plan.sh` (which wraps wplan's template extraction and performs the brownfield surgery), and keep `just check` green after every edit.
- Research any NEW dependency, library, or external API the change introduces (same research.wcl mechanism as recon) before the mini-PRD — the repo can't teach an agent technology it doesn't contain.
- Reproduce a bug during recon before speccing the fix — a failing command or test, recorded verbatim in a finding. If it cannot be reproduced, that is an open interview question, not an assumption.
- Record the repo's real build/test/lint commands and conventions as findings, and copy them INTO every spec body — briefs must stand alone, wbuild's agents never get the plan folder.
- Give every bug-fix spec a failing-regression-test-first task and an accept command that runs the project's full existing test suite, not just the new test.
- Resolve all six phase signoffs before rendering — brownfield plans legitimately mark most `:not_applicable`, but each skip must carry a why-note (`just check-full` enforces this).
- When the change adds or alters anything user-facing (a screen, CLI command, or endpoint), define or update the `surface` block (all elements, states, interactions) and cover it with a `scenario` — the wplan 1.1.0 gates (surface_coverage, surface_states, scenario_coverage) fail otherwise, and under-specified surfaces are how features come back half-finished.
- Add an import line to plan.wcl and a status row to status.wcl for every spec and finding file you create.
</always>
<ask>
- Before finalising the mini-PRD (present goals, non-goals and the spec DAG; the user approves before rendering).
- Before creating a plan when another plan in plans/ has unmerged specs touching overlapping paths — the owns_disjoint gate only sees within one plan.
</ask>
<never>
- Answer an interview question on the user's behalf, or invent WCL syntax beyond the template and schema (consult the wcl skill).
- Include refactoring, reformatting, or drive-by fixes outside the issue's scope in any spec — scope fences go in non-goals and rules.
- Hand-edit anything under out/, or start implementing — implementation is wbuild's job; this skill ends at the handoff.
</never>
</boundaries>

## Bundled files

- `${CLAUDE_SKILL_DIR}/scripts/new-issue-plan.sh` — extracts wplan's template into `plans/<slug>/` and strips the greenfield bootstraps (removes the two spec files, their plan.wcl imports, and their status rows), then reports whether `.tree/` is already gitignored in the repo.
