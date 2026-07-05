---
name: wad
description: "Reference and processes for WAD. Wil's Architecture Document: a typed WCL data model of a software system — C4 drill-down, arc42-style views — rendered into a twelve-chapter architecture book. Use when working with WAD or answering questions about it."
allowed-tools:
  - Bash
  - Read
disable-model-invocation: false
metadata:
  wskill_schema_version: 1.2.0
---

# WAD

<overview>

Wil's Architecture Document: a typed WCL data model of a software system — C4 drill-down, arc42-style views — rendered into a twelve-chapter architecture book.

**Upstream version:** `0.4.0`. If the real upstream has moved past this, the skill may be stale — bump `topic.version` and re-verify (see the update workflow).

A WAD is a typed architecture document. This skill creates and populates one — by interviewing you about a new design or scanning an existing codebase — keeps it current with extractor scripts, and decomposes reviewed changes into implementation specs for coding agents.

</overview>

## Parameters

<variables>

- `${CLAUDE_SKILL_DIR}`: path to this skill's directory (its `scripts/`, `assets/`, and `references/` live here).

- `$ARGUMENTS`: Path to a WAD folder, or the system/codebase to document. How to determine: Take it from the user's request. If empty, ask whether to create, populate, evolve, or review a WAD, then follow the matching process.

</variables>

<boundaries>

<always>

- Run `wcl check wad.wcl` after every data edit and keep it green before rendering.
- File every architecture fact under its correct view — consult the twelve-views map before writing.
- Ask the user rather than inventing architecture facts — especially support contacts, security details, and infrastructure that is not visible in the code.
- Record any decision made during a session as an `adr` block on the spot.
- When populating, follow the design-interview or codebase-scan process rather than free-forming.

</always>

<ask>

- Before running extractor scripts against live or production systems.
- Before overwriting hand-authored data with extractor-owned data (moving ownership of a block family to a script).
- Before marking a spec `:complete` or deleting data files.

</ask>

<never>

- Hand-edit files under `data/**/generated/` — fix the extractor and re-run it.
- Hand-edit a WAD's `schema/base.wcl` — it is scaffold-owned and synced (`kinds.wcl` / `extensions.wcl` are the editable files).
- Author a relation whose endpoints don't resolve to declared block ids.
- Model the system's builders, maintainers, or operators as personas — personas are USERS of the system; builders/operators are view-1 stakeholders with RACI rows.
- Model hosting, CI, CDN, git, or package-registry platforms as external_systems — anything the system merely runs, builds, or ships on is an infra_node in view 5; externals are functional integrations only.

</never>

</boundaries>

## Reference

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

_Reviewing the book, deriving specs from diffs, and staying current._
- [Specs and the change workflow](references/concept_spec_lifecycle.md)
- [Review a WAD](references/process_reviewing_a_wad.md)
- [Evolve the WAD and write the specs](references/process_evolving_and_spec.md)
- [Keep a WAD current](references/process_keeping_wad_current.md)
- [wcl diff (WAD usage)](references/entity_wcl_diff_wad.md)

### Tasks

_Start here: pick the process that matches the request._
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

- [Bundled files](references/assets_ref.md) — The scripts and data files shipped with this skill, and how to run them.

## Views

Beyond this skill, the wskill ships these views — build them with `just render` in the wskill folder:

- **book** (`wdoc/book/main.wcl`)
- **ai skill** (`wdoc/skill/main.wcl`)
