# Aggregate Functions

Aggregate functions reduce a list of numbers to a single scalar value. They are most useful for summarizing configuration data — computing totals, averages, and extremes across a collection of blocks or values.

## Reference

| Function | Signature | Description |
|---|---|---|
| `sum` | `sum(list: list) -> int\|float` | Sum of all elements |
| `avg` | `avg(list: list) -> float` | Arithmetic mean of all elements |
| `min_of` | `min_of(list: list) -> int\|float` | Smallest element |
| `max_of` | `max_of(list: list) -> int\|float` | Largest element |
| `count` | `count(list: list, fn: lambda) -> int` | Number of elements for which `fn` returns `true` |

> Note: `min` and `max` take two scalar arguments and compare them directly. `min_of` and `max_of` take a list and find the extreme value within it. See [Math Functions](./functions-math.md) for `min`/`max`.

## Examples

### sum

```wcl
let totals = [10, 25, 30, 15]
let total = sum(totals)    // 80
```

Sum an attribute across all matching blocks:

```wcl
let all_replicas = map(query(service), s => s.replicas)
let replica_count = sum(all_replicas)
```

### avg

```wcl
let scores = [80, 90, 70, 100, 85]
let mean = avg(scores)    // 85.0
```

`avg` always returns a `float`, even for integer input lists.

### min_of / max_of

```wcl
let latencies = [120, 45, 200, 88, 33]
let fastest = min_of(latencies)    // 33
let slowest = max_of(latencies)    // 200
```

Find the service with the highest replica count:

```wcl
let counts = map(query(service), s => s.replicas)
let max_replicas = max_of(counts)
```

### count

Count elements that satisfy a predicate:

```wcl
let nums = [1, 2, 3, 4, 5, 6, 7, 8]
let even_count = count(nums, x => x % 2 == 0)    // 4
let large_count = count(nums, x => x > 5)        // 3
```

Count matching blocks:

```wcl
let prod_count = count(query(service), s => s.env == "prod")
```

## Combining Aggregates with Higher-Order Functions

Aggregate functions work naturally after `map` and `filter`:

```wcl
// Average port number across all production services
let prod_ports = map(
  filter(query(service), s => s.env == "prod"),
  s => s.port
)
let avg_port = avg(prod_ports)
```

```wcl
// Total memory requested across all workers with > 2 replicas
let high_replica_workers = filter(query(worker), w => w.replicas > 2)
let memory_values = map(high_replica_workers, w => w.memory_mb)
let total_memory = sum(memory_values)
```

## Validation Use Case

Aggregates are useful inside `validate` blocks to enforce fleet-wide constraints:

```wcl
validate {
  let total = sum(map(query(service), s => s.replicas))
  assert total <= 50 : "Total replica count must not exceed 50"
}
```
