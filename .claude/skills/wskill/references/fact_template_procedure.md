# Template — process (procedure)

A **process** is a repeatable task written as ordered steps, for someone who already knows the
topic and wants the reliable sequence. The block is `procedure`; `process` is its user-facing
name. Every step carries a number, an optional title, and up to two addressable body
fragments: `body screen { … }` for what is on screen, `body steps { … }` for what to do.


## Skeleton

```wcl
// data/process/<id>.wcl — then add `import "./<id>.wcl"` to data/process/main.wcl
procedure <id> {
  title    = "<Verb phrase: what you are doing>"
  purpose  = "<What the reader will have achieved.>"
  preconditions = ["<What must already be true.>"]
  verification  = "<The observable signal it worked.>"
  audience = :both
  style    = :graph            // :graph (default) draws the flow chart; :simple for a straight list
  related  = [<up to ~5 ids>]

  step first { n = 1
    title = "<Short imperative title>"
    body screen { code "console" { source = <<'SH'
$ <the command>
SH
    } }
    body steps { p "<What to do, and what you should see.>" }
  }

  step decide { n = 2
    title = "<A branch>"
    shape = :decision           // :process (default) | :decision | :terminator
    body steps { p "<The question, and what each answer leads to.>" }
  }

  first -> decide
  decide -> first :no           // label a branch with :yes / :no
}
```

## Fields

| Field | What goes in it |
| --- | --- |
| `title` | A verb phrase — "Installing the skill", "Upgrading the schema version" |
| `purpose` | The outcome, not a restatement of the title |
| `preconditions` | What must be true before step 1. Empty is fine; vague is not |
| `verification` | How the reader knows it worked — an output, a file, a green check. Skipping this is the commonest defect in a runbook |
| `step <id>` | The id is what the `from -> to` flow statements wire together; `n` is the displayed number |
| `shape` | `:decision` for a branch, `:terminator` for a start/end node, `:process` otherwise |
| flow statements | `a -> b` edges, written in the procedure body. They draw the chart; without them the steps have no wiring |

## Filled example

```wcl
procedure installing_the_skill {
  title    = "Installing the rendered skill"
  purpose  = "Get the wskill's SKILL.md and references into a repo so Claude Code loads it."
  preconditions = ["A wskill folder that passes `just wskill-check`."]
  verification  = "`/help` in Claude Code lists the skill by name."
  audience = :both

  step render { n = 1
    title = "Render the skill projection"
    body screen { code "console" { source = <<'SH'
$ just skill-build
SH
    } }
    body steps { p "Writes `out/skill/` — a `SKILL.md` plus a `references/` page per unit." }
  }

  step copy { n = 2
    title = "Copy it into the repo"
    body screen { code "console" { source = <<'SH'
$ wcl wskill install . --repo <repo>
SH
    } }
    body steps { p "The folder name is the skill name Claude Code will show." }
  }

  render -> copy
}
```

## Done when

- The steps are in the order a reader performs them, each doing one thing.
- `verification` names something the reader can observe.
- Every step is reachable through the `->` flow (or `style = :simple` if a chart adds nothing).
- Commands are real and were run — not paraphrased from memory.
- It is pinned into an `index`.

## Related

- [Process](../references/concept_process.md) — Process supports Template — process (procedure): A unit (authored as a procedure) that captures the reliable sequence for doing a task — ordered steps, for someone who already knows the topic.

[← Back to SKILL.md](../SKILL.md)
