# Rendering and handoff

## Purpose

Produce both projections and give each role exactly what it needs.

## Prerequisites

- just check green with all specs in
- The analyze pass ran and its findings are resolved

## Flowchart

![diagram](../_wdoc/process_proc_render_handoff-diagram-1.svg)

## Steps

### Step 1: Render

```console
$ just render   # out/book (human) + out/specs (agents)
```

If the plan carries a WAD, also `just wad-extract` so the committed generated data never lags the plan - and re-run it whenever status.wcl moves during the build (the WAD's spec statuses track the plan's verbatim, via plan_state tags).

### Step 2: Hand off by role

Implementation agents get ONLY their out/specs/spec_<id>.md. The orchestrator gets out/specs/index.md (waves + worktree convention). The verification agent gets the verification procedure plus access to the plan folder. Re-run just render after any plan edit; never hand-edit out/.

> [!TIP]
> **Verification**
> out/book and out/specs exist; each brief is standalone; index.md lists the waves.

[← All processes](../references/processes_ref.md) · [← Back to SKILL.md](../SKILL.md)
