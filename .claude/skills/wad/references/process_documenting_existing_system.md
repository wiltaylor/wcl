# Document an existing system (the scan)

## Purpose

Populate a WAD from a real codebase: mechanical probes first, then a targeted interview for what code can't say.

## Prerequisites

- A scaffolded WAD inside (or beside) the repository
- Read access to the code, CI config, and IaC

## Flowchart

![diagram](../_wdoc/process_documenting_existing_system-diagram-1.svg)

## Steps

### Step 1: Built from a plan? Adopt its WAD

If the system was built by build mode from a plan that carried a WAD, don't scan cold — **graduate the plan's WAD** and treat the scan as backfill: move `wad/` from the plan folder to the repo's WAD home (conventionally `.wad/`), delete `scripts/extract_plan.py` and the `data/generated/plan.wcl` import, convert the derived blocks worth keeping into hand-authored data files (code is the source of truth once it exists), and install the normal code extractors. Then run the scan below only for what the plan never knew — infrastructure, build pipelines, externals, SOPs. No plan WAD? Scaffold fresh (`wcl init wad`) and scan everything.

### Step 2: Scan order

Work the scan-checklist rows top to bottom — repo layout → CI/build → entry points → externals → data layer → infrastructure → personas/governance → standards → SOPs. Cheap, high-signal sources come first; each row says what to read, what blocks to emit, and what to ask when the source is silent.

### Step 3: Probe and emit

For each row: read the sources, write the blocks, `wcl check`, render. Anything machine-derivable that will drift (pipelines, dependency edges, schemas) should be **extracted, not hand-copied** — write the extractor now (see the write-an-extractor process) so the fact stays true next month.

### Step 4: Respect the silence

Code cannot tell you: support contacts, approval chains, off-repo infrastructure, security posture, or why anything is the way it is. Leave those blank for the next step — a scanned WAD full of plausible inventions is worse than an empty one.

### Step 5: Gap interview

Run a targeted mini-interview from the question bank, restricted to what the probes couldn't fill — and back-fill the load-bearing decisions as `adr`s (“why is it like this?”). Still-unanswered items become open-question lines in an `article`.

### Step 6: Verify with the user

Serve the book with `--comment`, have the user walk it against reality, fix data from the comments, and record the reviewed revision as the diff baseline.

> [!TIP]
> **Verification**
> Every probe ran; extractors own the machine-derivable views; the gap interview covered the rest; the user spot-checked the rendered book against reality.

## Related

- [Codebase scan checklist (per view)](../references/fact_scan_checklist.md)

- [Interview question bank (per view)](../references/fact_interview_question_bank.md)

- [Write an extractor script](../references/process_writing_extractor.md)

[← Back to SKILL.md](../SKILL.md)
