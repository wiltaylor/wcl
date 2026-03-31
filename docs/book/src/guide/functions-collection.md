# Collection Functions

WCL's collection functions work with lists and maps. They are pure — none mutate their input; all return new values.

## Reference

| Function | Signature | Description |
|---|---|---|
| `len` | `len(coll: list\|map\|string) -> i64` | Number of elements (or characters for strings) |
| `keys` | `keys(m: map) -> list` | Ordered list of a map's keys |
| `values` | `values(m: map) -> list` | Ordered list of a map's values |
| `flatten` | `flatten(list: list) -> list` | Recursively flatten nested lists one level |
| `concat` | `concat(a: list, b: list) -> list` | Concatenate two lists |
| `distinct` | `distinct(list: list) -> list` | Remove duplicate values, preserving first occurrence order |
| `sort` | `sort(list: list) -> list` | Sort in ascending order (strings lexicographic, numbers numeric) |
| `reverse` | `reverse(list: list) -> list` | Reverse the order of a list |
| `contains` | `contains(list: list, value) -> bool` | True if `value` is in the list |
| `index_of` | `index_of(list: list, value) -> i64` | Zero-based index of first occurrence, or -1 if not found |
| `range` | `range(start: i64, end: i64) -> list` | List of integers from `start` (inclusive) to `end` (exclusive) |
| `zip` | `zip(a: list, b: list) -> list` | List of `[a_i, b_i]` pairs; length is the shorter of the two |

## Examples

### len

```wcl
let n1 = len([1, 2, 3])             // 3
let n2 = len({a: 1, b: 2})          // 2
let n3 = len("hello")               // 5
```

### keys / values

```wcl
let config = {host: "localhost", port: 5432, db: "main"}
let k = keys(config)    // ["host", "port", "db"]
let v = values(config)  // ["localhost", 5432, "main"]
```

### flatten

```wcl
let nested = [[1, 2], [3, [4, 5]], [6]]
let flat = flatten(nested)   // [1, 2, 3, [4, 5], 6]
```

`flatten` goes one level deep. Call it again for deeper nesting.

### concat

```wcl
let a = [1, 2, 3]
let b = [4, 5, 6]
let c = concat(a, b)    // [1, 2, 3, 4, 5, 6]
```

### distinct

```wcl
let dupes = ["a", "b", "a", "c", "b"]
let uniq = distinct(dupes)    // ["a", "b", "c"]
```

### sort

```wcl
let nums = sort([3, 1, 4, 1, 5, 9, 2])
// [1, 1, 2, 3, 4, 5, 9]

let words = sort(["banana", "apple", "cherry"])
// ["apple", "banana", "cherry"]
```

### reverse

```wcl
let rev = reverse([1, 2, 3, 4])    // [4, 3, 2, 1]
```

### contains

```wcl
let present = contains([10, 20, 30], 20)    // true
let absent  = contains(["a", "b"], "c")    // false
```

### index_of

```wcl
let i = index_of(["x", "y", "z"], "y")    // 1
let j = index_of(["x", "y", "z"], "w")    // -1
```

### range

```wcl
let r = range(0, 5)     // [0, 1, 2, 3, 4]
let r2 = range(3, 7)    // [3, 4, 5, 6]
```

`range` is most often used as the source of a for loop:

```wcl
for i in range(0, 3) {
  item item-${i} { position: i }
}
```

### zip

```wcl
let names = ["web", "api", "worker"]
let ports = [80, 8080, 9000]
let pairs = zip(names, ports)
// [["web", 80], ["api", 8080], ["worker", 9000]]
```

If the lists have different lengths, the result has the length of the shorter list.

## Combining Collection Functions

Collection functions compose naturally:

```wcl
let items = [3, 1, 4, 1, 5, 9, 2, 6, 5]
let result = reverse(sort(distinct(items)))
// [9, 6, 5, 4, 3, 2, 1]
```

For transforming or filtering list elements, see [Higher-Order Functions](./functions-higher-order.md). For numeric aggregation over lists, see [Aggregate Functions](./functions-aggregate.md).
