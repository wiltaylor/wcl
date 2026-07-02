# Member Access

_A dotted path reads a field from records, variants, and composites._

A dotted path reads a field. Access chains through records, variant payloads, and any composite that exposes named members.

```wcl
region = service.metadata.region
deep   = config.services.web.metadata.region
```

## Related

- [Function Calls](../references/concept_function_calls.md)

- [Fields](../references/concept_fields.md)

- [Blocks](../references/concept_blocks.md)

[← Back to SKILL.md](../SKILL.md)
