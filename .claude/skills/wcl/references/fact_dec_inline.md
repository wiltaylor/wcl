# @inline

`@inline(slot)` binds a block's positional label to a field by zero-based slot index. Multiple `@inline(n)` decorators expose multiple labels, so `route "GET" "/users"` fills slot `0` and slot `1`.

```wcl
@block("route") type Route {
  @inline(0) method: utf8
  @inline(1) path: utf8
}

route "GET" "/users" {}
```

## Related

- [@block](../references/fact_dec_block.md)

- [Blocks](../references/concept_blocks.md)

[← Back to SKILL.md](../SKILL.md)
