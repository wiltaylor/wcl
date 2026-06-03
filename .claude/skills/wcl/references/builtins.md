# Builtin Functions

Every built-in function in WCL — its signature, parameters, return value, and a usage example. Reflection and meta built-ins are introspected with `fn_signature`.

## Index

- [abs](#abs)
- [acos](#acos)
- [asin](#asin)
- [assert](#assert)
- [ast_string](#ast_string)
- [at](#at)
- [atan](#atan)
- [atan2](#atan2)
- [builtin_names](#builtin_names)
- [cbrt](#cbrt)
- [ceil](#ceil)
- [child_types](#child_types)
- [clamp](#clamp)
- [concat](#concat)
- [contains](#contains)
- [cos](#cos)
- [decorator_arg](#decorator_arg)
- [decorator_names](#decorator_names)
- [degrees](#degrees)
- [drop](#drop)
- [e](#e)
- [ends_with](#ends_with)
- [error](#error)
- [eval](#eval)
- [exp](#exp)
- [filter](#filter)
- [flatten](#flatten)
- [floor](#floor)
- [fn_signature](#fn_signature)
- [fold](#fold)
- [format](#format)
- [head](#head)
- [hypot](#hypot)
- [index_of](#index_of)
- [join](#join)
- [len](#len)
- [list_contains](#list_contains)
- [ln](#ln)
- [log10](#log10)
- [log2](#log2)
- [map](#map)
- [max](#max)
- [min](#min)
- [panic](#panic)
- [pi](#pi)
- [pow](#pow)
- [radians](#radians)
- [range](#range)
- [replace](#replace)
- [reverse](#reverse)
- [round](#round)
- [sign](#sign)
- [sin](#sin)
- [sort](#sort)
- [sort_connected](#sort_connected)
- [split](#split)
- [sqrt](#sqrt)
- [starts_with](#starts_with)
- [sum](#sum)
- [tail](#tail)
- [take](#take)
- [tan](#tan)
- [tau](#tau)
- [tensor](#tensor)
- [tensor_data](#tensor_data)
- [tensor_reshape](#tensor_reshape)
- [tensor_shape](#tensor_shape)
- [to_lower](#to_lower)
- [to_upper](#to_upper)
- [trim](#trim)
- [trunc](#trunc)
- [type_fields](#type_fields)
- [unique](#unique)
- [zip](#zip)

## Functions

### abs

```wcl
abs(x: number) -> f64
```

Absolute value.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
abs(-5)   // 5.0
```

### acos

```wcl
acos(x: number) -> f64
```

Arccosine, in radians, of a value in \[-1, 1\].

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
acos(1)   // 0.0
```

### asin

```wcl
asin(x: number) -> f64
```

Arcsine, in radians, of a value in \[-1, 1\].

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
asin(1)   // 1.5707963…  (π/2)
```

### assert

```wcl
assert(cond: bool, msg: utf8) -> none
```

Return `none` when `cond` is true, otherwise abort with `msg`.

| Parameter | Type | Description |
| --- | --- | --- |
| `cond` | `bool` | The condition that must hold. |
| `msg` | `utf8` | The error message reported when `cond` is false. |
| **Returns** | `none` | `none` when the assertion holds (otherwise aborts). |

**Example**

```wcl
assert(len(xs) > 0, "list must not be empty")
```

### ast_string

```wcl
ast_string(target: &T) -> utf8
```

Pretty-print the canonical source behind a reference (type/interface/union/symbol_set/block/field) or a function value.

| Parameter | Type | Description |
| --- | --- | --- |
| `target` | `&T` | A dataref to a declaration, or a function value. |
| **Returns** | `utf8` | The canonical (pretty-printed) source text. |

**Example**

```wcl
ast_string(Image)   // the Image type's source, pretty-printed
```

### at

```wcl
at(xs: [T], i: i64) -> T
```

The element at a zero-based index; errors if out of bounds or negative.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list to index. |
| `i` | `i64` | The zero-based index. |
| **Returns** | `T` | The element at `i`. |

**Example**

```wcl
at([10, 20, 30], 1)   // 20
```

### atan

```wcl
atan(x: number) -> f64
```

Arctangent, in radians.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
atan(1)   // 0.7853981…  (π/4)
```

### atan2

```wcl
atan2(a: number, b: number) -> f64
```

Arctangent of `a/b` in radians, using the signs of both to pick the quadrant.

| Parameter | Type | Description |
| --- | --- | --- |
| `a` | `number` | The first operand. |
| `b` | `number` | The second operand. |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
atan2(1, 1)   // 0.7853981…  (π/4)
```

### builtin_names

```wcl
builtin_names() -> [utf8]
```

The names of every registered built-in function, sorted. Pair with `fn_signature` to introspect each one.

| Parameter | Type | Description |
| --- | --- | --- |
| **Returns** | `[utf8]` | Every built-in's name, sorted alphabetically. |

**Example**

```wcl
builtin_names()   // ["abs", "acos", …, "zip"]
```

### cbrt

```wcl
cbrt(x: number) -> f64
```

Cube root.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
cbrt(27)   // 3.0
```

### ceil

```wcl
ceil(x: number) -> f64
```

Round up to the nearest integer.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
ceil(2.1)   // 3.0
```

### child_types

```wcl
child_types(target: &T) -> [&T]
```

Reflect a type into references to the element types of its `@child` / `@children` block slots (own slots first, then inherited via `extends`). Pair with `type_table` / `type_fields` to auto-document the blocks a `@document` declares.

| Parameter | Type | Description |
| --- | --- | --- |
| `target` | `&T` | A reference to a type or interface declaration. |
| **Returns** | `[&T]` | One type reference per block slot. Slots that accept a union or interface resolve to that type's name; scalar (non-block) fields are skipped. |

**Example**

```wcl
child_types(MyDoc)   // [&ProjectMeta, &Settings] — the element types of MyDoc's block slots
```

### clamp

```wcl
clamp(x: number, lo: number, hi: number) -> f64
```

Constrain `x` to the range `[lo, hi]`.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The value to clamp. |
| `lo` | `number` | The lower bound. |
| `hi` | `number` | The upper bound. |
| **Returns** | `f64` | `x` limited to `[lo, hi]`, as an f64. |

**Example**

```wcl
clamp(12, 0, 10)   // 10.0
```

### concat

```wcl
concat(a: utf8, b: utf8) -> utf8
```

Concatenate two strings into one.

| Parameter | Type | Description |
| --- | --- | --- |
| `a` | `utf8` | The left-hand string. |
| `b` | `utf8` | The string appended after `a`. |
| **Returns** | `utf8` | The two strings joined together. |

**Example**

```wcl
concat("foo", "bar")   // "foobar"
```

### contains

```wcl
contains(s: utf8, needle: utf8) -> bool
```

Whether a string contains a substring.

| Parameter | Type | Description |
| --- | --- | --- |
| `s` | `utf8` | The string to search. |
| `needle` | `utf8` | The substring to look for. |
| **Returns** | `bool` | `true` if the substring is present. |

**Example**

```wcl
contains("hello", "ell")   // true
```

### cos

```wcl
cos(x: number) -> f64
```

Cosine of an angle in radians.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
cos(0)   // 1.0
```

### decorator_arg

```wcl
decorator_arg(target: &T, decorator: utf8, slot: utf8) -> any
```

Read one named argument of a decorator on a referenced declaration (`none` if absent).

| Parameter | Type | Description |
| --- | --- | --- |
| `target` | `&T` | A reference to a type, field, block, or variant. |
| `decorator` | `utf8` | The decorator name, e.g. `"doc"`. |
| `slot` | `utf8` | The argument (slot) name to read. |
| **Returns** | `any` | The argument's value, or `none` if absent. |

**Example**

```wcl
decorator_arg(Image, "block", "name")   // "image"
```

### decorator_names

```wcl
decorator_names(target: &T) -> [utf8]
```

List the names of the decorators attached to a referenced declaration.

| Parameter | Type | Description |
| --- | --- | --- |
| `target` | `&T` | A reference to a type, field, block, or variant. |
| **Returns** | `[utf8]` | The decorator names, in source order. |

**Example**

```wcl
decorator_names(Image)   // ["block", "schemaless", …]
```

### degrees

```wcl
degrees(x: number) -> f64
```

Convert an angle from radians to degrees.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
degrees(pi())   // 180.0
```

### drop

```wcl
drop(xs: [T], n: i64) -> [T]
```

Every element of a list after the first `n`.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list to drop from. |
| `n` | `i64` | How many leading elements to skip. |
| **Returns** | `[T]` | The elements after the first `n`. |

**Example**

```wcl
drop([1, 2, 3, 4], 2)   // [3, 4]
```

### e

```wcl
e() -> f64
```

Euler's number e (≈ 2.71828).

| Parameter | Type | Description |
| --- | --- | --- |
| **Returns** | `f64` | The value of e. |

**Example**

```wcl
e()   // 2.71828…
```

### ends_with

```wcl
ends_with(s: utf8, suffix: utf8) -> bool
```

Whether a string ends with a suffix.

| Parameter | Type | Description |
| --- | --- | --- |
| `s` | `utf8` | The string to test. |
| `suffix` | `utf8` | The suffix to look for. |
| **Returns** | `bool` | `true` if the string ends with the suffix. |

**Example**

```wcl
ends_with("hello", "lo")   // true
```

### error

```wcl
error(msg: utf8) -> never
```

Abort evaluation with an error message.

| Parameter | Type | Description |
| --- | --- | --- |
| `msg` | `utf8` | The error message to report. |
| **Returns** | `never` | Never returns — aborts evaluation. |

**Example**

```wcl
if found { value } else { error("config not found") }
```

### eval

```wcl
eval(src: utf8) -> any
```

Parse a string as a WCL expression and evaluate it in the current scope.

| Parameter | Type | Description |
| --- | --- | --- |
| `src` | `utf8` | WCL expression source to parse and evaluate. |
| **Returns** | `any` | The value the expression evaluates to. |

**Example**

```wcl
eval("1 + 2 * 3")   // 7
```

### exp

```wcl
exp(x: number) -> f64
```

`e` raised to the power `x`.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
exp(1)   // 2.71828…
```

### filter

```wcl
filter(xs: [T], pred: fn (T) -> bool) -> [T]
```

Keep only the list elements for which the predicate returns `true`.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list to filter. |
| `pred` | `fn (T) -> bool` | Predicate deciding whether to keep an element. |
| **Returns** | `[T]` | The elements for which the predicate returned `true`. |

**Example**

```wcl
filter(range(0, 6), fn(x: i64) -> bool x % 2 == 0)   // [0, 2, 4]
```

### flatten

```wcl
flatten(xss: [[T]]) -> [T]
```

Concatenate a list of lists into a single list, one level deep.

| Parameter | Type | Description |
| --- | --- | --- |
| `xss` | `[[T]]` | A list whose elements are themselves lists. |
| **Returns** | `[T]` | The inner lists concatenated, one level deep. |

**Example**

```wcl
flatten([[1, 2], [3], [4, 5]])   // [1, 2, 3, 4, 5]
```

### floor

```wcl
floor(x: number) -> f64
```

Round down to the nearest integer.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
floor(2.7)   // 2.0
```

### fn_signature

```wcl
fn_signature(f: any) -> record
```

Describe a function's parameters and return type. Pass a function value, or a built-in's name as a string.

| Parameter | Type | Description |
| --- | --- | --- |
| `f` | `any` | A function value, or the name of a built-in as a utf8 string. |
| **Returns** | `record` | A record `{ doc, params: [{name, type, doc}], return_type, return_doc, signature, is_builtin }`. |

**Example**

```wcl
fn_signature("map")   // { signature: "fn(xs: [T], …) -> [U]", params: […], … }
```

### fold

```wcl
fold(xs: [T], init: U, f: fn (U, T) -> U) -> U
```

Reduce a list or tensor to a single value by repeatedly combining the accumulator with each element.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list or tensor to reduce. |
| `init` | `U` | The initial accumulator value. |
| `f` | `fn (U, T) -> U` | Combines the accumulator with the next element. |
| **Returns** | `U` | The final accumulator value. |

**Example**

```wcl
fold([1, 2, 3, 4], 0, fn(acc: i64, x: i64) -> i64 acc + x)   // 10
```

### format

```wcl
format(utf8, ...args) -> utf8
```

Substitute trailing arguments into a template's `{}` placeholders (`{{`/`}}` are literal braces).

| Parameter | Type | Description |
| --- | --- | --- |
| `template` | `utf8` | Template string with `{}` placeholders. |
| **Returns** | `utf8` | The template with placeholders substituted. |

**Example**

```wcl
format("{} = {}", "x", 42)   // "x = 42"
```

### head

```wcl
head(xs: [T]) -> T
```

The first element of a list or tensor (`none` when empty).

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | A list or tensor. |
| **Returns** | `T` | The first element, or `none` if empty. |

**Example**

```wcl
head([10, 20, 30])   // 10
```

### hypot

```wcl
hypot(a: number, b: number) -> f64
```

Length of the hypotenuse `sqrt(a² + b²)`.

| Parameter | Type | Description |
| --- | --- | --- |
| `a` | `number` | The first operand. |
| `b` | `number` | The second operand. |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
hypot(3, 4)   // 5.0
```

### index_of

```wcl
index_of(xs: [T], needle: T) -> i64
```

The index of the first element equal to `needle`, or `-1` if absent.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list to search. |
| `needle` | `T` | The value to look for. |
| **Returns** | `i64` | The zero-based index, or `-1` if not found. |

**Example**

```wcl
index_of(["a", "b", "c"], "b")   // 1
index_of(["a", "b"], "z")        // -1
```

### join

```wcl
join(parts: [utf8], sep: utf8) -> utf8
```

Join a list of strings into one, inserting a separator between each.

| Parameter | Type | Description |
| --- | --- | --- |
| `parts` | `[utf8]` | The strings to join. |
| `sep` | `utf8` | The separator inserted between parts. |
| **Returns** | `utf8` | The joined string. |

**Example**

```wcl
join(["a", "b", "c"], "-")   // "a-b-c"
```

### len

```wcl
len(xs: [T]) -> usize
```

The number of elements in a list or tensor, or characters in a string.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | A list, tensor, or string. |
| **Returns** | `usize` | The number of elements (or characters). |

**Example**

```wcl
len([10, 20, 30])   // 3
len("hello")        // 5
```

### list_contains

```wcl
list_contains(xs: [T], needle: T) -> bool
```

Whether a list contains a value equal to `needle`.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list to search. |
| `needle` | `T` | The value to look for. |
| **Returns** | `bool` | `true` if an equal element is present. |

**Example**

```wcl
list_contains([1, 2, 3], 2)   // true
```

### ln

```wcl
ln(x: number) -> f64
```

Natural (base-`e`) logarithm.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
ln(e())   // 1.0
```

### log10

```wcl
log10(x: number) -> f64
```

Base-10 logarithm.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
log10(1000)   // 3.0
```

### log2

```wcl
log2(x: number) -> f64
```

Base-2 logarithm.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
log2(8)   // 3.0
```

### map

```wcl
map(xs: [T], f: fn (T) -> U) -> [U]
```

Apply a function to every element of a list or tensor, returning the transformed collection.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list or tensor to transform. |
| `f` | `fn (T) -> U` | Function applied to each element. |
| **Returns** | `[U]` | A new collection of the transformed elements. |

**Example**

```wcl
map([1, 2, 3], fn(x: i64) -> i64 x * 2)   // [2, 4, 6]
```

### max

```wcl
max(a: number, b: number) -> f64
```

The larger of two numbers.

| Parameter | Type | Description |
| --- | --- | --- |
| `a` | `number` | The first operand. |
| `b` | `number` | The second operand. |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
max(3, 7)   // 7.0
```

### min

```wcl
min(a: number, b: number) -> f64
```

The smaller of two numbers.

| Parameter | Type | Description |
| --- | --- | --- |
| `a` | `number` | The first operand. |
| `b` | `number` | The second operand. |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
min(3, 7)   // 3.0
```

### panic

```wcl
panic(msg: utf8) -> never
```

Abort evaluation with an unrecoverable failure message.

| Parameter | Type | Description |
| --- | --- | --- |
| `msg` | `utf8` | The failure message to report. |
| **Returns** | `never` | Never returns — aborts evaluation. |

**Example**

```wcl
panic("unreachable state reached")
```

### pi

```wcl
pi() -> f64
```

The constant π (≈ 3.14159).

| Parameter | Type | Description |
| --- | --- | --- |
| **Returns** | `f64` | The value of π. |

**Example**

```wcl
pi()   // 3.14159…
```

### pow

```wcl
pow(a: number, b: number) -> f64
```

Raise `a` to the power `b`.

| Parameter | Type | Description |
| --- | --- | --- |
| `a` | `number` | The first operand. |
| `b` | `number` | The second operand. |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
pow(2, 10)   // 1024.0
```

### radians

```wcl
radians(x: number) -> f64
```

Convert an angle from degrees to radians.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
radians(180)   // 3.14159…
```

### range

```wcl
range(start: i64, end: i64) -> [i64]
```

The half-open integer range `[start, end)` as a list.

| Parameter | Type | Description |
| --- | --- | --- |
| `start` | `i64` | Inclusive lower bound. |
| `end` | `i64` | Exclusive upper bound; must be >= `start`. |
| **Returns** | `[i64]` | The integers from `start` up to (but excluding) `end`. |

**Example**

```wcl
range(1, 4)   // [1, 2, 3]
```

### replace

```wcl
replace(s: utf8, old: utf8, new: utf8) -> utf8
```

Replace every occurrence of a substring with another.

| Parameter | Type | Description |
| --- | --- | --- |
| `s` | `utf8` | The string to search. |
| `old` | `utf8` | The substring to find. |
| `new` | `utf8` | The replacement substring. |
| **Returns** | `utf8` | The string with every match replaced. |

**Example**

```wcl
replace("hello world", "world", "there")   // "hello there"
```

### reverse

```wcl
reverse(xs: [T]) -> [T]
```

Reverse the order of a list's elements.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list to reverse. |
| **Returns** | `[T]` | The list in reverse order. |

**Example**

```wcl
reverse([1, 2, 3])   // [3, 2, 1]
```

### round

```wcl
round(x: number) -> f64
```

Round to the nearest integer (ties away from zero).

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
round(2.5)   // 3.0
```

### sign

```wcl
sign(x: number) -> f64
```

The sign of `x`: `1`, `-1`, or `0`.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
sign(-3)   // -1.0
```

### sin

```wcl
sin(x: number) -> f64
```

Sine of an angle in radians.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
sin(pi() / 2)   // 1.0
```

### sort

```wcl
sort(xs: [T]) -> [T]
```

Sort a list — numerically for all-numeric lists, lexicographically for all-string lists.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | An all-numeric or all-string list. |
| **Returns** | `[T]` | The sorted list. |

**Example**

```wcl
sort([3, 1, 2])           // [1, 2, 3]
sort(["b", "a", "c"])     // ["a", "b", "c"]
```

### sort_connected

```wcl
sort_connected(items: [T], edges: [{source, destination, ...}]) -> [T]
```

Reorder a list so that items joined by edges cluster together (recursing into `children`).

| Parameter | Type | Description |
| --- | --- | --- |
| `items` | `[T]` | Items identified by an `id` field (possibly nested via `children`). |
| `edges` | `[{source, destination, ...}]` | Edge records linking item ids. |
| **Returns** | `[T]` | The reordered list, connected items adjacent. |

**Example**

```wcl
// Reorder items so nodes joined by edges sit next to each other.
sort_connected(nodes, edges)
```

### split

```wcl
split(s: utf8, sep: utf8) -> [utf8]
```

Split a string on every occurrence of a separator into a list of pieces.

| Parameter | Type | Description |
| --- | --- | --- |
| `s` | `utf8` | The string to split. |
| `sep` | `utf8` | The separator to split on. |
| **Returns** | `[utf8]` | The pieces between separators. |

**Example**

```wcl
split("a,b,c", ",")   // ["a", "b", "c"]
```

### sqrt

```wcl
sqrt(x: number) -> f64
```

Square root.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
sqrt(16)   // 4.0
```

### starts_with

```wcl
starts_with(s: utf8, prefix: utf8) -> bool
```

Whether a string begins with a prefix.

| Parameter | Type | Description |
| --- | --- | --- |
| `s` | `utf8` | The string to test. |
| `prefix` | `utf8` | The prefix to look for. |
| **Returns** | `bool` | `true` if the string starts with the prefix. |

**Example**

```wcl
starts_with("hello", "he")   // true
```

### sum

```wcl
sum(xs: [number]) -> number
```

Add together every element of a non-empty homogeneous numeric list or tensor.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[number]` | A non-empty list or tensor of one numeric type. |
| **Returns** | `number` | The total, in the element's numeric type. |

**Example**

```wcl
sum([1.5, 2.5, 3.0])   // 7.0
```

### tail

```wcl
tail(xs: [T]) -> [T]
```

Every element of a list or tensor except the first.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | A list or tensor. |
| **Returns** | `[T]` | The elements after the first. |

**Example**

```wcl
tail([10, 20, 30])   // [20, 30]
```

### take

```wcl
take(xs: [T], n: i64) -> [T]
```

The first `n` elements of a list (fewer if the list is shorter).

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list to take from. |
| `n` | `i64` | How many leading elements to keep. |
| **Returns** | `[T]` | The first `n` elements. |

**Example**

```wcl
take([1, 2, 3, 4], 2)   // [1, 2]
```

### tan

```wcl
tan(x: number) -> f64
```

Tangent of an angle in radians.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
tan(0)   // 0.0
```

### tau

```wcl
tau() -> f64
```

The constant τ = 2π (≈ 6.28319).

| Parameter | Type | Description |
| --- | --- | --- |
| **Returns** | `f64` | The value of τ (2π). |

**Example**

```wcl
tau()   // 6.28318…
```

### tensor

```wcl
tensor(data: [number], shape: [usize]) -> tensor<T>
```

Build a tensor from flat row-major data and a shape; the data length must equal the product of the dimensions.

| Parameter | Type | Description |
| --- | --- | --- |
| `data` | `[number]` | Flat, row-major element data. |
| `shape` | `[usize]` | The dimension sizes. |
| **Returns** | `tensor<T>` | The constructed tensor. |

**Example**

```wcl
tensor([1, 2, 3, 4, 5, 6], [2, 3])   // a 2×3 tensor
```

### tensor_data

```wcl
tensor_data(t: tensor<T>) -> [T]
```

The flat row-major element data of a tensor as a list.

| Parameter | Type | Description |
| --- | --- | --- |
| `t` | `tensor<T>` | The tensor to read. |
| **Returns** | `[T]` | The tensor's flat, row-major element data. |

**Example**

```wcl
tensor_data(tensor([1, 2, 3, 4], [2, 2]))   // [1, 2, 3, 4]
```

### tensor_reshape

```wcl
tensor_reshape(t: tensor<T>, shape: [usize]) -> tensor<T>
```

Reinterpret a tensor's data under a new shape; the element count must be unchanged.

| Parameter | Type | Description |
| --- | --- | --- |
| `t` | `tensor<T>` | The tensor to reshape. |
| `shape` | `[usize]` | The new dimension sizes. |
| **Returns** | `tensor<T>` | The same data under the new shape. |

**Example**

```wcl
tensor_reshape(tensor([1, 2, 3, 4], [2, 2]), [4])   // shape [4]
```

### tensor_shape

```wcl
tensor_shape(t: tensor<T>) -> [usize]
```

The dimension sizes of a tensor as a list.

| Parameter | Type | Description |
| --- | --- | --- |
| `t` | `tensor<T>` | The tensor to read. |
| **Returns** | `[usize]` | The tensor's dimension sizes. |

**Example**

```wcl
tensor_shape(tensor([1, 2, 3, 4], [2, 2]))   // [2, 2]
```

### to_lower

```wcl
to_lower(s: utf8) -> utf8
```

Lowercase every character of a string.

| Parameter | Type | Description |
| --- | --- | --- |
| `s` | `utf8` | The string to lowercase. |
| **Returns** | `utf8` | The lowercased string. |

**Example**

```wcl
to_lower("AbC")   // "abc"
```

### to_upper

```wcl
to_upper(s: utf8) -> utf8
```

Uppercase every character of a string.

| Parameter | Type | Description |
| --- | --- | --- |
| `s` | `utf8` | The string to uppercase. |
| **Returns** | `utf8` | The uppercased string. |

**Example**

```wcl
to_upper("abc")   // "ABC"
```

### trim

```wcl
trim(s: utf8) -> utf8
```

Remove leading and trailing whitespace from a string.

| Parameter | Type | Description |
| --- | --- | --- |
| `s` | `utf8` | The string to trim. |
| **Returns** | `utf8` | The string without leading/trailing whitespace. |

**Example**

```wcl
trim("  hi  ")   // "hi"
```

### trunc

```wcl
trunc(x: number) -> f64
```

Discard the fractional part, rounding toward zero.

| Parameter | Type | Description |
| --- | --- | --- |
| `x` | `number` | The input value (any number, widened to f64). |
| **Returns** | `f64` | The result, as an f64. |

**Example**

```wcl
trunc(2.9)   // 2.0
```

### type_fields

```wcl
type_fields(target: &T) -> [record]
```

Reflect a type or interface into a list of field-description records (own fields first, then inherited via `extends`).

| Parameter | Type | Description |
| --- | --- | --- |
| `target` | `&T` | A reference to a type or interface declaration. |
| **Returns** | `[record]` | One record per field: `{ name, type, is_function, optional, has_default, is_block, repeated, accepts, decorators }`. |

**Example**

```wcl
type_fields(Image)   // [{ name: "source", type: "utf8", … }, …]
```

### unique

```wcl
unique(xs: [T]) -> [T]
```

Remove duplicate elements from a list, keeping first-seen order.

| Parameter | Type | Description |
| --- | --- | --- |
| `xs` | `[T]` | The list to deduplicate. |
| **Returns** | `[T]` | The list with duplicates removed. |

**Example**

```wcl
unique([1, 2, 2, 3, 1])   // [1, 2, 3]
```

### zip

```wcl
zip(a: [A], b: [B]) -> [(A, B)]
```

Pair up elements of two lists by index, stopping at the shorter length.

| Parameter | Type | Description |
| --- | --- | --- |
| `a` | `[A]` | The first list. |
| `b` | `[B]` | The second list. |
| **Returns** | `[(A, B)]` | Index-paired `[a, b]` lists, up to the shorter length. |

**Example**

```wcl
zip([1, 2, 3], ["a", "b", "c"])   // [[1, "a"], [2, "b"], [3, "c"]]
```
