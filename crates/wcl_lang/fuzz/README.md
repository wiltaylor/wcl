# wcl_lang fuzz harness

`cargo-fuzz` targets for the parser, evaluator, and printer. Seeded
from `crates/wcl_lang/tests/...` example fixtures.

## Targets

- **`parse`** — `parse_for_edit(s)` must not panic on arbitrary
  bytes. `ParseError` is the expected failure mode.
- **`eval`** — `Document::open(s, "fuzz")` (parser + binder + schema
  validator) must not panic; on success, `schema_errors()` is forced
  to exercise the lazy paths.
- **`format_round_trip`** — for any `s` the parser accepts,
  `parse_for_edit → format::to_source → parse_for_edit` must succeed
  and produce a structurally equal AST. Guards against parser /
  printer drift.
- **`json_round_trip`** — for any value produced by evaluating a
  top-level field, `Value → JSON → serde_json::Value → JSON` must be
  byte-stable. Guards the hand-rolled `Value` serializer against key
  ordering, float formatting, and escape drift.
- **`set_edit_path`** — parse, replace every field's RHS with `0i64`,
  re-emit, and require the result to reparse. Guards the edit-path
  mutation API against AST shapes the printer cannot survive.

## Run

Requires nightly Rust and `cargo install cargo-fuzz`.

```bash
just fuzz-run parse                          # default budget, runs until killed
just fuzz-run parse -- -runs=10000           # bounded
just fuzz-run format_round_trip -- -max_total_time=30
```

The CI workflow runs `just fuzz-sweep` — a bounded pass over every
target (~15s each) — as part of every push.

## Corpus

Each target has its own `corpus/<target>/` directory seeded from
`examples/*.wcl`. Add new seeds as the language grows — small files
that exercise specific syntax are more useful than large ones.

A seed that reproduced a real crash keeps its exact bytes, so the
fixed path stays covered: `json_round_trip/divide_by_zero.txt` is the
artifact from the integer-`/`-by-zero panic (issue #30), and
`modulo_by_zero.txt` / `int_overflow.txt` cover the sibling faults
found alongside it.
