---
name: wad
description: "Plan, build, and document software — four modes over one lifecycle. plan: turn an idea into a gated plan (interview, research, PRD, spec DAG, self-contained agent briefs). issue: raise a bug or feature against an EXISTING codebase and compress it into a build-ready plan (recon, targeted interview, mini-PRD). build: execute a rendered plan — orchestrate implementation, verification, review, fix and merge agents over the spec DAG until every spec is merged and every scenario passes. doc: create, populate, evolve and maintain the WAD architecture book (C4 drill-down, arc42-style views, extractors, change specs). Use whenever the user wants to plan a project, fix or add something in an existing repo, raise an issue/ticket, execute/implement/'run'/build a plan, or create/update/query architecture documentation — even if they just say 'plan X', 'fix Y in Z', 'start building the plan', or 'document this system'."
user-invocable: true
argument-hint: "<plan|issue|build|doc> <idea | bug/feature [repo] | plan-folder | wad-folder>"
allowed-tools:
  - Bash
  - Read
  - Write
  - Edit
  - Glob
  - Grep
  - Task
disable-model-invocation: false
metadata:
  wskill_schema_version: 1.5.0
---

# WAD

<overview>

Plan, build, and document software as typed WCL data — four modes over one lifecycle: plan (gated greenfield planning), issue (brownfield change plans), build (agent-orchestrated implementation), and doc (Wil's Architecture Document: C4 drill-down, arc42-style views, a twelve-chapter book).

**Upstream version:** `0.4.0`. If the real upstream has moved past this, the skill may be stale — bump `topic.version` and re-verify (see the update workflow).

Four modes over one lifecycle. **plan** (new software): interview → research → PRD → spec DAG → briefs, every phase gated by `just check`. **issue** (change existing software): recon → targeted interview → mini-PRD → spec DAG, producing a plan build mode runs unchanged. **build** (implement): the orchestrator loop dispatches implementer/verifier/reviewer/fixer/scenario agents over the DAG. **doc** (architecture): populate and maintain the WAD — interview a new design, scan an existing codebase, keep it current with extractors, derive change specs from diffs.

Route by mode first, then follow that mode's workflow index in order. The modes hand off to each other: a plan carries a WAD of the future system (extracted from the plan itself) that graduates into the repo's living WAD after the build; issue recon starts by reading that WAD; build completion updates it (graduation or the keeping-current sweep).

Plan/issue templates are wcl built-ins — scaffold with `wcl init wplan <dest>/plan --defaults` (plan) or the bundled `scripts/new-issue-plan.sh` (issue); never reconstruct template files from memory.

</overview>

## Parameters

<variables>

- `${CLAUDE_SKILL_DIR}`: path to this skill's directory (its `scripts/`, `assets/`, and `references/` live here).

- `$ARGUMENTS`: <plan|issue|build|doc> plus the mode's argument: a project idea (plan), a bug/feature description and repo (issue), a plan folder path (build), or a WAD folder / system to document (doc). How to determine: Infer the mode from the request when unstated: a new project → plan; a change/bug on existing code → issue; executing an existing plan folder → build; architecture questions or WAD work → doc. If genuinely ambiguous, ask.

</variables>

<boundaries>

<always>

- Route by mode first and read the matching workflow index (plan / issue / build / doc) before acting.
- Keep the model green: `just check` in a plan folder / `wcl check wad.wcl` in a WAD after every edit, before rendering.
- Scaffold templates, never reconstruct them: `wcl init wplan <dest>/plan --defaults` (plan), `scripts/new-issue-plan.sh` (issue), `wcl init wad` (doc).
- Complete ALL research before finalising a PRD — implementation agents must never be left to research anything; consult installed wskill research pages (`.claude/skills/*/references/research_*.md`, then the same glob under `$HOME/.claude/skills/`) before the web, and reproduce a bug during recon before speccing its fix.
- Copy context INTO each spec's body (PRD excerpts, findings, conventions, contracts) — every exported brief must stand alone; keep status.wcl out of implementation-agent prompts.
- Define every surface (all elements, states, interactions), data model, and scenario with the user before spec breakdown; pin every cross-spec API in a contract block; set covers on every spec; resolve every phase signoff before rendering.
- In build mode: verify then review before merging (the reviewer must be a strong model), merge only :reviewed specs whose dependencies are :merged in dependency order, and run the final acceptance scenarios on trunk — the run is not complete until they all pass.
- Keep the architecture record current: re-run `just wad-extract` when a plan's status.wcl moves during a build; at completion, graduate a plan's WAD into the repo (greenfield) or run the keeping-current sweep on the existing living WAD (brownfield).
- In doc mode: file every architecture fact under its correct view, follow the interview or codebase-scan process rather than free-forming, ask rather than inventing facts (support contacts, security, off-code infrastructure), and record decisions as `adr` blocks on the spot.
- Author every reference field as a BARE identifier (`system = shop_system`, `source = customer`, `repos = [repo_shop]`) — never a quoted string. Quoted refs coerce on newer wcl but silently break every derived view (empty drill-down, dead roll-ups) on older binaries, and bare is the canonical form all examples use.

</always>

<ask>

- Before overwriting an existing plan/ or WAD folder, or deleting any data file.
- Before finalising a PRD or mini-PRD (present it — the user approves the phase transition), and before dispatching build mode's first wave (present the wave plan).
- When a post-merge check fails, whenever a spec goes :blocked, and before deleting a worktree with uncommitted work.
- Before creating an issue plan when another plan in plans/ has unmerged specs touching overlapping paths — the owns_disjoint gate only sees within one plan.
- Before running extractor scripts against live or production systems, and before marking a WAD spec `:complete`.

</ask>

<never>

- Answer an interview question on the user's behalf, or invent architecture facts or WCL syntax (the templates plus the verified-notes fact cover the tested forms; consult the wcl/wdoc skills beyond them).
- Write application code in the orchestrator session, let an agent mark its own work done, or merge a spec that skipped verification or review.
- Let two specs own the same path, include out-of-scope refactoring or drive-by fixes in an issue spec, or push to any remote unless the user asks.
- Hand-edit anything under out/, files under a WAD's `data/**/generated/` (fix the extractor and re-run), or a `schema/base.wcl` (scaffold-owned and synced; `kinds.wcl` / `extensions.wcl` are the editable files).
- Author a relation whose endpoints don't resolve; model builders/operators as personas (personas are USERS; builders are view-1 stakeholders); model hosting, CI, CDN, git, or package registries as external_systems (they are infra_nodes; externals are functional integrations only).
- Model a platform or service as a stakeholder (stakeholders are people/organisations), a linked library or framework as an external_system (it belongs in its container's `technology` field), or build/test/release instructions as sysadmin SOPs (they are view-6 `pipeline` blocks; SOPs are for operating the running system).

</never>

</boundaries>

## Reference

### Modes

_Start here: route the request to a mode, then follow its workflow. plan = new software (start with proc_starting_a_plan). issue = change existing software (proc_issue_pipeline; trivial single-file fixes skip the pipeline — see issue_mode). build = execute a rendered plan (fact_roles, then proc_orchestrator_loop). doc = the WAD itself (populate_strategies dispatches between interview, scan, and extractors)._
- [Starting a plan](references/process_proc_starting_a_plan.md)
- [The issue pipeline](references/process_proc_issue_pipeline.md)
- [The orchestrator loop](references/process_proc_orchestrator_loop.md)
- [Interview vs scan](references/concept_populate_strategies.md)
- [Create a new WAD](references/process_creating_a_wad.md)
- [Design a new system (the interview)](references/process_designing_new_system.md)
- [Document an existing system (the scan)](references/process_documenting_existing_system.md)
- [Write an extractor script](references/process_writing_extractor.md)
- [Review a WAD](references/process_reviewing_a_wad.md)
- [Evolve the WAD and write the specs](references/process_evolving_and_spec.md)
- [Keep a WAD current](references/process_keeping_wad_current.md)
- [Interview question bank (per view)](references/fact_interview_question_bank.md)
- [Codebase scan checklist (per view)](references/fact_scan_checklist.md)

### What a WAD is

_The format, its views, and how it is consumed._
- [What a WAD is](references/concept_what_is_a_wad.md)
- [The twelve views](references/concept_twelve_views.md)
- [The C4 drill-down](references/concept_c4_drilldown.md)
- [Create a new WAD](references/process_creating_a_wad.md)

### The data model

_How the model is organised, and the block reference per view._

#### Structure

- [WAD folder layout](references/fact_wad_layout.md)
- [Generated vs hand-authored data](references/concept_generated_vs_hand.md)
- [Relations wire the diagrams](references/concept_relations_model.md)
- [WAD vocabularies (kinds.wcl)](references/fact_wad_kinds.md)
- [Custom blocks (extensions.wcl) — a worked example](references/fact_extensions_worked_example.md)

#### Blocks by view

- [Overview blocks](references/fact_overview_blocks.md)
- [Context blocks](references/fact_context_blocks.md)
- [External-system blocks](references/fact_external_blocks.md)
- [Systems blocks](references/fact_system_blocks.md)
- [Infrastructure blocks](references/fact_infra_blocks.md)
- [Build & deploy blocks](references/fact_build_deploy_blocks.md)
- [Documentation blocks](references/fact_documentation_blocks.md)
- [Persona blocks](references/fact_persona_blocks.md)
- [System-admin blocks](references/fact_sysadmin_blocks.md)
- [Standards blocks](references/fact_standards_blocks.md)
- [Domain blocks](references/fact_domain_blocks.md)
- [Spec blocks & the status lifecycle](references/fact_spec_blocks.md)

### Populating a WAD

_The interview, the codebase scan, and the extractor scripts._
- [Interview vs scan](references/concept_populate_strategies.md)
- [Design a new system (the interview)](references/process_designing_new_system.md)
- [Document an existing system (the scan)](references/process_documenting_existing_system.md)
- [Interview question bank (per view)](references/fact_interview_question_bank.md)
- [Codebase scan checklist (per view)](references/fact_scan_checklist.md)
- [Write an extractor script](references/process_writing_extractor.md)
- [Extractor scripts](references/fact_extractor_anatomy.md)

### Review & change

_Reviewing the book, deriving specs from diffs, staying current, and how the four modes hand off._
- [Specs and the change workflow](references/concept_spec_lifecycle.md)
- [Review a WAD](references/process_reviewing_a_wad.md)
- [Evolve the WAD and write the specs](references/process_evolving_and_spec.md)
- [Keep a WAD current](references/process_keeping_wad_current.md)
- [wcl diff (WAD usage)](references/entity_wcl_diff_wad.md)

### Plan mode — workflow

_The planning-pipeline runbooks, in order. Follow these top to bottom for a new project._

A new project runs these top to bottom: start, interview, research, PRD, spec breakdown, analyze, render/handoff. The verification procedure runs per-spec during implementation.

- [Starting a plan](references/process_proc_starting_a_plan.md)
- [Running the interview](references/process_proc_interview.md)
- [Doing the research](references/process_proc_research.md)
- [Writing the PRD](references/process_proc_write_prd.md)
- [Breaking down the specs](references/process_proc_spec_breakdown.md)
- [Analyzing the plan](references/process_proc_analyze_plan.md)
- [Rendering and handoff](references/process_proc_render_handoff.md)
- [Verifying and reviewing a spec](references/process_proc_verify_spec.md)

### Plan mode — design

_Why the planning pipeline is shaped this way - gates, the DAG, ownership, briefs, roles._

The ideas the runbooks rely on. Read pipeline first; briefs and role_split explain the constraints most often violated.

- [The gated pipeline](references/concept_pipeline.md)
- [Gates are blocks, not lets](references/concept_gates.md)
- [The spec DAG and build waves](references/concept_dag_waves.md)
- [File ownership](references/concept_ownership.md)
- [Self-contained briefs](references/concept_briefs.md)
- [Surfaces](references/concept_surfaces.md)
- [Usage scenarios](references/concept_scenarios.md)
- [Interface contracts and as-built notes](references/concept_contracts.md)
- [Data models](references/concept_data_models.md)
- [Phase signoffs](references/concept_signoffs.md)
- [Implementation vs verification](references/concept_role_split.md)
- [The project context file](references/concept_project_context.md)
- [The plan's WAD](references/concept_plan_wad.md)
- [The lessons loop](references/concept_lessons_loop.md)

### Plan mode — lookup

_Reference tables: plan folder layout, gates, states, CLI, and behaviour verified against the wcl binary._

Cite, don't re-derive. fact_wcl_notes is binding when writing any non-template WCL.

- [The plan/ template layout](references/fact_fact_folder_layout.md)
- [The thirteen default gates](references/fact_fact_default_gates.md)
- [State vocabularies](references/fact_fact_state_vocab.md)
- [EARS requirement patterns](references/fact_fact_ears.md)
- [The plan/ justfile recipes](references/fact_fact_cli.md)
- [WCL behaviour verified against the binary (0.29/0.30-alpha)](references/fact_fact_wcl_notes.md)

### Issue mode — workflow

_Raise a bug or feature against an existing codebase and break it into a build-ready plan._

Start with the pipeline runbook; recon is its heart. Trivial single-file changes skip the pipeline entirely - fix them directly (see the issue-mode concept's when-NOT-to-use rule).

- [The issue pipeline](references/process_proc_issue_pipeline.md)
- [Codebase reconnaissance](references/process_proc_recon.md)
- [Issue mode: wplan compressed for brownfield](references/concept_issue_mode.md)
- [Brownfield spec shapes](references/concept_spec_shapes.md)
- [Template adaptation: the brownfield surgery](references/fact_fact_template_adaptation.md)
- [Living capability specs](references/concept_capabilities.md)

### Build mode — workflow

_Execute a rendered plan: orchestrate implementation, verification, review, fix and merge agents over the spec DAG until every spec is merged and every scenario passes._

Read the roles fact first, then the orchestrator loop - everything else is the loop's sub-runbooks. A run is complete when every spec is :merged AND every scenario passes on trunk, never at the last merge.

- [The build roles](references/fact_fact_roles.md)
- [The orchestrator loop](references/process_proc_orchestrator_loop.md)
- [The runner contract and .wbuild/](references/fact_fact_runner_contract.md)
- [Dispatching implementation](references/process_proc_dispatch.md)
- [The verification procedure](references/process_proc_verify.md)
- [The review procedure](references/process_proc_review.md)
- [The fix loop and merging](references/process_proc_fix_and_merge.md)
- [States and status.wcl](references/fact_fact_states.md)

- [Bundled files](references/assets_ref.md) — The scripts and data files shipped with this skill, and how to run them.

## Views

Beyond this skill, the wskill ships these views — build them with `just render` in the wskill folder:

- **book** (`wdoc/book/main.wcl`)
- **ai skill** (`wdoc/skill/main.wcl`)
