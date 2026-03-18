# Expression Evaluation

This page describes how WCL evaluates expressions: operator precedence, type rules, short-circuit behavior, and error conditions.

## Operator Precedence

Operators are listed from lowest to highest precedence:

| Level | Operators | Associativity |
|-------|-----------|---------------|
| 1 | `?:` (ternary) | Right |
| 2 | `\|\|` (logical or) | Left |
| 3 | `&&` (logical and) | Left |
| 4 | `==`, `!=` | Left |
| 5 | `<`, `>`, `<=`, `>=`, `=~` | Left |
| 6 | `+`, `-` | Left |
| 7 | `*`, `/`, `%` | Left |
| 8 | `!` (unary not), `-` (unary negation) | Right (prefix) |
| 9 | `.field`, `[index]`, `(call)` (postfix) | Left |

## Ternary Expression

```
condition ? then_value : else_value
```

The condition must evaluate to `bool`. Only the selected branch is evaluated.

## Logical Operators

| Operator | Types | Result |
|----------|-------|--------|
| `a \|\| b` | `bool`, `bool` | `bool` |
| `a && b` | `bool`, `bool` | `bool` |
| `!a` | `bool` | `bool` |

**Short-circuit evaluation**: `||` does not evaluate the right operand if the left is `true`. `&&` does not evaluate the right operand if the left is `false`.

## Equality Operators

| Operator | Types | Result |
|----------|-------|--------|
| `a == b` | any matching types | `bool` |
| `a != b` | any matching types | `bool` |

Equality is deep structural equality for lists and maps. Comparing values of different types always returns `false` for `==` and `true` for `!=` (no implicit coercion).

## Comparison Operators

| Operator | Types | Result |
|----------|-------|--------|
| `a < b` | `int`, `float`, `string` | `bool` |
| `a > b` | `int`, `float`, `string` | `bool` |
| `a <= b` | `int`, `float`, `string` | `bool` |
| `a >= b` | `int`, `float`, `string` | `bool` |
| `a =~ b` | `string`, `string` | `bool` |

The `=~` operator matches the left operand against the right operand as a regular expression (RE2 syntax). Returns `true` if there is any match.

Comparing across incompatible types (e.g., `int` with `string`) produces error E050.

## Arithmetic Operators

| Operator | Types | Result | Notes |
|----------|-------|--------|-------|
| `a + b` | `int`, `int` | `int` | |
| `a + b` | `float`, `float` | `float` | |
| `a + b` | `string`, `string` | `string` | Concatenation |
| `a + b` | `list`, `list` | `list` | Concatenation |
| `a - b` | `int`, `int` | `int` | |
| `a - b` | `float`, `float` | `float` | |
| `a * b` | `int`, `int` | `int` | |
| `a * b` | `float`, `float` | `float` | |
| `a / b` | `int`, `int` | `int` | Integer division; error E051 if `b == 0` |
| `a / b` | `float`, `float` | `float` | IEEE 754; `b == 0.0` produces infinity |
| `a % b` | `int`, `int` | `int` | Remainder; error E051 if `b == 0` |
| `-a` | `int` | `int` | Unary negation |
| `-a` | `float` | `float` | Unary negation |

## Field Access and Indexing

```wcl
obj.field        // access named field of a map or block
list[0]          // index into a list (0-based)
map["key"]       // index into a map by string key
```

Out-of-bounds list indexing produces error E054. Accessing a missing map key returns `null`.

## Function Calls

```wcl
value.to_string()
list.map(x => x * 2)
string.split(",")
```

Postfix call syntax is used for built-in functions and lambda application. Calling an unknown function produces error E052.

## String Interpolation

```wcl
let msg = "Hello, ${name}! Port is ${port + 1}."
```

Interpolated expressions (`${...}`) are evaluated and converted to their string representation before concatenation. Any expression type is allowed inside `${}`.

## Query Expressions

```wcl
let services = query(service | .port > 1024)
```

Query expressions run the pipeline query engine against the current document scope and return a list of matching resolved values. Queries are evaluated after scope construction is complete.

## Ref Expressions

```wcl
let api = ref(svc-api)
```

A `ref` expression resolves to the block or value with the given identifier. If no matching identifier is found, error E053 is produced.

## Lambda Expressions

```wcl
let double = x => x * 2
let add    = (a, b) => a + b
let clamp  = (v, lo, hi) => v < lo ? lo : (v > hi ? hi : v)
```

Lambdas capture their lexical scope. They are first-class values and can be passed to built-in higher-order functions.

## Error Conditions

| Code | Condition |
|------|-----------|
| E050 | Type mismatch in operator or function call |
| E051 | Division or modulo by zero |
| E052 | Call to unknown function |
| E053 | `ref()` target identifier not found |
| E054 | List index out of bounds |
| E040 | Reference to undefined name |
| E041 | Cyclic dependency between names |
