# Breaking down the specs

## Purpose

Produce the DAG of parallel-safe, weak-model-proof specs.

## Prerequisites

- Approved PRD

## Flowchart

![diagram](../_wdoc/process_proc_spec_breakdown-diagram-1.svg)

## Steps

### Step 1: Adapt the bootstrap specs

spec_000_repo (git init, ignore rules incl. .tree/) and spec_010_build (toolchain, build/test commands) always come first. Adapt the template's Rust defaults to the interview answers. spec_000 runs directly on main - nothing exists to worktree from yet.

### Step 2: Slice the work

Number sparsely (000, 010, 020...) so insertions never renumber the DAG; ids spec_NNN_name, branches spec/NNN-name. Size each spec so a weak agent holds the whole brief: one module, one subsystem, one integration. Before finalising the slice, score each candidate spec's complexity 1-10 (breadth of ownership, number of unknowns, integration surface) with a one-line rationale: 7+ means split it or record why it must stay whole, and the scores inform wave sizing during the build (dispatch fewer high-complexity specs concurrently). Over ~10 ordered tasks or sprawling ownership means split it and wire the DAG. Author a `contract` block for every API crossing a spec boundary, owned by the providing spec. End the DAG with an explicit integration spec that wires the modules together and carries the harness the final scenarios run on - per-module specs verifying green does not make them cohere. Give scenarios a `ready_after` spec where they become runnable before the end, so integration errors surface early.

### Step 3: Fill every spec's contract

depends_on (spec-level), `covers` listing the PRD requirement ids this spec implements (every :must requirement needs at least one coverer - the requirements_covered gate enforces it), `implements` listing the surfaces this spec delivers, `defines_models`/`uses_models` for every model it touches (exactly one definer each), `consumes` for every contract it builds against (the provider must be among its transitive dependencies), a `harness` whenever it implements surfaces or has walkthroughs (how the verifier runs the work: command, stubs, seed data) (exactly one spec per surface - the surface_coverage gate enforces it; the brief automatically carries each implemented surface's full contract), exclusive owns paths (last-toucher-owns for handoffs), allowed/not_allowed boundary, ordered tasks, acceptance checks each with a runnable command where possible plus walkthrough wstep children for surface behaviour, a done end-state checklist, and a body copying in the PRD excerpts, research findings and conventions this spec relies on. Repeat the most-violated boundary inside the relevant task text.

### Step 4: Wire each spec in

An import line in plan.wcl and a status row in status.wcl per spec - then just check. Fix anything the gates report before the next spec.

> [!TIP]
> **Verification**
> just check green: DAG acyclic, ownership disjoint, status covered, surfaces implemented exactly once, models defined exactly once, contract order holds, harnesses present.

[← All processes](../references/processes_ref.md) · [← Back to SKILL.md](../SKILL.md)
