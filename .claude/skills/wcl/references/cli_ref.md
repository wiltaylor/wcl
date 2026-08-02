# wcl — CLI reference

The WCL command-line interface — parse, check, evaluate, edit, format, diff, scaffold, the REPL, the language server, the browser editor, the wdoc generator, and the WAD helpers.

## Global switches

| Switch | Value | Description |
| --- | --- | --- |
| -h, --help | — | Print help and exit. |
| -V, --version | — | Print the wcl version and exit. |

## wcl parse

Parse a file and print the resulting document tree. Forces full evaluation, so every field, import and computed expression is exercised.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file. |

| Switch | Value | Description |
| --- | --- | --- |
| --profile | — | Record a call-tree profile of the document forcing and print it as JSON to stderr after the dump. |

```console
wcl parse site.wcl
```

## wcl check

Parse and validate against the schema. System imports resolve against the embedded wdoc library, so this is the fast edit-loop checker for wdoc projects too.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file, or `-` to read from stdin. |

| Switch | Value | Description |
| --- | --- | --- |
| --json | — | Emit the result as a JSON object on stdout (`ok`, `file`, `errors[]` with code / message / offset / length) instead of human-readable diagnostics. |

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
| --json | — | Emit the resolved value as JSON instead of the WCL display form. |
| --profile | — | Record a call-tree profile of the evaluation and print it as JSON to stderr after the value. |

```console
wcl eval site.wcl service.web.port
```

## wcl set

Update the field at a dotted path with a new WCL expression.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file (entry point — imports are followed). |
| path | required | Dotted path to the field whose value should be replaced. |
| value | required | New value, written as a WCL expression: strings, numbers with type suffixes, symbols, lists, records. |

```console
wcl set site.wcl service.web.port 9090u32
```

## wcl answer

Walk a document's pending `@answerable` interview questions (from `import <answer.wcl>`) and record the answers — arrow-key menus for choice questions, free text always available.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to the WCL document. |

| Switch | Value | Description |
| --- | --- | --- |
| --list | — | List the pending questions as JSON (id, prompt, kind, options, skippable) instead of prompting. |
| --id | ID | Answer one question non-interactively: the question block's label. |
| --text | TEXT | Free-text answer for `--id` (may combine with `--pick`). |
| --pick | OPTION | Pick an option by its id for `--id` (repeatable). |
| --skip | — | Skip the `--id` question: writes its declared skipped status. |

```console
wcl answer plan.wcl --id q_platforms --pick linux
```

## wcl fmt

Reformat to canonical form. Comments and blank-line groupings survive; indentation, brace style, number radix and string-delimiter choice are normalized.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file, or `-` to read from stdin and write the formatted source to stdout. |

| Switch | Value | Description |
| --- | --- | --- |
| --in-place | — | Overwrite the file in place (atomically). |
| --indent | N | Spaces per indentation level. |
| --no-trailing-comma | — | Strip the trailing comma the formatter places after every `match` arm. |

```console
wcl fmt site.wcl --in-place
```

## wcl diff

Compare two documents and print the changed entities and fields. Operates on the \*evaluated\* views (imports resolved), so a formatting-only edit produces no diff.

| Argument | Required | Description |
| --- | --- | --- |
| old | required | Old (base) document — a path or a `<rev>:<path>` git specifier. |
| new | required | New document — a path or a `<rev>:<path>` git specifier. |

| Switch | Value | Description |
| --- | --- | --- |
| --format | FORMAT | Output format. |

```console
wcl diff HEAD~1:config.wcl config.wcl
```

## wcl init

Scaffold a new project folder from a WCL template. The template declares `property` questions plus the `file` / `folder` blocks to generate.

| Argument | Required | Description |
| --- | --- | --- |
| template | optional | Built-in template name, a user template under `$XDG_DATA_HOME/wcl/templates/<name>/template.wcl`, or a path to a template `.wcl` file (or a folder holding `template.wcl`). |
| dest | optional | Destination directory. |

| Switch | Value | Description |
| --- | --- | --- |
| --answers | ANSWERS | Answer file (`.wcl` or `.json`) supplying property answers. |
| -D | KEY=VALUE | Supply a property answer inline (repeatable). Highest precedence. |
| --defaults | — | Non-interactive: never prompt; use defaults for unanswered properties. |
| --force | — | Write into the destination even if it already exists and is not empty. |
| --list | — | List the built-in templates and exit. |

```console
wcl init minimal ./app -D name=app --defaults
```

## wcl repl

Read-eval-print loop for ad-hoc WCL expressions. Without a file you can still evaluate self-contained expressions — arithmetic, string ops, builtin calls.

| Argument | Required | Description |
| --- | --- | --- |
| file | optional | Optional WCL file whose top-level fields the REPL should resolve identifiers against. |

```console
wcl repl site.wcl
```

## wcl lsp

Run the WCL language server. Defaults to stdio — the transport editors expect.

| Switch | Value | Description |
| --- | --- | --- |
| --tcp | HOST:PORT | Listen for inbound TCP connections instead of using stdio. Each connection runs as an independent LSP session. |
| --log | LOG | Write `tracing` log lines to this file. |

```console
wcl lsp
```

## wcl editor

Serve a browser-based editor for the current directory: a gitignore-aware file tree, CodeMirror editing with WCL language support and LSP (completion, hover, diagnostics), and a live wdoc preview built from the root document down.

| Argument | Required | Description |
| --- | --- | --- |
| root | optional | Root `.wcl` document. |

| Switch | Value | Description |
| --- | --- | --- |
| --addr | ADDR | Bind address, or `auto` to pick the first free port near 8080. |

```console
wcl editor main.wcl --addr 127.0.0.1:8139
```

## wcl wdoc

The wdoc static-site / skill generator. Has its own subcommands.

```console
wcl wdoc build wdoc/book/main.wcl --out out/book
```

### wcl wdoc build

Render every `page` block in the file to `<out>/<name>.html`.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file declaring one or more `page` blocks. |

| Switch | Value | Description |
| --- | --- | --- |
| --out | DIR | Output directory. Created if missing. |
| --site | SITE | Build only this named `site`, flat at `<out>`. |
| --profile | — | Record a call-tree profile of the document evaluation driving the build and print it as JSON to stderr. |

```console
wcl wdoc build wdoc/book/main.wcl --out out/book
```

### wcl wdoc markdown

Render every `page` block to a folder of Markdown files under `<out>` — one `.md` per page, with diagrams / terminals / wireframes written as standalone `.svg` files the Markdown references.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file declaring one or more `page` blocks. |

| Switch | Value | Description |
| --- | --- | --- |
| --out | DIR | Output directory. Created if missing. |
| --site | SITE | Build only this named `site`, flat at `<out>`. |

```console
wcl wdoc markdown docs/main.wcl --out docs/_md
```

### wcl wdoc skill

Render the file to an agent / Claude skill folder under `<out>`: the start page becomes `SKILL.md` (front matter from the site's `skill { }` block), every other page goes under `references/`, and `file` blocks ship into their `dir`.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file declaring a `:ai_skill` site. |

| Switch | Value | Description |
| --- | --- | --- |
| --out | DIR | Output directory (the skill folder). Created if missing. |
| --site | SITE | Build only this named `site`. |

```console
wcl wdoc skill wdoc/skill/main.wcl --out out/skill
```

### wcl wdoc pdf

Render each `site` to `<out>/<name>.pdf` — a pure-Rust PDF, no browser or external tools.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file declaring one or more `page` blocks. |

| Switch | Value | Description |
| --- | --- | --- |
| --out | DIR | Output directory. Created if missing. |
| --site | SITE | Render only this named `site`. |
| --page-size | PAGE_SIZE | Page size. |

```console
wcl wdoc pdf wdoc/book/main.wcl --out out/pdf
```

### wcl wdoc serve

Run a local dev server. Watches the source for `.wcl` changes but does not rebuild automatically — press Enter in the console (or `POST /__wdoc_rebuild`) to rebuild, then the browser reloads.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to a WCL source file declaring one or more `page` blocks. |

| Switch | Value | Description |
| --- | --- | --- |
| --addr | ADDR | Bind address, or `auto` to pick the first free port near 8080. |
| --out | DIR | Output directory. |
| --site | SITE | Serve only this named `site` (at `/`). |

```console
wcl wdoc serve wdoc/book/main.wcl --addr 127.0.0.1:8080
```

### wcl wdoc comments

List the review comments stored in the `comments.wcl` sidecars under the file's directory — left from the `wcl editor` preview pane — or `resolve <id>` to delete one.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to the WCL source file (the doc's entry point). |

| Switch | Value | Description |
| --- | --- | --- |
| --format | FORMAT | Output format. |
| --site | SITE | Restrict to one named `site`. |

```console
wcl wdoc comments docs/main.wcl --format json
wcl wdoc comments docs/main.wcl resolve c12ab3
```

### wcl wdoc review

Wait for a reviewer to finish, then print the comments — the agent side of the review handshake. Blocks until the reviewer clicks "Send to agent" in the preview pane of a running `wcl editor`, then lists the comments like `comments`.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to the WCL source file (the doc's entry point). |

| Switch | Value | Description |
| --- | --- | --- |
| --format | FORMAT | Output format for the comments printed once released. |

```console
wcl wdoc review docs/main.wcl
```

### wcl wdoc training

List the course answers stored in the `training.wcl` sidecars under the file's directory — left by a training site running under `wcl wdoc serve` — or grade one.

| Argument | Required | Description |
| --- | --- | --- |
| file | required | Path to the WCL source file (the doc's entry point). |

| Switch | Value | Description |
| --- | --- | --- |
| --format | FORMAT | Output format. |
| --pending | — | Only list answers still awaiting a grader. |

```console
wcl wdoc training wdoc/training/main.wcl --pending
```

## wcl wad

WAD (architecture document) helpers. Scaffold a WAD with `wcl init wad`.

```console
wcl wad spec --from HEAD~3 .wad/wad.wcl
```

### wcl wad spec

Derive a change-spec skeleton from a WAD diff: compare the working tree against a reviewed git revision (evaluated views, imports resolved from each side) and write a schema-valid `spec` block — status `:planning`, the exact entity/field change list, TODO rationale/instructions — into `data/specs/` beside the entry document.

| Argument | Required | Description |
| --- | --- | --- |
| entry | optional | WAD root document. |

| Switch | Value | Description |
| --- | --- | --- |
| --from | FROM | Reviewed baseline revision to diff from (any git rev). |
| --id | ID | Spec id — also the filename. |
| --title | TITLE | Spec title. |
| --include-specs | — | Keep changes to `spec` entities in the change list. |
| --format | FORMAT | What the command produces. |

```console
wcl wad spec --from v1.2 --id spec_billing --title "Billing split" wad.wcl
```

## wcl wskill

wskill helpers. Scaffold a wskill with `wcl init wskill`.

```console
wcl wskill check docs/wskills/wcl
```

### wcl wskill check

Validate one wskill or a collection, resolve artifact entries from each parsed model, build every declared projection in scratch space, and report per-view model coverage. Writes no generated output.

| Argument | Required | Description |
| --- | --- | --- |
| entry | optional | A wskill folder, an entry `.wcl`, or a directory containing wskills. |

```console
wcl wskill check docs/wskills
```

### wcl wskill install

Render every declared AI-skill artifact and install its skill folders and agents into a repository's `.claude/skills/` and `.claude/agents/` directories. Accepts one wskill or a collection.

| Argument | Required | Description |
| --- | --- | --- |
| entry | optional | A wskill folder, an entry `.wcl`, or a directory containing wskills. |

| Switch | Value | Description |
| --- | --- | --- |
| --repo | PATH | Repository root that owns `.claude/`. |
| --check | — | Render in scratch space and compare against the repository without writing. |

```console
wcl wskill install docs/wskills --repo . --check
```

### wcl wskill graph

Print the wskill's model — units, index trees, `related` and pin edges, per-unit block lists with the file and byte span each is written at, and the units no index pins — as JSON on stdout. Reads the data model only: no build, no editor.

| Argument | Required | Description |
| --- | --- | --- |
| entry | optional | The wskill folder, or an entry `.wcl` inside it. |

| Switch | Value | Description |
| --- | --- | --- |
| --rev | REV | Read the model at this git revision instead of the working tree. |

```console
wcl wskill graph docs/wskills/wcl --rev HEAD~1
```

### wcl wskill lint

Run every wskill rule over the model and report the findings — errors, warnings and curator candidates from one pass. Reads the data model only: no build, no editor, and lint never writes.

| Argument | Required | Description |
| --- | --- | --- |
| entry | optional | The wskill folder, or an entry `.wcl` inside it. |

| Switch | Value | Description |
| --- | --- | --- |
| --format | FORMAT | Output format: `text` (default, one line per finding) or `json` (an array of finding objects). |
| --severity | LIST | Report only these severities: `error`, `warn`, `candidate` (comma-separated, repeatable). Default: all three. |
| --deny | SEVERITY | Fail on findings this certain or more: `error` (default), `warn`, `candidate`. |

```console
wcl wskill lint docs/wskills/wcl --severity error --deny error
```

### wcl wskill audit

Diff the model across a git range: the union graph — before ∪ after, with removed units and edges marked removed — plus the findings each changed unit gained and a header of the health metrics that moved.

| Argument | Required | Description |
| --- | --- | --- |
| entry | optional | The wskill folder, or an entry `.wcl` inside it. |

| Switch | Value | Description |
| --- | --- | --- |
| --range | RANGE | The git range to audit. Default: `HEAD~1`. |
| --format | FORMAT | Output format: `text` (default, a header strip and one row per changed node) or `json` (the whole union graph, which is what an audit view renders). |

```console
wcl wskill audit docs/wskills/wcl --range main... --format json
```

### wcl wskill op

Apply structural ops to a wskill — the one id-addressed op vocabulary the browser editor writes through, as JSON on the command line.

| Argument | Required | Description |
| --- | --- | --- |
| entry | optional | The wskill folder, or any `.wcl` inside it. |

| Switch | Value | Description |
| --- | --- | --- |
| --op | JSON | One op, as JSON (repeatable). A JSON array of ops works too. |
| --file | PATH | Read the ops from a file (an op object or an array of them); `-` reads stdin. |
| --dry-run | — | Print the ops that would be applied and write nothing. |

```console
wcl wskill op docs/wskills/wcl --op '{"op":"pin_unit","index":"reference","unit":"alpha"}'
```
