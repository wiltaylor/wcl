# try / catch

_Evaluates a body; on failure the rendered message binds to the catch name and the handler's value is the result._

`try body catch name => handler` evaluates the body; if evaluation fails — a builtin error, an `error()` call, a cycle, or a propagated field error — the rendered message binds to the catch name (a `utf8`) and the handler's value is the result. Both sides accept a `{ ... }` block.

```wcl
rate    = try parse_rate(raw) catch m => 1.0
summary = try {
  let r = risky()
  format("ok: {}", r)
} catch msg {
  format("failed: {}", msg)
}
```

> [!NOTE]
> **Catches everything**
> try makes any evaluation failure recoverable, including cycles and upstream field errors — use it where a fallback is meaningful, not to paper over schema mistakes (wcl check still reports those).

## Related

- [Block Expressions](../references/concept_block_expressions.md)

[← Back to SKILL.md](../SKILL.md)
