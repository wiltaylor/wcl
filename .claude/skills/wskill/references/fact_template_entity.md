# Template — entity

An **entity** is a concrete NAMED thing in the topic's world: a person, an organisation, a
tool, a command, a library, a service, a file format, a place. It is not a catch-all — an idea
is a `concept`, a value or table is a `fact`, a task is a `procedure`. `kind` comes from the
closed `EntityKind` vocabulary in `schema/kinds.wcl`; extend that file when a topic needs a
new one.


## Skeleton

```wcl
// data/entity/<id>.wcl — then add `import "./<id>.wcl"` to data/entity/main.wcl
entity <id> {
  name     = "<The thing's actual name>"
  kind     = :<software|tool|command|person|organization|library|service|…>
  summary  = "<One sentence: what it is and what it is for.>"
  audience = :both
  related  = [<up to ~5 ids>]

  body {
    p "<What it is, in one paragraph.>"

    table {
      header = ["Attribute", "Value"]
      rows = [
        ["<attribute>", "<value>"],
      ]
    }
  }
}
```

## Fields

| Field | What goes in it |
| --- | --- |
| `name` | The thing's real name, spelled as its owners spell it — `wcl`, not `WCL CLI` |
| `kind` | A member of `EntityKind` (`schema/kinds.wcl`). `wcl check` rejects anything else, which is the point: it stops entity becoming a dumping ground |
| `summary` | Optional in the schema, but write it — it is the index row |
| `body` | One paragraph of what-it-is, then an attributes table if the thing has stable facts worth looking up |

Attribute rows are also authorable with pipe syntax when there are many of them:

```wcl
attributes:
  | "flag" | "-l"          |
  | "name" | "long format" |
```

## Filled example

```wcl
entity ripgrep {
  name     = "ripgrep"
  kind     = :tool
  summary  = "A recursive line-oriented search tool that respects .gitignore by default."
  audience = :both
  related  = [ignore_rules]

  body {
    p "`rg` searches directory trees for a regex, skipping anything the repository ignores. It is the default grep replacement in editor integrations because that filtering is on without configuration."
    table {
      header = ["Attribute", "Value"]
      rows = [
        ["Binary", "`rg`"],
        ["Respects `.gitignore`", "Yes, unless `--no-ignore`"],
        ["Follows symlinks", "No, unless `--follow`"],
      ]
    }
  }
}
```

## Done when

- You could point at the thing — it has a proper name of its own.
- `kind` is a real `EntityKind` member, not the closest-looking one.
- The body says what it is; behaviours it has are separate concepts or facts.
- It is pinned into an `index`.

## Related

- [Entity](../references/concept_entity.md) — Entity supports Template — entity: A concrete NAMED thing in the topic's world — a person, software, a place, an organisation. Reserved: never a catch-all.

[← Back to SKILL.md](../SKILL.md)
