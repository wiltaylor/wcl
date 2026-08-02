# Referential Integrity

_Validating that id fields name an existing block with @ref._

## Referential integrity with @ref

A field holding an `identifier` (or `list<identifier>`) that is semantically a reference to
another block can declare that with `@ref("kind")`. `wcl check` then verifies every id names
an existing block of that kind — anywhere in the document — and reports a dangling reference
otherwise.


```wcl
@block("screen") type Screen { @inline(0) id: identifier  name: utf8 }
@block("flow") type Flow {
  @inline(0) id: identifier
  @ref("screen") entry_screen: identifier   // must be a declared screen id
  @ref("screen") steps: list<identifier>    // every element checked
}
```

[← Back to SKILL.md](../SKILL.md)
