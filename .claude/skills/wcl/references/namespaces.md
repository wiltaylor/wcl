# Namespaces

A `namespace` declaration scopes a file's declarations under a dotted path, `use` pulls names from other namespaces into local scope, and a `ns::kind` qualifier picks a block schema from a specific namespace. Together they let independently-authored libraries (like the wdoc stdlib, which lives in `namespace wdoc`) share a document without name collisions.

## Declaring a namespace

`namespace` takes a dotted path and must be the **first item** in the file — at most one per file, top level only, no decorators. Every declaration in the file then lives under that path: with `namespace company`, a `type Point` is fully qualified as `company.Point`. A dotted declaration name nests further — `type utils.Point` becomes `company.utils.Point`.

```wcl
namespace company

type utils.Point  { x: f64  y: f64 }
type shapes.Circle { center: utils.Point  radius: f64 }
```

Imported files keep their own `namespace` — an `import` brings their declarations in, but the names stay qualified under the imported file's path. See [Imports & Modules](../references/imports.md).

## use declarations

`use` brings qualified names into local scope so they can be written bare. It is top-level only; an unknown target or a duplicate alias is an error when the document is opened.

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

Block (and table) kinds are namespace-scoped too. A `::`-qualified kind at the instance site selects the `@block` declaration from that namespace, even when a local declaration shadows the bare kind. A namespace alias from `use … as` also works left of the `::`.

```wcl
import <wdoc.wcl>

// A local @block("process") shadows the bare kind…
@block("process") type MyProcess { @inline(0) text: utf8  cost: i64 }

process "mine" { cost = 3 }          // → MyProcess (local wins)
wdoc::process "theirs" { }           // → wdoc's Process, explicitly
```

## How bare names resolve

A bare kind prefers a declaration in the referencing file's own namespace; otherwise it falls back to an imported one. So a user `@block("process")` deterministically shadows a library's — there is no silent collision. Two same-kind declarations in the **same** namespace are an error (`DuplicateBlockKind`); the same kind across different namespaces is fine — disambiguate at the instance with `::`.

Importing a namespaced library also adds its namespace to the bare-name search path automatically: after `import <wdoc.wcl>`, a bare `page` or `SvgFundamental` resolves into `wdoc.*` without an explicit `use wdoc`. The same per-namespace model governs `@document` schemas, which merge per namespace — see [Schema & Decorators](../references/schema.md).

> [!NOTE]
> **import brings declarations in; namespace and use control their names**
> An import decides which files participate in the document. The imported file's namespace decides what its declarations are called, and your use declarations (or :: qualifiers) decide how you refer to them.
