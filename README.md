[![CI](https://github.com/wiltaylor/wcl/actions/workflows/ci.yml/badge.svg)](https://github.com/wiltaylor/wcl/actions/workflows/ci.yml)
[![Deploy Site](https://github.com/wiltaylor/wcl/actions/workflows/deploy-site.yml/badge.svg)](https://github.com/wiltaylor/wcl/actions/workflows/deploy-site.yml)

# WCL — Wil's Configuration Language

A statically-typed, block-structured configuration language with first-class support for composition, validation, and tooling.

```wcl
server web-prod {
    host = "0.0.0.0"
    port = 8080
    workers = max(4, 2)
}

schema "server" {
    host: string @optional
    port: int @validate(min = 1024, max = 65535)
    workers: int
}
```

## Documentation

- **Website** — [wcl.dev](https://wcl.dev)
- **Docs** — [wcl.dev/docs](https://wcl.dev/docs/)

## Packages

| Language | Package | Install |
|----------|---------|---------|
| Rust | `wcl` | `cargo add wcl` |
| Python | `pywcl` | `pip install pywcl` |
| JavaScript | `wcl-wasm` | `npm install wcl-wasm` |
| Go | `github.com/wiltaylor/wcl/bindings/go` | `go get github.com/wiltaylor/wcl/bindings/go` |
| .NET | `WclLang` | `dotnet add package WclLang` |
| Java/JVM | `io.github.wiltaylor:wcl` | Gradle/Maven |
| Ruby | `wcl` | `gem install wcl` |
| Zig | `wcl` | `zig fetch --save git+https://github.com/wiltaylor/wcl` |
| C/C++ | `libwcl_ffi` | Link against static library |

## Contributing

Contributions are welcome! If you find a bug or have a feature request, please [open an issue](https://github.com/wiltaylor/wcl/issues).

## License

WCL is licensed under the [MIT License](LICENSE).

Copyright (c) 2026 Wil Taylor
