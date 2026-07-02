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

## Nested namespaces

Namespaces nest arbitrarily deep. Both forms below produce the same `acme.graphics.color.RGB` — a multi-segment `namespace` path, or a dotted declaration name under a shorter namespace:

```wcl
namespace acme.graphics

// nests via the declaration name
type color.RGB { r: u8  g: u8  b: u8 }   // acme.graphics.color.RGB
```

Within a namespace, sibling and child segments are reachable by their relative path — `color.RGB` from inside `acme.graphics` — while a fully-qualified path always works:

```wcl
namespace acme.graphics

type color.RGB  { r: u8  g: u8  b: u8 }
type theme.Swatch {
  fill:   color.RGB              // relative: resolves to acme.graphics.color.RGB
  stroke: acme.graphics.color.RGB   // fully-qualified: same type
}
```

Imported files keep their own `namespace` — an `import` brings their declarations in, but the names stay qualified under the imported file's path. See [Imports & Modules](../references/concept_imports.md).

## Related

- [use Declarations](../references/concept_use_declarations.md)

- [Qualified Block Kinds](../references/concept_qualified_kinds.md)

- [Imports & Modules](../references/concept_imports.md)

- [Identifiers](../references/concept_identifiers.md)

- [Document Schema](../references/concept_document_schema.md)

[← Back to SKILL.md](../SKILL.md)
