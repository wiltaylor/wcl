# WCL — Wil's Configuration Language

WCL is a typed configuration language with schemas, expressions, functions, and
imports. A document declares its schema alongside its data, so `wcl check` can
validate it before another program reads it.

The same language powers **wdoc**, which renders documentation to HTML websites,
Markdown, and PDF. The `wcl` binary also provides formatting, field editing,
evaluated diffs, a REPL, and an LSP server for editor support.

WCL is pre-release software. The language and APIs may change while they stabilise.

## Install

From a checkout with Rust and Cargo installed:

```bash
cargo install --path crates/wcl --locked
wcl --version
```

For prebuilt binaries, see the [releases](https://github.com/wiltaylor/wcl/releases)
and the [installer](install.sh). The installer requires `--pre` for pre-releases.
The [reference manual](https://wcl.dev/reference/) covers the language, CLI, and wdoc.

## Quickstart

This `config.wcl` declares a server schema and one server:

```wcl
@block("server") type Server {
  @inline(0) id: identifier
  host: utf8
  @default(8080) port: u16
}

@document type Config {
  @children("server") servers: list<Server>
}

server web {
  host = "localhost"
}
```

Validate it, then read the port supplied by the schema default:

```bash
wcl check config.wcl                  # prints OK
wcl get config.wcl servers.web.port   # prints 8080
```

Change `host` to a number and `wcl check` reports a schema violation.

## Scaffolding a project

`wcl init` generates a new project folder from a template. A template is a
WCL document declaring `property` questions plus the `file` / `folder` blocks
to create; WCL ships a few built-ins.

```bash
wcl init --list                      # list built-in templates
wcl init minimal ./my-project        # prompts for each property
wcl init minimal ./app -D name=app --defaults   # non-interactive
```

Built-in templates include `minimal` (a single commented `main.wcl`),
`website` (custom HTML layout), and three multi-folder wdoc projects:
`page`, `book` (sidebar TOC), and `presentation` (slide deck).
The `page`, `book`, and `presentation` templates lay out `main.wcl` with
`schema/`, `data/`, and `wdoc/` folders, and project custom data into one
generated page per entry:

```bash
wcl init page ./my-site -D name="My Site" --defaults
wcl wdoc build ./my-site/main.wcl --out ./my-site/_site
```

`<template>` may also be a path to your own template `.wcl` file, or the name
of a user template installed under `$XDG_DATA_HOME/wcl/templates/<name>/`
(default `~/.local/share/wcl/templates/<name>/`) as a folder containing a
`template.wcl` — these show up in `wcl init --list` too. Inside a template, a
file's contents pull in answers with the `answer("name")` builtin (in an
interpolating heredoc):

```wcl
import <scaffold.wcl>

property "name" {
  prompt  = "Project name"
  default = "my-project"
}

file "main.wcl" {
  content = $<<WCL
// ${answer("name")}
WCL
}
```

## Layout

- `crates/wcl_lang` — parser, evaluator, schema validation, formatting, and document APIs
- `crates/wcl` — `wcl` CLI binary
- `crates/wcl_lsp` — language server behind `wcl lsp`
- `crates/wcl_wdoc` — documentation schemas and HTML, Markdown, and PDF rendering
- `editors/` — VS Code extension and tree-sitter grammar
- `examples/` — sample configurations and wdoc documents
- `docs/reference/` — reference manual source, written in WCL

## Development

```bash
just workspace-build      # cargo build --workspace
just workspace-test       # cargo test --workspace
just workspace-lint       # clippy with -D warnings
just workspace-bench      # criterion benchmarks
just cli-run -- check examples/basic.wcl
```

Install + editor integrations:

```bash
just cli-install          # release build installed to ~/.local/bin or WCL_INSTALL_DIR
just vscode-build         # npm install + tsc compile (editors/vscode)
just vscode-package       # produce a .vsix via @vscode/vsce
just vscode-install       # install the .vsix into VS Code via `code --install-extension`
```

Run `just --list` to see every recipe grouped by purpose.

## License

WCL is licensed under the [MIT License](LICENSE). Copyright (c) 2026 Wil Taylor.
