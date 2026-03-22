# WCL — Wil's Configuration Language

A statically-typed, block-structured configuration language with schemas, validation, macros, tables, queries, and a full LSP server.

```wcl
server web {
    host = "0.0.0.0"
    port = 8080
    workers = 4
}

schema "server" {
    host: string
    port: int @validate(min = 1, max = 65535)
    workers: int
}
```

## Quick Start

```rust
use wcl::{parse, ParseOptions, Value};

let doc = parse(r#"
    server web {
        port = 8080
        host = "localhost"
    }
"#, ParseOptions::default());

assert!(!doc.has_errors());

if let Some(Value::Map(server)) = doc.values.get("server") {
    println!("{:?}", server.get("web"));
}
```

## Links

- **Website**: [wcl.dev](https://wcl.dev)
- **Documentation**: [wcl.dev/docs](https://wcl.dev/docs/)
- **GitHub**: [github.com/wiltaylor/wcl](https://github.com/wiltaylor/wcl)

## License

MIT
