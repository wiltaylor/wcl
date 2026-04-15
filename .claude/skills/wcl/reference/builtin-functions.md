# Built-in Functions

Source of truth: `crates/wcl_lang/src/eval/functions.rs::builtin_registry()` (line 467).

Unknown function → E052. Argument count/type mismatch → E050.

## Strings (Section 14.1)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `upper(s)` | `string → string` | Uppercase |
| `lower(s)` | `string → string` | Lowercase |
| `trim(s)` | `string → string` | Strip whitespace |
| `trim_prefix(s, prefix)` | `(string, string) → string` | Remove prefix |
| `trim_suffix(s, suffix)` | `(string, string) → string` | Remove suffix |
| `replace(s, from, to)` | `(string, string, string) → string` | Replace all literal |
| `split(s, sep)` | `(string, string) → list(string)` | Split on sep |
| `join(list, sep)` | `(list, string) → string` | Join with sep |
| `starts_with(s, prefix)` | `(string, string) → bool` | |
| `ends_with(s, suffix)` | `(string, string) → bool` | |
| `contains(s, sub)` | `(string, string) → bool` | |
| `length(s)` | `string → i64` | Character count |
| `substr(s, start, end)` | `(string, i64, i64) → string` | Half-open range |
| `format(fmt, ...args)` | `(string, ...) → string` | Printf-style |
| `regex_match(s, pat)` | `(string, string) → bool` | Full-string regex test |
| `regex_capture(s, pat)` | `(string, string) → list(string)` | Capture groups |
| `regex_replace(s, pat, to)` | `(string, string, string) → string` | Replace first |
| `regex_replace_all(s, pat, to)` | `(string, string, string) → string` | Replace all |
| `regex_split(s, pat)` | `(string, string) → list(string)` | Split on regex |
| `regex_find(s, pat)` | `(string, string) → string` | First match or "" |
| `regex_find_all(s, pat)` | `(string, string) → list(string)` | All matches |

## Math (Section 14.2)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `abs(n)` | `number → number` | Absolute value |
| `min(a, b)` | `(number, number) → number` | Two-arg min |
| `max(a, b)` | `(number, number) → number` | Two-arg max |
| `floor(n)` | `f64 → i64` | Floor |
| `ceil(n)` | `f64 → i64` | Ceiling |
| `round(n)` | `f64 → i64` | Round half-away |
| `sqrt(n)` | `f64 → f64` | Square root |
| `pow(base, exp)` | `(f64, f64) → f64` | Exponent |

## Collections (Section 14.3)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `len(x)` | `list \| map \| string → i64` | Length |
| `keys(m)` | `map → list(string)` | Map keys |
| `values(m)` | `map → list` | Map values |
| `flatten(list)` | `list → list` | One level |
| `concat(a, b)` | `(list, list) → list` | Append |
| `distinct(list)` | `list → list` | Deduplicate |
| `sort(list)` | `list → list` | Ascending |
| `reverse(list)` | `list → list` | Reverse order |
| `index_of(list, x)` | `(list, any) → i64` | -1 if absent |
| `range(start, end)` | `(i64, i64) → list(i64)` | Half-open [start, end) |
| `zip(a, b)` | `(list, list) → list` | Pair elements |

## Table Manipulation (Section 14.3b)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `find(list, key, value)` | `(list, string, any) → map \| null` | First matching row |
| `insert_row(list, row)` | `(list, map) → list` | Append row |
| `remove_rows(list, key, value)` | `(list, string, any) → list` | Drop matching |
| `update_rows(list, key, value, updates)` | `(list, string, any, map) → list` | Patch matching |

## Higher-Order (Section 14.4)

Use lambda syntax: `x => x * 2`.

| Function | Signature | Purpose |
|----------|-----------|---------|
| `map(list, fn)` | `(list, lambda) → list` | Transform each |
| `filter(list, fn)` | `(list, lambda) → list` | Keep where true |
| `every(list, fn)` | `(list, lambda) → bool` | All true |
| `some(list, fn)` | `(list, lambda) → bool` | Any true |
| `reduce(list, init, fn)` | `(list, any, lambda) → any` | Left fold |
| `count(list, fn)` | `(list, lambda) → i64` | Count where true |

## Aggregates (Section 14.5)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `sum(list)` | `list(number) → number` | |
| `avg(list)` | `list(number) → f64` | |
| `min_of(list)` | `list(number) → number` | List min |
| `max_of(list)` | `list(number) → number` | List max |

## Hash / Encoding (Section 14.6)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `sha256(s)` | `string → string` | Hex digest |
| `base64_encode(s)` | `string → string` | |
| `base64_decode(s)` | `string → string` | |
| `json_encode(v)` | `any → string` | |

## Type Coercion (Section 14.7)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `to_string(v)` | `any → string` | |
| `to_int(v)` | `any → i64` | |
| `to_float(v)` | `any → f64` | |
| `to_bool(v)` | `any → bool` | |
| `type_of(v)` | `any → string` | Type name |

## Date / Duration (Section 14.8)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `date(s)` | `string → date` | Parse `YYYY-MM-DD` |
| `duration(s)` | `string → duration` | Parse `PnYnMnDTnHnMnS` |

## Reference / Introspection (Section 14.9)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `has(x, key)` | `(map \| block, string) → bool` | Attribute present |
| `has_decorator(block, name)` | `(block, string) → bool` | Decorator present |
| `is_imported(path)` | `string → bool` | Special-cased in evaluator |
| `has_schema(name)` | `string → bool` | Special-cased in evaluator |
| `ref(target)` | `ident \| string → block` | See syntax.md |

`is_imported` and `has_schema` are evaluator special-cases, not in the function registry.
