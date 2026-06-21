# String builtins

The string builtins cover searching, splitting/joining, reshaping, slicing, and formatting. All indexing is character-based, not byte-based.

| Function | Use |
| --- | --- |
| `to_upper(s)` | Uppercase the string |
| `to_lower(s)` | Lowercase the string |
| `split(s, sep)` | Split into a list on each `sep` |
| `join(xs, sep)` | Join a list of strings with `sep` between |
| `contains(s, sub)` | True if `s` contains the substring `sub` |
| `starts_with(s, p)` | True if `s` begins with prefix `p` |
| `ends_with(s, q)` | True if `s` ends with suffix `q` |
| `trim(s)` | Strip leading and trailing whitespace |
| `chars(s)` | Split into a list of single-character strings |
| `format(fmt, …)` | Interpolate args into `{}` slots of `fmt` |
| `replace(s, from, to)` | Replace occurrences of `from` with `to` |
| `slice(s, start, end)` | The substring from `start` up to `end` |
| `len(s)` | The number of characters in the string |

> [!NOTE]
> **to_upper / to_lower, not uppercase**
> WCL's case builtins are `to_upper` and `to_lower`; substring slicing is `slice` (there is no `substring` builtin), and string length uses the same `len` as lists.

## Related

- [Strings](../references/concept_strings.md)

- [Expressions](../references/concept_expressions.md)

[← All facts](../references/facts_ref.md)
