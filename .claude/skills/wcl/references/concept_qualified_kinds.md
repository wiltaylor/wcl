# Qualified Block Kinds

_Block and table kinds are namespace-scoped; a ::-qualified kind picks a schema from a specific namespace._

## Qualified block kinds

Block (and table) kinds are namespace-scoped too. A `::`-qualified kind at the instance site selects the `@block` declaration from that namespace, even when a local declaration shadows the bare kind.

```wcl
import <wdoc.wcl>

// A local @block("process") shadows the bare kind...
@block("process") type MyProcess { @inline(0) text: utf8  cost: i64 }

process "mine" { cost = 3 }          // -> MyProcess (local wins)
wdoc::process "theirs" { }           // -> wdoc's Process, explicitly
```

## How bare names resolve

A bare kind prefers a declaration in the referencing file's own namespace; otherwise it falls back to an imported one. So a user `@block("process")` deterministically shadows a library's. Two same-kind declarations in the **same** namespace are an error; the same kind across different namespaces is fine — disambiguate at the instance with `::`.

> [!NOTE]
> **import vs namespace vs use**
> An import decides which files participate. The imported file's namespace decides what its declarations are called, and your use declarations (or :: qualifiers) decide how you refer to them.

## Related

- [Namespaces](../references/concept_namespaces.md)

- [use Declarations](../references/concept_use_declarations.md)

- [Imports & Modules](../references/concept_imports.md)

[← Back to SKILL.md](../SKILL.md)
