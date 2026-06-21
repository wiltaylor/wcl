# Namespaces

_Scope declarations under a dotted path; use and :: control how names resolve._

A `namespace` declaration scopes a file's declarations under a dotted path, `use` pulls names from other namespaces into local scope, and a `ns::kind` qualifier picks a block schema from a specific namespace. Together they let independently-authored libraries share a document without name collisions.

## Declaring a namespace

`namespace` takes a dotted path and must be the **first item** in the file. Every declaration then lives under that path: with `namespace company`, a `type Point` is fully qualified `company.Point`. A dotted declaration name nests further — `type utils.Point` becomes `company.utils.Point`.

```wcl
namespace company

type utils.Point  { x: f64  y: f64 }
type shapes.Circle { center: utils.Point  radius: f64 }
```

Imported files keep their own `namespace` — an `import` brings their declarations in, but the names stay qualified under the imported file's path. See [Imports & Modules](../references/concept_imports.md).

## use declarations

`use` brings qualified names into local scope so they can be written bare. It is top-level only; an unknown target or duplicate alias is an error when the document is opened.

```wcl
use company.utils.Point          // bind the leaf: write `Point`
use company.utils.Point as P     // leaf under another name: `P`
use company.utils                // whole namespace: every member resolves bare
use company.utils as U           // namespace alias: `U.Point`
use company.shapes.{Circle, Square as Sq}   // pick several members at once
```

| Form | Effect |
| --- | --- |
| `use ns.Name` | Binds `Name` locally |
| `use ns.Name as Alias` | Binds the member under `Alias` |
| `use ns` | Adds the namespace to the bare-name search path |
| `use ns as Alias` | Namespace alias — members reachable as `Alias.Name` |
| `use ns.{A, B as C}` | Binds several members in one declaration |

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

- [Imports & Modules](../references/concept_imports.md)

- [Identifiers](../references/concept_identifiers.md)

- [Schema & Decorators](../references/concept_schema.md)

[← All concepts](../references/concepts_ref.md)
