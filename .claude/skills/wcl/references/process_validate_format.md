# Validate and format a WCL file

## Purpose

Confirm a file conforms to its schema, then normalise its layout before committing.

## Prerequisites

- The file and any files it imports are on disk.

## Flowchart

![diagram](../_wdoc/process_validate_format-diagram-1.svg)

## Steps

### Step 1: Check it parses and validates

```console
$ wcl check config.wcl
ok
```

> [!NOTE]
> **Exit codes**
> 0 = valid, 1 = parse error, 2 = schema violation. Script against these for CI gates.

Run `wcl check config.wcl`. A clean run prints `ok` and exits 0; fix any reported parse or schema errors before moving on.

### Step 2: Format in place

```console
$ wcl fmt config.wcl --in-place
```

Run `wcl fmt config.wcl --in-place` to rewrite the file in canonical form. Indentation, brace style, number radix and string delimiters are normalised; comments and blank-line groupings survive.

### Step 3: Re-check the formatted file

```console
$ wcl check config.wcl
ok
```

Formatting never changes meaning, but re-run `wcl check` as a cheap confirmation that the rewrite is still valid.

> [!TIP]
> **Verification**
> `wcl check` prints `ok` and exits 0, and a second `wcl fmt` run produces no further changes.

## Related

- [CLI Reference](../references/concept_cli.md)

- [Block Schema](../references/concept_block_schema.md)

[← Back to SKILL.md](../SKILL.md)
