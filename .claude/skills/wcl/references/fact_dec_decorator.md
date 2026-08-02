# @decorator

`@decorator("name")` makes a type the schema for a user-defined `@name` decorator's arguments.
This lets you declare your own decorators with typed, validated arguments rather than relying
only on the built-in set.


```wcl
@decorator("range") type Range {
  @inline(0) min: i64
  @inline(1) max: i64
}

@block("field") type Field {
  @range(0, 100) score: i64
}
```

[← Back to SKILL.md](../SKILL.md)
