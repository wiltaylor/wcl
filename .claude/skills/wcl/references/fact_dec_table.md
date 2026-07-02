# @table

`@table("kind")` marks a row type as the schema for pipe-table syntax. Each `| ... |` row is parsed against the type's fields in declaration order, giving terse tabular data with full schema checking.

```wcl
@table("user") type User {
  name: utf8
  age:  u8
}

user
| name    | age |
| Ada     | 36  |
| Grace   | 45  |
```

## Related

- [Tables](../references/concept_tables.md)

- [@block](../references/fact_dec_block.md)

[← Back to SKILL.md](../SKILL.md)
