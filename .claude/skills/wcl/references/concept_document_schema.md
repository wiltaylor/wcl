# Document Schema

_Marking a document root with @document, and how schemas merge per namespace._

## Composing document schemas

Several `@document` schemas can govern the same namespace, and they **merge**: a top-level field or block is legal if **any** of them declares it. This lets you import a library that ships its own `@document` and still add your own top-level tags.

```wcl
import <wdoc.wcl>            // brings in wdoc's library @document

@document
type ProjectDoc {           // your own root schema — merges with wdoc's
  @children("project_meta") metas: list<ProjectMeta>
}

@block("project_meta")
type ProjectMeta { @inline(0) id: identifier  owner: utf8 }

project_meta info { owner = "Wil" }   // your tag, alongside wdoc pages
page index { text { span "Hello" {} } }
```

Imported (library) `@document` schemas merge silently. Only a **second root-authored** `@document` in the same namespace is an error — you get one root schema, which composes with whatever the imports provide.

> [!NOTE]
> **Reflection**
> decorator_names(T) and decorator_arg(T, name, slot) read decorators back at evaluation time — used by libraries (like wdoc) that dispatch on a block's declared kind.

## Related

- [Block Schema](../references/concept_block_schema.md)

- [Referential Integrity](../references/concept_ref_integrity.md)

- [Child-count Constraints](../references/concept_child_count_constraints.md)

- [Records](../references/concept_records.md)

[← Back to SKILL.md](../SKILL.md)
