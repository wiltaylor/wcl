# WCL CLI

Source of truth: `crates/wcl/src/cli/mod.rs`. All commands accept `--lib-path <DIR>` (repeatable) and `--no-default-lib-paths` where they resolve imports.

## `wcl validate <file>`

Run the full pipeline and report diagnostics.

Flags: `--strict` (warnings as errors), `--schema <path>`, `--var KEY=VALUE` (repeatable).

Exit: 0 OK, 1 errors (or warnings with `--strict`).

```bash
wcl validate config.wcl
wcl validate config.wcl --strict
wcl validate config.wcl --var env=prod --var region=us-east-1
```

## `wcl fmt <file>`

Format or check.

Flags: `--write` (modify file in place), `--check` (exit 1 if not formatted; no output).

```bash
wcl fmt config.wcl --write
wcl fmt config.wcl --check
```

## `wcl eval <file> [expression]`

Evaluate and optionally project via a query/selector.

Flags: `--format wcl|json` (default `wcl`), `--var KEY=VALUE` (repeatable).

```bash
wcl eval config.wcl
wcl eval config.wcl "service"
wcl eval config.wcl ".port"
wcl eval config.wcl "service | .port"
wcl eval config.wcl --format json
```

## `wcl lsp`

Start the language server.

Flags: `--tcp <addr>` (defaults to stdio).

```bash
wcl lsp                    # stdio — editor integration
wcl lsp --tcp 127.0.0.1:9257
```

## `wcl set <file> <spec>`

Set an attribute on blocks matching a query. Spec form: `<selector> ~> .path = <expr>`.

```bash
wcl set config.wcl 'service ~> .port = 9000'
wcl set config.wcl 'service#api ~> .env = "staging"'
wcl set config.wcl 'database[.env == "prod"] ~> .replicas = 5'
```

## `wcl add <file> <spec>`

Add a top-level item or insert into matching blocks.

```bash
wcl add config.wcl 'service api { port = 8080 }'
wcl add config.wcl 'service ~> port = 8080'
wcl add config.wcl 'service#web ~> host = "localhost"'
```

## `wcl remove <file> <spec>`

Remove blocks or attributes matching a pipeline.

```bash
wcl remove config.wcl 'service#deprecated'
wcl remove config.wcl 'service ~> .debug_port'
wcl remove config.wcl 'database[.env == "staging"]'
```

## `wcl table <action>`

### `wcl table insert <file> <table> <values>`
Pipe-delimited row.

```bash
wcl table insert config.wcl users '"alice" | "engineering" | true'
```

### `wcl table remove <file> <table> --where <cond>`
```bash
wcl table remove config.wcl users --where 'name == "alice"'
```

### `wcl table update <file> <table> --where <cond> --set <assignments>`
```bash
wcl table update config.wcl users \
  --where 'name == "alice"' \
  --set 'role = "admin", max_items = 500'
```

## `wcl docs <files>`

Generate schema docs as an mdBook.

Flags: `--output <dir>` (default `docs-out`), `--title <title>` (default `WCL Schema Reference`).

```bash
wcl docs schema.wcl --output ./api-docs --title "API Schema"
```

## `wcl transform run <name> --file <file>`

Execute a named transform from `<file>`.

Flags: `--input <file>` (stdin if omitted), `--output <file>` (stdout if omitted), `--param KEY=VALUE` (repeatable).

```bash
wcl transform run normalize --file transforms.wcl --input data.json --output out.json
wcl transform run normalize --file transforms.wcl --param format=json < input.txt
```

## `wcl wdoc <action>` (built with `wdoc` feature)

### `wcl wdoc build <files>`
Render wdoc documents to static HTML.

Flags: `--output <dir>` (default `wdoc-out`), `--var KEY=VALUE`.

### `wcl wdoc validate <files>`
Structure-only validation.

### `wcl wdoc serve <files>`
Dev server with live reload. Flags: `--port <num>` (default `3000`), `--open`.

### `wcl wdoc install-library`
Install `wdoc.wcl` into user library dir so editors/LSP resolve `import <wdoc.wcl>`.

Flags: `--force` (overwrite).

```bash
wcl wdoc build site.wcl --output ./html
wcl wdoc serve site.wcl --open
wcl wdoc install-library --force
```
