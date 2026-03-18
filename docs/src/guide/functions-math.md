# Math Functions

WCL's math functions operate on `int` and `float` values. Functions that accept either type promote integers to floats where needed and are noted below.

## Reference

| Function | Signature | Description |
|---|---|---|
| `abs` | `abs(n: int\|float) -> int\|float` | Absolute value; preserves input type |
| `min` | `min(a: int\|float, b: int\|float) -> int\|float` | Smaller of two values |
| `max` | `max(a: int\|float, b: int\|float) -> int\|float` | Larger of two values |
| `floor` | `floor(n: float) -> int` | Round down to nearest integer |
| `ceil` | `ceil(n: float) -> int` | Round up to nearest integer |
| `round` | `round(n: float) -> int` | Round to nearest integer (half-up) |
| `sqrt` | `sqrt(n: int\|float) -> float` | Square root; always returns float |
| `pow` | `pow(base: int\|float, exp: int\|float) -> float` | Raise base to the power of exp; always returns float |

## Examples

### abs

```wcl
let a = abs(-42)     // 42
let b = abs(-3.14)   // 3.14
let c = abs(7)       // 7
```

### min / max

```wcl
let lo = min(10, 3)    // 3
let hi = max(10, 3)    // 10

// Combine with variables
let clamped = max(0, min(100, input_value))
```

### floor / ceil / round

```wcl
let f = 3.7
let down = floor(f)   // 3
let up   = ceil(f)    // 4
let near = round(f)   // 4

let g = 3.2
let down2 = floor(g)  // 3
let up2   = ceil(g)   // 4
let near2 = round(g)  // 3
```

### sqrt

```wcl
let root = sqrt(16)     // 4.0
let root2 = sqrt(2)     // 1.4142135623730951
```

### pow

```wcl
let squared = pow(4, 2)     // 16.0
let cubed   = pow(2, 10)    // 1024.0
let frac    = pow(8, 0.333) // ~2.0
```

## Integer Arithmetic

Standard arithmetic operators work on integers without calling functions:

```wcl
let sum  = 10 + 3     // 13
let diff = 10 - 3     // 7
let prod = 10 * 3     // 30
let quot = 10 / 3     // 3  (integer division)
let rem  = 10 % 3     // 1
```

For floating-point division, ensure at least one operand is a float:

```wcl
let ratio = 10.0 / 3  // 3.3333...
```

## Combining Math with Collections

For aggregate operations over lists (sum, average, min/max of a list), see [Aggregate Functions](./functions-aggregate.md). For per-element operations, use [Higher-Order Functions](./functions-higher-order.md):

```wcl
let values = [1, 4, 9, 16]
let roots = map(values, x => sqrt(x))   // [1.0, 2.0, 3.0, 4.0]
```
