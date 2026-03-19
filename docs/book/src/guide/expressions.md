# Expressions

Expressions appear on the right-hand side of attribute bindings, inside `let` declarations, in query `where` clauses, in decorator arguments, and in lambda bodies. WCL expressions are eagerly evaluated after dependency-order resolution.

## Operator Precedence

The table below lists all operators from lowest to highest precedence. Operators on the same row have equal precedence and are left-associative unless noted.

| Precedence | Operator(s)            | Description                        | Associativity |
|------------|------------------------|------------------------------------|---------------|
| 1 (lowest) | `? :`                  | Ternary conditional                | Right         |
| 2          | `\|\|`                  | Logical OR                         | Left          |
| 3          | `&&`                   | Logical AND                        | Left          |
| 4          | `!`                    | Logical NOT (unary)                | Right (prefix)|
| 5          | `==` `!=` `<` `>` `<=` `>=` `=~` | Comparison / regex  | Left          |
| 6          | `+` `-`                | Additive                           | Left          |
| 7          | `*` `/` `%`            | Multiplicative                     | Left          |
| 8          | `-` (unary)            | Negation                           | Right (prefix)|
| 9 (highest)| `()` `[]` `.` calls    | Grouping, index, member, call      | Left          |

Use parentheses to override the default precedence.

## Arithmetic

The `+`, `-`, `*`, `/`, and `%` operators work on numeric types. When one operand is an `int` and the other is a `float`, the `int` is promoted to `float`.

```wcl
sum      = 10 + 3        // 13
diff     = 10 - 3        // 7
product  = 10 * 3        // 30
quotient = 10 / 3        // 3   (integer division)
float_q  = 10 / 3.0      // 3.333...
remainder = 10 % 3       // 1
```

The `+` operator also concatenates strings:

```wcl
greeting = "Hello, " + "world!"  // "Hello, world!"
```

Division by zero is a runtime error.

## Comparison

Comparison operators return a `bool`.

```wcl
a == b    // equal
a != b    // not equal
a <  b    // less than
a >  b    // greater than
a <= b    // less than or equal
a >= b    // greater than or equal
```

The `=~` operator tests whether the left operand (a `string`) matches the right operand (a regex literal string):

```wcl
is_api     = name =~ "^api-"
is_version = tag  =~ "^v[0-9]+"
```

## Logical

```wcl
a && b    // true if both are true   (short-circuits: b not evaluated if a is false)
a || b    // true if either is true  (short-circuits: b not evaluated if a is true)
!a        // true if a is false
```

Short-circuit evaluation means the right operand of `&&` and `||` is only evaluated when necessary.

## Ternary

```wcl
condition ? then_value : else_value
```

Both branches must produce values, but only the selected branch is evaluated:

```wcl
mode     = debug ? "verbose" : "quiet"
timeout  = is_production ? 5000 : 500
endpoint = use_tls ? "https://${host}" : "http://${host}"
```

Ternaries can be chained, though deeply nested ternaries reduce readability:

```wcl
level = score >= 90 ? "A"
      : score >= 80 ? "B"
      : score >= 70 ? "C"
      : "F"
```

## Member Access

Use `.` to access an attribute of a block value:

```wcl
db_host = config.database.host
version = app.meta.version
```

Member access chains are resolved left-to-right. Accessing a missing key is a runtime error unless the field is declared `optional` in a schema.

## Index Access

Lists are zero-indexed. Maps accept string or identifier keys.

```wcl
first_port  = ports[0]
last_port   = ports[len(ports) - 1]
debug_flag  = env_vars["DEBUG"]
```

Out-of-bounds list access is a runtime error.

## Function Calls

Built-in and macro-defined functions are called with standard `name(args...)` syntax:

```wcl
upper_name = upper("hello")          // "HELLO"
count      = len([1, 2, 3])          // 3
hex_sum    = to_string(0xFF + 1)     // "256"
joined     = join(", ", ["a","b"])   // "a, b"
```

See the [Functions](./functions.md) section for all built-in functions.

## Lambda Expressions

Lambdas are anonymous functions used with higher-order functions like `map()`, `filter()`, and `sort_by()`.

Single-parameter shorthand (no parentheses needed):

```wcl
doubled = map([1, 2, 3], x => x * 2)   // [2, 4, 6]
```

Multi-parameter lambda:

```wcl
products = map(pairs, (a, b) => a * b)
```

Block lambdas allow multiple `let` bindings before the final expression:

```wcl
result = map(items, x => {
  let scaled = x * factor
  let clamped = min(scaled, 100)
  clamped
})
```

Inside a block lambda, `let` bindings are local to the lambda body. The last expression is the return value.

Lambdas are **not** values that can be stored in attributes. They exist only as arguments to higher-order functions.

## Grouping

Parentheses override precedence in the usual way:

```wcl
val = (a + b) * c
neg = -(x + y)
```

## Query Expressions

The `query` keyword selects blocks and returns a list:

```wcl
servers      = query server
prod_servers = query server where env == "production"
```

See [Query Engine](./query-engine.md) for the full query language.
