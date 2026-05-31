---
name: wcl
description: "Author, validate and format WCL — the typed configuration language — and wdoc documents. Use when writing or editing .wcl files, WCL schemas or builtins, or wdoc documentation sites."
---

# Authoring WCL

WCL is a typed configuration language built around **fields** (`name = value`) and **blocks** (`kind "label" { … }`), with a real type system, first-class functions, pattern matching, and a schema layer that validates a document's structure. `wdoc` — the static-site / document generator these references are written in — is one WCL application built on that base.

> [!NOTE]
> **When to use this skill**
> Reach for it whenever you write or edit `.wcl` files: authoring or reading WCL configuration, defining `type` / `interface` / `union` schemas and `@decorator`s, calling builtins, or building wdoc documentation. The `references/` folder is the full language, builtin, CLI and wdoc reference — consult the page for the feature you're touching before guessing syntax.

## Validate your work

WCL has a CLI. After writing or editing a document, check it and format it:

```bash
wcl check path/to/file.wcl      # parse + schema-validate; reports diagnostics
wcl fmt path/to/file.wcl        # canonical formatting (use --check in CI)
wcl get path/to/file.wcl field  # evaluate and print one field's value
wcl eval path/to/file.wcl       # evaluate the whole document to JSON
```

The full command set — `parse`, `check`, `eval` / `get`, `set`, `fmt`, `repl`, `lsp`, and the `wdoc` subcommands — is in the [CLI reference](references/cli.md).

## Language reference

One page per feature. Start with [fields & blocks](references/fields_blocks.md) (the core shape) and [schema & decorators](references/schema.md) (how documents are typed and validated).

- [Fields & blocks](references/fields_blocks.md) — the `name = value` / `kind "label" { … }` foundation
- [Tables](references/tables.md) — the `@table` row shorthand for repeated blocks
- [Identifiers](references/identifiers.md) — what may be a name
- Types: [numbers](references/numbers.md), [strings](references/strings.md), [booleans](references/booleans.md), [symbols](references/symbols.md), [optionals](references/optionals.md), [lists](references/lists.md), [tensors](references/tensors.md), [records](references/records.md)
- [Interfaces](references/interfaces.md), [references](references/references.md) (`&T`) and [unions](references/unions.md) — structural typing and variants
- [Connections](references/connections.md) — typed relationships between blocks
- [Expressions](references/expressions.md), [functions](references/functions.md) and [control flow](references/control_flow.md) — computation, `fn` literals, pattern matching
- [Schema & decorators](references/schema.md) — `@document` / `@block` / `@child(ren)` / `@inline` / `@default` / `@table` and the type system
- [Imports & modules](references/imports.md) — quoted disk imports and `<angle-bracket>` system imports

## Builtins & CLI

- [Builtin functions](references/builtins.md) — collections, strings, lists, math, tensors and control-flow helpers
- [CLI reference](references/cli.md) — every `wcl` subcommand and its flags

## wdoc (documentation generator)

If the task involves building docs, sites, PDFs or skills with WCL, start at the [wdoc overview](references/wdoc_overview.md). Key pages: [sites & templates](references/wdoc_sites.md), [pages](references/wdoc_pages.md), [formatting](references/wdoc_formatting.md), [diagrams](references/wdoc_diagrams.md), [Markdown output](references/wdoc_markdown.md) and [skill folders](references/wdoc_skills.md). The rest of the `references/` folder covers charts, timelines, terminals, maps, icons, images, math and styling.

> [!TIP]
> **Authoring tips**
> Prefer the declared schema: bare record literals coerce to the matching union variant by shape. `let name = expr` items are composition helpers — resolvable by name but invisible to the document's fields / JSON / schema. When unsure of a builtin's signature or a block's fields, open its reference page rather than inferring.
