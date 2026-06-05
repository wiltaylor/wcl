# Imports & Modules

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
> Start the document you pass to wcl wdoc build with import <wdoc.wcl>, and page / text / code / callout and the rest of the stdlib are in scope there and in every page file it imports. Imported page files don't repeat the line.

## How imports compose

Top-level imports are eager; an `import` inside a block is lazy. Imported declarations participate fully in the importer: they take part in structural validation and in name resolution, so a type or `let` declared in an imported file is usable as if it were local.

An `import` inside a block also splices the imported file's top-level block instances into the enclosing block as children — exactly as if they had been written inline — so you can factor a nested subtree into its own file and nest it under a parent block. The spliced instances are validated against the parent's `@child` / `@children` slots like any literal child. Top-level connection statements in the imported file are block-scoped to the importer the same way, and a connection anywhere can name a block that an in-block import pulled into the tree.

Hosts that drive WCL can also tag each block with its source file, which lets tooling order imported-versus-local output (wdoc uses this for CSS cascade, for instance).
