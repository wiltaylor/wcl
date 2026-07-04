# WCL — builtin functions

Every WCL builtin, grouped by category. Signature, parameters, and an example each.

## Collection functions

### all(xs: \[T\], pred: fn (T) -> bool) → bool

`true` when the predicate holds for every element (short-circuits; `true` for an empty list).

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to test. |
| pred | fn (T) -> bool | Predicate applied to each element. |
| returns | bool | `true` if every element satisfies the predicate. |

```wcl
all([2, 4, 6], fn(x: i64) -> bool { x % 2 == 0 })   // are all elements even? → true
```

### any(xs: \[T\], pred: fn (T) -> bool) → bool

`true` when the predicate holds for at least one element (short-circuits).

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to test. |
| pred | fn (T) -> bool | Predicate applied to each element. |
| returns | bool | `true` if any element satisfies the predicate. |

```wcl
any([1, 2, 3], fn(x: i64) -> bool { x > 2 })   // is any element greater than 2? → true
```

### at(xs: \[T\], i: i64) → T

The element at a zero-based index; errors if out of bounds or negative.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to index. |
| i | i64 | The zero-based index. |
| returns | T | The element at `i`. |

```wcl
at([10, 20, 30], 1)   // element at index 1 → 20
```

### drop(xs: \[T\], n: i64) → \[T\]

Every element of a list after the first `n`.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to drop from. |
| n | i64 | How many leading elements to skip. |
| returns | \[T\] | The elements after the first `n`. |

```wcl
drop([1, 2, 3, 4], 2)   // skip the first 2 elements → [3, 4]
```

### enumerate(xs: \[T\]) → \[\[i64, T\]\]

Pair every element with its zero-based index, as `[index, element]` pairs.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to enumerate. |
| returns | \[\[i64, T\]\] | `[index, element]` pairs. |

```wcl
enumerate(["a", "b"])   // pair each element with its index → [[0, "a"], [1, "b"]]
```

### filter(xs: \[T\], pred: fn (T) -> bool) → \[T\]

Keep only the list elements for which the predicate returns `true`.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to filter. |
| pred | fn (T) -> bool | Predicate deciding whether to keep an element. |
| returns | \[T\] | The elements for which the predicate returned `true`. |

```wcl
filter([1, 2, 3, 4], fn(x: i64) -> bool { x % 2 == 0 })   // keep the even numbers → [2, 4]
```

### find(xs: \[T\], pred: fn (T) -> bool) → T

The first element for which the predicate returns `true`, or `none`.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to search. |
| pred | fn (T) -> bool | Predicate applied to each element. |
| returns | T | The first matching element, or `none`. |

```wcl
find([1, 2, 3], fn(x: i64) -> bool { x > 1 })   // first element greater than 1 → 2
```

### flatten(xss: \[\[T\]\]) → \[T\]

Concatenate a list of lists into a single list, one level deep.

| Parameter | Type | Description |
| --- | --- | --- |
| xss | \[\[T\]\] | A list whose elements are themselves lists. |
| returns | \[T\] | The inner lists concatenated, one level deep. |

```wcl
flatten([[1, 2], [3]])   // concatenate the inner lists, one level deep → [1, 2, 3]
```

### fold(xs: \[T\], init: U, f: fn (U, T) -> U) → U

Reduce a list or tensor to a single value by repeatedly combining the accumulator with each element.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list or tensor to reduce. |
| init | U | The initial accumulator value. |
| f | fn (U, T) -> U | Combines the accumulator with the next element. |
| returns | U | The final accumulator value. |

```wcl
fold([1, 2, 3], 0, fn(acc: i64, x: i64) -> i64 { acc + x })   // sum the list, starting from 0 → 6
```

### group_by(xs: \[T\], key: fn (T) -> K) → \[record\]

Group elements by a key function into `{ key, items }` records, in first-seen key order.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to group. |
| key | fn (T) -> K | Maps each element to its group key. |
| returns | \[record\] | One `{ key, items }` record per distinct key. |

```wcl
group_by([1, 2, 3, 4], fn(x: i64) -> i64 { x % 2 })   // group by even/odd → [{ key: 1, items: [1, 3] }, { key: 0, items: [2, 4] }]
```

### head(xs: \[T\]) → T

The first element of a list or tensor (`none` when empty).

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | A list or tensor. |
| returns | T | The first element, or `none` if empty. |

```wcl
head([1, 2, 3])   // take the first element → 1
```

### index_of(xs: \[T\], needle: T) → i64

The index of the first element equal to `needle`, or `-1` if absent.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to search. |
| needle | T | The value to look for. |
| returns | i64 | The zero-based index, or `-1` if not found. |

```wcl
index_of([10, 20, 30], 20)   // index of the matching element → 1
```

### len(xs: \[T\]) → usize

The number of elements in a list or tensor, or characters in a string.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | A list, tensor, or string. |
| returns | usize | The number of elements (or characters). |

```wcl
len([10, 20, 30])   // count the elements → 3
```

### list_contains(xs: \[T\], needle: T) → bool

Whether a list contains a value equal to `needle`.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to search. |
| needle | T | The value to look for. |
| returns | bool | `true` if an equal element is present. |

```wcl
list_contains([1, 2, 3], 2)   // is the value in the list? → true
```

### map(xs: \[T\], f: fn (T) -> U) → \[U\]

Apply a function to every element of a list or tensor, returning the transformed collection.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list or tensor to transform. |
| f | fn (T) -> U | Function applied to each element. |
| returns | \[U\] | A new collection of the transformed elements. |

```wcl
map([1, 2, 3], fn(x: i64) -> i64 { x * 2 })   // double each element → [2, 4, 6]
```

### max_by(xs: \[T\], key: fn (T) -> K) → T

The element with the largest key, or `none` for an empty list.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to search. |
| key | fn (T) -> K | Maps each element to its comparison key. |
| returns | T | The element with the largest key, or `none`. |

```wcl
max_by(["a", "abc", "ab"], fn(s: utf8) -> i64 { len(s) })   // element with the largest key → "abc"
```

### min_by(xs: \[T\], key: fn (T) -> K) → T

The element with the smallest key, or `none` for an empty list.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to search. |
| key | fn (T) -> K | Maps each element to its comparison key. |
| returns | T | The element with the smallest key, or `none`. |

```wcl
min_by(["abc", "a", "ab"], fn(s: utf8) -> i64 { len(s) })   // element with the smallest key → "a"
```

### range(start: i64, end: i64) → \[i64\]

The half-open integer range `[start, end)` as a list.

| Parameter | Type | Description |
| --- | --- | --- |
| start | i64 | Inclusive lower bound. |
| end | i64 | Exclusive upper bound; must be >= `start`. |
| returns | \[i64\] | The integers from `start` up to (but excluding) `end`. |

```wcl
range(0, 4)   // integers from 0 up to (not incl.) 4 → [0, 1, 2, 3]
```

### reverse(xs: \[T\]) → \[T\]

Reverse the order of a list's elements.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to reverse. |
| returns | \[T\] | The list in reverse order. |

```wcl
reverse([1, 2, 3])   // reverse the order → [3, 2, 1]
```

### slice(xs: utf8 | \[T\], start: i64, end: i64) → utf8 | \[T\]

The half-open range `[start, end)` of a string's characters or a list's elements (bounds are clamped).

| Parameter | Type | Description |
| --- | --- | --- |
| xs | utf8 \| \[T\] | The string or list to slice. |
| start | i64 | Inclusive start index (clamped to the length). |
| end | i64 | Exclusive end index (clamped to the length). |
| returns | utf8 \| \[T\] | The sub-string / sub-list. |

```wcl
slice("hello", 0, 2)   // characters from index 0 up to (not incl.) 2 → "he"
```

### sort(xs: \[T\]) → \[T\]

Sort a list — numerically for all-numeric lists, lexicographically for all-string lists.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | An all-numeric or all-string list. |
| returns | \[T\] | The sorted list. |

```wcl
sort([3, 1, 2])   // sort numerically → [1, 2, 3]
```

### sort_by(xs: \[T\], key: fn (T) -> K) → \[T\]

Sort a list by a key function (stable). Keys must be all numeric or all strings.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to sort. |
| key | fn (T) -> K | Maps each element to its sort key. |
| returns | \[T\] | The elements ordered by ascending key. |

```wcl
sort_by(["abc", "a", "ab"], fn(s: utf8) -> i64 { len(s) })   // sort by length → ["a", "ab", "abc"]
```

### sort_connected(items: \[T\], edges: \[{source, destination, ...}\]) → \[T\]

Reorder a list so that items joined by edges cluster together (recursing into `children`).

| Parameter | Type | Description |
| --- | --- | --- |
| items | \[T\] | Items identified by an `id` field (possibly nested via `children`). |
| edges | \[{source, destination, ...}\] | Edge records linking item ids. |
| returns | \[T\] | The reordered list, connected items adjacent. |

```wcl
sort_connected(nodes, edges)   // cluster edge-connected items together → the reordered nodes
```

### sum(xs: \[number\]) → number

Add together every element of a non-empty homogeneous numeric list or tensor.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[number\] | A non-empty list or tensor of one numeric type. |
| returns | number | The total, in the element's numeric type. |

```wcl
sum([1, 2, 3, 4])   // add up the elements → 10
```

### tail(xs: \[T\]) → \[T\]

Every element of a list or tensor except the first.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | A list or tensor. |
| returns | \[T\] | The elements after the first. |

```wcl
tail([1, 2, 3])   // everything after the first → [2, 3]
```

### take(xs: \[T\], n: i64) → \[T\]

The first `n` elements of a list (fewer if the list is shorter).

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to take from. |
| n | i64 | How many leading elements to keep. |
| returns | \[T\] | The first `n` elements. |

```wcl
take([1, 2, 3, 4], 2)   // keep the first 2 elements → [1, 2]
```

### unique(xs: \[T\]) → \[T\]

Remove duplicate elements from a list, keeping first-seen order.

| Parameter | Type | Description |
| --- | --- | --- |
| xs | \[T\] | The list to deduplicate. |
| returns | \[T\] | The list with duplicates removed. |

```wcl
unique([1, 2, 2, 3, 1])   // drop duplicates, keep first-seen order → [1, 2, 3]
```

### zip(a: \[A\], b: \[B\]) → \[(A, B)\]

Pair up elements of two lists by index, stopping at the shorter length.

| Parameter | Type | Description |
| --- | --- | --- |
| a | \[A\] | The first list. |
| b | \[B\] | The second list. |
| returns | \[(A, B)\] | Index-paired `[a, b]` lists, up to the shorter length. |

```wcl
zip([1, 2, 3], ["a", "b"])   // pair up by index, stopping at the shorter → [[1, "a"], [2, "b"]]
```

## String functions

### chars(s: utf8) → \[utf8\]

The characters of a string as a list of one-character strings.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to split into characters. |
| returns | \[utf8\] | One string per character. |

```wcl
chars("abc")   // explode into single-character strings → ["a", "b", "c"]
```

### concat(a: utf8, b: utf8) → utf8

Concatenate two strings into one.

| Parameter | Type | Description |
| --- | --- | --- |
| a | utf8 | The left-hand string. |
| b | utf8 | The string appended after `a`. |
| returns | utf8 | The two strings joined together. |

```wcl
concat("foo", "bar")   // join the two strings → "foobar"
```

### contains(s: utf8, needle: utf8) → bool

Whether a string contains a substring.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to search. |
| needle | utf8 | The substring to look for. |
| returns | bool | `true` if the substring is present. |

```wcl
contains("hello", "ell")   // does the string contain the substring? → true
```

### ends_with(s: utf8, suffix: utf8) → bool

Whether a string ends with a suffix.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to test. |
| suffix | utf8 | The suffix to look for. |
| returns | bool | `true` if the string ends with the suffix. |

```wcl
ends_with("hello", "lo")   // does it end with this suffix? → true
```

### format(template: utf8) → utf8

Substitute trailing arguments into a template's `{}` placeholders (`{{`/`}}` are literal braces).

| Parameter | Type | Description |
| --- | --- | --- |
| template | utf8 | Template string with `{}` placeholders. |
| returns | utf8 | The template with placeholders substituted. |

```wcl
format("{} = {}", "x", 42)   // substitute the args into the {} slots → "x = 42"
```

### format_unit(value: i64, type: utf8, unit: utf8) → utf8

Render a base-unit value in a chosen literal unit, looking the factor up from a unit type by name — the inverse of literal-unit resolution, so it stays correct if the type's `@unit` factor changes.

| Parameter | Type | Description |
| --- | --- | --- |
| value | i64 | The stored value, in the type's base unit. |
| type | utf8 | The unit type's dotted name, e.g. `"std.ByteSize"`. |
| unit | utf8 | The unit to render in, e.g. `"MiB"`. |
| returns | utf8 | The value divided by the unit's factor, suffixed with the unit (e.g. `"5 MiB"`). |

```wcl
format_unit(5242880, "std.ByteSize", "MiB")   // render bytes as MiB → "5 MiB"
```

### format_unit_value(value: i64, factor: i64, unit: utf8) → utf8

Render a number in a unit given its factor explicitly — the primitive behind `format_unit` for callers that already hold the factor.

| Parameter | Type | Description |
| --- | --- | --- |
| value | i64 | The stored value, in the type's base unit. |
| factor | i64 | The unit's multiplier (base units per one unit). |
| unit | utf8 | The unit label to append. |
| returns | utf8 | `value / factor` followed by the unit label. |

```wcl
format_unit_value(5242880, 1048576, "MiB")   // divide by the factor and label → "5 MiB"
```

### join(parts: \[utf8\], sep: utf8) → utf8

Join a list of strings into one, inserting a separator between each.

| Parameter | Type | Description |
| --- | --- | --- |
| parts | \[utf8\] | The strings to join. |
| sep | utf8 | The separator inserted between parts. |
| returns | utf8 | The joined string. |

```wcl
join(["a", "b", "c"], "-")   // join with a dash between pieces → "a-b-c"
```

### pad_end(s: utf8, width: i64, pad: utf8) → utf8

Right-pad a string with a fill pattern until it is `width` characters long.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to pad. |
| width | i64 | The target character count. |
| pad | utf8 | The fill pattern (repeated / truncated as needed). |
| returns | utf8 | The padded string (unchanged if already wide enough). |

```wcl
pad_end("42", 5, "0")   // right-pad to width 5 → "42000"
```

### pad_start(s: utf8, width: i64, pad: utf8) → utf8

Left-pad a string with a fill pattern until it is `width` characters long.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to pad. |
| width | i64 | The target character count. |
| pad | utf8 | The fill pattern (repeated / truncated as needed). |
| returns | utf8 | The padded string (unchanged if already wide enough). |

```wcl
pad_start("42", 5, "0")   // left-pad to width 5 → "00042"
```

### repeat(s: utf8, n: i64) → utf8

A string repeated `n` times (empty for `n <= 0`).

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to repeat. |
| n | i64 | How many copies to concatenate. |
| returns | utf8 | `n` copies of `s`. |

```wcl
repeat("ab", 3)   // concatenate 3 copies → "ababab"
```

### replace(s: utf8, old: utf8, new: utf8) → utf8

Replace every occurrence of a substring with another.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to search. |
| old | utf8 | The substring to find. |
| new | utf8 | The replacement substring. |
| returns | utf8 | The string with every match replaced. |

```wcl
replace("hello world", "world", "there")   // replace the matched substring → "hello there"
```

### split(s: utf8, sep: utf8) → \[utf8\]

Split a string on every occurrence of a separator into a list of pieces.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to split. |
| sep | utf8 | The separator to split on. |
| returns | \[utf8\] | The pieces between separators. |

```wcl
split("a,b,c", ",")   // split on each comma → ["a", "b", "c"]
```

### starts_with(s: utf8, prefix: utf8) → bool

Whether a string begins with a prefix.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to test. |
| prefix | utf8 | The prefix to look for. |
| returns | bool | `true` if the string starts with the prefix. |

```wcl
starts_with("hello", "he")   // does it begin with this prefix? → true
```

### to_lower(s: utf8) → utf8

Lowercase every character of a string.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to lowercase. |
| returns | utf8 | The lowercased string. |

```wcl
to_lower("AbC")   // lowercase every character → "abc"
```

### to_upper(s: utf8) → utf8

Uppercase every character of a string.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to uppercase. |
| returns | utf8 | The uppercased string. |

```wcl
to_upper("abc")   // uppercase every character → "ABC"
```

### trim(s: utf8) → utf8

Remove leading and trailing whitespace from a string.

| Parameter | Type | Description |
| --- | --- | --- |
| s | utf8 | The string to trim. |
| returns | utf8 | The string without leading/trailing whitespace. |

```wcl
trim("  hi  ")   // strip leading/trailing whitespace → "hi"
```

## Path & glob functions

### glob_match(pattern: utf8, path: utf8) → bool

Match one concrete path against a glob. `*` stays within a segment, `**` spans segments, `?` matches one character, `[a-z]` / `[!x]` are character classes. A trailing `/` on the pattern matches the whole subtree.

| Parameter | Type | Description |
| --- | --- | --- |
| pattern | utf8 | The glob pattern. |
| path | utf8 | The concrete path to test. |
| returns | bool | `true` if the path matches the pattern. |

```wcl
glob_match("src/*.rs", "src/main.rs")   // one segment, .rs extension → true
```

### glob_overlaps(a: utf8, b: utf8) → bool

Whether two glob patterns can match a common path. Concrete paths are patterns too, so this subsumes `glob_match` for overlap gates. Trailing `/` means the whole subtree. Conservative: exotic negated-class pairings may report `true` when no shared path exists, never `false` when one does.

| Parameter | Type | Description |
| --- | --- | --- |
| a | utf8 | The first glob pattern (or concrete path). |
| b | utf8 | The second glob pattern (or concrete path). |
| returns | bool | `true` if some path is matched by both patterns. |

```wcl
glob_overlaps("src/", "src/*.rs")   // the subtree owns every .rs directly under src → true
```

### path_contains(parent: utf8, child: utf8) → bool

Segment-aware path prefix test: whether `child` is `parent` itself or lives under it. Splits on `/`, so `src/` does not contain `src2/x`. A path contains itself.

| Parameter | Type | Description |
| --- | --- | --- |
| parent | utf8 | The containing path (trailing slash optional). |
| child | utf8 | The path to test. |
| returns | bool | `true` if `child` equals `parent` or is nested beneath it. |

```wcl
path_contains("src/", "src/core/mod.rs")   // nested under src/, segment-aware → true
```

## Math functions

### abs(x: f64) → f64

Absolute value.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
abs(-7.5)   // magnitude, dropping the sign → 7.5
```

### acos(x: number) → f64

Arccosine, in radians, of a value in \[-1, 1\].

| Parameter | Type | Description |
| --- | --- | --- |
| x | number | The input value (any number, widened to f64). |
| returns | f64 | The result, as an f64. |

```wcl
acos(1.0)   // arccosine of 1 → 0.0
```

### asin(x: number) → f64

Arcsine, in radians, of a value in \[-1, 1\].

| Parameter | Type | Description |
| --- | --- | --- |
| x | number | The input value (any number, widened to f64). |
| returns | f64 | The result, as an f64. |

```wcl
asin(0.0)   // arcsine of 0 → 0.0
```

### atan(x: number) → f64

Arctangent, in radians.

| Parameter | Type | Description |
| --- | --- | --- |
| x | number | The input value (any number, widened to f64). |
| returns | f64 | The result, as an f64. |

```wcl
atan(0.0)   // arctangent of 0 → 0.0
```

### atan2(a: number, b: number) → f64

Arctangent of `a/b` in radians, using the signs of both to pick the quadrant.

| Parameter | Type | Description |
| --- | --- | --- |
| a | number | The first operand. |
| b | number | The second operand. |
| returns | f64 | The result, as an f64. |

```wcl
atan2(1.0, 1.0)   // angle of the vector (1, 1) → 0.7853981633974483
```

### cbrt(x: f64) → f64

Cube root.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
cbrt(27)   // cube root → 3.0
```

### ceil(x: f64) → f64

Round up to the nearest integer.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
ceil(3.1)   // round up to a whole number → 4.0
```

### clamp(x: number, lo: number, hi: number) → f64

Constrain `x` to the range `[lo, hi]`.

| Parameter | Type | Description |
| --- | --- | --- |
| x | number | The value to clamp. |
| lo | number | The lower bound. |
| hi | number | The upper bound. |
| returns | f64 | `x` limited to `[lo, hi]`, as an f64. |

```wcl
clamp(12.0, 0.0, 10.0)   // constrain the value into [0, 10] → 10.0
```

### cos(x: f64) → f64

Cosine of an angle in radians.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
cos(0.0)   // cosine of 0 radians → 1.0
```

### degrees(x: number) → f64

Convert an angle from radians to degrees.

| Parameter | Type | Description |
| --- | --- | --- |
| x | number | The input value (any number, widened to f64). |
| returns | f64 | The result, as an f64. |

```wcl
degrees(pi())   // convert π radians to degrees → 180.0
```

### e() → f64

Euler's number e (≈ 2.71828).

| Parameter | Type | Description |
| --- | --- | --- |
| returns | f64 | The value of e. |

```wcl
e()   // Euler's number → 2.718281828459045
```

### exp(x: f64) → f64

`e` raised to the power `x`.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
exp(0.0)   // e raised to the power 0 → 1.0
```

### floor(x: f64) → f64

Round down to the nearest integer.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
floor(3.9)   // round down to a whole number → 3.0
```

### hypot(a: f64, b: f64) → f64

Length of the hypotenuse `sqrt(a² + b²)`.

| Parameter | Type | Description |
| --- | --- | --- |
| a | f64 | The first operand. |
| b | f64 | The second operand. |
| returns | f64 | The result. |

```wcl
hypot(3, 4)   // hypotenuse of a 3-4 right triangle → 5.0
```

### ln(x: f64) → f64

Natural (base-`e`) logarithm.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
ln(1.0)   // natural (base-e) logarithm of 1 → 0.0
```

### log10(x: f64) → f64

Base-10 logarithm.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
log10(1000.0)   // base-10 logarithm of 1000 → 3.0
```

### log2(x: f64) → f64

Base-2 logarithm.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
log2(8.0)   // base-2 logarithm of 8 → 3.0
```

### max(a: f64, b: f64) → f64

The larger of two numbers.

| Parameter | Type | Description |
| --- | --- | --- |
| a | f64 | The first operand. |
| b | f64 | The second operand. |
| returns | f64 | The result. |

```wcl
max(3, 7.5)   // the larger of the two → 7.5
```

### min(a: f64, b: f64) → f64

The smaller of two numbers.

| Parameter | Type | Description |
| --- | --- | --- |
| a | f64 | The first operand. |
| b | f64 | The second operand. |
| returns | f64 | The result. |

```wcl
min(3, 7.5)   // the smaller of the two → 3.0
```

### pi() → f64

The constant π (≈ 3.14159).

| Parameter | Type | Description |
| --- | --- | --- |
| returns | f64 | The value of π. |

```wcl
pi()   // the constant π → 3.141592653589793
```

### pow(a: f64, b: f64) → f64

Raise `a` to the power `b`.

| Parameter | Type | Description |
| --- | --- | --- |
| a | f64 | The first operand. |
| b | f64 | The second operand. |
| returns | f64 | The result. |

```wcl
pow(2, 10)   // 2 raised to the 10th power → 1024.0
```

### radians(x: number) → f64

Convert an angle from degrees to radians.

| Parameter | Type | Description |
| --- | --- | --- |
| x | number | The input value (any number, widened to f64). |
| returns | f64 | The result, as an f64. |

```wcl
radians(180.0)   // convert 180 degrees to radians → 3.141592653589793
```

### round(x: f64) → f64

Round to the nearest integer (ties away from zero).

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
round(2.5)   // nearest integer (ties away from zero) → 3.0
```

### sign(x: f64) → f64

The sign of `x`: `1`, `-1`, or `0`.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
sign(-3.0)   // negative input -> -1.0 → -1.0
```

### sin(x: f64) → f64

Sine of an angle in radians.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
sin(0.0)   // sine of 0 radians → 0.0
```

### sqrt(x: f64) → f64

Square root.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
sqrt(144)   // square root → 12.0
```

### tan(x: f64) → f64

Tangent of an angle in radians.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
tan(0.0)   // tangent of 0 radians → 0.0
```

### tau() → f64

The constant τ = 2π (≈ 6.28319).

| Parameter | Type | Description |
| --- | --- | --- |
| returns | f64 | The value of τ (2π). |

```wcl
tau()   // the constant τ = 2π → 6.283185307179586
```

### trunc(x: f64) → f64

Discard the fractional part, rounding toward zero.

| Parameter | Type | Description |
| --- | --- | --- |
| x | f64 | The input value. |
| returns | f64 | The result. |

```wcl
trunc(3.9)   // drop the fractional part → 3.0
```

## Record functions

### keys(r: record) → \[utf8\]

The field names of a record, in deterministic (sorted) order.

| Parameter | Type | Description |
| --- | --- | --- |
| r | record | A record value (or a union variant with a record body). |
| returns | \[utf8\] | The field names. |

```wcl
keys({ name: "Rex", age: 4 })   // field names, sorted → ["age", "name"]
```

### map_values(r: record, f: fn (T) -> U) → record

Apply a function to every field value of a record, keeping the keys.

| Parameter | Type | Description |
| --- | --- | --- |
| r | record | The record to transform. |
| f | fn (T) -> U | Function applied to each field value. |
| returns | record | A record with the same keys and transformed values. |

```wcl
map_values({ low: 1, high: 9 }, fn(x: i64) -> i64 { x * 2 })   // double every value, keep the keys → { high: 18, low: 2 }
```

### merge(a: record, b: record) → record

Combine two records into one; fields of `b` win on a name clash.

| Parameter | Type | Description |
| --- | --- | --- |
| a | record | The base record. |
| b | record | The overriding record. |
| returns | record | A record with the union of both field sets. |

```wcl
merge({ host: "localhost", port: 80 }, { port: 8080 })   // the second record wins on the port clash → { host: "localhost", port: 8080 }
```

### values(r: record) → \[T\]

The field values of a record, in the same order as `keys`.

| Parameter | Type | Description |
| --- | --- | --- |
| r | record | A record value (or a union variant with a record body). |
| returns | \[T\] | The field values. |

```wcl
values({ name: "Rex", age: 4 })   // field values, in key order → [4, "Rex"]
```

## Reflection functions

### ast_string(target: &T) → utf8

Pretty-print the canonical source behind a reference (type/interface/union/symbol_set/block/field) or a function value.

| Parameter | Type | Description |
| --- | --- | --- |
| target | &T | A dataref to a declaration, or a function value. |
| returns | utf8 | The canonical (pretty-printed) source text. |

```wcl
ast_string(Image)   // pretty-print a type's source → "type Image { ... }"
```

### builtin_names() → \[utf8\]

The names of every registered built-in function, sorted. Pair with `fn_signature` to introspect each one.

| Parameter | Type | Description |
| --- | --- | --- |
| returns | \[utf8\] | Every built-in's name, sorted alphabetically. |

```wcl
builtin_names()   // names of every builtin → ["abs", "acos", ..., "zip"]
```

### child_types(target: &T) → \[&T\]

Reflect a type into references to the element types of its `@child` / `@children` block slots (own slots first, then inherited via `extends`). Pair with `type_table` / `type_fields` to auto-document the blocks a `@document` declares.

| Parameter | Type | Description |
| --- | --- | --- |
| target | &T | A reference to a type or interface declaration. |
| returns | \[&T\] | One type reference per block slot. Slots that accept a union or interface resolve to that type's name; scalar (non-block) fields are skipped. |

```wcl
child_types(MyDoc)   // element types of the doc’s block slots → [&ProjectMeta, &Settings]
```

### decl_info(target: &T) → record

Describe a top-level declaration: its name, kind, doc comment, and schema classification (block / table / decorator / document).

| Parameter | Type | Description |
| --- | --- | --- |
| target | &T | A reference to a type, interface, union, or symbol_set declaration. |
| returns | record | `{ name, full_name, kind, doc, is_imported, is_document, block_kind, table_kind, decorator_name, extends }`. The classification fields are `none` when the decorator is absent. |

```wcl
decl_info(MyDoc)   // declaration metadata for a type → { kind: "document", is_document: true, name: "MyDoc" }
```

### decorator_arg(target: &T, decorator: utf8, slot: utf8) → any

Read one named argument of a decorator on a referenced declaration (`none` if absent).

| Parameter | Type | Description |
| --- | --- | --- |
| target | &T | A reference to a type, field, block, or variant. |
| decorator | utf8 | The decorator name, e.g. `"doc"`. |
| slot | utf8 | The argument (slot) name to read. |
| returns | any | The argument's value, or `none` if absent. |

```wcl
decorator_arg(Image, "block", "name")   // read a decorator argument by name → "image"
```

### decorator_names(target: &T) → \[utf8\]

List the names of the decorators attached to a referenced declaration.

| Parameter | Type | Description |
| --- | --- | --- |
| target | &T | A reference to a type, field, block, or variant. |
| returns | \[utf8\] | The decorator names, in source order. |

```wcl
decorator_names(Image)   // the decorators on the Image type → ["block", "schemaless", ...]
```

### doc_comment(target: &T) → utf8

The doc comment — the contiguous run of `#` / `//` lines immediately above a declaration — attached to a reference, or `\"\"` when there is none. Complements `decorator_arg(x, \"doc\", …)` for `@doc(\"…\")` metadata.

| Parameter | Type | Description |
| --- | --- | --- |
| target | &T | A reference to a type, interface, union, variant, symbol_set, or field. |
| returns | utf8 | The joined comment text, or `\"\"` when absent. |

```wcl
doc_comment(Image)   // the doc comment above a type → the comment lines above the Image type
```

### eval(src: utf8) → any

Parse a string as a WCL expression and evaluate it in the current scope.

| Parameter | Type | Description |
| --- | --- | --- |
| src | utf8 | WCL expression source to parse and evaluate. |
| returns | any | The value the expression evaluates to. |

```wcl
eval("1 + 2 * 3")   // parse and evaluate a WCL expression → 7
```

### fn_signature(f: any) → record

Describe a function's parameters and return type. Pass a function value, or a built-in's name as a string.

| Parameter | Type | Description |
| --- | --- | --- |
| f | any | A function value, or the name of a built-in as a utf8 string. |
| returns | record | A record `{ doc, params: [{name, type, doc}], return_type, return_doc, signature, is_builtin }`. |

```wcl
fn_signature("map")   // describe the map builtin → { signature: "fn(xs: [T], ...) -> [U]", ... }
```

### namespace_decls(ns: utf8) → \[&T\]

List references to every top-level declaration (`type` / `interface` / `union` / `symbol_set`) in a namespace, for schema-documentation generators. Pair with `decl_info`, `doc_comment`, `type_fields`, and `ast_string` to render each. Imported (library) declarations are included — filter on `decl_info(d).is_imported` to drop them.

| Parameter | Type | Description |
| --- | --- | --- |
| ns | utf8 | The namespace, dotted (e.g. `"wdoc"`); `""` for the root namespace. |
| returns | \[&T\] | One reference per declaration: types first, then interfaces, unions, symbol sets, in source order. |

```wcl
namespace_decls("wdoc")   // every top-level decl in the wdoc namespace → [&Page, &Site, ...]
```

### type_fields(target: &T) → \[record\]

Reflect a type or interface into a list of field-description records (own fields first, then inherited via `extends`).

| Parameter | Type | Description |
| --- | --- | --- |
| target | &T | A reference to a type or interface declaration. |
| returns | \[record\] | One record per field: `{ name, type, is_function, optional, has_default, is_block, repeated, accepts, decorators }`. |

```wcl
type_fields(Image)   // reflect the Image type into field records → [{ name: "source", type: "utf8", ... }, ...]
```

## Tensor functions

### tensor(data: \[number\], shape: \[usize\]) → tensor<T>

Build a tensor from flat row-major data and a shape; the data length must equal the product of the dimensions.

| Parameter | Type | Description |
| --- | --- | --- |
| data | \[number\] | Flat, row-major element data. |
| shape | \[usize\] | The dimension sizes. |
| returns | tensor<T> | The constructed tensor. |

```wcl
tensor([1, 2, 3, 4], [2, 2])   // build a 2x2 tensor from flat data → a 2x2 tensor
```

### tensor_data(t: tensor<T>) → \[T\]

The flat row-major element data of a tensor as a list.

| Parameter | Type | Description |
| --- | --- | --- |
| t | tensor<T> | The tensor to read. |
| returns | \[T\] | The tensor's flat, row-major element data. |

```wcl
tensor_data(tensor([1, 2, 3, 4], [2, 2]))   // the flat row-major data → [1, 2, 3, 4]
```

### tensor_reshape(t: tensor<T>, shape: \[usize\]) → tensor<T>

Reinterpret a tensor's data under a new shape; the element count must be unchanged.

| Parameter | Type | Description |
| --- | --- | --- |
| t | tensor<T> | The tensor to reshape. |
| shape | \[usize\] | The new dimension sizes. |
| returns | tensor<T> | The same data under the new shape. |

```wcl
tensor_reshape(tensor([1, 2, 3, 4], [2, 2]), [4])   // reshape 2x2 into 1-D of length 4 → a length-4 tensor
```

### tensor_shape(t: tensor<T>) → \[usize\]

The dimension sizes of a tensor as a list.

| Parameter | Type | Description |
| --- | --- | --- |
| t | tensor<T> | The tensor to read. |
| returns | \[usize\] | The tensor's dimension sizes. |

```wcl
tensor_shape(tensor([1, 2, 3, 4], [2, 2]))   // the dimension sizes → [2, 2]
```

## Control & error functions

### assert(cond: bool, msg: utf8) → none

Return `none` when `cond` is true, otherwise abort with `msg`.

| Parameter | Type | Description |
| --- | --- | --- |
| cond | bool | The condition that must hold. |
| msg | utf8 | The error message reported when `cond` is false. |
| returns | none | `none` when the assertion holds (otherwise aborts). |

```wcl
assert(1 + 1 == 2, "math is broken")   // verify a condition holds → none
```

### error(msg: utf8) → never

Abort evaluation with an error message.

| Parameter | Type | Description |
| --- | --- | --- |
| msg | utf8 | The error message to report. |
| returns | never | Never returns — aborts evaluation. |

```wcl
error("unreachable state")   // abort evaluation with a message → (aborts)
```

### panic(msg: utf8) → never

Abort evaluation with an unrecoverable failure message.

| Parameter | Type | Description |
| --- | --- | --- |
| msg | utf8 | The failure message to report. |
| returns | never | Never returns — aborts evaluation. |

```wcl
panic("invariant violated")   // abort with an unrecoverable failure → (aborts)
```

[← Back to SKILL.md](../SKILL.md)
