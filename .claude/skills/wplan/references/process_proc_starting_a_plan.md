# Starting a plan

## Purpose

Place and scaffold the verified template, and prove it checks before any content goes in.

## Prerequisites

- wcl and just installed
- The user has named the project/idea

## Flowchart

![diagram](../_wdoc/process_proc_starting_a_plan-diagram-1.svg)

## Steps

### Step 1: Decide where the plan lives

Inside a project repo: `plan/` at the repo root (or `docs/plan/` if the user prefers). Central planning folder: one plan-shaped folder per project. On claude.ai: build under a scratch folder and present the folder as a download. Ask the user if it is not obvious from context.

### Step 2: Scaffold the template

```console
$ wcl init wplan <destination>/plan --defaults
```

Use the `wplan` built-in template of `wcl init`. Never reconstruct the template from memory - the shipped template is verified against the wcl binary in CI.

### Step 3: Prove the empty plan checks

```console
$ cd <destination>/plan && just check
```

All default gates must pass on the empty template (they hold vacuously). If this fails now, fix the environment now - every later error will then be about content.

> [!TIP]
> **Verification**
> just check prints OK twice and every default gate passing.

[← All processes](../references/processes_ref.md) · [← Back to SKILL.md](../SKILL.md)
