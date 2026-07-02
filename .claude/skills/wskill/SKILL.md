---
name: wskill
description: "Create, edit, update and render wskill documents — self-contained WCL folders that capture everything about a topic (reference, processes, curated indexes) and project into both a human-readable book and a Claude Code skill. Use when the user wants to build, update, render, or install a wskill, capture a topic as a reusable skill, or asks about /wskill."
wskill_schema_version: 1.0.0
allowed-tools:
  - Bash
  - Read
disallowed-tools: []
disable-model-invocation: false
---

# wskill

A WCL document designed to hold complete set of knowledge on a topic and expose it in multiple views for multiple audiences.

**Upstream version:** `1.0.0`. If the real upstream has moved past this, the skill may be stale — bump `topic.version` and re-verify (see the update workflow).

wskill is a unified format that allows saving knowledge for use by Humans and AI.

## Parameters

Values to pass when invoking this skill — reference them as `$ARGUMENTS`, `$1`, `$2`, … in the prompt.

| Parameter | Description | How to determine the value |
| --- | --- | --- |
| $ARGUMENTS | The topic to capture, or the existing wskill to build / update / render. | Take it from the user's request (a topic name, or a path to a wskill folder). If empty, ask what to work on. |

<Boundary>

**Always:**

- Classify every new unit with the decision guide (fact `unit_decision_guide`) before creating it — entities are reserved for concrete NAMED things (people, software, places); ideas are concepts, values are facts, tasks are processes.

- Create content in data/ using the schema in schema/; give an entity a `kind` from schema/kinds.wcl.

- When new data has a recurring structure the base kinds don't fit, model it as a typed schema extension (see `creating_schema_extension`) rather than forcing it into prose.

- Run `wcl check wskill.wcl` after every change and keep it green before rendering.

**Ask first:**

- Before deleting data files or overwriting an existing wskill.

**Never:**

- Hand-edit a wskill's `schema/base.wcl` — it is generated/propagated. Topic-owned edits go in schema/kinds.wcl and schema/extensions.wcl.

- Invent schema fields or block kinds that aren't in the reflected schema reference — check the `Schema blocks` pages instead of guessing.

- Hand-edit anything under out/ — it is generated output.

</Boundary>

## Reference

### WSkill

_What a wskill is, why it exists, and how to start one._

- [What is it?](references/concept_wskill_concept.md)

- [The view family](references/concept_views.md)

- [Creating a new wskill](references/process_creating_a_wskill.md)

### Authoring

_Capturing knowledge: decomposition, classification, the capture loop, and the tools._

- [Decomposing information](references/concept_decomposing_information.md)

- [Which unit kind? — the decision guide](references/fact_unit_decision_guide.md)

- [Adding content to a wskill](references/process_adding_content.md)

- [Building the wskill index](references/process_building_the_index.md)

- [The wskill folder layout](references/fact_folder_layout.md)

- [The wcl commands an author uses](references/fact_authoring_cli.md)

- [Assets (images & data files)](references/concept_assets.md)

### Views & Rendering

_The four views, what controls what renders where, and how to build and install each._

- [The view family](references/concept_views.md)

- [Audience control](references/concept_audience_control.md)

- [Visibility scoping (@only / @except)](references/concept_visibility_scoping.md)

- [Anatomy of the AI skill](references/concept_skill_anatomy.md)

- [Setting up AI skill generation](references/process_setup_ai_skill.md)

- [Building and installing the AI skill](references/process_installing_the_skill.md)

- [The presentation view](references/concept_presentation_view.md)

- [The training view](references/concept_training_view.md)

- [The standard just recipes](references/fact_justfile_recipes.md)

- [Collections and registries](references/concept_collections_registries.md)

- [Claude Code](references/entity_claude_code.md)

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

- [Building the wskill index](references/process_building_the_index.md)

- [Setting up AI skill generation](references/process_setup_ai_skill.md)

- [Building and installing the AI skill](references/process_installing_the_skill.md)

- [Attaching a wskill to a registry](references/process_attaching_to_registry.md)

- [Creating a schema extension](references/process_creating_schema_extension.md)

- [Creating the presentation view](references/process_creating_presentation.md)

- [Creating the training view](references/process_creating_training_book.md)

- [Updating a wskill when its source changes](references/process_updating_a_wskill.md)

- [Upgrading a wskill to a new base schema](references/process_upgrading_schema_version.md)

- [Reviewing a wskill (human ⇄ agent loop)](references/process_reviewing_a_wskill.md)

- [Editing a wskill in the browser](references/process_editing_via_serve.md)

## Views

Beyond this skill, the wskill ships these views — build them with `just render` in the wskill folder:

- **book** (`wdoc/book/main.wcl`)

- **ai skill** (`wdoc/skill/main.wcl`)

- **presentation** — An introduction to the wskill format as an overview deck. (`wdoc/presentation/main.wcl`)

- **training** — Build your first wskill — a hands-on lesson series. (`wdoc/training/main.wcl`)
