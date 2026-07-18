# Answer Mode

_The `@answerable` decorator turns question blocks into a guided interview: `wcl answer` walks the pending ones and writes the answers back._

Documents that track interview-style questions as blocks (a planning pipeline, an elicitation checklist) get a \*respondent\* experience on top: walk the open questions, take answers, flip statuses — never show the respondent raw WCL. A block type opts in with `@answerable` from `import <answer.wcl>`, mapping the roles onto its own field names:

```wcl
import <answer.wcl>

@answerable(prompt = "question", response = "answer", status = "status",
            pending = :open, resolved = :answered, skipped = :dropped)
@block("question")
type PlanQuestion {
  @inline(0) id: identifier
  question: utf8
  @default(:open) status: symbol
  answer: utf8?
  kind: symbol?    // :single_select | :multi_select | :free_text
  @children("option") options: list<AnswerOption>
}

question q_platforms {
  question = "What platforms must be supported?"
  kind = :single_select
  option linux { label = "Linux x86_64" }
  option mac   { label = "macOS (Apple Silicon)" }
}
```

A question whose status equals `pending` is open. Answering writes the composed text into the response field and sets the status to `resolved`; skipping (offered only when `skipped` is declared) sets that status and leaves the response untouched. Choice questions carry `option` child blocks (the stdlib's `AnswerOption`, or your own kind named by the `options` argument) and a selection kind; every choice question \*always\* also offers free text — options accelerate, they never constrain. Picked labels land in the single response field as prose (joined with `", "`, typed text appended after `" — "`), so downstream consumers read one utf8 field.

`wcl answer <file>` consumes it in the terminal: arrow-key menus on a TTY, numbered line input otherwise, `--list` / `--id` for scripts and agents. It writes one question at a time through the validating edit pipeline, so an interrupted session loses nothing.

## Related

- [Block Schema](../references/concept_block_schema.md)

- [CLI Reference](../references/concept_cli.md)

[← Back to SKILL.md](../SKILL.md)
