# Template — concept

A **concept** captures one idea the reader must understand: a model, a pattern, a principle,
an explanation of \*why\*. If the note is a value, a named thing, or a task, it is a different
kind — check [the decision guide](../references/fact_unit_decision_guide.md) first.


## Skeleton

```wcl
// data/concept/<id>.wcl — then add `import "./<id>.wcl"` to data/concept/main.wcl
concept <id> {
  name     = "<Noun phrase, sentence case>"
  summary  = "<One sentence that stands alone in an index.>"
  audience = :both              // :book (default) | :ai | :both
  related  = [<up to ~5 ids>]
  tags     = []                 // optional

  body {
    p "<The idea, stated in the first sentence.>"

    h2 "<Sub-heading, only if the unit needs one>"
    p "<Why it works this way, or when it applies.>"

    code "<lang>" { source = <<'EXAMPLE'
<the smallest example that shows the idea>
EXAMPLE
    }

    callout "<Short title>" { class = ["note"]  body = "<The exception or caveat.>" }
  }
}
```

## Fields

| Field | What goes in it |
| --- | --- |
| `<id>` | snake_case, stable, unique across every unit kind — it is the page name and the link target |
| `name` | The idea's name. Not a sentence, no trailing period |
| `summary` | One sentence. It is the index row, so it must make sense with no surrounding context |
| `related` | The units a reader would go to next — roughly 3-5 (see [Linking discipline](../references/fact_linking_discipline.md)) |
| `audience` | `:both` to reach the skill as well as the book. The default is `:book` only |
| `body` | Prose, tables, code. State the idea first; everything after it is support |

## Filled example

```wcl
concept fast_forward {
  name     = "Fast-forward merge"
  summary  = "When the target branch has no commits the source lacks, git moves the pointer instead of creating a merge commit."
  audience = :both
  related  = [merge_commit, branch_pointer]

  body {
    p "A fast-forward merge moves the branch pointer forward instead of recording a merge commit. Git can do this only when the target branch has no commits the source branch is missing — the history is already a straight line."
    p "The result is a history with no merge bubble, and no record that a branch existed. Pass `--no-ff` when the branch itself is the thing worth keeping in the log."
  }
}
```

## Done when

- It holds exactly one idea — the summary needs no "and".
- The first sentence of the body states that idea.
- It is self-contained: no "see the upstream docs".
- `related` has 5 ids or fewer, each one a genuine next step.
- It is pinned into an `index`, or it will not appear in the nav.
- `just wskill-check` is green and `just render` produces its page.

## Related

- [Concept](../references/concept_concept.md)

[← Back to SKILL.md](../SKILL.md)
