# Document an existing system (the scan)

## Purpose

Populate a WAD from a real codebase: mechanical probes first, then a targeted interview for what code can't say.

## Prerequisites

- A scaffolded WAD inside (or beside) the repository
- Read access to the code, CI config, and IaC

## Flowchart

![diagram](../_wdoc/process_documenting_existing_system-diagram-1.svg)

## Steps

### Step 1: Scan order

Work the scan-checklist rows top to bottom — repo layout → CI/build → entry points → externals → data layer → infrastructure → personas/governance → standards → SOPs. Cheap, high-signal sources come first; each row says what to read, what blocks to emit, and what to ask when the source is silent.

### Step 2: Probe and emit

For each row: read the sources, write the blocks, `wcl check`, render. Anything machine-derivable that will drift (pipelines, dependency edges, schemas) should be **extracted, not hand-copied** — write the extractor now (see the write-an-extractor process) so the fact stays true next month.

### Step 3: Respect the silence

Code cannot tell you: support contacts, approval chains, off-repo infrastructure, security posture, or why anything is the way it is. Leave those blank for the next step — a scanned WAD full of plausible inventions is worse than an empty one.

### Step 4: Gap interview

Run a targeted mini-interview from the question bank, restricted to what the probes couldn't fill — and back-fill the load-bearing decisions as `adr`s (“why is it like this?”). Still-unanswered items become open-question lines in an `article`.

### Step 5: Verify with the user

Serve the book with `--comment`, have the user walk it against reality, fix data from the comments, and record the reviewed revision as the diff baseline.

> [!TIP]
> **Verification**
> Every probe ran; extractors own the machine-derivable views; the gap interview covered the rest; the user spot-checked the rendered book against reality.

## Related

- [Codebase scan checklist (per view)](../references/fact_scan_checklist.md)

- [Interview question bank (per view)](../references/fact_interview_question_bank.md)

- [Write an extractor script](../references/process_writing_extractor.md)

[← Back to SKILL.md](../SKILL.md)
