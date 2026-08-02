# Member Access

_A dotted path reads a field from records, variants, and composites._

A dotted path reads a field. Access chains through records, variant payloads, and any
composite that exposes named members.


```wcl
region = service.metadata.region
deep   = config.services.web.metadata.region
```

[← Back to SKILL.md](../SKILL.md)
