# WCL — Wil's Configuration Language

A configuration language being rebuilt from scratch on the `rewrite` branch. The previous implementation lives on `main`; this branch starts over with a smaller, faster core. Expect the language surface to change while it stabilises.

## Status

Currently supports HCL-like fields and blocks:

```wcl
name = "alpha"
count = 3
enabled = true

service "web" {
  port = 8080
  metadata {
    region = "us-east-1"
  }
}
```

## Scaffolding a project

`wcl init` generates a new project folder from a template. A template is a
WCL document declaring `property` questions plus the `file` / `folder` blocks
to create; WCL ships a few built-ins.

```bash
wcl init --list                      # list built-in templates
wcl init minimal ./my-project        # prompts for each property
wcl init minimal ./app -D name=app --defaults   # non-interactive
wcl init minimal ./app --answers answers.json   # answers from a file (.wcl or .json)
```

Built-in templates: `minimal` (a single commented `main.wcl`), plus three
multi-folder wdoc projects — `page` (website), `book` (sidebar TOC), and
`presentation` (slide deck). The multi-folder ones lay out `main.wcl` with
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

## Browser editor

`wcl editor` serves a browser IDE for the current directory: a
gitignore-aware file tree, tabbed CodeMirror editing of any text file, WCL
language support (completion, hover, diagnostics from the built-in language
server), and a live preview pane that renders the wdoc page you are editing
— unsaved changes included — from the project's root document (an explicit
argument, else `./main.wcl`).

```bash
cd my-site && wcl editor          # then open the printed URL
```

## Layout

- `crates/wcl_lang` — parser and AST library (`wcl_lang::parse`, `wcl_lang::parse_file`)
- `crates/wcl` — `wcl` CLI binary (`wcl parse`, `wcl check`)
- `editor-ui/` — the `wcl editor` web frontend (SolidJS + Forge), embedded into the binary at build time
- `examples/` — sample input files

## Development

```bash
just workspace-build      # cargo build --workspace
just workspace-test       # cargo test --workspace
just workspace-lint       # clippy with -D warnings
just workspace-bench      # criterion benchmarks
just cli-run -- check examples/basic.wcl
```

Building the `wcl` crate builds the editor frontend when it is stale, which
needs `pnpm` on PATH (`editor-ui/`). Without node/pnpm, set
`WCL_EDITOR_UI_SKIP=1` to embed a placeholder page instead — everything else
works normally.

Editor + install:

```bash
just cli-install          # cargo install --path crates/wcl --locked
just vscode-build         # npm install + tsc compile (editors/vscode)
just vscode-package       # produce a .vsix via @vscode/vsce
just vscode-install       # install the .vsix into VS Code via `code --install-extension`
```

Run `just --list` to see every recipe grouped by purpose.

## License

WCL is licensed under the [MIT License](LICENSE). Copyright (c) 2026 Wil Taylor.
