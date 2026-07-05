# Analyzing the plan

## Purpose

A judgment pass over the whole plan before rendering - catching what gates cannot express.

## Prerequisites

- just check green with all specs in

## Flowchart

![diagram](../_wdoc/process_proc_analyze_plan-diagram-1.svg)

## Steps

### Step 1: Read the plan end to end

Gates verify structure; this pass verifies sense. Read prd.wcl, surfaces.wcl, scenarios.wcl, models.wcl, contracts.wcl and every spec in one sitting - drift hides between files read on different days.

### Step 2: Hunt unmeasurable language

Flag every requirement, done item or accept check using an adjective with no metric - fast, secure, robust, scalable, simple, intuitive. Each one either gains a measurable form (a number, an EARS SHALL clause, a runnable command) or moves to goals, which are allowed to be aspirational.

### Step 3: Check cross-artifact consistency

Terminology: the same thing must have the same name in the PRD, surfaces, models and specs (a Glossary drift check - 'task' in the PRD, 'item' in a model, 'todo' in a brief is three bugs waiting). Duplication: two requirements or two specs saying overlapping things means merge or sharpen the boundary. Contradiction: a non-goal that a spec quietly implements, a rule a task violates. Coverage direction: covers/implements/defines_models say specs deliver the plan - also check the reverse, that no spec does significant work no requirement or surface asked for.

### Step 4: Spot-check brief self-containedness

Render a draft (just specs) and read the two briefs with the most dependencies as if you were the weak agent: every referenced convention, finding and contract must be IN the body, every command runnable as written, no 'see the PRD'. Fix at the source (the spec's body), never in out/.

### Step 5: Resolve findings before rendering

Fix what you found (plan edits, then just check again), or record a deliberate acceptance in lessons.wcl with a why. Do not carry known ambiguity into briefs - weak agents treat ambiguity as permission.

> [!TIP]
> **Verification**
> No unmeasurable :must requirement, no terminology drift across artifacts, and the spot-checked briefs stand alone; just check still green.

[← Back to SKILL.md](../SKILL.md)
