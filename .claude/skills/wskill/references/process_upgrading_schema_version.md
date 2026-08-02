# Upgrading a wskill to a new base schema

## Purpose

Move an existing wskill onto a newer wskill base schema without losing content.

## Prerequisites

- A newer `wcl` release is installed (the base schema ships inside the binary).

## Flowchart

![diagram](../_wdoc/process_upgrading_schema_version-diagram-1.svg)

## Steps

### Step 1: Install the new wcl

```console
$ wcl --version
```

There is nothing to copy: the base schema and the shared projection templates are embedded in the binary and reach your wskill through `import <wskill.wcl>` / `import <wskill/book.wcl>`. Upgrading the toolchain IS the upgrade — which is also why the whole set moves together and never half-moves.

### Step 2: Add any new topic-owned files

The new base may expect a topic-owned file you don't have yet (the vocabularies in `schema/kinds.wcl` are the standing example — the base references `EntityKind` and `ArtifactKind` but cannot ship them). Scaffold a throwaway wskill (`wcl init wskill /tmp/wskill-ref --defaults`) and copy what's missing. Your `kinds.wcl` / `extensions.wcl` are yours; keep them and merge any new baseline entries.

### Step 3: Re-check anything you took over

```console
$ grep -rn '^import' wdoc/
```

A part you overrode — a book main you declared yourself, a page you replaced — is a copy the upgrade cannot reach. List your projection entries' imports: whatever they do NOT import from `<wskill/…>` is yours to port by hand. Everything they do import is already current.

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
>
> `wcl check` passes, `schema_version` matches the new base, and every shipped view renders.

## Related

- [Creating a new wskill](../references/process_creating_a_wskill.md)

- [Structured data](../references/concept_structured_data.md)

- [The wskill folder layout](../references/fact_folder_layout.md)

[← Back to SKILL.md](../SKILL.md)
