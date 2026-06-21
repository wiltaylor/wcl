---
name: WCL
description: "Reference and processes for WCL. A typed configuration and schema language: records, unions, interfaces, decorators, and a document model that gathers and validates structured data. Use when working with WCL or answering questions about it."
wskill_schema_version: 1.1.0
allowed-tools: []
disallowed-tools: []
disable-model-invocation: false
---

# WCL

WCL is a typed configuration & schema language. This skill captures its full reference as data — the language, the builtin library, and the `wcl` CLI — projected from one model.

## Parameters

Values to pass when invoking this skill — reference them as `$ARGUMENTS`, `$1`, `$2`, … in the prompt.

| Parameter | Description | How to determine the value |
| --- | --- | --- |
| $ARGUMENTS | The WCL topic, builtin, or `wcl` subcommand to look up. | Take it from the user's request — e.g. the function name, type, or subcommand they asked about. If empty, summarise the reference and ask what they need. |
| $1 | Optional area to scope the answer to: `language`, `builtins`, or `cli`. | Infer from the question; default to searching all areas when unset. |

<Boundary>

**Always:**

- Cite the exact reference page when answering.

- Prefer the documented builtin/CLI form over guesses.

**Ask first:**

- Before running `wcl set` or any command that edits files.

**Never:**

- Invent builtins, flags, or syntax that aren't in the reference.

</Boundary>

## Reference

- [Language reference](references/language_ref.md) — the WCL language, area by area.

- [CLI reference](references/cli_ref.md) — the `wcl` CLI: every subcommand, its arguments and switches.
