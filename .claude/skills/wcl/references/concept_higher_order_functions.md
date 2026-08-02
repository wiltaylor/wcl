# Higher-order Functions

_Functions that take and return other functions._

Functions can take and return other functions. This is how the collection builtins like `map`,
`filter`, and `fold` are parameterised.


```wcl
adder = fn(x: i32) -> fn(i32) -> i32 fn(y: i32) -> i32 x + y
add3  = adder(3)
seven = add3(4)

doubled = map([1, 2, 3], fn(x: i64) -> i64 x * 2)
```

[← Back to SKILL.md](../SKILL.md)
