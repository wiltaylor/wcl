# Imports & Modules

_Pull another file's declarations into the document — disk and system forms._

An `import` pulls another file's declarations into the current document. There are two forms, distinguished by how the path is written.

## Disk imports

A quoted path imports a file from disk, resolved relative to the importing file.

```wcl
import "./pages/values.wcl"
import "../shared/types.wcl"
```

## System imports

An angle-bracket path imports a **system** module — a file provided by the host program through a registry, not the filesystem. wdoc's standard library is served this way: a single `import <wdoc.wcl>` pulls in the whole stdlib.

```wcl
import <wdoc.wcl>
```

> [!NOTE]
> **Import the wdoc stdlib once, at the root**
> Start the document you pass to wcl wdoc build with import <wdoc.wcl>, and the stdlib is in scope there and in every page file it imports. Imported page files do not repeat the line.

## How imports compose

Top-level imports are eager; an `import` inside a block is lazy. Imported declarations participate fully in the importer — in structural validation and name resolution — so a type or `let` declared in an imported file is usable as if local. An imported file keeps its own declaring `namespace`. See [Namespaces](../references/concept_namespaces.md).

An `import` inside a block also splices the imported file's top-level block instances into the enclosing block as children — exactly as if written inline — so you can factor a nested subtree into its own file. The spliced instances are validated against the parent's `@child` / `@children` slots like any literal child.

## Related

- [Namespaces](../references/concept_namespaces.md)

- [Identifiers](../references/concept_identifiers.md)

[← Back to SKILL.md](../SKILL.md)
