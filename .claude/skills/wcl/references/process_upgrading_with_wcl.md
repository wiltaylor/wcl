# Upgrade a document when WCL moves

## Purpose

Bring a WCL model back in sync after installing a newer wcl release.

## Prerequisites

- A model with `source` blocks recording `reflects_version`, and a newly-installed `wcl` binary.

## Flowchart

![diagram](../_wdoc/process_upgrading_with_wcl-diagram-1.svg)

## Steps

### Step 1: Compare recorded and installed versions

```console
$ wcl --version
wcl 0.25.0-alpha
$ grep reflects_version wskill.wcl
  reflects_version = "0.24.1-alpha"
```

Read each source's `reflects_version` and compare it against the installed `wcl --version`. If they already match, stop — there is nothing to upgrade. Otherwise every source that lags marks content to re-verify.

### Step 2: Re-run wcl check on the model

```console
$ wcl check wskill.wcl
```

> [!NOTE]
> **Exit codes**
> 0 = valid, 1 = parse error, 2 = schema violation — a new release can tighten either.

Run `wcl check` on the model's root document with the new binary. A release can add constraints, change builtin behaviour, or deprecate syntax, so a file that was clean under the old version may now report errors.

### Step 3: Fix what the new version rejects

```console
$ wcl check wskill.wcl
ok            # after fixes
```

Work through the reported errors — each names the file, span, and rule. Fix the source (not the schema, unless the release genuinely changed the vocabulary) and re-run `wcl check` until it is clean.

### Step 4: Re-render the projections

```console
$ just render
```

Rebuild every shipped view so the generated output reflects the upgraded model. Spot-check a page or two — rendering can change even where validation stayed green.

### Step 5: Bump the version metadata

```console
$ wcl set wskill.wcl topics.wcl.version '"0.25.0-alpha"'
```

Record the upgrade: bump `topic.version`, and update each touched source's `last_checked` date and `reflects_version` to the new release. The next upgrade sweep starts from these fields.

> [!TIP]
> **Verification**
> `wcl check` is clean under the new binary, the projections rebuild, and every source's `reflects_version` matches `wcl --version`.

## Related

- [CLI Reference](../references/concept_cli.md)

- [wcl](../references/entity_wcl_cli.md)

- [Validate and format a WCL file](../references/process_validate_format.md)

[← Back to SKILL.md](../SKILL.md)
