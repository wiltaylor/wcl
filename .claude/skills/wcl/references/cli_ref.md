# wcl — CLI reference

The WCL command-line interface — parse, check, evaluate, edit, format, the language server, and the wdoc generator.

## Global switches

| Switch | Value | Description |
| --- | --- | --- |
| -h, --help | — | Print help and exit. |
| -V, --version | — | Print the wcl version and exit. |

## wcl parse

Parse a file and print the resulting document tree (forces full evaluation).

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file. |

| Switch | Value | Description |
| --- | --- | --- |
| --profile | — | Write a call-tree profile as JSON to stderr. |

```console
wcl parse site.wcl
```

## wcl check

Parse and validate against the schema. Exit 0 = valid, 1 = parse error, 2 = schema violation.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | WCL source file, or `-` to read from stdin. |

| Switch | Value | Description |
| --- | --- | --- |
| --json | — | Emit the result as a JSON object instead of human-readable diagnostics. |

```console
wcl check site.wcl
```

## wcl eval

Resolve a dotted path from the document root and print the value.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file. |
| path | required | Dotted path to resolve from the document root. |

| Switch | Value | Description |
| --- | --- | --- |
| --json | — | Emit the value as JSON instead of the WCL display form. |
| --profile | — | Write an evaluation profile as JSON to stderr. |

```console
wcl eval site.wcl service.web.port
```

## wcl set

Update the field at a dotted path with a new WCL expression, following imports to the declaring file.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Entry-point WCL file (imports are followed). |
| path | required | Dotted path to the field to replace. |
| value | required | New value, written as a WCL expression. |

```console
wcl set site.wcl service.web.port 9090u32
```

## wcl answer

Walk a document's pending `@answerable` interview questions (from `import <answer.wcl>`) and record the answers — arrow-key menus for choice questions, free text always available, each answer written back immediately through the validating edit pipeline.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to the WCL document (imports are followed; answers land in the declaring file). |

| Switch | Value | Description |
| --- | --- | --- |
| --list | — | List the pending questions as JSON instead of prompting. |
| --id | ID | Answer one question non-interactively: the question block's label. |
| --text | TEXT | Free-text answer for `--id` (may combine with `--pick`). |
| --pick | OPTION | Pick an option by its id for `--id` (repeatable). |
| --skip | — | Skip the `--id` question: writes its declared skipped status. |

```console
wcl answer plan.wcl --id q_platforms --pick linux
```

## wcl fmt

Reformat to canonical form (comments and blank-line groupings preserved).

| Argument | Required | Description |
| --- | --- | --- |
| file | required | WCL source file, or `-` for stdin. |

| Switch | Value | Description |
| --- | --- | --- |
| --in-place | — | Overwrite the file in place instead of printing to stdout. |
| --indent | N | Spaces per indentation level (default 2). |
| --no-trailing-comma | — | Drop the trailing comma after match arms. |

```console
wcl fmt site.wcl --in-place
```

## wcl diff

Compare two documents (evaluated views) and print the changed entities and fields.

| Argument | Required | Description |
| --- | --- | --- |
| old | required | Left side — a file or a `<rev>:<path>` git specifier. |
| new | required | Right side — a file or a `<rev>:<path>` git specifier. |

| Switch | Value | Description |
| --- | --- | --- |
| --format | wcl\|json | Output format (default: a re-parseable WCL tree). |

```console
wcl diff HEAD~1:config.wcl config.wcl
```

## wcl init

Scaffold a new project folder from a WCL template.

| Argument | Required | Description |
| --- | --- | --- |
| template | required | Built-in name, user template, or path to a template .wcl. |
| dest | optional | Destination directory (default: the answered `name`). |

| Switch | Value | Description |
| --- | --- | --- |
| --list | — | List the built-in templates and exit. |
| --defaults | — | Use defaults for every property — no prompts. |
| -D, --define | KEY=VALUE | Preset a template property (repeatable). |

```console
wcl init minimal ./app -D name=app --defaults
```

## wcl repl

Read-eval-print loop for ad-hoc WCL expressions.

| Argument | Required | Description |
| --- | --- | --- |
| file | optional | Optional file whose document is in scope in the REPL. |

```console
wcl repl site.wcl
```

## wcl lsp

Run the WCL language server over stdio (for editor integrations).

```console
wcl lsp
```

## wcl wdoc

The wdoc static-site / skill generator. Has its own subcommands.

```console
wcl wdoc build wdoc/book/main.wcl --out out/book
```

### wcl wdoc build

Build a wdoc document into a static site.

| Argument | Required | Description |
| --- | --- | --- |
| entry | required | Path to the wdoc entry template (e.g. wdoc/book/main.wcl). |

| Switch | Value | Description |
| --- | --- | --- |
| --out | DIR | Output directory for the generated site. |

```console
wcl wdoc build wdoc/book/main.wcl --out out/book
```

### wcl wdoc skill

Project a wdoc :ai_skill target into a SKILL.md + references/ folder.

| Argument | Required | Description |
| --- | --- | --- |
| entry | required | Path to the wdoc skill entry template. |

| Switch | Value | Description |
| --- | --- | --- |
| --out | DIR | Output directory for the generated skill. |

```console
wcl wdoc skill wdoc/skill/main.wcl --out out/skill
```

### wcl wdoc pdf

Render a wdoc document to a PDF.

| Argument | Required | Description |
| --- | --- | --- |
| entry | required | Path to the wdoc entry template. |

| Switch | Value | Description |
| --- | --- | --- |
| --out | FILE | Output PDF path. |
| --page-size | A4\|Letter | Page size (default A4). |

```console
wcl wdoc pdf wdoc/book/main.wcl --out book.pdf
```

### wcl wdoc serve

Build and serve a wdoc site locally with live reload. Watches for `.wcl` changes; press Enter in the console (or `POST /__wdoc_rebuild`) to rebuild. Browser editing lives in `wcl editor`.

| Argument | Required | Description |
| --- | --- | --- |
| entry | required | Path to the wdoc entry template. |

| Switch | Value | Description |
| --- | --- | --- |
| --addr | ADDR | Address to bind (default 127.0.0.1:8080). |

```console
wcl wdoc serve wdoc/book/main.wcl --addr 127.0.0.1:8080
```
