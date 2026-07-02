# let bindings

_A let names a reusable value or function; resolves by name but is not document data._

A `let` names a reusable value or function. It resolves by name in sibling and descendant expressions but is **not** document data — it never appears in output, JSON, or schema validation. Lexically scoped, lazily evaluated, cycle-detected.

## Item form

At file scope or inside any block, `let name = expr` introduces a name. No terminator.

```wcl
let base_port = 8080u32

service "web" {
  port = base_port
}
service "api" {
  port = base_port + 1u32
}
```

Because a `let` can bind a function, it doubles as a composition helper. See [Function Literals](../references/concept_function_literals.md).

```wcl
let scale = fn(p: f64) -> f64 p * 2.0

a = scale(3.0)    // 6.0
b = scale(4.5)    // 9.0
```

## Related

- [Block Expressions](../references/concept_block_expressions.md)

- [Function Literals](../references/concept_function_literals.md)

- [fn Items](../references/concept_fn_items.md)

[← Back to SKILL.md](../SKILL.md)
