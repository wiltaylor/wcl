# Inspect and edit values with eval, set and diff

## Purpose

Read a single field, change it from the command line, and confirm exactly what changed.

## Prerequisites

- A valid WCL file whose fields you want to read or edit.

## Flowchart

![diagram](../_wdoc/process_inspect_values-diagram-1.svg)

## Steps

### Step 1: Read a field with eval

```console
$ wcl eval site.wcl service.web.port
80
```

Run `wcl eval site.wcl <dotted.path>` (aliased `wcl get`) to resolve a path from the document root and print its value. Add `--json` for machine-readable output.

### Step 2: Edit a field with set

```console
$ wcl set site.wcl service.web.port 9090u32
```

> [!WARNING]
> **Quoting**
> The value is parsed as a WCL expression — quote shell-special characters, e.g. `wcl set site.wcl name '"alpha"'`.

Run `wcl set site.wcl <path> <value>` to rewrite the field. `wcl set` follows the import chain and edits the file that actually declares the field.

### Step 3: Confirm the change with diff

```console
$ wcl diff HEAD:site.wcl site.wcl
modified "service:web" {
  field "port" { kind = :changed  old = 80u32  new = 9090u32 }
}
```

Run `wcl diff <old> <new>` over the evaluated views to see exactly what changed. Either side may be a `<rev>:<path>` git specifier, so you can diff the working tree against a committed version.

> [!TIP]
> **Verification**
> `wcl eval` returns the new value and `wcl diff` reports a single modified field — confirming the edit landed where you intended.

## Related

- [CLI Reference](../references/concept_cli.md)

- [References](../references/concept_references.md)

[← Back to SKILL.md](../SKILL.md)
