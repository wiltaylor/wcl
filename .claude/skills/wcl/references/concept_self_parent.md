# self & parent

_Keywords that navigate the enclosing scope — self is the current block, parent the one around it._

Inside a block, the `self` and `parent` keywords are navigators into the document — the same kind of lazy reference a `&T` field holds. `self` resolves to the **current** block (the innermost enclosing scope); `parent` resolves to the block **around** it. Both walk the lexical scope chain upward, so a field can read a sibling or an ancestor without naming the whole path.

## self

`self` names the current block. Member access off it reads a field declared in the same block — handy when a name would otherwise be shadowed, or to be explicit about where a value comes from.

```wcl
box "panel" {
  width  = 480.0
  height = 270.0
  // self.width reads this block's own `width` field.
  ratio  = self.width / self.height
}
```

## parent

`parent` names the enclosing block one level out. Use it to read a value declared on the container from within a nested block. Referencing `parent` at the document root is an error — there is no scope above it.

```wcl
service "web" {
  region = "us-east-1"
  metadata {
    // parent.region reaches the surrounding service block.
    inherited_region = parent.region
  }
}
```

> [!NOTE]
> **Navigators, not copies**
> self and parent reify to references resolved lazily through lexical scope (innermost block out, then the document root) — the same resolution a `&T` field uses. See [References](../references/concept_references.md).

## Related

- [References](../references/concept_references.md)

- [Fields](../references/concept_fields.md)

- [Blocks](../references/concept_blocks.md)

[← Back to SKILL.md](../SKILL.md)
