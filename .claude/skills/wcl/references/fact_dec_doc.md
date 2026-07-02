# @doc

`@doc` carries documentation metadata attached to a declaration. The text travels with the type or field so tooling can surface it in hovers, generated reference, and other introspection.

```wcl
@block("server") type Server {
  @doc("Listening port; must be free at startup.")
  port: u16
}
```

## Related

- [@block](../references/fact_dec_block.md)

[← Back to SKILL.md](../SKILL.md)
