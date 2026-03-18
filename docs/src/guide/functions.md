# Functions

WCL provides 50+ built-in functions covering strings, math, collections, higher-order operations, aggregation, hashing, encoding, type coercion, and block references. All built-ins are **pure**, **deterministic**, and have **no side effects** — given the same inputs they always return the same output, and they never modify state.

Functions are called with parentheses:

```wcl
let s = upper("hello")         // "HELLO"
let n = max(1, 2, 3)           // 3
let xs = filter([1,2,3], x => x > 1)  // [2, 3]
```

## Built-in Categories

| Category | Page | Functions |
|---|---|---|
| String | [String Functions](./functions-string.md) | `upper`, `lower`, `trim`, `trim_prefix`, `trim_suffix`, `replace`, `split`, `join`, `starts_with`, `ends_with`, `contains`, `length`, `substr`, `format`, `regex_match`, `regex_capture` |
| Math | [Math Functions](./functions-math.md) | `abs`, `min`, `max`, `floor`, `ceil`, `round`, `sqrt`, `pow` |
| Collection | [Collection Functions](./functions-collection.md) | `len`, `keys`, `values`, `flatten`, `concat`, `distinct`, `sort`, `reverse`, `contains`, `index_of`, `range`, `zip` |
| Higher-Order | [Higher-Order Functions](./functions-higher-order.md) | `map`, `filter`, `every`, `some`, `reduce` |
| Aggregate | [Aggregate Functions](./functions-aggregate.md) | `sum`, `avg`, `min_of`, `max_of`, `count` |
| Hash & Encoding | [Hash & Encoding Functions](./functions-hash-encoding.md) | `sha256`, `base64_encode`, `base64_decode`, `json_encode` |
| Type Coercion | [Type Coercion Functions](./functions-type-coercion.md) | `to_string`, `to_int`, `to_float`, `to_bool`, `type_of` |
| Reference & Query | [Reference & Query Functions](./functions-ref-query.md) | `ref`, `query`, `has`, `has_decorator` |

## User-Defined Functions

WCL does not have a `fn` keyword. Instead, user-defined functions are **lambdas stored in `let` bindings**:

```wcl
let double = x => x * 2
let add = (x, y) => x + y
let greet = name => "Hello, " + name + "!"
```

Single-parameter lambdas omit the parentheses. Multi-parameter lambdas use `(a, b, ...)`. The body is a single expression — lambdas do not have block bodies.

Once bound, they are called exactly like built-ins:

```wcl
let result = double(21)         // 42
let sum = add(10, 32)           // 42
let msg = greet("world")        // "Hello, world!"
```

Lambdas can be passed to higher-order functions:

```wcl
let evens = filter([1,2,3,4,5], x => x % 2 == 0)  // [2, 4]
let doubled = map([1,2,3], double)                  // [2, 4, 6]
```

### Limitations

- **No recursion** — a lambda cannot call itself.
- **No mutation** — lambdas are pure expressions; they cannot modify outer state.
- **No variadic arguments** — arity is fixed at definition.
- **No default parameters** — all parameters must be supplied at the call site.

## Exporting Functions

Use `export let` to make a lambda available to other files that import this one:

```wcl
// utils.wcl
export let clamp = (val, lo, hi) => max(lo, min(hi, val))
export let percent = (n, total) => (n / total) * 100.0
```

```wcl
// main.wcl
import "utils.wcl"

config display {
  value: clamp(input, 0, 100)
  pct: percent(input, 200)
}
```

## Custom Functions (Rust Host)

When using WCL as a Rust library, host programs can register custom `fn(&[Value]) -> Result<Value, String>` functions that are callable from WCL expressions. This allows domain-specific logic to be exposed to configuration files.

Custom functions are registered via `ParseOptions.functions` (a `FunctionRegistry`). See the [Rust Library Usage](../getting-started/rust-library.md#custom-functions) guide for details and examples.

### Function Declarations (`declare`)

Library files can include `declare` statements that describe functions provided by the host application:

```wcl
declare my_fn(input: string, count: int) -> string
```

This serves two purposes:
1. The LSP uses declarations for completions and signature help
2. If a declared function is called but not registered by the host, a clear error is produced: *"function 'X' is declared in library but not registered by host application"*

## Purity Guarantee

Because all functions (built-in and user-defined) are pure and the language has no I/O or mutation, a WCL document always evaluates to the same result given the same inputs. This makes configs safe to cache, diff, and analyze statically.
