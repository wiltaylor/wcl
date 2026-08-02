# @document

`@document` marks a type as the document-root schema. The toolchain gathers every block
instance that matches the schema's `@child`/`@children` fields and assembles them into one
document value that `wcl check` validates.


Document schemas compose per namespace: when several `@document` types are visible (for
example a library's and your own), they **merge**, so you can import a base schema and still
add your own top-level fields.


```wcl
@document type Config {
  @children("server") servers: list<Server>
}

@block("server") type Server {
  @inline(0) name: identifier
  port: u16
}

server web { port = 8080u16 }   // instance fields use `=`, not the `:` of a type
```

## Related

- [@children](../references/fact_dec_children.md)

[← Back to SKILL.md](../SKILL.md)
