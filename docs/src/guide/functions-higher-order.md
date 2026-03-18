# Higher-Order Functions

Higher-order functions accept a list and a **lambda** (anonymous function) and apply the lambda to elements. They are the primary tool for transforming and testing collections in WCL.

Lambdas use the `=>` syntax:

```wcl
// Single parameter — no parentheses needed
x => x * 2

// Multiple parameters
(acc, x) => acc + x
```

Lambdas can also reference named functions from `let` bindings:

```wcl
let double = x => x * 2
let doubled = map([1, 2, 3], double)   // [2, 4, 6]
```

## Reference

| Function | Signature | Description |
|---|---|---|
| `map` | `map(list: list, fn: lambda) -> list` | Apply `fn` to each element; return list of results |
| `filter` | `filter(list: list, fn: lambda) -> list` | Keep elements for which `fn` returns `true` |
| `every` | `every(list: list, fn: lambda) -> bool` | True if `fn` returns `true` for all elements |
| `some` | `some(list: list, fn: lambda) -> bool` | True if `fn` returns `true` for at least one element |
| `reduce` | `reduce(list: list, fn: lambda, initial) -> any` | Fold list into a single value using `fn(accumulator, element)` |

## map

Transform each element of a list:

```wcl
let nums = [1, 2, 3, 4, 5]
let squared = map(nums, x => x * x)       // [1, 4, 9, 16, 25]
let strs = map(nums, x => to_string(x))   // ["1", "2", "3", "4", "5"]
```

Map over strings extracted from blocks:

```wcl
let services = query(service)
let names = map(services, s => s.name)    // ["web", "api", "worker"]
```

## filter

Keep only elements that satisfy a predicate:

```wcl
let nums = [1, 2, 3, 4, 5, 6]
let evens = filter(nums, x => x % 2 == 0)   // [2, 4, 6]
let large = filter(nums, x => x > 3)        // [4, 5, 6]
```

Filter blocks by attribute values:

```wcl
let prod_svcs = filter(query(service), s => s.env == "prod")
```

## every

Check that all elements satisfy a predicate:

```wcl
let all_positive = every([1, 2, 3], x => x > 0)     // true
let all_even     = every([2, 4, 6], x => x % 2 == 0) // true
let all_small    = every([1, 2, 100], x => x < 10)   // false
```

Short-circuits on the first `false` result.

## some

Check that at least one element satisfies a predicate:

```wcl
let has_large = some([1, 2, 3, 100], x => x > 50)   // true
let has_neg   = some([1, 2, 3], x => x < 0)         // false
```

Short-circuits on the first `true` result.

## reduce

Fold a list into a single accumulated value:

```wcl
let nums = [1, 2, 3, 4, 5]
let total = reduce(nums, (acc, x) => acc + x, 0)    // 15
let product = reduce(nums, (acc, x) => acc * x, 1)  // 120
```

Build a string from a list:

```wcl
let words = ["hello", "world", "wcl"]
let sentence = reduce(words, (acc, w) => acc + " " + w, "")
// " hello world wcl"  (note the leading space from the empty initial)
```

Build a map from a list of pairs:

```wcl
let pairs = [["a", 1], ["b", 2], ["c", 3]]
let m = reduce(pairs, (acc, p) => merge(acc, {[p[0]]: p[1]}), {})
// {a: 1, b: 2, c: 3}
```

## Composing Higher-Order Functions

Higher-order functions compose naturally:

```wcl
let nums = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]

// Sum of squares of even numbers
let result = reduce(
  map(filter(nums, x => x % 2 == 0), x => x * x),
  (acc, x) => acc + x,
  0
)
// filter -> [2, 4, 6, 8, 10]
// map    -> [4, 16, 36, 64, 100]
// reduce -> 220
```

## Using Named Lambdas

Store predicates and transformers in `let` bindings for reuse:

```wcl
let is_prod = s => s.env == "prod"
let get_port = s => s.port

let prod_services = filter(query(service), is_prod)
let prod_ports    = map(prod_services, get_port)
```

Exported lambdas (via `export let`) can be shared across files the same way as data.
