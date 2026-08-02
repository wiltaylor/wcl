# @inline

`@inline(slot)` makes a schema field positional by zero-based slot index. On a block schema it
binds a positional label to the field; on a decorator schema it binds a positional argument.
Multiple `@inline(n)` decorators expose multiple positions, so `route "GET" "/users"` fills
slot `0` and slot `1`. A decorator slot without `@inline` is named-only.


```wcl
@block("route") type Route {
  @inline(0) method: utf8
  @inline(1) path: utf8
}

route "GET" "/users" {}
```

[← Back to SKILL.md](../SKILL.md)
