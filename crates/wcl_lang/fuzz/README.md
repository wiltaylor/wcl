# wcl_lang fuzz harness

`cargo-fuzz` targets for the parser, evaluator, and printer. Seeded
from `crates/wcl_lang/tests/...` example fixtures.

## Targets

- **`parse`** — `parse_for_edit(s)` must not panic on arbitrary
  bytes. `ParseError` is the expected failure mode.
- **`eval`** — `Document::open(s, "fuzz")` (parser + binder + schema
  validator) must not panic; on success, `schema_errors()` is forced
  to exercise the lazy paths.
- **`eval_fields`** — open the document, then evaluate every field,
  `let` and block-body field expression directly with
  `Document::eval_expr`, schema or no schema. Arbitrary input rarely
  has a `@document` schema, so `eval` seldom reaches a builtin; this
  target runs them. Evaluation errors are expected; panics, aborts
  and hangs are not.
- **`format_round_trip`** — for any `s` the parser accepts,
  `parse_for_edit → format::to_source → parse_for_edit` must succeed
  and produce a structurally equal AST. Guards against parser /
  printer drift.
- **`json_round_trip`** — for any value produced by evaluating a
  top-level field, `Value → JSON → serde_json::Value → JSON` must be
  byte-stable. Guards the hand-rolled `Value` serializer against key
  ordering, float formatting, and escape drift.
- **`set_edit_path`** — for up to eight fields at any depth, replace
  the field's RHS with `0i64` through `edit::replace_field` (the call
  behind `wcl set`) and require it to succeed, which includes the
  result reparsing. Guards the edit API against AST shapes the printer
  cannot survive.

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
