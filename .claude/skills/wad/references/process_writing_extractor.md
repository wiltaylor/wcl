# Write an extractor script

## Purpose

Automate one machine-derivable slice of the WAD so it stays honest against its source of truth.

## Prerequisites

- uv installed
- A source of truth worth automating (CI config, build metadata, a database, an API)

## Flowchart

![diagram](../_wdoc/process_writing_extractor-diagram-1.svg)

## Steps

### Step 1: Pick one concern

One script = one output file = one source of truth. Name it `extract_<thing>.py`; its output is `data/generated/<thing>.wcl`.

### Step 2: Start from the template

Copy `extractor_template.py` (bundled with this skill; `extract_repo.py` ships in every scaffold). Set the PEP 723 header's `dependencies` for whatever the source needs (`pyyaml`, a DB driver, `httpx`…).

### Step 3: Map source records to WAD blocks

Decide which block kinds the findings become (relations? pipeline stages? a `code_item :db_schema` with `db_table`/`db_column` children?). When the data genuinely fits no base family, declare a **typed custom block** in `schema/extensions.wcl` and emit that instead, adding a matching render to the book template — better a first-class block than data contorted into the wrong shape. Anything needing human naming — like source name → WAD container id — goes in a small in-script table, so customising the mapping is editing data, not logic.

### Step 4: Emit deterministically

Banner comment first, `namespace wcl.wad`, then blocks — collections sorted, ids derived from source names, no timestamps. Full overwrite of the one output file; an empty result still writes the banner.

### Step 5: Wire and verify

Add the one `import "./<thing>.wcl"` line in `data/generated/main.wcl` (once, committed). Run the script twice — the second run must be byte-identical — then `wcl check` and render. If the extractor replaces hand-authored blocks, delete those and leave a comment pointing at the script.

> [!TIP]
> **Verification**
> Running the script twice leaves `git status` quiet; `wcl check` stays green; the output file's blocks render in the book.

## Related

- [Extractor scripts](../references/fact_extractor_anatomy.md)

- [Generated vs hand-authored data](../references/concept_generated_vs_hand.md)

[← Back to SKILL.md](../SKILL.md)
