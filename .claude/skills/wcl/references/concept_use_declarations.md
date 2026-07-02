# use Declarations

_use pulls qualified names from other namespaces into local scope so they can be written bare._

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

## Related

- [Namespaces](../references/concept_namespaces.md)

- [Qualified Block Kinds](../references/concept_qualified_kinds.md)

- [Imports & Modules](../references/concept_imports.md)

[← Back to SKILL.md](../SKILL.md)
