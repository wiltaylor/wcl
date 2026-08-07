# Lists, tensors and records

The three composite **values** you write. The three composite **type declarations** — unions,
interfaces and optionals — are in `lang_types.md`. The distinction is the point: a record is a
value you write, a union is a type you declare.

## Lists

A `list<T>` is an ordered, homogeneous sequence. Lists carry every many-of-the-same-thing in
WCL, including the data gathered by `@children` and the rows of a `table`.

```wcl
@document
type Doc {
  xs:    list<i64>
  names: list<utf8>
  empty: list<i64>
  grid:  list<list<i64>>
}

xs    = [1, 2, 3, 4]
names = ["alice", "bob"]
empty = []
grid  = [
  [1, 2, 3],
  [4, 5, 6],      // a trailing comma is legal
]
```

The element type comes from the field's declared type. With no declared type in context, it is
inferred from the contents.

### Indexing

**There is no index operator.** `items[0]` is a parse error: `[` only ever opens a list
literal, a tensor shape, or a table row. Read an element with the `at` builtin.

```wcl
first  = at(items, 0)
middle = slice([1, 2, 3, 4], 1, 3)     // [2, 3]
```

A dotted path *does* accept an integer segment — `steps.1` — but that is a **label** match on a
gathered block list, not positional indexing. See `lang_documents.md`.

### Working with lists

The collection builtins do the work: `len`, `head`, `tail`, `at`, `take`, `drop`, `slice`,
`range`, `reverse`, `sort`, `unique`, `flatten`, `zip`, `enumerate`, `sum`, `list_contains`,
`index_of`, `map`, `filter`, `fold`, `any`, `all`, `find`, `sort_by`, `min_by`, `max_by`,
`group_by`.

```wcl
doubled   = map([1, 2, 3], fn(x: i64) -> i64 x * 2)              // [2, 4, 6]
evens     = filter(range(0, 10), fn(x: i64) -> bool x % 2 == 0)
total     = fold([1, 2, 3], 0, fn(a: i64, x: i64) -> i64 a + x)  // 6
has_admin = any(users, fn(u: User) -> bool u.role == :admin)
first_big = find([3, 8, 12], fn(x: i64) -> bool x > 5)           // 8
by_len    = sort_by(["ccc", "a", "bb"], fn(s: utf8) -> i64 len(s))
buckets   = group_by([1, 2, 3, 4], fn(x: i64) -> i64 x % 2)
// [{ key: 1, items: [1, 3] }, { key: 0, items: [2, 4] }]
pairs     = enumerate(["a", "b"])                                // [[0, "a"], [1, "b"]]
```

Signatures and return values for all of them are in `lang_builtins.md`.

## Tensors

A `tensor<T, [dims…]>` is an N-dimensional array. Unlike a `list<list<…>>` it carries an
explicit **shape**. The type system and any host reading the value can therefore see its rank
and its per-axis sizes.

```wcl
type Model {
  weights: tensor<f64, [10, 20]>     // fixed 10 × 20 matrix
  batch:   tensor<f64, [N, 3]>       // N rows of 3 floats — N is symbolic
  volume:  tensor<u8,  [W, H, D]>    // three symbolic dims
}
```

A dimension is either a fixed integer or a **symbolic name** the host resolves. Symbolic dims
document a shape the language does not check.

Build one from a flat list plus a shape. The data length must equal the product of the
dimensions.

```wcl
m = tensor([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3])   // 2 × 3
```

| Builtin | Result |
| --- | --- |
| `tensor_data(t)` | flat `list<T>` of the elements |
| `tensor_shape(t)` | `list<usize>` of the per-axis sizes |
| `tensor_reshape(t, shape)` | the same data viewed under a new shape |

```wcl
m_t = tensor_reshape(m, [3, 2])     // same six numbers, 3 × 2
```

Reach for a tensor when the data is genuinely rectangular and the rank matters — matrices,
images, batches. For ragged or one-dimensional data a `list<T>` is simpler.

## Records

A `type Name { … }` declares a **named record**: a fixed set of named, typed fields. Records
describe field values, function parameters and the backing type of every block.

```wcl
type Dog {
  name: utf8
  age:  u32
}

@document
type Kennel {
  resident: Dog
}

resident = { name: "Rex", age: 4u32 }
```

### There is no constructor call

A record **value** is a bare `{ field: value, … }` literal. There is no `Dog { … }` syntax. The
literal takes its type from the position it lands in — the declared type of the field,
parameter or return it fills. WCL checks it against that type there.

That is why a record literal in a slot with no declared type is untyped and unvalidated: with
nothing to check against, it is simply an anonymous record.

Note the punctuation: a record literal uses `field: value` with **commas**, the same colon a
type declaration uses, and not the `=` of a document field.

### Field access

A dotted path off a **local** name reads a member. The local may be a `let` item, a `let … ;`
binding, or a function parameter. The path chains through nested records and variant payloads.

```wcl
let origin = { x: 0.0, inner: { zone: "a" } }

zone = origin.inner.zone                 // "a"
name = { let r = at(rows, 0); r.name }   // bind, then read
```

> **A document *field* holding a record is not walkable this way.** A document path descends
> through blocks and stops at the field. So `meta.region`, where `meta` is a field, is an
> unresolved reference. A call result behaves the same way: `at(rows, 0).name` fails with
> `not_a_reference`. Bind the value with a `let` first, or destructure it with `match`. See
> `lang_expressions.md`.

### `extends`

A record may `extends` another and inherit its fields. The child is a structural superset, so
it satisfies any context that accepts the parent.

```wcl
type Pet extends Dog {
  breed: utf8
}
```

### Record builtins

| Builtin | Result |
| --- | --- |
| `keys(r)` | the field names, sorted |
| `values(r)` | the values, in the same sorted order |
| `merge(a, b)` | the two records combined; `b` wins on a clash |
| `map_values(r, f)` | every value transformed, keys kept |

```wcl
let rex      = { name: "Rex", age: 4u32 }
let names    = keys(rex)                        // ["age", "name"]
let defaults = { host: "localhost", port: 80 }
let cfg      = merge(defaults, { port: 8080 })  // { host: "localhost", port: 8080 }
let doubled  = map_values({ low: 1, high: 9 }, fn(x: i64) -> i64 x * 2)
```

### Records and blocks

Decorate a record type with `@block("kind")` and it becomes a block kind. Its labels then bind
positionally to the fields marked `@inline(N)`. See `lang_schemas.md`.

## Bare-record coercion to a union variant

This is the one rule in this chapter that surprises people, and the one most likely to explain
a confusing error message.

**When the declared type is a union — or a `list<Union>` — a bare record literal coerces to the
matching variant by shape.** You do not have to write the `Union::Variant` tag.

```wcl
union ChartSeries {
  Of { name: utf8  values: list<f64> }
  Ref { source: utf8 }
}

// `series: list<ChartSeries>` — the variant is inferred per element.
series = [
  { name: "North", values: [42.0, 55.0] },   // → ChartSeries::Of
  { source: "sales.csv" },                   // → ChartSeries::Ref
]
```

### How the match works

1. Candidates are the variants whose declared field-**name set** is exactly equal to the
   record's. Not a subset, not a superset — equal.
2. If more than one candidate survives, the field **types** narrow it further.
3. Exactly one survivor is the answer. No survivor is the `VariantNoMatch` violation, which
   reads `no variant of 'X' matches the supplied shape`. More than one survivor is an ambiguity
   error. Both surface when the field **evaluates**, so a consumer reading the field sees them.
   `wcl check` alone may not.

Two consequences follow directly:

- **A bare record must carry the full field set, optional members included.** The shape is what
  picks the variant, so leaving an optional out changes the name set and loses the match. Only
  the explicit `Union::Variant { … }` form may omit an optional.
- **Two variants with the same field names and the same field types cannot be told apart.** Tag
  those explicitly.

The explicit form always wins and is the escape hatch:

```wcl
a = ChartSeries::Of { name: "North", values: [42.0] }
b = ChartSeries::Ref { source: "sales.csv" }
```

### Where the coercion runs

Three places, which is exactly the set of positions where a value meets a declared type:

- a **field's** value, against the field's declared type;
- a **variant payload** being built, against the variant's declared field types (so a bare
  record nested inside another one also infers);
- a **function argument**, against the parameter's declared type.

Nowhere else. A bare record assigned to a `let` with no type in context stays an anonymous
record, because there is no declared type to match against.

## Worked example

```wcl
union Source {
  Inline { rows: list<i64> }
  File   { path: utf8  header: bool }
}

@block("chart") type Chart {
  @inline(0) id: identifier
  data:   Source
  label:  utf8
  totals: list<i64>
  shape:  tensor<i64, [2, 2]>
}

@document type Report {
  @children("chart") charts: list<Chart>
}

chart sales {
  data   = { path: "sales.csv", header: true }   // → Source::File, by shape
  label  = { let d = self.data; d.path }         // bind, then read the payload
  totals = map([1, 2, 3], fn(x: i64) -> i64 x * 10)
  shape  = tensor([1, 2, 3, 4], [2, 2])
}
```

```console
$ wcl get report.wcl charts.sales.totals
[10, 20, 30]
$ wcl get report.wcl charts.sales.label
"sales.csv"
$ wcl get report.wcl charts.sales.data
Source::File { header: true, path: "sales.csv" }
```

Drop `header` from that record and reading `data` fails with `no variant of 'Source' matches the
supplied shape`: `{ path }` alone equals no variant's name set. That is the coercion rule
working, not a bug.
