# Updating a wskill when its source changes

## Purpose

Keep a wskill faithful to its upstream — check each source for drift, and update and re-pin the units only when the source has actually moved.

## Prerequisites

- The wskill declares its `source`s and a pinned `topic.version`

## Flowchart

![diagram](../_wdoc/process_updating_a_wskill-diagram-1.svg)

## Steps

### Step 1: Start a review

Begin when you are notified the upstream moved, or on a regular cadence. The goal is to detect drift between the live source and what the wskill already records — not to re-read everything from scratch.

### Step 2: Compare against each source

```wcl
source upstream {
  kind             = "docs"
  locator          = "https://example.com/manual"
  covers           = "the reference this wskill summarises"
  last_checked     = "2026-06-24"
  reflects_version = "4.2"
}
```

For each `source` block, compare the live upstream against the `reflects_version` it carried when last checked, and refresh `last_checked` to today. The sources are update-workflow metadata, not topic content — they exist precisely so a review can tell at a glance what each unit was last reconciled against.

### Step 3: Source changed?

If the upstream still matches every source's `reflects_version`, the wskill is already faithful — take the **no** branch and stop. If anything moved, take the **yes** branch to bring the affected units back in line.

### Step 4: Update the affected units

Fold each change into the model: edit the `concept`, `entity`, `fact` or `procedure` units the change touches, keeping each atomic and re-checking its `related` links. Only touch the units the source actually moved. Where a change raises something only the topic owner can settle (a policy choice, an ambiguity the source doesn't resolve), don't guess — capture it as a `question` block; the owner answers later via `wcl answer wskill.wcl`, and the next sweep folds each `:answered` question into real units and deletes the block. Sweep the `research` blocks too: a finding whose subject moved past its `applies_to` gets re-verified (update `checked`) or flipped to `status = :stale`; a finding replaced by new research becomes `:superseded` with `superseded_by` naming the replacement.

### Step 5: Re-verify and re-pin

```console
$ wcl check wskill.wcl
OK
$ just render        # rebuild the book + skill projections
```

Run `wcl check wskill.wcl` and rebuild both projections so the change renders cleanly. Then re-pin the metadata: set each touched source's `reflects_version` to the upstream you reconciled against, and bump `topic.version` so consumers can tell the wskill has moved.

### Step 6: Up to date

The wskill faithfully reflects its sources again, and the version pin records which upstream it now describes.

> [!TIP]
> **Verification**
> `wcl check wskill.wcl` passes, every touched `source` has a current `last_checked` / `reflects_version`, and `topic.version` reflects the upstream the wskill now describes.

## Related

- [What is it?](../references/concept_wskill_concept.md)

- [Self-Contained Content](../references/concept_selfcontained.md)

- [Separation of Data and Presentation](../references/concept_datapresentation.md)

- [Creating a new wskill](../references/process_creating_a_wskill.md)

[← Back to SKILL.md](../SKILL.md)
