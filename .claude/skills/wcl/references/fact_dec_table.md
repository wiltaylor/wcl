# @table

`@table("kind")` marks a row type as the schema for pipe-table syntax. Each `| ... |` row is
parsed against the type's fields in declaration order, giving terse tabular data with full
schema checking.


```wcl
@table("user") type User {
  name: utf8
  age:  u8
}

@document type Roster {
  users: list<User>
}

users:                    // the FIELD name, with a colon; rows follow
  | "Ada"   | 36 |
  | "Grace" | 45 |
```

[← Back to SKILL.md](../SKILL.md)
