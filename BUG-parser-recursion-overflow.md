# Bug: deeply nested expressions overflow the parser stack (no recursion limit)

Found 2026-06-10 by the `format_round_trip` fuzzer while verifying the
BUG-fmt-heredoc-roundtrip fix. After the formatter bugs were fixed, the fuzzer's next
finding was in the **parser**: a few KB of `((((((…` (or any deeply self-nesting
construct) drives the recursive-descent parser past the stack and the process aborts
with a stack overflow (ASan: `stack-overflow … in collect_trivia`, plain build: SIGSEGV).

## Repro

```bash
python3 -c "print('x = ' + '(' * 4000 + '1' + ')' * 4000)" > deep.wcl
wcl parse deep.wcl     # aborts (stack overflow), exit 134
```

The minimized fuzz artifact is preserved at
`crates/wcl_lang/fuzz/artifacts/format_round_trip/crash-eba9954880a65f3e817272ba6320a758860750d4`.

## Expected

A `parse error: expression nesting too deep` diagnostic (with span) at some generous
fixed depth — never a crash. Anything that takes untrusted input (`wcl check`, the LSP
on a live buffer, `wdoc serve` watching a file mid-edit) can hit this.

## Suggested direction

A depth counter threaded through the recursive-descent entry points (`parse_expr`,
type-reference parsing, pattern parsing, and block/item nesting), erroring past a cap
(e.g. 256). The lexer itself is iterative; the frames in the report are
parser-recursion frames. Worth a `parse` fuzz-target corpus entry plus a unit test on
the cap's error message once fixed.

## Scope note

Separate from the fmt round-trip fix that surfaced it: this is parser robustness, it
predates the formatter work, and it needs guards across every recursive parse path
rather than a spot fix.
