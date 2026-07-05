# Keep a WAD current

## Purpose

The maintenance sweep that stops the document drifting from reality.

## Prerequisites

- A populated WAD with a recorded baseline

## Flowchart

![diagram](../_wdoc/process_keeping_wad_current-diagram-1.svg)

## Steps

### Step 1: Trigger

Run the sweep when something meaningful lands: code merged, infrastructure changed, a decision made, an incident that revealed the doc was wrong. Build mode's completion step is a **standing trigger** — every finished plan run (greenfield or issue) ends by updating the architecture record, either by graduating the plan's WAD or by running this sweep on the existing one. Manual changes outside the pipeline still need the human trigger.

### Step 2: Re-run the extractors

`just extract` (or `just wad-extract` in the WCL repo) — generated data re-derives from the live sources; the git diff of `data/generated/` IS the machine-visible drift.

### Step 3: Diff against the baseline

`wcl diff <baseline>:wad.wcl wad.wcl` — read what moved. Update the hand-authored blocks the changes invalidate (a renamed crate's relations, a retired container's deploy targets).

### Step 4: Read the commit history

Have an LLM read the **git log since the last sweep** and list what changed that the WAD hasn't caught up with — renamed modules, new services, retired jobs, decisions visible only in commit messages. Commits are the change log the document must answer to; extractors only cover the machine-derivable slice.

### Step 5: Capture decisions

Anything decided since the last sweep becomes an `adr` now, while the context is fresh. New operational learnings update the SOPs.

### Step 6: Check, render, mini-review

`wcl check`, render, and have the owner spot-check the affected chapters. Commit; if the sweep was substantial, record a fresh reviewed baseline.

> [!TIP]
> **Verification**
> Extractors are git-quiet after a re-run, `wcl diff` against the baseline shows only intended drift, and spot-checked chapters match reality.

## Related

- [Generated vs hand-authored data](../references/concept_generated_vs_hand.md)

[← Back to SKILL.md](../SKILL.md)
