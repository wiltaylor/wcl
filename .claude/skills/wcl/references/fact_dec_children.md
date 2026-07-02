# @children

`@children("kind")` declares a field that holds a **list** of nested blocks of a kind. If the field's element type is a union, instances are dispatched to the matching variant by record shape.

```wcl
@block("router") type Router {
  @children("route") routes: list<Route>
}

@block("route") type Route {
  @inline(0) path: utf8
}

router main {
  route "/users" {}
  route "/orders" {}
}
```

## Related

- [@child](../references/fact_dec_child.md)

- [@block](../references/fact_dec_block.md)

- [Document Schema](../references/concept_document_schema.md)

[← Back to SKILL.md](../SKILL.md)
