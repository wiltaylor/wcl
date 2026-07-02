# Upgrading a wskill to a new base schema

## Purpose

Move an existing wskill onto a newer wskill base schema without losing content.

## Prerequisites

- A newer wskill scaffold is available (a newer `wcl` release).

## Flowchart

![diagram](../_wdoc/process_upgrading_schema_version-diagram-1.svg)

## Steps

### Step 1: Scaffold a reference copy

```console
$ wcl init wskill /tmp/wskill-ref --defaults
$ diff schema/base.wcl /tmp/wskill-ref/schema/base.wcl
```

Scaffold a throwaway wskill with the new `wcl` and diff its `schema/base.wcl` against yours — that diff IS the upgrade. Check the header's `Schema version:` line for how far apart you are.

### Step 2: Create any new topic-owned files FIRST

If the new base imports topic-owned files you don't have yet (e.g. `schema/kinds.wcl`), copy them from the reference scaffold BEFORE replacing base.wcl — a base that imports a missing file fails `wcl check` with a confusing unknown-type error. Your existing `kinds.wcl`/`extensions.wcl` are yours; keep them and merge any new baseline entries.

### Step 3: Replace the generated files

```console
$ cp /tmp/wskill-ref/schema/base.wcl schema/base.wcl
# template sets only if you never customised them:
$ diff -r wdoc/ /tmp/wskill-ref/wdoc/
```

Overwrite `schema/base.wcl` with the new one (it is generated — never hand-merged). Diff the `wdoc/` template sets too: take the new ones wholesale if you never customised them, otherwise port the diff into your customised copies.

### Step 4: Check and fix the data

```console
$ wcl check wskill.wcl        # every violation, file + line
```

Run `wcl check` and fix what it reports — renamed fields, newly constrained values (e.g. a free-text entity `kind` becoming a `:symbol` from kinds.wcl), new required fields. The errors are the migration checklist.

### Step 5: Bump schema_version and re-render

```console
$ just render && just book-serve
```

Set `schema_version` in `wskill.wcl` to the new base's version, re-render every shipped view, and spot-check the book. Commit the upgrade as one change.

> [!TIP]
> **Verification**
> `wcl check` passes on the new base, `schema_version` matches the base header, and every shipped view renders.

## Related

- [Creating a new wskill](../references/process_creating_a_wskill.md)

- [Structured data](../references/concept_structured_data.md)

- [The wskill folder layout](../references/fact_folder_layout.md)

[← Back to SKILL.md](../SKILL.md)
