# Comments

_Line comments with `//` or `#`, preserved across fmt and edits._

Two line-comment forms, `//` and `#`, both run to the end of the line. There are no block comments. Comments and blank-line grouping survive `wcl fmt` and the `wcl set` edit path.

```wcl
// A leading comment describes the next item.
service "web" {
  port   = 8080u32   // trailing same-line comment
  # hash comments work too
  region = "us-east-1"
}
```

## Related

- [Fields](../references/concept_fields.md)

- [Blocks](../references/concept_blocks.md)

[← Back to SKILL.md](../SKILL.md)
