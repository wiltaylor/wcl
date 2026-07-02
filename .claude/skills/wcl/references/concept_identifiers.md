# Identifiers

_The naming rule for fields, types, kinds, labels, and bindings, plus reserved words._

Identifiers are the names used throughout WCL — for fields, types, block kinds, block labels, variants, symbols, `let` bindings, and imported items. The lexical rule is the same everywhere, with one convenience exception for block labels.

## Lexical rule

An identifier starts with an **ASCII letter** (`a`-`z`, `A`-`Z`) or an **underscore**, and continues with letters, digits, or underscores. No Unicode, no dashes, no spaces.

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

Block **labels** (the name after a block kind) are the one place a bare identifier may contain `-` and `/` connectors — so kebab-case class names and path-like page names need no quoting. The connector sits directly between name parts (no surrounding spaces).

```wcl
class dgm-box {}              // kebab-case, bare
class wdoc-series-1 {}        // trailing number is fine
page reference/intro {}       // path-like
page api/v1/users {}

class "dgm-box" {}            // quoting still works
```

## Reserved words

A handful of words are reserved by the lexer and cannot be used as identifiers in any position:

| Reserved | Used for |
| --- | --- |
| `true`, `false` | Boolean literals |
| `none` | The none literal |
| `if`, `else`, `match` | Control flow |

Other words that look special — `type`, `interface`, `union`, `symbol_set`, `let`, `import`, `connection`, `fn`, `extends`, `as` — are recognised only in declaration positions, so they may also appear as ordinary identifiers. It reads more clearly to keep them for their declaration use.

## Naming conventions

The standard library follows Rust-like conventions, but the language does not enforce them. Pick a style and stay consistent.

| Convention | Used for |
| --- | --- |
| `snake_case` | Fields, `let` bindings, block kinds, symbols |
| `PascalCase` | Types, interfaces, unions, variants, symbol sets |
| `SCREAMING` | Rarely; reserve for visibly-constant globals |

## Related

- [Namespaces](../references/concept_namespaces.md)

- [Symbols](../references/concept_symbols.md)

[← Back to SKILL.md](../SKILL.md)
