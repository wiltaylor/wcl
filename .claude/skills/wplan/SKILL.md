---
name: wplan
description: "Reference and processes for wplan. Plan a software project as a gated WCL pipeline: interview, research, PRD, a DAG of parallel-safe specs, and self-contained markdown briefs weak AI agents can execute. Use when working with wplan or answering questions about it."
user-invocable: true
argument-hint: "<project idea | path to existing plan/ folder>"
allowed-tools:
  - Bash
  - Read
disable-model-invocation: false
metadata:
  wskill_schema_version: 1.2.0
---

# wplan

<overview>

Plan a software project as a gated WCL pipeline: interview, research, PRD, a DAG of parallel-safe specs, and self-contained markdown briefs weak AI agents can execute.

**Upstream version:** `1.6.0`. If the real upstream has moved past this, the skill may be stale — bump `topic.version` and re-verify (see the update workflow).

Turn an idea into (1) a human-readable wdoc book and (2) self-contained markdown spec briefs that implementation agents - including weak models - execute in parallel without any wcl/wdoc dependency. Everything lives in typed WCL so `just check` gates progress and validates the spec DAG.

Follow the workflow index in order. The verified template ships as an asset - extract it with scripts/new-plan.sh; never reconstruct it from memory.

</overview>

## Parameters

<variables>

- `${CLAUDE_SKILL_DIR}`: path to this skill's directory (its `scripts/`, `assets/`, and `references/` live here).

- `$ARGUMENTS`: The project/idea to plan, or the path to an existing plan/ folder to continue. How to determine: Take it from the user's request. If empty, ask what to plan.

</variables>

<boundaries>

<always>

- Extract the template with scripts/new-plan.sh rather than writing schema/wdoc files from scratch, and keep `just check` green after every edit.

- Add an import line to plan.wcl for every new spec and research finding file, and a status row to status.wcl for every spec.

- Complete ALL research before finalising the PRD - implementation agents must never be left to research anything.

- Consult installed wskill research pages (`.claude/skills/*/references/research_*.md`, then the same glob under `$HOME/.claude/skills/`) before web research, and cite a reused page in the finding.

- Copy context INTO each spec's body (PRD excerpts, research findings, conventions); every exported brief must stand alone.

- Keep status.wcl out of implementation-agent briefs - only the verification agent knows it exists.

- Define every surface (all elements, states, interactions), data model, and scenario with the user before spec breakdown - under-specified surfaces and models are the main causes of half-finished applications.

- Pin every API that crosses a spec boundary in a contract block, and resolve every phase signoff (:done or :not_applicable with a why) before rendering - just check-full gates this.

- Set covers on every spec (the requirements_covered gate blocks orphaned :must requirements) and run the analyze pass before rendering - gates verify structure, the analyze pass verifies sense.

</always>

<ask>

- Before overwriting an existing plan/ folder or deleting any data file.

- Before finalising the PRD (present it; the user approves the phase transition).

</ask>

<never>

- Answer an interview question on the user's behalf.

- Let two specs own the same path, or hand-edit anything under out/.

- Invent WCL/wdoc syntax - the template plus the verified-notes fact cover the tested forms; consult the wcl/wdoc skills beyond them.

</never>

</boundaries>

## Reference

- [Workflow](references/index_workflow.md) — The pipeline runbooks, in order. Follow these top to bottom for a new project.

- [Design](references/index_design.md) — Why the pipeline is shaped this way - gates, the DAG, ownership, briefs, roles.

- [Lookup](references/index_lookup.md) — Reference tables: folder layout, gates, states, CLI, and behaviour verified against the wcl binary.

- [Concepts](references/concepts_ref.md) — core ideas, one page each.

- [Facts](references/facts_ref.md) — value tables and constants.

- [Processes](references/processes_ref.md) — task runbooks.

- [Glossary](references/glossary_ref.md) — terms and definitions.

- [Related skills](references/related_ref.md) — cross-references to other wskills.

- [Bundled files](references/assets_ref.md) — scripts and data shipped with this skill.
