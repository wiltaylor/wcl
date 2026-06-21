# Record builtins

Four builtins operate on record values. `keys` and `values` list field names and values in deterministic (sorted-by-name) order; `merge` combines two records, the second winning on a clash; `map_values` transforms every value while keeping the keys.

| Function | Use |
| --- | --- |
| `keys(r)` | List the record's field names (sorted) |
| `values(r)` | List the field values (same order as `keys`) |
| `merge(a, b)` | Combine two records; `b`'s fields win on a clash |
| `map_values(r, f)` | Apply `f` to every value, keeping the keys |

```wcl
let rex      = Dog { name: "Rex", age: 4u32 }
let names    = keys(rex)                       // ["age", "name"]
let defaults = { host: "localhost", port: 80 }
let cfg      = merge(defaults, { port: 8080 }) // { host: "localhost", port: 8080 }
let doubled  = map_values({ low: 1, high: 9 }, fn(x: i64) -> i64 x * 2)
```

## Related

- [Records](../references/concept_records.md)

- [Expressions](../references/concept_expressions.md)

[← All facts](../references/facts_ref.md)
