---
name: wskill
description: "Create, edit, update and render wskill documents — self-contained WCL folders that capture everything about a topic (reference, processes, curated indexes) and project into both a human-readable book and a Claude Code skill. Use when the user wants to build, update, render, or install a wskill, capture a topic as a reusable skill, research a topic into a wskill (deep-research a subject and author it as a skill), or asks about /wskill."
allowed-tools:
  - Bash
  - Read
disable-model-invocation: false
metadata:
  wskill_schema_version: 1.3.0
---

# wskill

<overview>

A WCL document designed to hold complete set of knowledge on a topic and expose it in multiple views for multiple audiences.

**Upstream version:** `1.2.0`. If the real upstream has moved past this, the skill may be stale — bump `topic.version` and re-verify (see the update workflow).

wskill is a unified format that allows saving knowledge for use by Humans and AI.

</overview>

## Parameters

<variables>

- `${CLAUDE_SKILL_DIR}`: path to this skill's directory (its `scripts/`, `assets/`, and `references/` live here).

- `$ARGUMENTS`: The topic to capture, or the existing wskill to build / update / render. How to determine: Take it from the user's request (a topic name, or a path to a wskill folder). If empty, ask what to work on.

</variables>

<boundaries>

<always>

- Route with the skill-vs-wskill rule (fact `skill_vs_wskill`) before scaffolding anything: AI-only knowledge → a plain canonical skill via /meta-skill; knowledge humans also consume → a wskill.
- Classify every new unit with the decision guide (fact `unit_decision_guide`) before creating it — entities are reserved for concrete NAMED things (people, software, places); ideas are concepts, values are facts, tasks are processes, dated findings are research.
- Capture durable investigation output as `research` blocks under data/research/ so it ships at references/research_<id>.md for other agents and pipelines to reuse.
- For whole-topic research-to-wskill authoring, follow the `researching_a_topic` runbook — dispatch `wskill-researcher` subagents in parallel rather than researching serially in-session.
- Create content in data/ using the schema in schema/; give an entity a `kind` from schema/kinds.wcl. Start from the unit kind's fill-out template (facts `template_concept` / `template_entity` / `template_fact` / `template_procedure` / `template_index` / `template_example` / `template_research`).
- Follow the house style (fact `writing_style`) and keep `related` to roughly 3-5 ids per content unit (fact `linking_discipline`) — never write a hub unit whose body is only links to its children.
- When new data has a recurring structure the base kinds don't fit, model it as a typed schema extension (see `creating_schema_extension`) rather than forcing it into prose.
- Run `wcl check wskill.wcl` after every change and keep it green before rendering.

</always>

<ask>

- Before deleting data files or overwriting an existing wskill.

</ask>

<never>

- Hand-edit a wskill's `schema/base.wcl` — it is generated/propagated. Topic-owned edits go in schema/kinds.wcl and schema/extensions.wcl.
- Invent schema fields or block kinds that aren't in the reflected schema reference — check the `Schema blocks` pages instead of guessing.
- Hand-edit anything under out/ — it is generated output.

</never>

</boundaries>

## Reference

### WSkill

_What a wskill is, why it exists, and how to start one._
- [What is it?](references/concept_wskill_concept.md)
- [Different Views](references/concept_views.md)
- [Creating a new wskill](references/process_creating_a_wskill.md)

### Authoring

_Capturing knowledge: decomposition, classification, house style, and the capture loop._
- [Decomposing information](references/concept_decomposing_information.md)
- [Which unit kind? — the decision guide](references/fact_unit_decision_guide.md)
- [Writing style for wskill content](references/fact_writing_style.md)
- [Linking discipline — link sparingly](references/fact_linking_discipline.md)
- [The wskill folder layout](references/fact_folder_layout.md)

#### Unit templates

_A copy-paste skeleton, worked example and done-when checklist for every unit kind._
- [Template — concept](references/fact_template_concept.md)
- [Template — entity](references/fact_template_entity.md)
- [Template — fact](references/fact_template_fact.md)
- [Template — process (procedure)](references/fact_template_procedure.md)
- [Template — index](references/fact_template_index.md)
- [Template — example](references/fact_template_example.md)
- [Template — research](references/fact_template_research.md)

### Views & Rendering

_The four views, what controls what renders where, and how to build and install each._
- [Visibility scoping (@only / @except)](references/concept_visibility_scoping.md)
- [Anatomy of the AI skill](references/concept_skill_anatomy.md)
- [Setting up AI skill generation](references/process_setup_ai_skill.md)
- [Building and installing the AI skill](references/process_installing_the_skill.md)
- [The presentation view](references/concept_presentation_view.md)
- [The training view](references/concept_training_view.md)

### Extending

_Custom schemas and projections for topic-specific data, and the component system behind the shared look._
- [Custom projections (schema extension modules)](references/concept_custom_projection.md)
- [Creating a schema extension](references/process_creating_schema_extension.md)
- [Components: one look for every unit](references/concept_components_look_feel.md)

### Task runbooks

_Step-by-step runbooks for building and maintaining a wskill — classify with the decision guide before creating any unit._
- [Which unit kind? — the decision guide](references/fact_unit_decision_guide.md)
- [Creating a new wskill](references/process_creating_a_wskill.md)
- [Adding content to a wskill](references/process_adding_content.md)
- [Capturing research into a wskill](references/process_capturing_research.md)
- [Researching a topic into a wskill](references/process_researching_a_topic.md)
- [Building the wskill index](references/process_building_the_index.md)
- [Setting up AI skill generation](references/process_setup_ai_skill.md)
- [Building and installing the AI skill](references/process_installing_the_skill.md)
- [Attaching a wskill to a registry](references/process_attaching_to_registry.md)
- [Creating a schema extension](references/process_creating_schema_extension.md)
- [Creating the presentation view](references/process_creating_presentation.md)
- [Creating the training view](references/process_creating_training_book.md)
- [Updating a wskill when its source changes](references/process_updating_a_wskill.md)
- [Upgrading a wskill to a new base schema](references/process_upgrading_schema_version.md)
- [Editing a wskill in the browser](references/process_editing_via_serve.md)

## Views

Beyond this skill, the wskill ships these views — build them with `just render` in the wskill folder:

- **book** (`wdoc/book/main.wcl`)
- **ai skill** (`wdoc/skill/main.wcl`)
