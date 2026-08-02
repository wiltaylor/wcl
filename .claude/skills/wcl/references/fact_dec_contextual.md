# @contextual

`@contextual` on a `@block` type says its placement is decided by **context**, not by kind. Such a block is legal wherever nested blocks are allowed at all — no parent has to list it as a child kind — and its body is not validated in place, because the body only has meaning once expanded with bindings.

It also marks the block as one whose children are **generated**. The host registers an \*expander\* (a Rust callback on the `Environment`) that says what a block of that kind expands to; the language consults it when it projects a `@children("kind")` slot, so generated blocks land in the slot exactly like authored ones. A decorator can declare \*that\* a block expands; it cannot carry \*how\* — that is behaviour, and it belongs to the host that defined the vocabulary.

Demanding a `@contextual` block's generated children from a document opened without the host's expander is a hard error naming the kind. Parsing and formatting never demand them, so `wcl parse` and `wcl fmt` are unaffected; anything that evaluates the document must open it with the host environment.

```wcl
// The host declares its repetition block as contextual...
@block("repeat") @contextual
type Repeat {
  @schemaless each: list<Item>
  as: symbol
}

// ...so it is legal inside `deck`, which declares no such child kind,
// and the cards it generates join `deck`'s `cards` slot.
@block("deck") type Deck {
  @inline(0) name: identifier
  @children("card") cards: list<Card>
}

deck main {
  card literal { title = "authored" }
  repeat { each = ["one", "two"]  as = :m
    card $"g_${m}" { title = $"generated ${m}" }
  }
}
```

## Related

- [@block](../references/fact_dec_block.md)

- [@children](../references/fact_dec_children.md)

- [Block Schema](../references/concept_block_schema.md)

[← Back to SKILL.md](../SKILL.md)
