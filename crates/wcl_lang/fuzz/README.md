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

## Run

Requires nightly Rust and `cargo install cargo-fuzz`.

```bash
just fuzz parse                          # default budget, runs until killed
just fuzz parse -- -runs=10000          # bounded
just fuzz format_round_trip -- -max_total_time=30
```

The CI workflow runs `parse` for a 30-second smoke as part of every
push (not blocking PRs).

## Corpus

Each target has its own `corpus/<target>/` directory seeded from
`examples/*.wcl`. Add new seeds as the language grows — small files
that exercise specific syntax are more useful than large ones.
