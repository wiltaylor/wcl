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

## Layout

- `crates/wcl_lang` — parser and AST library (`wcl_lang::parse`, `wcl_lang::parse_file`)
- `crates/wcl` — `wcl` CLI binary (`wcl parse`, `wcl check`)
- `examples/` — sample input files

## Development

```bash
just workspace-build      # cargo build --workspace
just workspace-test       # cargo test --workspace
just workspace-lint       # clippy with -D warnings
just workspace-bench      # criterion benchmarks
just cli-run -- check examples/basic.wcl
```

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
