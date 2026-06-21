# Collection builtins

The collection builtins transform, search, and reshape lists. Higher-order ones (`map`, `filter`, `find`, `any`, `all`, `group_by`) take a function literal `fn(x: T) -> U …`.

| Function | Use |
| --- | --- |
| `map(xs, f)` | Apply `f` to each element → a new list |
| `filter(xs, p)` | Keep elements for which predicate `p` is true |
| `flatten(xss)` | Concatenate a list of lists into one list |
| `concat(a, b)` | Join two strings (or two lists) end to end |
| `any(xs, p)` | True if any element satisfies `p` |
| `all(xs, p)` | True if every element satisfies `p` (true for empty) |
| `find(xs, p)` | First element satisfying `p`, or `none` |
| `at(xs, i)` | The element at index `i` |
| `head(xs)` | The first element |
| `tail(xs)` | All but the first element |
| `take(xs, n)` | The first `n` elements |
| `drop(xs, n)` | All but the first `n` elements |
| `reverse(xs)` | The list in reverse order |
| `zip(a, b)` | Pair up elements → a list of `[a, b]` pairs |
| `enumerate(xs)` | Pair each element with its index → `[i, x]` |
| `sort(xs)` | Sort ascending (`sort_by` for a key function) |
| `group_by(xs, key)` | Group elements into `{ key, items }` records by key |
| `unique(xs)` | Remove duplicate elements, keeping first occurrence |
| `len(xs)` | The number of elements (also the length of a string) |

## Related

- [Lists](../references/concept_lists.md)

- [Expressions](../references/concept_expressions.md)

- [Functions](../references/concept_functions.md)

[← All facts](../references/facts_ref.md)
