# Lists

_Ordered, homogeneous sequences — list<T> — and the collection builtins over them._

A `list<T>` is an ordered, homogeneous sequence of values of type `T`. Lists are how you write any many-of-the-same-thing — including the data flowing through `@children`, `@connections`, and table rows.

## Literals

Write a list literal with square brackets and commas. The element type is inferred from its contents, or pinned by the field's declared type.

```wcl
xs:    list<i64>  = [1, 2, 3, 4]
names: list<utf8> = ["alice", "bob"]
empty: list<i64>  = []
```

## Nested lists

List elements can themselves be lists. Use this for matrices, lookup tables, or any rectangular grid.

```wcl
grid: list<list<i64>> = [
  [1, 2, 3],
  [4, 5, 6],
  [7, 8, 9],
]
```

## Working with lists

The collection builtins — `map`, `filter`, `fold`, `len`, `sum`, `head`, `tail`, `range`, `flatten`, `zip`, `reverse`, `sort`, `unique`, `list_contains`, `index_of`, `at`, `take`, `drop`, `slice`, `enumerate` — operate on `list<T>`.

```wcl
doubled = map([1, 2, 3], fn(x: i64) -> i64 x * 2)        // [2, 4, 6]
evens   = filter(range(0, 10), fn(x: i64) -> bool x % 2 == 0)
total   = fold([1, 2, 3], 0, fn(a: i64, x: i64) -> i64 a + x)
```

## Higher-order helpers

Beyond `map`/`filter`/`fold`, predicate and key-function helpers cover the common shapes: `any`, `all`, and `find` test or search; `sort_by`, `min_by`, and `max_by` order or pick by a key; `group_by` buckets elements into `{ key, items }` records.

```wcl
has_admin = any(users, fn(u: User) -> bool u.role == :admin)
first_big = find([3, 8, 12], fn(x: i64) -> bool x > 5)        // 8
by_len    = sort_by(["ccc", "a", "bb"], fn(s: utf8) -> i64 len(s))
buckets   = group_by([1, 2, 3, 4], fn(x: i64) -> i64 x % 2)
// [{ key: 1, items: [1, 3] }, { key: 0, items: [2, 4] }]
pairs     = enumerate(["a", "b"])                              // [[0, "a"], [1, "b"]]
middle    = slice([1, 2, 3, 4], 1, 3)                          // [2, 3]
```

> [!NOTE]
> **Tabular data lives in lists**
> A field declared list<RowType> can be populated by a list literal or by the pipe-row table syntax — see Tables.

## Related

- [Tables](../references/concept_tables.md)

- [Tensors](../references/concept_tensors.md)

[← Back to SKILL.md](../SKILL.md)
