# Identifiers

Identifiers are the names you use throughout WCL — for fields, types, block kinds, block labels, variants, symbols, `let` bindings, and imported items. The lexical rule is the same everywhere, with one convenience exception for block labels (see below).

## Lexical rule

An identifier starts with an **ASCII letter** (`a`–`z`, `A`–`Z`) or an **underscore** (`_`), and continues with letters, digits, or underscores. No Unicode, no dashes, no spaces.

```wcl
name           // ok
my_field       // ok
_internal      // ok
v2             // ok
HTTPStatus     // ok

2nd_attempt    // NOT ok — must not start with a digit
kebab-case     // NOT ok as a field/type name — dashes aren't identifier chars
```

## Block labels: kebab-case and paths

Block **labels** (the name after a block kind) are the one place a bare identifier may contain `-` and `/` connectors — so kebab-case class names and path-like page names need no quoting. The connector must sit directly between name parts (no surrounding spaces).

```wcl
class dgm-box {}              // kebab-case, bare
class wdoc-series-1 {}        // trailing number is fine
page reference/intro {}       // path-like
page api/v1/users {}

class "dgm-box" {}            // quoting still works (and is left as-is by `wcl fmt`)
```

## Reserved words

A handful of words are reserved by the lexer and cannot be used as identifiers in any position:

| Reserved | Used for |
| --- | --- |
| `true`, `false` | Boolean literals |
| `none` | The none literal |
| `if`, `else`, `match` | Control flow |

Other words that look special — `type`, `interface`, `union`, `symbol_set`, `let`, `import`, `connection`, `fn`, `extends`, `as` — are recognised only in declaration positions, so they may also appear as ordinary identifiers in field names or expressions. Avoid relying on that: it reads more clearly when these names are kept for their declaration use.

## Where identifiers appear

| Position | Example |
| --- | --- |
| Field name | `port` in `port = 8080u32` |
| Type name | `Service` in `type Service { … }` |
| Block kind | `service` in `service "web" { … }` |
| Variant name | `Circle` in `Shape::Circle { … }` |
| Symbol name | `amber` in `:amber` |
| `let` binding | `base_port` in `let base_port = …` |
| Imported file's items | Types and `let`s from `import "./x.wcl"` |

## Naming conventions

The standard library and these docs follow Rust-like conventions, but the language doesn't enforce them. Pick a style and stay consistent within a codebase.

| Convention | Used for |
| --- | --- |
| `snake_case` | Fields, `let` bindings, block kinds, symbols |
| `PascalCase` | Types, interfaces, unions, variants, symbol sets |
| `SCREAMING` | Rarely; reserve for visibly-constant globals |
