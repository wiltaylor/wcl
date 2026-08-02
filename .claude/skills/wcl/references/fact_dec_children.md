# @children

`@children("kind")` declares a field that holds a **list** of nested blocks of a kind. If the
field's element type is a union, instances are dispatched to the matching variant by record
shape.


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

[← Back to SKILL.md](../SKILL.md)
