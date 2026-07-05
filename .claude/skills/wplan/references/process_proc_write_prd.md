# Writing the PRD

## Purpose

Capture goals, non-goals, prioritised requirements and the universal project rules, then configure the gates.

## Prerequisites

- questions_closed and research_done both pass

## Flowchart

![diagram](../_wdoc/process_proc_write_prd-diagram-1.svg)

## Steps

### Step 1: Author prd.wcl

Goals, non-goals, requirements with :must/:should/:could/:wont priority. Phrase requirements in EARS form where possible - "WHEN \[event\] THE SYSTEM SHALL \[observable behaviour\]" (see the EARS patterns fact); a requirement that names an observable behaviour converts mechanically into an accept command later. Every :must requirement will need a covering spec at breakdown time (requirements_covered gate). Rules are copied verbatim into EVERY brief - keep them short, imperative and universal; anything spec-specific belongs in that spec.

### Step 2: Define every surface

One `surface` block per screen/command/endpoint in surfaces.wcl: purpose, entry, layout text, every element, all four standard states for screens (empty/loading/error/populated - the surface_states gate enforces this), and every interaction with its outcome. Draft surfaces BEFORE research where possible; research often changes them, so revisit and refine AFTER research completes. Walk each surface through with the user. Wireframes (wf_\* widgets) go in the body for the book - agents get the structured fields as text.

### Step 3: Define the data models

One `model` block per stored/shared entity in models.wcl: fields with types and validation, persistence, relations. Like surfaces: draft before research, refine after (research often settles persistence). Every model will need exactly one defining spec at breakdown time.

### Step 4: Write the usage scenarios

End-to-end scenario blocks in scenarios.wcl: a user goal, the surfaces it touches, and ordered action/expect steps through the finished application. Scenarios are the system-level definition of done and every surface must appear in at least one (scenario_coverage gate). Confirm them with the user.

### Step 5: Contracts come later

Interface contracts (contracts.wcl) are usually authored during spec breakdown, once the spec boundaries exist - but if research already fixed an API shape, capture it now.

### Step 6: Configure gates.wcl

You own the gates. Keep the defaults; uncomment or add project-specific assertions (the template ships optional strictness gates as comments).

### Step 7: Project the WAD and review it with the user

```console
$ just wad-extract && just wad-serve   # review with click-to-comment
```

The steering pass: hand-author the planned system's containers in wad/data/systems/ (and real personas if the plan has distinct user kinds), fill the attribution tables in scripts/extract_plan.py, then extract and serve. The book now shows the finished solution - every screen with its wireframe, the domain model as an ER diagram, each scenario as a flowchart. Walk it with the user; comments land in the book's comments.wcl sidecar, and every correction is a plan edit (fix surfaces/models/scenarios, re-extract) - never an edit to the generated file. Record decisions made during the walk as `adr` blocks in the WAD on the spot.

### Step 8: User approval and signoffs

Present the PRD, surfaces, models and scenarios (the WAD walk usually IS this presentation); on approval, resolve the phase signoffs in signoffs.wcl (interview/research/surfaces/scenarios/data_model/prd each :done, or :not_applicable with a why-note - e.g. surfaces n/a for a pure library). `just check-full` must pass before spec breakdown.

> [!TIP]
> **Verification**
> just check-full green and explicit user approval recorded in the conversation.

[← All processes](../references/processes_ref.md) · [← Back to SKILL.md](../SKILL.md)
