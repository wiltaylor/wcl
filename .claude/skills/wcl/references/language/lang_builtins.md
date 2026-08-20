# Builtins

The builtin set is **closed**. The language ships 100 of them and you cannot add one from WCL —
a host adds one in Rust. All 100 are in the tables below, each with its exact signature and a
worked example, so take every name and argument count from there: an unknown callee is
`wcl::eval::unknown_builtin`, and a wrong argument count is `wcl::eval::builtin_arity`.

The list is self-describing at runtime, which is how you check it in a version you do not have
this page for:

```wcl
@schemaless names = builtin_names()   // every name, sorted
@schemaless sigs  = map(builtin_names(), fn(n: utf8) -> record { fn_signature(n) })
```

`fn_signature` answers a record with `doc`, `params` (each `{name, type, doc}`), `return_type`,
`return_doc` and `is_builtin`. Pass it a function value or a builtin's name as a string.

## Reading the signatures

- `[T]` is a list. Where a builtin also accepts a tensor, its description says so
  (`map`, `fold`, `len`, `sum`, `head`, `tail`).
- `fn (T) -> U` is a function value. Write it as a literal: `fn(x: i64) -> i64 { x * 2 }`.
  There are no default arguments and no named arguments.
- `number` means any numeric type. The math family widens its input to `f64` and answers `f64`.
- `record` is a `{ key: value }` literal or a value of record shape.
- `&T` is a reference to a declaration — you write the declaration's name (`Image`,
  `Field.score`), not a string.

## The builtins

### Lists and collections

| Signature | What it does |
| --- | --- |
| `all(xs: [T], pred: fn (T) -> bool) -> bool` | `true` when the predicate holds for every element (short-circuits; `true` for an empty list). |
| `any(xs: [T], pred: fn (T) -> bool) -> bool` | `true` when the predicate holds for at least one element (short-circuits). |
| `at(xs: [T], i: i64) -> T` | The element at a zero-based index; errors if out of bounds or negative. |
| `drop(xs: [T], n: i64) -> [T]` | Every element of a list after the first `n`. |
| `enumerate(xs: [T]) -> [[i64, T]]` | Pair every element with its zero-based index, as `[index, element]` pairs. |
| `filter(xs: [T], pred: fn (T) -> bool) -> [T]` | Keep only the list elements for which the predicate returns `true`. |
| `find(xs: [T], pred: fn (T) -> bool) -> T` | The first element for which the predicate returns `true`, or `none`. |
| `flatten(xss: [[T]]) -> [T]` | Concatenate a list of lists into a single list, one level deep. |
| `fold(xs: [T], init: U, f: fn (U, T) -> U) -> U` | Reduce a list or tensor to a single value by repeatedly combining the accumulator with each element. |
| `group_by(xs: [T], key: fn (T) -> K) -> [record]` | Group elements by a key function into `{ key, items }` records, in first-seen key order. |
| `head(xs: [T]) -> T` | The first element of a list or tensor (`none` when empty). |
| `index_of(xs: [T], needle: T) -> i64` | The index of the first element equal to `needle`, or `-1` if absent. |
| `len(xs: [T]) -> usize` | The number of elements in a list or tensor, or characters in a string. |
| `list_contains(xs: [T], needle: T) -> bool` | Whether a list contains a value equal to `needle`. |
| `map(xs: [T], f: fn (T) -> U) -> [U]` | Apply a function to every element of a list or tensor, returning the transformed collection. |
| `max_by(xs: [T], key: fn (T) -> K) -> T` | The element with the largest key, or `none` for an empty list. |
| `min_by(xs: [T], key: fn (T) -> K) -> T` | The element with the smallest key, or `none` for an empty list. |
| `range(start: i64, end: i64) -> [i64]` | The half-open integer range `[start, end)` as a list. |
| `reverse(xs: [T]) -> [T]` | Reverse the order of a list's elements. |
| `slice(xs: utf8 \| [T], start: i64, end: i64) -> utf8 \| [T]` | The half-open range `[start, end)` of a string's characters or a list's elements (bounds are clamped). |
| `sort(xs: [T]) -> [T]` | Sort a list — numerically for all-numeric lists, lexicographically for all-string lists. |
| `sort_by(xs: [T], key: fn (T) -> K) -> [T]` | Sort a list by a key function (stable). Keys must be all numeric or all strings. |
| `sort_connected(items: [T], edges: [{source, destination, ...}]) -> [T]` | Reorder a list so that items joined by edges cluster together (recursing into `children`). |
| `sum(xs: [number]) -> number` | Add together every element of a non-empty homogeneous numeric list or tensor. |
| `tail(xs: [T]) -> [T]` | Every element of a list or tensor except the first. |
| `take(xs: [T], n: i64) -> [T]` | The first `n` elements of a list (fewer if the list is shorter). |
| `unique(xs: [T]) -> [T]` | Remove duplicate elements from a list, keeping first-seen order. |
| `zip(a: [A], b: [B]) -> [(A, B)]` | Pair up elements of two lists by index, stopping at the shorter length. |

```wcl
all([2, 4, 6], fn(x: i64) -> bool { x % 2 == 0 })   // are all elements even? → true
any([1, 2, 3], fn(x: i64) -> bool { x > 2 })   // is any element greater than 2? → true
at([10, 20, 30], 1)   // element at index 1 → 20
drop([1, 2, 3, 4], 2)   // skip the first 2 elements → [3, 4]
enumerate(["a", "b"])   // pair each element with its index → [[0, "a"], [1, "b"]]
filter([1, 2, 3, 4], fn(x: i64) -> bool { x % 2 == 0 })   // keep the even numbers → [2, 4]
find([1, 2, 3], fn(x: i64) -> bool { x > 1 })   // first element greater than 1 → 2
flatten([[1, 2], [3]])   // concatenate the inner lists, one level deep → [1, 2, 3]
fold([1, 2, 3], 0, fn(acc: i64, x: i64) -> i64 { acc + x })   // sum the list, starting from 0 → 6
group_by([1, 2, 3, 4], fn(x: i64) -> i64 { x % 2 })   // group by even/odd → [{ key: 1, items: [1, 3] }, { key: 0, items: [2, 4] }]
head([1, 2, 3])   // take the first element → 1
index_of([10, 20, 30], 20)   // index of the matching element → 1
len([10, 20, 30])   // count the elements → 3
list_contains([1, 2, 3], 2)   // is the value in the list? → true
map([1, 2, 3], fn(x: i64) -> i64 { x * 2 })   // double each element → [2, 4, 6]
max_by(["a", "abc", "ab"], fn(s: utf8) -> i64 { len(s) })   // element with the largest key → "abc"
min_by(["abc", "a", "ab"], fn(s: utf8) -> i64 { len(s) })   // element with the smallest key → "a"
range(0, 4)   // integers from 0 up to (not incl.) 4 → [0, 1, 2, 3]
reverse([1, 2, 3])   // reverse the order → [3, 2, 1]
slice("hello", 0, 2)   // characters from index 0 up to (not incl.) 2 → "he"
sort([3, 1, 2])   // sort numerically → [1, 2, 3]
sort_by(["abc", "a", "ab"], fn(s: utf8) -> i64 { len(s) })   // sort by length → ["a", "ab", "abc"]
sort_connected([{ id: "a" }, { id: "b" }, { id: "c" }], [{ source: "a", destination: "c" }])   // pull "c" up beside "a" → [{ id: "a" }, { id: "c" }, { id: "b" }]
sum([1, 2, 3, 4])   // add up the elements → 10
tail([1, 2, 3])   // everything after the first → [2, 3]
take([1, 2, 3, 4], 2)   // keep the first 2 elements → [1, 2]
unique([1, 2, 2, 3, 1])   // drop duplicates, keep first-seen order → [1, 2, 3]
zip([1, 2, 3], ["a", "b"])   // pair up by index, stopping at the shorter → [[1, "a"], [2, "b"]]
```

### Strings

| Signature | What it does |
| --- | --- |
| `chars(s: utf8) -> [utf8]` | The characters of a string as a list of one-character strings. |
| `concat(a: utf8, b: utf8) -> utf8` | Concatenate two strings into one. |
| `contains(s: utf8, needle: utf8) -> bool` | Whether a string contains a substring. |
| `ends_with(s: utf8, suffix: utf8) -> bool` | Whether a string ends with a suffix. |
| `format(template: utf8) -> utf8` | Substitute trailing arguments into a template's `{}` placeholders (`{{`/`}}` are literal braces). |
| `format_unit(value: i64, type: utf8, unit: utf8) -> utf8` | Render a base-unit value in a chosen unit, looking the factor up from a unit type by name: `format_unit(size, "std.ByteSize", "MiB")` → `"5 MiB"`. The inverse of literal-unit resolution, so it stays correct if the type's `@unit` factor changes. |
| `format_unit_value(value: i64, factor: i64, unit: utf8) -> utf8` | Render a number in a unit given its factor explicitly: `format_unit_value(5242880, 1048576, "MiB")` → `"5 MiB"`. The primitive behind `format_unit` for callers that already hold the factor. |
| `join(parts: [utf8], sep: utf8) -> utf8` | Join a list of strings into one, inserting a separator between each. |
| `pad_end(s: utf8, width: i64, pad: utf8) -> utf8` | Right-pad a string with a fill pattern until it is `width` characters long. |
| `pad_start(s: utf8, width: i64, pad: utf8) -> utf8` | Left-pad a string with a fill pattern until it is `width` characters long. |
| `repeat(s: utf8, n: i64) -> utf8` | A string repeated `n` times (empty for `n <= 0`). |
| `replace(s: utf8, old: utf8, new: utf8) -> utf8` | Replace every occurrence of a substring with another. |
| `split(s: utf8, sep: utf8) -> [utf8]` | Split a string on every occurrence of a separator into a list of pieces. |
| `starts_with(s: utf8, prefix: utf8) -> bool` | Whether a string begins with a prefix. |
| `to_lower(s: utf8) -> utf8` | Lowercase every character of a string. |
| `to_upper(s: utf8) -> utf8` | Uppercase every character of a string. |
| `trim(s: utf8) -> utf8` | Remove leading and trailing whitespace from a string. |

```wcl
chars("abc")   // explode into single-character strings → ["a", "b", "c"]
concat("foo", "bar")   // join the two strings → "foobar"
contains("hello", "ell")   // does the string contain the substring? → true
ends_with("hello", "lo")   // does it end with this suffix? → true
format("{} = {}", "x", 42)   // substitute the args into the {} slots → "x = 42"
format_unit(5242880, "std.ByteSize", "MiB")   // render bytes as MiB → "5 MiB"
format_unit_value(5242880, 1048576, "MiB")   // divide by the factor and label → "5 MiB"
join(["a", "b", "c"], "-")   // join with a dash between pieces → "a-b-c"
pad_end("42", 5, "0")   // right-pad to width 5 → "42000"
pad_start("42", 5, "0")   // left-pad to width 5 → "00042"
repeat("ab", 3)   // concatenate 3 copies → "ababab"
replace("hello world", "world", "there")   // replace the matched substring → "hello there"
split("a,b,c", ",")   // split on each comma → ["a", "b", "c"]
starts_with("hello", "he")   // does it begin with this prefix? → true
to_lower("AbC")   // lowercase every character → "abc"
to_upper("abc")   // uppercase every character → "ABC"
trim("  hi  ")   // strip leading/trailing whitespace → "hi"
```

### Math

| Signature | What it does |
| --- | --- |
| `abs(x: number) -> f64` | Absolute value. |
| `acos(x: number) -> f64` | Arccosine, in radians, of a value in [-1, 1]. |
| `asin(x: number) -> f64` | Arcsine, in radians, of a value in [-1, 1]. |
| `atan(x: number) -> f64` | Arctangent, in radians. |
| `atan2(a: number, b: number) -> f64` | Arctangent of `a/b` in radians, using the signs of both to pick the quadrant. |
| `cbrt(x: number) -> f64` | Cube root. |
| `ceil(x: number) -> f64` | Round up to the nearest integer. |
| `clamp(x: number, lo: number, hi: number) -> f64` | Constrain `x` to the range `[lo, hi]`. |
| `cos(x: number) -> f64` | Cosine of an angle in radians. |
| `degrees(x: number) -> f64` | Convert an angle from radians to degrees. |
| `e() -> f64` | Euler's number e (≈ 2.71828). |
| `exp(x: number) -> f64` | `e` raised to the power `x`. |
| `floor(x: number) -> f64` | Round down to the nearest integer. |
| `hypot(a: number, b: number) -> f64` | Length of the hypotenuse `sqrt(a² + b²)`. |
| `ln(x: number) -> f64` | Natural (base-`e`) logarithm. |
| `log10(x: number) -> f64` | Base-10 logarithm. |
| `log2(x: number) -> f64` | Base-2 logarithm. |
| `max(a: number, b: number) -> f64` | The larger of two numbers. |
| `min(a: number, b: number) -> f64` | The smaller of two numbers. |
| `pi() -> f64` | The constant π (≈ 3.14159). |
| `pow(a: number, b: number) -> f64` | Raise `a` to the power `b`. |
| `radians(x: number) -> f64` | Convert an angle from degrees to radians. |
| `round(x: number) -> f64` | Round to the nearest integer (ties away from zero). |
| `sign(x: number) -> f64` | The sign of `x`: `1`, `-1`, or `0`. |
| `sin(x: number) -> f64` | Sine of an angle in radians. |
| `sqrt(x: number) -> f64` | Square root. |
| `tan(x: number) -> f64` | Tangent of an angle in radians. |
| `tau() -> f64` | The constant τ = 2π (≈ 6.28319). |
| `trunc(x: number) -> f64` | Discard the fractional part, rounding toward zero. |

```wcl
abs(-7.5)   // magnitude, dropping the sign → 7.5
acos(1.0)   // arccosine of 1 → 0.0
asin(0.0)   // arcsine of 0 → 0.0
atan(0.0)   // arctangent of 0 → 0.0
atan2(1.0, 1.0)   // angle of the vector (1, 1) → 0.7853981633974483
cbrt(27)   // cube root → 3.0
ceil(3.1)   // round up to a whole number → 4.0
clamp(12.0, 0.0, 10.0)   // constrain the value into [0, 10] → 10.0
cos(0.0)   // cosine of 0 radians → 1.0
degrees(pi())   // convert π radians to degrees → 180.0
e()   // Euler's number → 2.718281828459045
exp(0.0)   // e raised to the power 0 → 1.0
floor(3.9)   // round down to a whole number → 3.0
hypot(3, 4)   // hypotenuse of a 3-4 right triangle → 5.0
ln(1.0)   // natural (base-e) logarithm of 1 → 0.0
log10(1000.0)   // base-10 logarithm of 1000 → 3.0
log2(8.0)   // base-2 logarithm of 8 → 3.0
max(3, 7.5)   // the larger of the two → 7.5
min(3, 7.5)   // the smaller of the two → 3.0
pi()   // the constant π → 3.141592653589793
pow(2, 10)   // 2 raised to the 10th power → 1024.0
radians(180.0)   // convert 180 degrees to radians → 3.141592653589793
round(2.5)   // nearest integer (ties away from zero) → 3.0
sign(-3.0)   // negative input -> -1.0 → -1.0
sin(0.0)   // sine of 0 radians → 0.0
sqrt(144)   // square root → 12.0
tan(0.0)   // tangent of 0 radians → 0.0
tau()   // the constant τ = 2π → 6.283185307179586
trunc(3.9)   // drop the fractional part → 3.0
```

### Records

| Signature | What it does |
| --- | --- |
| `keys(r: record) -> [utf8]` | The field names of a record, in deterministic (sorted) order. |
| `map_values(r: record, f: fn (T) -> U) -> record` | Apply a function to every field value of a record, keeping the keys. |
| `merge(a: record, b: record) -> record` | Combine two records into one; fields of `b` win on a name clash. |
| `values(r: record) -> [T]` | The field values of a record, in the same order as `keys`. |

```wcl
keys({ name: "Rex", age: 4 })   // field names, sorted → ["age", "name"]
map_values({ low: 1, high: 9 }, fn(x: i64) -> i64 { x * 2 })   // double every value, keep the keys → { high: 18, low: 2 }
merge({ host: "localhost", port: 80 }, { port: 8080 })   // the second record wins on the port clash → { host: "localhost", port: 8080 }
values({ name: "Rex", age: 4 })   // field values, in key order → [4, "Rex"]
```

### Tensors

| Signature | What it does |
| --- | --- |
| `tensor(data: [number], shape: [usize]) -> tensor<T>` | Build a tensor from flat row-major data and a shape; the data length must equal the product of the dimensions. |
| `tensor_data(t: tensor<T>) -> [T]` | The flat row-major element data of a tensor as a list. |
| `tensor_reshape(t: tensor<T>, shape: [usize]) -> tensor<T>` | Reinterpret a tensor's data under a new shape; the element count must be unchanged. |
| `tensor_shape(t: tensor<T>) -> [usize]` | The dimension sizes of a tensor as a list. |

```wcl
tensor([1, 2, 3, 4], [2, 2])   // build a 2x2 tensor from flat data → a 2x2 tensor
tensor_data(tensor([1, 2, 3, 4], [2, 2]))   // the flat row-major data → [1, 2, 3, 4]
tensor_reshape(tensor([1, 2, 3, 4], [2, 2]), [4])   // reshape 2x2 into 1-D of length 4 → a length-4 tensor
tensor_shape(tensor([1, 2, 3, 4], [2, 2]))   // the dimension sizes → [2, 2]
```

### Paths and globs

| Signature | What it does |
| --- | --- |
| `glob_match(pattern: utf8, path: utf8) -> bool` | Match one concrete path against a glob. `*` stays within a segment, `**` spans segments, `?` matches one character, `[a-z]` / `[!x]` are character classes. A trailing `/` on the pattern matches the whole subtree. |
| `glob_overlaps(a: utf8, b: utf8) -> bool` | Whether two glob patterns can match a common path. Concrete paths are patterns too, so this subsumes `glob_match` for overlap gates. Trailing `/` means the whole subtree. Conservative: exotic negated-class pairings may report `true` when no shared path exists, never `false` when one does. |
| `path_contains(parent: utf8, child: utf8) -> bool` | Segment-aware path prefix test: whether `child` is `parent` itself or lives under it. Splits on `/`, so `src/` does not contain `src2/x`. A path contains itself. |

```wcl
glob_match("src/*.rs", "src/main.rs")   // one segment, .rs extension → true
glob_overlaps("src/", "src/*.rs")   // the subtree owns every .rs directly under src → true
path_contains("src/", "src/core/mod.rs")   // nested under src/, segment-aware → true
```

### Reflection

| Signature | What it does |
| --- | --- |
| `ast_string(target: &T) -> utf8` | Pretty-print the canonical source behind a reference (type/interface/union/symbol_set/block/field) or a function value. |
| `builtin_names() -> [utf8]` | The names of every registered built-in function, sorted. Pair with `fn_signature` to introspect each one. |
| `child_types(target: &T) -> [&T]` | Reflect a type into references to the element types of its `@child` / `@children` block slots (own slots first, then inherited via `extends`). Pair with `type_table` / `type_fields` to auto-document the blocks a `@document` declares. |
| `decl_info(target: &T) -> record` | Describe a top-level declaration: its name, kind, doc comment, and schema classification (block / table / decorator / document). |
| `decorator_arg(target: &T, decorator: utf8, slot: utf8) -> any` | Read one named argument of a decorator on a referenced declaration (`none` if absent). |
| `decorator_names(target: &T) -> [utf8]` | List the names of the decorators attached to a referenced declaration. |
| `decorators_for_kind(kind: utf8) -> [&T]` | List references to decorator schemas applicable to a block kind. Pair with `decl_info`, `doc_comment`, and `type_fields` to render them. |
| `doc_comment(target: &T) -> utf8` | The doc comment — the contiguous run of `#` / `//` lines immediately above a declaration — attached to a reference, or `""` when there is none. Complements `decorator_arg(x, "doc", …)` for `@doc("…")` metadata. |
| `eval(src: utf8) -> any` | Parse a string as a WCL expression and evaluate it in the current scope. |
| `fn_signature(f: any) -> record` | Describe a function's parameters and return type. Pass a function value, or a built-in's name as a string. |
| `namespace_decls(ns: utf8) -> [&T]` | List references to every top-level declaration (`type` / `interface` / `union` / `symbol_set`) in a namespace, for schema-documentation generators. Pair with `decl_info`, `doc_comment`, `type_fields`, and `ast_string` to render each. Imported (library) declarations are included — filter on `decl_info(d).is_imported` to drop them. |
| `type_fields(target: &T) -> [record]` | Reflect a type or interface into a list of field-description records (own fields first, then inherited via `extends`). |

```wcl
ast_string(Image)   // pretty-print a type's source → "type Image { ... }"
builtin_names()   // names of every builtin → ["abs", "acos", ..., "zip"]
child_types(MyDoc)   // element types of the doc’s block slots → [&ProjectMeta, &Settings]
decl_info(MyDoc)   // declaration metadata for a type → { kind: "document", is_document: true, name: "MyDoc" }
decorator_arg(Image, "block", "name")   // read a decorator argument by name → "image"
decorator_names(Image)   // the decorators on the Image type → ["block", "schemaless", ...]
decorators_for_kind("image")   // decorator schemas legal on image blocks → [Visibility, ...]
doc_comment(Image)   // the doc comment above a type → the comment lines above the Image type
eval("1 + 2 * 3")   // parse and evaluate a WCL expression → 7
fn_signature("map")   // describe the map builtin → { signature: "fn(xs: [T], ...) -> [U]", ... }
namespace_decls("wdoc")   // every top-level decl in the wdoc namespace → [&Page, &Site, ...]
type_fields(Image)   // reflect the Image type into field records → [{ name: "source", type: "utf8", ... }, ...]
```

### Control and errors

| Signature | What it does |
| --- | --- |
| `assert(cond: bool, msg: utf8) -> none` | Return `none` when `cond` is true, otherwise abort with `msg`. |
| `error(msg: utf8) -> never` | Abort evaluation with an error message. |
| `panic(msg: utf8) -> never` | Abort evaluation with an unrecoverable failure message. |

```wcl
assert(1 + 1 == 2, "math is broken")   // verify a condition holds → none
error("unreachable state")   // abort evaluation with a message → (aborts)
panic("invariant violated")   // abort with an unrecoverable failure → (aborts)
```

## Notes on the sharp edges

- **`len` counts characters, not bytes**, for a string.
- **`at` errors on an out-of-range or negative index.** `slice` clamps its bounds instead.
  `head` / `find` / `min_by` / `max_by` answer `none` on an empty list.
- **`index_of` answers `-1`** when the value is absent, not `none`.
- **`sum` needs a non-empty, homogeneous numeric list.** Mixed widths are not summed for you.
- **`sort` and `sort_by` need all-numeric or all-string keys.** `sort_by` is stable.
- **`keys` and `values` are sorted by key**, so they always agree with each other, and
  `values({ name: "Rex", age: 4 })` is `[4, "Rex"]` — `age` before `name`.
- **`format` uses `{}` placeholders**, and `{{` / `}}` are literal braces. It is not a printf
  format string. String interpolation (`$"…${x}…"`) is usually clearer.
- **`error` and `panic` abort evaluation**; `assert` answers `none` when it passes. Because
  fields evaluate lazily, a field that calls one of these is only reached if something asks for
  it — see [`lang_evaluation.md`](lang_evaluation.md).
- **`eval` parses and evaluates a string in the current scope.** It is a real escape hatch, and
  it defeats every static check. Prefer a `fn` item.
- **The reflection family takes a declaration reference, not a string.** `type_fields(Image)`,
  not `type_fields("Image")`. `decorators_for_kind` is the exception — a kind *is* a string.
- **`glob_overlaps` is deliberately conservative.** It may answer `true` for an exotic
  negated-class pairing that shares no real path. It never answers `false` when one exists.

## Two more that only exist under wdoc

A host may register builtins of its own. wdoc adds two. They are in the environment the `wcl`
CLI builds, so `builtin_names()` run through the CLI answers 102 names rather than 100. They
are useful only in a document that imports the wdoc standard library:

| Name | Purpose |
| --- | --- |
| `page_metadata(ctx)` | A page's position, neighbours and active path in the shared site reading order. |
| `__wdoc_slot(slots, name, field)` | Internal: resolve one field of a declared template slot. |

See [`../wdoc/wdoc_templates.md`](../wdoc/wdoc_templates.md) and
[`../wdoc/wdoc_sites.md`](../wdoc/wdoc_sites.md).
