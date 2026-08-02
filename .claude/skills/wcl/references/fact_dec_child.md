# @child

`@child("kind")` declares a field that holds a **single** nested block of the given kind. Make
the field optional (`Type?`) when the child may be omitted. Contrast with `@children`, which
holds a list.


```wcl
@block("service") type Service {
  @child("metadata") meta: Metadata?
}

@block("metadata") type Metadata {
  owner: utf8
}

service api {
  metadata { owner = "platform" }
}
```

## Related

- [@children](../references/fact_dec_children.md)

[← Back to SKILL.md](../SKILL.md)
