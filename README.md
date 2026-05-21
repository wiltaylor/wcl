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
just build   # cargo build --workspace
just test    # cargo test --workspace
just lint    # clippy with -D warnings
just bench   # criterion benchmarks
just run -- check examples/basic.wcl
```

## License

WCL is licensed under the [MIT License](LICENSE). Copyright (c) 2026 Wil Taylor.
