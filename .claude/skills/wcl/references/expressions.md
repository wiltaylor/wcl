# Expressions

Every field value, function argument, and `let` right-hand side is an **expression** — something that produces a value. WCL's expression grammar is small: literals, identifiers, member access, calls, operators, and a handful of constructor forms.

> [!NOTE]
> **Covered elsewhere**
> Literal forms live with their types (Numbers, Strings, Booleans, Symbols, Optionals, Lists, Tensors). Function literals are in Functions. if / match / let / block expressions are in Control Flow. This chapter focuses on the connective tissue.

## Operators

| Group | Operators |
| --- | --- |
| Arithmetic | `+` `-` `*` `/` `%` |
| Comparison | `==` `!=` `<` `<=` `>` `>=` |
| Logical | `&&` `\|\|` `!` |
| Coalescing | `??` (left value unless `none`) |
| Unary | `-expr` (negate), `!expr` (not) |

Precedence follows the usual conventions: unary tightest, then `*` `/` `%`, then `+` `-`, then comparison, then `&&`, then `||`, then `??` loosest. Use parentheses to group when in doubt.

## Numeric promotion

Arithmetic and comparison widen mixed numeric operands to a common type, so cross-width and integer/float mixing work without explicit casts.

```wcl
a = 1 + 2.0        // i64 widened to f64 → 3.0
b = 1u32 == 1i64   // true
c = 3.0 * 2u8      // 6.0
```

## Member access

A dotted path reads a field. Access chains through records, variant payloads, and any composite that exposes named members.

```wcl
region = service.metadata.region
deep   = config.services.web.metadata.region
```

## Function calls

Parentheses call a function or constructor. Arguments are expressions, evaluated lazily; the call evaluates the body in a fresh context.

```wcl
n      = len(items)
total  = sum(map(items, fn(x: i64) -> i64 x * 2))
shape  = Point { x: 1.0, y: 2.0 }
```

## Variant construction

Build a union variant with `Union::Variant`. Record bodies go in braces, typeref payloads in parentheses, unit constructors stand bare.

```wcl
a = Shape::Circle { radius: 5.0, stroke: 0.5 }
b = Shape::Polygon(7)
c = Shape::Empty
```

See [Unions](../references/unions.md) for declaring the surrounding variant set.

## List literals

Square brackets and commas build a `list<T>`. See [Lists](../references/lists.md) for nested lists and the collection builtins.

```wcl
xs    = [1, 2, 3]
names = ["alice", "bob"]
empty = []
```
