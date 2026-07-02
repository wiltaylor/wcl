# String Interpolation

_Opt-in with a `$` prefix; `${ ... }` slots splice evaluated expressions into the string._

## Interpolation

Interpolation is opt-in: prefix the string literal with `$`. Inside a `$"..."`
(or `$<<TAG`), `${ ... }` slots evaluate any expression and splice the result
into the string. Without the prefix `${...}` is literal text. All four encodings
accept the `$` prefix.


```wcl
greeting = $"Hello, ${name}! Count: ${count + 1u32}"

block = $<<MSG
You have ${len(items)} items waiting.
The first is ${head(items)}.
MSG
```

## Related

- [Strings](../references/concept_strings.md)

- [String Literals](../references/concept_string_literals.md)

[← Back to SKILL.md](../SKILL.md)
