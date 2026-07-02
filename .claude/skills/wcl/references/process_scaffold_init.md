# Scaffold a new project with wcl init

## Purpose

Start a new WCL project from a built-in template instead of an empty file.

## Prerequisites

- You have chosen a destination directory that is empty (or will use `--force`).

## Flowchart

![diagram](../_wdoc/process_scaffold_init-diagram-1.svg)

## Steps

### Step 1: List the available templates

```console
$ wcl init --list
Built-in templates:
  minimal
  page
  book
  presentation
```

Run `wcl init --list` to see the built-in templates: `minimal` (a single commented `main.wcl`) plus the multi-folder wdoc projects `page`, `book` and `presentation`.

### Step 2: Scaffold from a template

```console
$ wcl init minimal ./my-project
Project name [my-project]:
created ./my-project
```

Run `wcl init <template> <dest>`, e.g. `wcl init minimal ./my-project`. You are prompted for each `property` the template declares; pass `-D name=value` to answer inline or `--defaults` to skip prompts.

### Step 3: Validate the generated project

```console
$ wcl check ./my-project/main.wcl
ok
```

Change into the new folder and run `wcl check` on its `main.wcl` to confirm the scaffold validates before you start editing.

> [!TIP]
> **Verification**
> The destination contains the template's files and `wcl check` on its entry `main.wcl` exits 0.

## Related

- [CLI Reference](../references/concept_cli.md)

[← Back to SKILL.md](../SKILL.md)
