# Namespaces and imports

Three separate questions, and they are easy to confuse:

- An **import** decides which files take part in the document.
- The imported file's **namespace** decides what its declarations are called.
- Your **`use` declarations** (or a `::` qualifier) decide how you write those names.

## `namespace`

A `namespace` declaration scopes every declaration in the file under a dotted path. It must be
the **first item in the file** — anything before it is a parse error
(`namespace declaration must be the first item in the file`).

```wcl
namespace company.net

@block("server") type Server {
  @inline(0) id: identifier
  host: utf8
  @default(8080) port: u16
}
```

`Server` is now `company.net.Server`. A file with no `namespace` declares into the root
namespace.

A dotted **declaration name** nests further. Both of these produce `acme.graphics.color.RGB`:

```wcl
namespace acme.graphics
type color.RGB { r: u8  g: u8  b: u8 }
```

```wcl
namespace acme.graphics.color
type RGB { r: u8  g: u8  b: u8 }
```

Inside a namespace, a sibling or child segment resolves by its relative path. A fully-qualified
path always works too:

```wcl
namespace acme.graphics

type color.RGB { r: u8  g: u8  b: u8 }

type theme.Swatch {
  fill:   color.RGB                // relative
  stroke: acme.graphics.color.RGB  // fully qualified — the same type
}
```

## `use`

`use` binds a qualified name locally so you can write it bare. It is **top-level only**. An
unknown target or a duplicate alias fails when the document opens, not at evaluation time:

```console
$ wcl check f.wcl
wcl::parse

  × unknown use target 'company.net.Nope'
```

| Form | Effect |
| --- | --- |
| `use ns.Name` | Binds `Name` locally. |
| `use ns.Name as Alias` | Binds the member under `Alias`. |
| `use ns` | Adds the namespace to the bare-name search path. |
| `use ns as Alias` | Namespace alias — members are reachable as `Alias.Name`. |
| `use ns.{A, B as C}` | Binds several members in one declaration. |

What the path names tells the two bare forms apart. A path that names a declaration binds that
leaf. A path that is only a **prefix** of declarations becomes a wildcard search path.

```wcl
import "./lib.wcl"

use company.net.Server        // write `Server`
use company.net.Server as S   // write `S`
use company.net               // every member of the namespace resolves bare
use company.net as net        // write `net.Server`
use company.net.{Server, Pool as P}
```

**You often need no `use` at all.** Importing a namespaced file adds that file's namespace to
this file's resolution search paths. A bare reference to an imported declaration therefore
already resolves. `use` is for a shorter name, a different name, or an explicit record of the
dependency.

## Qualified block kinds

Block (and table) kinds are namespace-scoped too. A **bare** kind prefers a declaration in the
referencing file's own namespace, and falls back to an imported one. Your own
`@block("server")` therefore shadows a library's, deterministically.

Write `ns::kind` at the instance site to pick the other one:

```wcl
import "./lib.wcl"        // declares company.net.Server, kind "server"
use company.net as net

@block("server") type MyServer { @inline(0) id: identifier  tier: utf8 }

@document type Config {
  @children("server") servers: list<MyServer>
}

server local { tier = "dev" }                  // MyServer — the local declaration wins
net::server web { host = "example.com" }       // company.net.Server, explicitly
```

```console
$ wcl get config.wcl servers.web.port
8080
$ wcl get config.wcl servers.local.tier
"dev"
```

The `web` block answers `8080` from `company.net.Server`'s `@default`, which is the proof that
the qualifier picked the other schema.

The qualifier before `::` resolves the way any other name does, so it may be a namespace alias
(`net::`) or a full path (`company.net::`).

Two declarations of the same kind string in the **same** namespace are an error
(`DuplicateBlockKind`) — a reference to that kind would be ambiguous. The same kind in
*different* namespaces is fine; that is what `::` exists to disambiguate.

## Imports

Two forms, told apart by how the path is written.

**Disk import** — a quoted path, resolved relative to the **importing file** and then
canonicalised. A path that does not exist fails when the document opens.

```wcl
import "./pages/values.wcl"
import "../shared/types.wcl"
```

**System import** — an angle-bracket path, served by the host program through a `Registry`
rather than by the filesystem. This is how wdoc ships its standard library:

```wcl
import <wdoc.wcl>
```

A system import is *never* touched against the filesystem. It resolves inside the registry's
own namespace, under a virtual root. A registered file's own system imports are therefore
**importer-relative**. A library part registered under `mylib/component/` reaches wdoc's
library as `<../../wdoc.wcl>`, with `.` and `..` collapsed lexically. A system import naming an
unregistered key fails with `no system import registered for <key>`.

## How imports compose

- **Top-level imports are eager.** They are followed when the document opens, transitively.
- **An import inside a block is lazy.** It loads the first time evaluation crosses into it.
- **Name resolution is order-independent.** The importer's declarations and every imported
  file's declarations participate in one search. It does not matter whether the `import` line
  sits above or below the use of the name.
- **Each module loads at most once per document.** The second load is a silent no-op — a
  repeated `import <wdoc.wcl>` in several files, or a diamond where two imports pull in the
  same third file. Declarations are never duplicated.
- **A cycle is an error.** Re-entering a path still in the active import chain reports:

  ```console
  $ wcl check d.wcl
  wcl::parse

    × import cycle detected at '/…/e.wcl'
     ╭─[/…/d.wcl:1:8]
   1 │ import "./e.wcl"
     ·        ────┬────
     ·            ╰── cycle
  ```

  A diamond is not a cycle. Only a genuine back-edge is.
- **An imported file keeps its own `namespace`.** Importing does not flatten anything into your
  namespace; it only makes the names reachable.

## Splicing a subtree with an in-block import

An `import` written **inside a block** does something extra. The imported file's top-level block
instances become children of the enclosing block, exactly as if written inline. They are
validated against the parent's `@child` / `@children` slots like any literal child. This is how
you factor a long nested subtree into its own file.

`steps.wcl`:

```wcl
step "mix"
step "bake"
```

`recipe.wcl`:

```wcl
@block("step") type Step { @inline(0) label: utf8 }

@block("recipe") type Recipe {
  @inline(0) id: identifier
  @children("step") steps: list<Step>
  @schemaless count: i64?
}

@document type Book { @children("recipe") recipes: list<Recipe> }

recipe cake {
  import "./steps.wcl"
  count = len(steps)
}
```

```console
$ wcl get recipe.wcl recipes.cake.count
2
```

## Gotchas

- `namespace` must be the first item. Not the first *declaration* — the first item.
- A `use` target must already be declared, or be a real prefix of declarations. There is no
  forward declaration.
- `use` binds names for **expressions and type references**. It does not rename block kinds;
  a kind is chosen by the bare-name rule or by `::`.
- Two `@document` schemas can co-govern a namespace — they merge, see
  [`lang_schemas.md`](lang_schemas.md). Two same-kind `@block` declarations in one namespace
  cannot.
- A relative disk import needs a base directory. A document opened from a source string with no
  path cannot resolve `"./x.wcl"`.
