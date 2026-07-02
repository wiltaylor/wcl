# CLI Reference

_The wcl binary: parse, check, eval, set, fmt, repl, lsp, init, wdoc, and diff._

The `wcl` binary drives parsing, checking, evaluation, editing, formatting, the language server, and wdoc.

## wcl parse

Parse a file and print the resulting document tree (forcing full evaluation). `--profile` writes a call-tree profile as JSON to stderr.

```console
wcl parse site.wcl
```

## wcl check

Parse and validate against the schema. Exit code `0` means valid, `1` a parse error, `2` a schema violation. System imports resolve against the embedded wdoc library, so this is the fast edit-loop checker for wdoc projects too.

```console
wcl check site.wcl
```

## wcl eval / wcl get

Resolve a dotted path from the document root and print the value. `--json` emits JSON; `--profile` writes a profile to stderr.

```console
wcl eval site.wcl service.web.port
wcl get  site.wcl name --json
```

## wcl set

Update the field at a dotted path with a new WCL expression, following the import chain to the declaring file. Quote shell-special values.

```console
wcl set site.wcl name '"alpha"'
wcl set site.wcl service.web.port 9090u32
wcl set site.wcl accent :gold
```

## wcl fmt

Reformat to canonical form (comments and blank-line groupings preserved). `--in-place` overwrites; `--indent N` sets indent width; `--no-trailing-comma` drops the trailing comma after match arms.

```console
wcl fmt site.wcl                 # to stdout
wcl fmt site.wcl --in-place
wcl fmt site.wcl --indent 4
```

## wcl repl

An interactive read-eval-print loop. Pass a file to resolve identifiers against its top-level fields. `:quit` or EOF exits.

```console
wcl repl
wcl repl site.wcl
```

## wcl lsp

Run the language server (diagnostics, formatting, symbols, go-to-definition, completion, hover, code actions). Defaults to stdio; `--tcp ADDR` listens on a socket and `--log FILE` writes trace logs.

```console
wcl lsp
wcl lsp --tcp 127.0.0.1:9257 --log /tmp/wcl-lsp.log
```

## wcl init

Scaffold a new project folder from a WCL template. The `<template>` is a built-in name (see `wcl init --list`), an installed user template, or a path to a template file or folder; `[dest]` is the destination directory. Built-in templates: `minimal`, plus three wdoc projects `page`, `book`, and `presentation`.

```console
wcl init --list                                  # list built-in templates
wcl init minimal ./my-project                    # prompt for each property
wcl init minimal ./app -D name=app --defaults    # non-interactive, no prompts
wcl init minimal ./app --answers answers.json    # answers from a file
wcl init ./my-template.wcl ./out                 # a template of your own
```

## wcl wdoc

Build or serve a wdoc documentation site. `build` renders HTML to `--out`; `serve` runs a live-reloading dev server; `pdf` renders one PDF per site; `markdown` renders a folder of `.md` files for AI consumers; `skill` renders a Claude skill folder (`SKILL.md` + `references/` + `assets/`) from a `:ai_skill` site. `--site NAME` filters to one named site.

```console
wcl wdoc build docs/main.wcl --out docs/_site
wcl wdoc serve docs/main.wcl --addr 127.0.0.1:8080
wcl wdoc pdf docs/main.wcl --out target/pdf --page-size letter
wcl wdoc markdown docs/main.wcl --out docs/_md
wcl wdoc skill docs/my-skill.wcl --out ./my-skill
```

## wcl diff

Compare two documents and print the changed entities and fields. The comparison is over the evaluated document views (imports resolved), so a formatting-only edit yields an empty diff. The default output is a re-parseable WCL tree; `--format json` gives a flat array of change objects. Either side may be a `<rev>:<path>` git specifier.

```console
wcl diff old.wcl new.wcl
wcl diff old.wcl new.wcl --format json
wcl diff HEAD~1:config.wcl config.wcl    # a committed version vs the working tree
wcl diff main:a.wcl feature:a.wcl        # across two branches
```

## Related

- [Imports & Modules](../references/concept_imports.md)

[← Back to SKILL.md](../SKILL.md)
