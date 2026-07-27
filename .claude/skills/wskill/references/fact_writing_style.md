# Writing style for wskill content

A wskill is read two ways: a person scanning a book page, and an agent pulling the
same text out of `references/`. Both want the same thing — the fact, stated once,
with nothing to skip past. Write to be \*findable and correct\*, not to be enjoyed.


## The rules

| Rule | Why |
| --- | --- |
| Lead with the answer, then the detail | A reader who stops after one sentence should still have the fact |
| Present tense, active voice, second person for instructions | "`wcl check` reports the violation", not "violations will be reported" |
| State the rule, then its exception | Exceptions buried first read as the rule |
| One idea per paragraph; 2-4 sentences | Long paragraphs hide the sentence that matters |
| Enumerable values go in a table, syntax goes in a code block | Prose is the worst container for a list of options |
| Name the concrete thing | "the `@children` field" beats "the relevant field" |
| Every claim must be checkable | If you can't test or cite it, it is an opinion — say so or cut it |

## Do not write

| Avoid | Instead |
| --- | --- |
| Marketing adjectives — powerful, seamless, blazing, simply, just | Say what it does. "Simply run X" reads as "you should have known this" |
| "As we saw above", "as mentioned earlier" | Every page is read standalone, and the skill splits pages into separate files. Restate the clause or link it |
| "See the official docs for details" | Capture the substance here — a wskill that defers is not [self-contained](../references/concept_selfcontained.md) |
| Hedges — generally, typically, in most cases | Either it is true, or state the condition under which it isn't |
| Repeating a sibling unit's explanation | One fact lives in exactly one unit. Link, or restate a single clause — never a whole section |

## Field conventions

The record fields are read far more often than the body — they are the index rows, the nav labels, and the link text.

| Field | Write it as |
| --- | --- |
| `name` / `title` | A noun phrase naming the thing, in sentence case. Not a sentence, no trailing period |
| `summary` | One sentence, under ~140 characters, that stands alone in an index. If it needs an "and", the unit is not [atomic](../references/concept_atomic_note.md) |
| `purpose` (procedure) | What the reader will have achieved when the steps are done |
| `verification` (procedure) | The observable signal it worked — a command's output, a file that now exists |
| `audience` | Set it deliberately: `:book` is the default, so a unit an agent needs must say `:both` |

## Wrong vs right

| Tempting (wrong) | Correct | Why |
| --- | --- | --- |
| "WCL has a powerful and flexible type system that lets you do many things." | "WCL types are fixed-width integers, two float widths, four string encodings, symbols, lists, tensors, records, unions and interfaces." | The second sentence can be checked; the first cannot |
| "You'll probably want to run `wcl check` here." | "Run `wcl check`: it reports schema violations with file and line." | Say what to do and what it gives you |
| A concept that re-explains the three fields of the fact it links to | A concept that states the model and links the fact once | Duplicated text drifts — one of the two copies will go stale |
| "See [example.com/docs](https://example.com/docs) for the full list." | The full list, as a table, in the unit | The skill has no browser; a link is not content |

## Length

A unit that runs past roughly a screen of prose is usually two units. Split it and link the halves — see [Linking discipline](../references/fact_linking_discipline.md).

## Related

- [Atomic Note](../references/concept_atomic_note.md)

- [Linking discipline — link sparingly](../references/fact_linking_discipline.md)

- [Self-Contained Content](../references/concept_selfcontained.md)

[← Back to SKILL.md](../SKILL.md)
