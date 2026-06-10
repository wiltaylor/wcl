# CLI Reference

The `wcl` binary drives parsing, checking, evaluation, editing, formatting, the language server, and wdoc.

## wcl parse

Parse a file and print the resulting document tree (forcing full evaluation). `--profile` writes a call-tree profile as JSON to stderr.

```bash
wcl parse site.wcl
```

## wcl check

Parse and validate against the schema. Exit code `0` means valid, `1` a parse error, `2` a schema violation.

```bash
wcl check site.wcl
```

## wcl eval / wcl get

Resolve a dotted path from the document root and print the value. `--json` emits JSON; `--profile` writes a profile to stderr.

```bash
wcl eval site.wcl service.web.port
wcl get  site.wcl name --json
```

## wcl set

Update the field at a dotted path with a new WCL expression, following the import chain to the file that declares it. Quote shell-special values.

```bash
wcl set site.wcl name '"alpha"'
wcl set site.wcl service.web.port 9090u32
wcl set site.wcl accent :gold
```

## wcl fmt

Reformat to canonical form (comments and blank-line groupings preserved). `--in-place` overwrites; `--indent N` sets indent width; `--no-trailing-comma` drops the trailing comma after match arms.

```bash
wcl fmt site.wcl                 # to stdout
wcl fmt site.wcl --in-place
wcl fmt site.wcl --indent 4
```

## wcl repl

An interactive read-eval-print loop. Pass a file to resolve identifiers against its top-level fields. `:quit` or EOF exits.

```bash
wcl repl
wcl repl site.wcl
```

## wcl lsp

Run the language server (diagnostics, formatting, symbols, go-to-definition, completion, hover, code actions). Defaults to stdio; `--tcp ADDR` listens on a socket and `--log FILE` writes trace logs.

```bash
wcl lsp
wcl lsp --tcp 127.0.0.1:9257 --log /tmp/wcl-lsp.log
```

## wcl wdoc

Build or serve a wdoc documentation site. `build` renders HTML to `--out`; `serve` runs a live-reloading dev server; `pdf` renders one PDF per site (`--page-size a4|letter`, default `a4`); `markdown` (alias `md`) renders a folder of `.md` files with diagrams as standalone `.svg` files, aimed at AI / text consumers; `skill` renders an agent / Claude skill folder (`SKILL.md` + `references/` + `scripts/` + `assets/`) from a `:ai_skill` site. `--site NAME` filters to one named site.

```bash
wcl wdoc build docs/main.wcl --out docs/_site
wcl wdoc serve docs/main.wcl --addr 127.0.0.1:8080
wcl wdoc pdf docs/main.wcl --out target/pdf --page-size letter
wcl wdoc markdown docs/main.wcl --out docs/_md
wcl wdoc skill docs/my-skill.wcl --out ./my-skill
```
