# @default

`@default(expr)` supplies the value a field takes when the author omits it. The expression is
any constant value of the field's type.


Absent-equivalent defaults — an empty list, `false`, an empty string — are always safe to
emit, since they behave the same as leaving the field out.


```wcl
@block("server") type Server {
  @default(8080) port: u16
  @default([]) tags: list<utf8>
}

server web {}   // port = 8080, tags = []
```

[← Back to SKILL.md](../SKILL.md)
