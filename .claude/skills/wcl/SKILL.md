---
name: WCL
description: "Reference and processes for WCL. A typed configuration and schema language: records, unions, interfaces, decorators, and a document model that gathers and validates structured data. Use when working with WCL or answering questions about it."
wskill_schema_version: 1.0.0
allowed-tools: []
disallowed-tools: []
disable-model-invocation: false
---

# WCL

A typed configuration and schema language: records, unions, interfaces, decorators, and a document model that gathers and validates structured data.

**Upstream version:** `0.24.1-alpha`. If the real upstream has moved past this, the skill may be stale — bump `topic.version` and re-verify (see the update workflow).

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

### Quick Start

_Get WCL running in a few minutes: declare a type, write data, check and evaluate._

## Install

WCL is pre-release only for now, so install the newest pre-release with the install script:

```console
curl -fsSL https://wcl.dev/install.sh | sh -s -- --pre
```

On a platform without a prebuilt binary (e.g. macOS), build from source with Cargo instead:

```console
cargo install --git https://github.com/wiltaylor/wcl -p wcl --locked
```

If `~/.local/bin` is not on your `PATH`, add it. Verify with `wcl --version`.

## A minimal document

Declare a block type, point a `@document` at it, then write an instance:

```wcl
@block("server") type Server {
  @inline(0) id: identifier
  host: utf8
  @default(8080) port: u16
}
@document type Config { @children("server") servers: list<Server> }

server web { host = "localhost" }
```

## Check and evaluate

Validate the document against its schema, then evaluate it:

```console
$ wcl check config.wcl     # type-checks the document, reports errors
$ wcl eval config.wcl      # prints the evaluated data
```

### Language

_The WCL language, area by area — syntax, types, expressions, control flow, functions, modules, and schema._

### Tasks

_Step-by-step runbooks for the day-to-day wcl workflows._

- [Validate and format a WCL file](references/process_validate_format.md)

- [Inspect and edit values with eval, set and diff](references/process_inspect_values.md)

- [Scaffold a new project with wcl init](references/process_scaffold_init.md)

- [Upgrade a document when WCL moves](references/process_upgrading_with_wcl.md)

- [Builtin functions](references/builtins_ref.md) — every builtin, grouped by category

- [CLI reference](references/cli_ref.md) — the `wcl` CLI — every subcommand, its arguments and switches

## Views

Beyond this skill, the wskill ships these views — build them with `just render` in the wskill folder:

- **book** (`wdoc/book/main.wcl`)

- **ai skill** (`wdoc/skill/main.wcl`)

- **presentation** — WCL in a nutshell — an overview deck. (`wdoc/presentation/main.wcl`)

- **training** — Learn WCL — a hands-on lesson series. (`wdoc/training/main.wcl`)
