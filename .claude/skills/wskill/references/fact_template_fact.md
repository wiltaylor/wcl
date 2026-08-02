# Template — fact

A **fact** is a container of values: a default, a limit, a lookup table, a version matrix, an
option list, a glossary. The test is that nobody argues with it — you would cite it, not
explain it. A fact whose body is mostly prose is usually a concept in disguise.


## Skeleton

```wcl
// data/fact/<id>.wcl — then add `import "./<id>.wcl"` to data/fact/main.wcl
fact <id> {
  title    = "<What the values are>"
  audience = :both
  related  = [<up to ~5 ids>]

  body {
    p "<One sentence of context: what these values are and when they apply.>"

    table {
      header = ["<Column>", "<Column>"]
      rows = [
        ["<value>", "<meaning>"],
      ]
    }

    code "<lang>" { source = <<'EXAMPLE'
<the values in use, if a sample helps>
EXAMPLE
    }
  }
}
```

## Fields

| Field | What goes in it |
| --- | --- |
| `title` | Names the values, not the topic — "Default ports", "Escape sequences", "Exit codes" |
| `related` | The concept the values belong to, and little else |
| `body` | Lead with one sentence of context, then the table. A fact with no table or code block is rare and probably misfiled |

There is no `summary` field on a fact: the `title` carries it, so make the title specific
enough to stand alone in an index.


## Filled example

```wcl
fact exit_codes {
  title    = "Exit codes"
  audience = :both
  related  = [validate_format]

  body {
    p "`wcl check` distinguishes the two failure modes by exit code, so a script can tell a broken file from a schema violation."
    table {
      header = ["Code", "Meaning"]
      rows = [
        ["`0`", "Valid"],
        ["`1`", "Parse error — the file is not WCL"],
        ["`2`", "Schema violation — the file parses but breaks its @document"],
      ]
    }
  }
}
```

## Done when

- The values are the content; the prose is one sentence of context.
- Every row is checkable against the thing it documents.
- The title is specific enough to be an index row on its own.
- It is pinned into an `index`.

## Related

- [Fact](../references/concept_fact.md)

[← Back to SKILL.md](../SKILL.md)
