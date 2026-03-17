# WCL — Wil's Configuration Language

## Specification v0.1.0

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Design Principles](#2-design-principles)
3. [Lexical Grammar](#3-lexical-grammar)
4. [Type System](#4-type-system)
5. [Values and Literals](#5-values-and-literals)
6. [Comments and Trivia](#6-comments-and-trivia)
7. [Attributes](#7-attributes)
8. [Blocks](#8-blocks)
9. [Identifiers and the `id` Type](#9-identifiers-and-the-id-type)
10. [Variables and Scoping](#10-variables-and-scoping)
11. [Expressions](#11-expressions)
12. [Control Flow Structures](#12-control-flow-structures)
13. [String Interpolation](#13-string-interpolation)
14. [Built-in Functions](#14-built-in-functions)
15. [Decorators](#15-decorators)
16. [Decorator Schemas](#16-decorator-schemas)
17. [Schemas and Validation](#17-schemas-and-validation)
18. [Data Tables](#18-data-tables)
19. [Import System](#19-import-system)
20. [Partial Declarations and Merging](#20-partial-declarations-and-merging)
21. [Macros](#21-macros)
22. [Query System](#22-query-system)
23. [Document Validation](#23-document-validation)
24. [Evaluation Pipeline](#24-evaluation-pipeline)
25. [Error Handling and Diagnostics](#25-error-handling-and-diagnostics)
26. [Serialization and Deserialization (Serde)](#26-serialization-and-deserialization-serde)
27. [Crate Architecture](#27-crate-architecture)
28. [CLI Interface](#28-cli-interface)
29. [File Extension and MIME Type](#29-file-extension-and-mime-type)
30. [EBNF Grammar Summary](#30-ebnf-grammar-summary)
31. [Complete Examples](#31-complete-examples)

---

## 1. Introduction

WCL (Wil's Configuration Language) is a statically-typed, block-structured configuration language designed for human-readable configuration with first-class support for composition, validation, and tooling. It draws syntactic inspiration from HCL (HashiCorp Configuration Language) and extends it with a rich type system, schemas, decorators, macros, data tables, a query engine, and partial declarations for cross-file composition.

WCL is implemented in Rust using the `nom` parser combinator library and exposes its data model through a `serde` integration layer for seamless interoperability with the Rust ecosystem.

### 1.1 Goals

- Human-readable and human-writable configuration format
- First-class type system with schema validation
- Composability across files via imports and partial declarations
- Metaprogramming through hygienic macros
- Built-in query engine for document introspection
- Full comment preservation for round-trip tooling (formatters, LSP, linters)
- Declarative expressions that evaluate at parse time
- No side effects, no network access, no shell execution
- Secure by design: no remote imports, jailed file access, no code execution beyond pure expressions

### 1.2 Non-Goals

- WCL is NOT a general-purpose programming language
- WCL does NOT support recursion (in expressions) or mutable state
- WCL does NOT support arbitrary looping or iteration beyond declarative `for` expansion and `if/else` conditional blocks
- WCL does NOT support remote file imports or network access
- WCL does NOT support arbitrary code execution or shell commands

---

## 2. Design Principles

1. **Declarations over instructions**: WCL documents describe desired state, not procedures to achieve it.
2. **Explicit over implicit**: Variables use `let`, partials use `partial`, types are declared in schemas.
3. **Composition over inheritance**: Partial declarations merge fragments; imports compose files.
4. **Validate early, validate everything**: Schemas, decorator schemas, type checking, and document-level validations all run before data reaches consumers.
5. **Tooling-first**: Comment preservation, spans on every AST node, and structured diagnostics make IDE/LSP support a first-class concern.
6. **Security by default**: No network access, jailed file system access, depth limits on all recursive operations.

---

## 3. Lexical Grammar

### 3.1 Character Encoding

WCL source files MUST be valid UTF-8. The byte order mark (BOM) is permitted but ignored at the start of a file.

### 3.2 Line Endings

WCL accepts LF (`\n`), CR+LF (`\r\n`), and CR (`\r`) as line terminators. All are normalized to LF internally.

### 3.3 Whitespace

Whitespace characters are:
- Space (U+0020)
- Horizontal tab (U+0009)
- Newline (U+000A, after normalization)

Whitespace is significant only for separating tokens. It is not significant within blocks or between attributes.

### 3.4 Keywords

The following are reserved keywords and MUST NOT be used as identifiers, block types, or variable names:

```
let       partial    macro     schema    table
import    export     query     ref       for
in        true       false     null      if
else      when       inject    set       remove
self      validation  decorator_schema
```

Note: `import_table` and `import_raw` are built-in function names, not reserved keywords. They follow normal function call syntax and could technically be shadowed by user-defined variables (though this is strongly discouraged).

### 3.5 Tokens

WCL source is tokenized into the following token types:

| Token | Pattern | Examples |
|-------|---------|----------|
| IDENT | `[a-zA-Z_][a-zA-Z0-9_]*` | `service`, `port`, `my_var` |
| IDENTIFIER_LIT | `[a-zA-Z_][a-zA-Z0-9_-]*` | `svc-payments`, `my_id`, `node-01` |
| STRING_LIT | `"..."` (double-quoted with escapes) | `"hello"`, `"line\n"` |
| INT_LIT | `[0-9]+` or `0x[0-9a-fA-F]+` or `0o[0-7]+` or `0b[01]+` | `42`, `0xFF`, `0o77`, `0b1010` |
| FLOAT_LIT | `[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?` | `3.14`, `1.0e10` |
| BOOL_LIT | `true` \| `false` | `true` |
| NULL_LIT | `null` | `null` |
| LBRACE | `{` | |
| RBRACE | `}` | |
| LBRACKET | `[` | |
| RBRACKET | `]` | |
| LPAREN | `(` | |
| RPAREN | `)` | |
| EQUALS | `=` | |
| COMMA | `,` | |
| PIPE | `\|` | |
| DOT | `.` | |
| DOTDOT | `..` | |
| HASH | `#` | |
| AT | `@` | |
| COLON | `:` | |
| QUESTION | `?` | |
| PLUS | `+` | |
| MINUS | `-` | |
| STAR | `*` | |
| SLASH | `/` | |
| PERCENT | `%` | |
| EQEQ | `==` | |
| NEQ | `!=` | |
| LT | `<` | |
| GT | `>` | |
| LTE | `<=` | |
| GTE | `>=` | |
| MATCH | `=~` | |
| AND | `&&` | |
| OR | `\|\|` | |
| NOT | `!` | |
| FATARROW | `=>` | |
| INTERP_START | `${` | |
| INTERP_END | `}` (within interpolation) | |
| LINE_COMMENT | `// ...` | |
| BLOCK_COMMENT | `/* ... */` | |
| DOC_COMMENT | `/// ...` | |

### 3.6 Identifier Literals vs Identifiers

WCL distinguishes between two similar but different token types:

- **IDENT**: A standard identifier used for keywords, block type names, attribute names, variable names. May contain letters, digits, and underscores. MUST start with a letter or underscore. Hyphens are NOT allowed.
- **IDENTIFIER_LIT**: An identifier literal used for the `id` type. May contain letters, digits, underscores, and hyphens. MUST start with a letter or underscore. MUST NOT start with a digit or hyphen.

The parser distinguishes these by context: IDENTIFIER_LIT appears in inline id positions on block declarations, in `ref()` calls, in query `#` selectors, and as values assigned to attributes typed as `identifier`.

### 3.7 String Escape Sequences

Within double-quoted strings, the following escape sequences are recognized:

| Sequence | Meaning |
|----------|---------|
| `\\` | Backslash |
| `\"` | Double quote |
| `\n` | Newline (LF) |
| `\r` | Carriage return |
| `\t` | Horizontal tab |
| `\uXXXX` | Unicode code point (4 hex digits) |
| `\UXXXXXXXX` | Unicode code point (8 hex digits) |
| `${...}` | String interpolation (see Section 13) |

### 3.8 Heredoc Strings

For multi-line strings without excessive escaping, WCL supports heredoc syntax:

```wcl
description = <<EOF
This is a multi-line string.
It preserves line breaks and "quotes" without escaping.
Indentation is relative to the closing marker.
EOF
```

Indented heredocs strip leading whitespace up to the indentation level of the closing marker:

```wcl
description = <<-EOF
    This line has no leading whitespace in the result.
    Neither does this one.
    EOF
```

String interpolation is active within heredocs. To disable it, prefix the marker with a single quote:

```wcl
template = <<'EOF'
This ${is_not_interpolated} and remains literal.
EOF
```

---

## 4. Type System

WCL has a structural type system used for schema validation. Types are NOT inferred at the expression level during evaluation — the evaluator works with concrete values. Types are checked during schema validation after evaluation.

### 4.1 Primitive Types

| Type | Description | Literal Examples |
|------|-------------|-----------------|
| `string` | UTF-8 text | `"hello"`, `"line\n"` |
| `int` | 64-bit signed integer | `42`, `-1`, `0xFF` |
| `float` | 64-bit IEEE 754 floating point | `3.14`, `1.0e-3` |
| `bool` | Boolean | `true`, `false` |
| `null` | Null/absent value | `null` |
| `identifier` | Identifier literal (see Section 9) | `svc-payments`, `my-id` |

### 4.2 Composite Types

| Type Syntax | Description |
|-------------|-------------|
| `list(T)` | Ordered list of values of type `T` |
| `map(K, V)` | Key-value map; `K` must be `string` or `identifier` |
| `set(T)` | Unordered set of unique values of type `T` |

### 4.3 Special Types

| Type Syntax | Description |
|-------------|-------------|
| `ref(schema_name)` | Reference to a block matching a named schema |
| `any` | Accepts any type (opt out of type checking for this field) |
| `union(T1, T2, ...)` | Value must be one of the listed types |
| `function` | A callable lambda value (see Section 11.10) |

The `function` type is an internal type used by the evaluator. It CANNOT be used in schema type declarations — functions exist only during evaluation and are erased or skipped during serde deserialization. Attempting to use `function` as a schema field type is an error.

### 4.4 Type Coercion Rules

WCL uses strict typing with minimal implicit coercion:

- `int` → `float`: Allowed implicitly in arithmetic when one operand is `float`
- All other coercions MUST be explicit using built-in conversion functions: `to_string()`, `to_int()`, `to_float()`, `to_bool()`
- No implicit coercion between `string` and numeric types
- No implicit coercion between `string` and `bool`
- No implicit coercion between `int`/`float` and `bool`
- `identifier` is NOT implicitly coerced to `string`; use `to_string(id)` if needed

---

## 5. Values and Literals

### 5.1 String Literals

Double-quoted strings with escape sequences and interpolation:

```wcl
name = "Alice"
greeting = "Hello, ${name}!"
multiline = "line one\nline two"
```

### 5.2 Integer Literals

```wcl
decimal     = 42
negative    = -17
hexadecimal = 0xFF
octal       = 0o755
binary      = 0b10101010
```

Underscores are permitted as visual separators and are ignored: `1_000_000`, `0xFF_FF`.

### 5.3 Float Literals

```wcl
pi    = 3.14159
sci   = 6.022e23
small = 1.0e-10
```

### 5.4 Boolean Literals

```wcl
enabled = true
debug   = false
```

### 5.5 Null Literal

```wcl
optional_field = null
```

### 5.6 List Literals

```wcl
ports  = [8080, 8081, 8082]
names  = ["alice", "bob", "charlie"]
mixed  = [1, "two", true]  // valid only if schema allows `list(any)`
nested = [[1, 2], [3, 4]]
```

Trailing commas are permitted:

```wcl
tags = [
    "web",
    "prod",
    "critical",
]
```

### 5.7 Map Literals

```wcl
labels = {
    environment = "production"
    team        = "platform"
    cost_center = "eng-42"
}
```

Maps use the same `key = value` syntax as block attributes. Keys must be IDENT tokens or string literals. Nested maps are supported.

### 5.8 Identifier Literals

Identifier literals are unquoted tokens used for the `id` type:

```wcl
service svc-payments "payments" {
    dependency = svc-auth
}
```

See Section 9 for full details.

---

## 6. Comments and Trivia

WCL preserves all comments in the AST to enable lossless round-trip processing. This is critical for formatters, LSP support, and automated refactoring tools.

### 6.1 Comment Styles

#### Line Comments

```wcl
// This is a line comment
port = 8080  // Trailing comment on same line
```

#### Block Comments

```wcl
/* This is a block comment
   that spans multiple lines */
port = /* inline block comment */ 8080
```

Block comments nest:

```wcl
/* outer /* inner */ still in outer */
```

#### Doc Comments

```wcl
/// This is a doc comment.
/// It can span multiple consecutive lines.
/// Doc comments attach to the next declaration.
service svc-api "api" {
    /// The port this service listens on.
    port = 8080
}
```

Doc comments are syntactically `///` (three slashes). They are semantically distinct from line comments and are intended for documentation extraction, LSP hover information, and generated documentation.

### 6.2 Comment Attachment Rules

Comments are associated with AST nodes based on their position:

1. **Leading comments**: A comment on the line(s) immediately preceding a node (with no blank line separating them) attaches as a leading comment of that node.
2. **Trailing comments**: A comment on the same line after a node's value attaches as a trailing comment of that node.
3. **Floating comments**: A comment inside a block that is not adjacent to any attribute or child block (separated by blank lines on both sides) is stored as a floating comment within that block's body.
4. **Doc comments**: `///` comments always attach as leading documentation to the next declaration. A doc comment not followed by a declaration is a parse error.

### 6.3 Trivia Model

Every AST node carries a `Trivia` structure:

```rust
struct Trivia {
    /// Comments associated with this node
    comments: Vec<Comment>,
    /// Number of blank lines preceding this node (for formatting preservation)
    leading_newlines: u32,
}

struct Comment {
    text: String,
    style: CommentStyle,       // Line, Block, Doc
    placement: CommentPlacement, // Leading, Trailing, Floating
    span: Span,
}
```

### 6.4 Blank Line Preservation

The number of consecutive blank lines before each node is recorded. Formatters SHOULD preserve the author's blank line grouping (up to a configurable maximum, default 2).

---

## 7. Attributes

Attributes are key-value pairs that represent the data within blocks. They are the fundamental unit of configuration data.

### 7.1 Syntax

```
attribute = decorator* IDENT "=" expression
```

Examples:

```wcl
port     = 8080
name     = "my-service"
enabled  = true
tags     = ["web", "prod"]
timeout  = 30 * 1000
greeting = "Hello, ${user}!"
```

### 7.2 Attribute Names

Attribute names MUST be valid IDENT tokens: start with a letter or underscore, followed by letters, digits, or underscores. Hyphens are NOT permitted in attribute names (use underscores instead).

### 7.3 Attribute Values

The right-hand side of an attribute is a full expression (see Section 11). This means attributes can have:

- Literal values: `port = 8080`
- Variable references: `port = base_port`
- Arithmetic: `port = base_port + 1`
- Function calls: `name = upper("my-service")`
- String interpolation: `url = "https://${host}:${port}"`
- Ternary expressions: `replicas = env == "prod" ? 3 : 1`
- Query calls: `all_ports = query(service | .port)`
- Ref calls: `upstream = ref(svc-auth)`
- Raw file content: `sql = import_raw("./query.sql")`

### 7.4 Duplicate Attributes

Within a single block (non-partial), duplicate attribute names are a parse error:

```wcl
config {
    port = 8080
    port = 9090  // ✗ error: duplicate attribute 'port'
}
```

In partial declarations, duplicate attributes across fragments are handled by the merge system (see Section 20).

---

## 8. Blocks

Blocks are the primary structural element in WCL. They group related attributes and can be nested.

### 8.1 Syntax

```
block = decorator* "partial"? IDENT IDENTIFIER_LIT? STRING_LIT* "{" body "}"
body  = (attribute | block | table | let_binding | macro_call | for_loop | conditional | comment)*
```

The block declaration has the following components, in order:

1. **Decorators** (optional, zero or more): `@decorator_name(args)`
2. **Partial keyword** (optional): `partial`
3. **Block type** (required): An IDENT naming the type of block: `service`, `database`, `config`, etc.
4. **Inline ID** (optional): An IDENTIFIER_LIT for the block's unique identifier: `svc-payments`
5. **Labels** (optional, zero or more): Quoted string literals providing human-readable names or variant identifiers: `"payments"`, `"primary"`
6. **Body** (required): Delimited by `{` and `}`, containing attributes, nested blocks, tables, let bindings, macro calls, and comments.

### 8.2 Examples

```wcl
// Minimal block — type and body only
config {
    debug = true
}

// Block with labels
database "primary" {
    host = "db.internal"
}

// Block with inline ID
service svc-payments {
    port = 8443
}

// Block with inline ID and label
service svc-payments "payments" {
    port = 8443
}

// Block with multiple labels
resource "aws_instance" "web" {
    ami = "ami-12345"
}

// Block with inline ID, label, and decorators
@deprecated("use svc-payments-v2")
service svc-payments "payments" {
    port = 8443
}

// Partial block
partial service svc-payments "payments" {
    port = 8443
}

// Nested blocks
service svc-gateway "gateway" {
    port = 8000

    listener listener-internal "internal" {
        bind = "127.0.0.1"
        port = 9000
    }

    listener listener-external "external" {
        bind = "0.0.0.0"
        port = 8000
    }
}
```

### 8.3 Block Type Names

Block type names are IDENT tokens. They are user-defined — WCL does not have a fixed set of block types. The meaning of block types is determined by schemas and the consuming application.

The following block type names are reserved and have special semantics:

- `schema` — defines a type schema (see Section 17)
- `decorator_schema` — defines a decorator schema (see Section 16)
- `table` — defines a data table (see Section 18)
- `validation` — defines a document-level validation rule (see Section 23)
- `macro` — defines a macro (see Section 21) — note: `macro` is used with function syntax, not block syntax

### 8.4 Block Nesting

Blocks can be nested to arbitrary depth:

```wcl
infrastructure {
    cluster "primary" {
        node_pool "workers" {
            scaling {
                min = 3
                max = 10
            }
        }
    }
}
```

---

## 9. Identifiers and the `id` Type

### 9.1 Overview

The `identifier` type is a first-class type in WCL representing a unique, addressable name for a block. Identifiers are unquoted alphanumeric strings with limited special characters, designed to be used as stable, human-readable keys for referencing blocks across a document or across files.

### 9.2 Identifier Literal Syntax

```
IDENTIFIER_LIT = [a-zA-Z_][a-zA-Z0-9_-]*
```

Rules:
- MUST start with an ASCII letter (`a-z`, `A-Z`) or underscore (`_`)
- May contain ASCII letters, ASCII digits (`0-9`), underscores (`_`), and hyphens (`-`)
- MUST NOT start with a digit
- MUST NOT start with a hyphen
- MUST NOT contain spaces or any characters other than those listed above
- Is case-sensitive: `svc-Auth` and `svc-auth` are different identifiers
- MUST NOT be a reserved keyword

Valid examples: `svc-payments`, `my_service_01`, `_internal`, `A`, `node-3-replica`

Invalid examples: `3-invalid`, `-starts-dash`, `has spaces`, `special!char`

### 9.3 Inline ID on Block Declarations

The primary way to assign an identifier to a block is inline at the declaration site, immediately after the block type name:

```wcl
service svc-payments "payments" {
    // ...
}
```

The inline ID appears between the block type and any string labels. It is syntactically an IDENTIFIER_LIT token (unquoted) and is visually distinct from labels (which are quoted strings).

### 9.4 Uniqueness Constraint

Within a given scope, no two non-partial blocks may share the same inline ID. This is enforced during the scope construction phase:

```wcl
service svc-payments "payments" {
    port = 8443
}

service svc-payments "other" {
    port = 9000
}
// ✗ error: duplicate id 'svc-payments' in scope; first defined at line 1
```

The uniqueness constraint applies per-scope. Nested blocks in different parent blocks may reuse the same ID:

```wcl
cluster cluster-a {
    node node-1 { }  // OK
}

cluster cluster-b {
    node node-1 { }  // OK — different parent scope
}
```

### 9.5 Partial Exception

The `partial` keyword exempts a block from the uniqueness error. Instead, multiple `partial` blocks with the same ID in the same scope are merged (see Section 20):

```wcl
partial service svc-payments "payments" {
    port = 8443
}

partial service svc-payments "payments" {
    monitoring {
        interval = 15
    }
}
// ✓ valid — these merge into a single service block
```

All instances sharing an ID MUST be declared `partial`. Mixing partial and non-partial declarations with the same ID is an error.

### 9.6 ID as Attribute Type

The `identifier` type can be used in schemas for attributes that hold references to other blocks:

```wcl
schema "dependency" {
    target_service = identifier @ref("service")
    priority       = int
}
```

When an attribute is typed as `identifier` with a `@ref` decorator, the validator checks that the identifier value resolves to an existing block of the specified type.

### 9.7 ID References with `ref()`

The `ref()` built-in function resolves an identifier to its target block, allowing access to that block's attributes:

```wcl
service svc-auth {
    port = 8001
    host = "auth.internal"
}

service svc-gateway {
    port = 8000
    auth_url = "http://${ref(svc-auth).host}:${ref(svc-auth).port}/auth"
}
```

`ref(identifier)` returns a block reference value. Attribute access via `.` notation retrieves attribute values from the referenced block. If the identifier does not resolve to any block, it is an evaluation error.

### 9.8 ID in Queries

The `#` shorthand provides id-based selection in queries:

```wcl
let auth = query(service#svc-auth)
// Equivalent to: query(service | .id == svc-auth)
```

See Section 22 for full query syntax.

---

## 10. Variables and Scoping

### 10.1 Variable Declarations

Variables are declared using the `let` keyword. By default, they exist for computation and intermediate values within the current file, and are erased from the document before serde deserialization — they are never visible to consumers or to files that import the current file.

```
let_binding = "let" IDENT "=" expression
```

Examples:

```wcl
let base_port = 8000
let env_suffix = env == "prod" ? "" : "-${env}"
let all_ports = query(service | .port)
```

### 10.2 Exported Variables

The `export` keyword makes a variable available to files that import the current file. Exported variables are merged into the importing document's scope alongside blocks, schemas, and other top-level content.

#### 10.2.1 Export with Assignment

```
export_binding = "export" "let" IDENT "=" expression
```

```wcl
// shared.wcl
export let default_region = "ap-southeast-2"
export let base_port = 8000
export let make_url = (host, port) => "https://${host}:${port}"

let internal_secret = "private"  // NOT exported — file-private
```

```wcl
// main.wcl
import "./shared.wcl"

// Exported variables are directly available — no prefix
service svc-api "api" {
    region = default_region       // ✓ from shared.wcl export
    port   = base_port + 1        // ✓ from shared.wcl export
    url    = make_url("api", base_port + 1)  // ✓ exported function
}

// internal_secret is NOT available — it's a private let
```

#### 10.2.2 Re-export Without Assignment

The `export` keyword can be used without `let` and without an assignment to re-export a name that was imported from another file. This creates a transitive export chain — files that import the current file will also see the re-exported names:

```
reexport = "export" IDENT
```

```wcl
// types.wcl
export let base_port = 8000
export let default_region = "ap-southeast-2"

// prelude.wcl — re-exports everything from types
import "./types.wcl"
export base_port           // re-export to downstream importers
export default_region      // re-export to downstream importers
export let env = "prod"    // also export a new variable

// main.wcl
import "./prelude.wcl"

// All three are available:
service svc-api "api" {
    port   = base_port        // ✓ re-exported from types.wcl via prelude.wcl
    region = default_region   // ✓ re-exported from types.wcl via prelude.wcl
    env    = env              // ✓ directly exported from prelude.wcl
}
```

Re-export can only reference names that are currently in scope — either from the current file's own `export let` / `let` bindings, or from imported files' exported names. Re-exporting a name that doesn't exist is an error:

```wcl
export nonexistent  // ✗ error: 'nonexistent' is not defined in this scope
```

Re-exporting a private `let` binding promotes it to exported:

```wcl
let helper = x => x * 2  // private
export helper             // now exported to importers
```

#### 10.2.3 Export Scoping Rules

- `export let` and `export` (re-export) are **top-level only** — they MUST NOT appear inside blocks, for loops, conditionals, or macros.
- Exported names participate in the same uniqueness rules as attributes — no two exported names may collide in the merged document. If two imported files both export a variable with the same name, it is an error (similar to duplicate attributes).
- Exported variables are still erased before serde deserialization. They exist only during the evaluation phase and are not visible to consumers via the serde `Deserializer`.
- Exported variables DO participate in the dependency graph and topo-sort evaluation just like regular `let` bindings.

### 10.3 Variable, Exported Variable, and Attribute Comparison

| Property | `let` (private) | `export let` (exported) | Attribute |
|----------|-----------------|-------------------------|-----------|
| Syntax | `let name = expr` | `export let name = expr` | `name = expr` |
| Visible to serde | No (erased) | No (erased) | Yes (except function values) |
| Visible to importers | No | Yes | Yes |
| Can be re-exported | No (but can be promoted via `export name`) | Yes (via `export name` in downstream) | N/A (always merged) |
| Can be decorated | No | No | Yes |
| Subject to schema validation | No | No | Yes |
| Appears in query results | No | No | Yes |
| Can hold function values | Yes | Yes | Yes |
| Valid inside blocks | Yes | No (top-level only) | Yes |

### 10.4 Scope Model

WCL uses lexical (block-structured) scoping with the following scope kinds:

1. **Module scope**: The top level of a file. Contains top-level `let` bindings, attributes, and blocks. After import resolution, all imported content is merged into the root file's module scope.
2. **Block scope**: Created by each block's `{ }` body. Can see names from parent scopes.
3. **Macro scope**: Each macro expansion creates an isolated scope (see Section 21.8 for hygiene rules).

### 10.5 Name Resolution

When a name is referenced in an expression, it is resolved by walking the scope chain from the current scope upward:

1. Check variables in the current scope.
2. Check attributes in the current scope.
3. If not found, repeat steps 1-2 in the parent scope.
4. Continue until the module scope is reached.
5. If not found in any scope, it is an undefined reference error.

For qualified paths (e.g., `config.server.port`), the first segment is resolved via the scope chain, and subsequent segments walk into child block scopes.

### 10.6 Shadowing

A `let` or `export let` binding in an inner scope may shadow a binding with the same name in an outer scope. This is permitted but produces a warning by default:

```wcl
let port = 8000

config {
    let port = 9000  // warning: 'port' shadows module-level binding at line 1
    actual_port = port  // resolves to 9000
}
```

Shadowing warnings can be suppressed explicitly:

```wcl
@allow(shadowing)
let port = 9000  // no warning
```

### 10.7 Unused Variable Warnings

Variables that are declared but never referenced produce a warning:

```wcl
let unused_var = 42  // warning: variable 'unused_var' is never read
```

### 10.8 Evaluation Order

Variables and attributes within a scope are evaluated in dependency order, not declaration order. The evaluator builds a dependency graph from all references within each scope and performs a topological sort:

```wcl
config {
    c = a + b    // depends on a, b
    a = 1        // no deps
    b = a + 1    // depends on a
}
// Evaluation order: a → b → c
```

If the dependency graph contains a cycle, it is an error:

```wcl
config {
    a = b + 1
    b = a + 1  // ✗ error: cyclic reference: a → b → a
}
```

---

## 11. Expressions

Expressions appear on the right-hand side of attribute assignments, variable declarations, decorator arguments, macro arguments, and within string interpolation. All expressions are evaluated during the evaluation phase to produce concrete values.

### 11.1 Expression Grammar

Listed from lowest to highest precedence:

| Precedence | Operators | Associativity | Description |
|------------|-----------|---------------|-------------|
| 1 | `? :` | Right | Ternary conditional |
| 2 | `\|\|` | Left | Logical OR |
| 3 | `&&` | Left | Logical AND |
| 4 | `==` `!=` | Left | Equality |
| 5 | `<` `>` `<=` `>=` `=~` | Left | Comparison / regex match |
| 6 | `+` `-` | Left | Addition / subtraction |
| 7 | `*` `/` `%` | Left | Multiplication / division / modulo |
| 8 | `!` `-` (unary) | Right | Logical NOT / negation |
| 9 | `.` `[` `(` | Left | Member access / index / call |
| 10 | (atoms) | — | Literals, refs, identifiers, parens |

### 11.2 Literal Expressions

```wcl
42              // int
3.14            // float
"hello"         // string
true            // bool
null            // null
svc-payments    // identifier (in appropriate context)
[1, 2, 3]      // list
{ a = 1 }      // map
```

### 11.3 Arithmetic Expressions

```wcl
a + b       // addition (int+int→int, float+float→float, int+float→float)
a - b       // subtraction
a * b       // multiplication
a / b       // division (int/int→int with truncation, float/float→float)
a % b       // modulo (int only)
-a          // unary negation
```

Type rules:
- `int OP int → int` (for `+`, `-`, `*`, `%`)
- `int / int → int` (truncating integer division)
- `float OP float → float`
- `int OP float → float` (implicit widening of the int operand)
- `float OP int → float`
- Any other type combination is a type error

Division by zero is an evaluation error.

### 11.4 Comparison Expressions

```wcl
a == b      // equality
a != b      // inequality
a < b       // less than
a > b       // greater than
a <= b      // less than or equal
a >= b      // greater than or equal
a =~ "pat"  // regex match (string =~ string → bool)
```

Comparison rules:
- Equality (`==`, `!=`) works on all types; values must be the same type or it is an error
- Ordering (`<`, `>`, `<=`, `>=`) works on `int`, `float`, and `string` (lexicographic)
- Regex match (`=~`) requires left operand `string` and right operand `string` (a valid regex pattern)
- All comparison expressions evaluate to `bool`

### 11.5 Logical Expressions

```wcl
a && b      // logical AND (bool && bool → bool)
a || b      // logical OR  (bool || bool → bool)
!a          // logical NOT (! bool → bool)
```

Both operands MUST be `bool`. Short-circuit evaluation applies: if the left operand of `&&` is `false`, the right operand is not evaluated. If the left operand of `||` is `true`, the right operand is not evaluated.

### 11.6 Ternary Conditional

```wcl
condition ? then_value : else_value
```

The condition MUST evaluate to `bool`. The then and else branches may be any expression. The result type is the common type of the two branches (both must be the same type, or one may be `null`).

```wcl
replicas = env == "prod" ? 3 : 1
suffix   = debug ? "-debug" : ""
```

### 11.7 Member Access

```wcl
config.server.port      // access attribute 'port' in nested block
ref(svc-auth).port      // access attribute on a referenced block
my_map.key              // access map value by key
```

### 11.8 Index Access

```wcl
my_list[0]              // access list element by index
my_map["key"]           // access map value by string key
```

Index out of bounds on a list is an evaluation error. Accessing a non-existent key on a map is an evaluation error (use `has()` to check first).

### 11.9 Function Calls

```wcl
upper("hello")          // built-in function call
len([1, 2, 3])          // returns 3
query(service | .port)  // query function
ref(svc-auth)           // reference function
import_raw("./q.sql")   // raw file import
```

See Section 14 for the full list of built-in functions.

### 11.10 Lambda Expressions and User-Defined Functions

Lambda expressions define anonymous functions that can be used inline or stored in variables as user-defined functions.

#### 11.10.1 Syntax

```
lambda = param_list "=>" expression
param_list = IDENT
           | "(" IDENT ("," IDENT)* ")"
```

A single-parameter lambda omits the parentheses. Multi-parameter lambdas require parentheses:

```wcl
// Single parameter — no parens needed
x => x * 2

// Multiple parameters — parens required
(a, b) => a + b

// No parameters — empty parens
() => 42
```

#### 11.10.2 Inline Usage

Lambdas can be used inline as arguments to higher-order built-in functions:

```wcl
let doubled = map([1, 2, 3], x => x * 2)
let all_positive = every([1, 2, 3], x => x > 0)
let urls = map(query(service | .port), p => "http://localhost:${p}")
let total = reduce([1, 2, 3, 4], 0, (acc, x) => acc + x)
```

#### 11.10.3 User-Defined Functions

Lambdas are first-class values and can be stored in `let` bindings to create reusable user-defined functions. Once bound to a variable, they are called with the same syntax as built-in functions:

```wcl
// Define functions
let double = x => x * 2
let add = (a, b) => a + b
let clamp = (value, lo, hi) => min(max(value, lo), hi)
let greet = name => "Hello, ${name}!"

// Call them like built-in functions
port       = double(4000)              // 8000
total      = add(base_port, offset)    // base_port + offset
safe_port  = clamp(port, 1024, 65535)  // bounded
welcome    = greet("Alice")            // "Hello, Alice!"
```

#### 11.10.4 Functions Calling Functions

User-defined functions can call other user-defined functions and built-in functions:

```wcl
let square = x => x * x
let distance = (x1, y1, x2, y2) => {
    let dx = x2 - x1
    let dy = y2 - y1
    sqrt(square(dx) + square(dy))
}

let normalize_port = port => clamp(port, 1024, 65535)
let service_url = (name, port) => "https://${name}.internal:${normalize_port(port)}"

config "api" {
    url = service_url("api", 443)
}
```

Note: The body of a multi-statement lambda uses block expression syntax `{ ... }` where intermediate `let` bindings are allowed, and the final expression is the return value.

#### 11.10.5 Functions as Arguments

User-defined functions can be passed as arguments to higher-order functions:

```wcl
let is_critical = svc => contains(svc.tags, "critical")
let critical_services = filter(query(service), is_critical)

let to_url = svc => "https://${svc.name}.internal:${svc.port}"
let all_urls = map(query(service | .env == "prod"), to_url)

// Composing transformations
let process = x => double(add(x, 10))
let results = map([1, 2, 3], process)  // [22, 24, 26]
```

#### 11.10.6 Functions in Block Bodies

User-defined functions can be defined at any scope level — module-level, inside blocks, or inside for loops. They follow the same scoping rules as `let` bindings (see Section 10):

```wcl
// Module-level function — available everywhere
let port_for = (base, offset) => base + offset

config "services" {
    // Block-level function — only visible in this block
    let make_url = (name, port) => "https://${name}:${port}"

    api_url    = make_url("api", port_for(8000, 1))
    admin_url  = make_url("admin", port_for(8000, 2))
}
```

#### 11.10.7 Functions from Imports

User-defined functions can be shared across files using `export let`. Since exported variables are merged into the importing document's scope, exported functions are called directly by name:

```wcl
// helpers.wcl
export let format_port = (host, port) => "${host}:${port}"
export let make_endpoint = (name, port, path) => "https://${format_port(name, port)}${path}"

let internal_helper = x => x * 2  // private — not available to importers
```

```wcl
// main.wcl
import "./helpers.wcl"

config {
    // Directly available — no prefix needed
    endpoint = make_endpoint("api", 8080, "/v1")
}
```

Functions defined with private `let` are NOT available to the importing document. Use `export let` to share functions.

For re-exporting functions from upstream files through a "prelude" pattern:

```wcl
// prelude.wcl — re-exports useful functions from multiple files
import "./string-helpers.wcl"
import "./math-helpers.wcl"
export format_port       // from string-helpers.wcl
export make_endpoint     // from string-helpers.wcl
export clamp             // from math-helpers.wcl
```

Note: Exported variables (including functions) are erased before serde deserialization. They exist only during the WCL evaluation phase.

#### 11.10.8 Limitations

- **No recursion**: A function cannot call itself, directly or indirectly. This is detected during evaluation as a cyclic dependency and produces an error. WCL is a configuration language, not a general-purpose runtime.
- **No mutation**: Functions are pure — they cannot modify variables or state. Each call produces a new value.
- **No variadic arguments**: Functions have a fixed number of parameters. Use a list argument if you need variable arity.
- **No default parameters**: Unlike macros, lambda parameters do not support defaults. All arguments must be provided at the call site.
- **No type annotations on parameters**: Lambda parameters are untyped. Type checking happens at the call site based on the expressions passed in and how they're used in the body.

#### 11.10.9 Block Expression Syntax

For functions that need intermediate computation, block expression syntax is supported. The block is delimited by `{ }` and may contain `let` bindings. The final expression in the block is the return value:

```wcl
let hypotenuse = (a, b) => {
    let a2 = a * a
    let b2 = b * b
    sqrt(a2 + b2)
}

let classify_port = port => {
    let is_privileged = port < 1024
    let is_registered = port >= 1024 && port < 49152
    is_privileged ? "privileged" : (is_registered ? "registered" : "dynamic")
}
```

Block expressions are only valid as the body of a lambda — they cannot appear in arbitrary expression positions.

---

## 12. Control Flow Structures

WCL provides two block-level control flow structures: `for` loops for generating repeated content from iterable expressions, and `if/else` conditionals for including or excluding sections based on a boolean expression. Both are declarative — they expand into concrete AST nodes during the evaluation phase and are fully erased before serde deserialization.

### 12.1 For Loops

#### 12.1.1 Syntax

```
for_loop = "for" IDENT [ "," IDENT ] "in" expression "{" body "}"
```

The `for` loop iterates over a list (or the result of a query, range, or any expression that evaluates to a list) and expands its body once per element, with the iterator variable bound to the current element.

#### 12.1.2 Basic Usage

```wcl
let regions = ["ap-southeast-2", "us-east-1", "eu-west-1"]

for region in regions {
    service svc-api-${region} "api-${region}" {
        region   = region
        port     = 8080
        replicas = 3
    }
}

// Expands to three service blocks:
// service svc-api-ap-southeast-2 "api-ap-southeast-2" { region = "ap-southeast-2" ... }
// service svc-api-us-east-1 "api-us-east-1" { region = "us-east-1" ... }
// service svc-api-eu-west-1 "api-eu-west-1" { region = "eu-west-1" ... }
```

#### 12.1.3 Index Variable

An optional second variable captures the zero-based index:

```wcl
let services = ["auth", "payments", "gateway"]
let base_port = 8000

for svc, idx in services {
    service svc-${svc} "${svc}" {
        port = base_port + idx
        name = svc
    }
}

// Produces:
// service svc-auth     "auth"     { port = 8000, name = "auth" }
// service svc-payments "payments" { port = 8001, name = "payments" }
// service svc-gateway  "gateway"  { port = 8002, name = "gateway" }
```

#### 12.1.4 Iterating Over Expressions

The `in` clause accepts any expression that evaluates to a list. This includes query results, ranges, function results, and variable references:

```wcl
// Iterate over a range
for i in range(0, 5) {
    worker worker-${i} {
        id_num = i
    }
}

// Iterate over query results
for svc in query(service | .env == "prod") {
    alert alert-${svc.name} {
        target   = svc.name
        port     = svc.port
        severity = "critical"
    }
}

// Iterate over map keys
let env_ports = { dev = 3000, staging = 4000, prod = 8000 }

for env in keys(env_ports) {
    listener listener-${env} {
        port = env_ports[env]
        env  = env
    }
}
```

#### 12.1.5 Nested For Loops

For loops can be nested. Each level introduces its own iterator variable in a new scope:

```wcl
let regions = ["ap-southeast-2", "us-east-1"]
let tiers   = ["web", "api", "worker"]

for region in regions {
    for tier in tiers {
        service svc-${region}-${tier} "${region}-${tier}" {
            region = region
            tier   = tier
            port   = 8080
        }
    }
}
// Produces 6 service blocks (2 regions × 3 tiers)
```

#### 12.1.6 For Loops Inside Blocks

For loops can appear inside block bodies to generate child content:

```wcl
let endpoints = [
    { name = "health", path = "/healthz", method = "GET" },
    { name = "ready",  path = "/readyz",  method = "GET" },
    { name = "metrics", path = "/metrics", method = "GET" },
]

service svc-api "api" {
    port = 8080

    for ep in endpoints {
        route route-${ep.name} {
            path   = ep.path
            method = ep.method
        }
    }
}
```

#### 12.1.7 For Loops with Macros

For loops compose naturally with macros — the loop body can contain macro calls:

```wcl
macro service_endpoint(name, port) {
    endpoint "${name}" {
        port = port
        health_check {
            path     = "/health/${name}"
            interval = 15
        }
    }
}

let services = [
    { name = "auth",     port = 8001 },
    { name = "payments", port = 8002 },
    { name = "billing",  port = 8003 },
]

config "gateway" {
    for svc in services {
        service_endpoint(svc.name, svc.port)
    }
}
```

#### 12.1.8 For Loop Scoping

Each iteration of a `for` loop creates a new child scope. The iterator variable (and optional index variable) are `let` bindings in that scope:

- Iterator variables shadow outer bindings of the same name (with no shadowing warning — this is expected behavior for loop variables)
- Variables declared with `let` inside the loop body are scoped to that single iteration
- The loop body can reference variables from enclosing scopes

```wcl
let name = "outer"

for name in ["a", "b", "c"] {
    // 'name' here is the iterator, not "outer" — no shadowing warning
    let full_name = "svc-${name}"  // scoped to this iteration

    service svc-${name} {
        label = full_name
    }
}
// 'name' is "outer" again here
```

#### 12.1.9 For Loop Expansion

For loops expand during the **control flow expansion** phase, which occurs after macro expansion and before scope construction of the final document. The loop expression is evaluated, and the body is replicated once per element with the iterator bound.

If the expression does not evaluate to a list, it is an evaluation error. An empty list produces zero iterations (the loop body is not emitted).

### 12.2 Conditional Blocks (if/else)

#### 12.2.1 Syntax

```
conditional = "if" expression "{" body "}" [ "else" ( conditional | "{" body "}" ) ]
```

Conditional blocks include or exclude sections of the document based on a boolean expression.

#### 12.2.2 Basic Usage

```wcl
let env = "prod"

if env == "prod" {
    service svc-monitoring "monitoring" {
        port     = 9090
        replicas = 3
    }
}

if env == "dev" {
    service svc-debug "debug-tools" {
        port  = 9999
        debug = true
    }
}
```

#### 12.2.3 If/Else

```wcl
let env = "staging"

if env == "prod" {
    tls {
        cert_path   = "/etc/certs/prod.pem"
        min_version = "1.3"
    }
} else if env == "staging" {
    tls {
        cert_path   = "/etc/certs/staging.pem"
        min_version = "1.2"
    }
} else {
    // Dev — no TLS
    debug_mode = true
}
```

#### 12.2.4 If/Else Inside Block Bodies

Conditionals can appear inside block bodies to selectively include attributes or child blocks:

```wcl
let enable_monitoring = true
let enable_tls = true
let env = "prod"

service svc-api "api" {
    port = 8080
    env  = env

    if enable_monitoring {
        monitoring {
            enabled   = true
            interval  = 15
            threshold = 0.99
        }
    }

    if enable_tls {
        tls {
            cert_path   = "/etc/certs/api.pem"
            min_version = "1.3"
        }
    }

    if env == "prod" {
        replicas = 5
        priority = "critical"
    } else {
        replicas = 1
        priority = "low"
    }
}
```

#### 12.2.5 Conditionals with Queries

Conditionals can use query results as their condition:

```wcl
if len(query(service | .env == "prod")) > 0 {
    load_balancer lb-prod "production" {
        algorithm = "round_robin"
        targets   = query(service | .env == "prod" | .port)
    }
}

if some(query(service), s => !has(s, "tls")) {
    @warning
    validation "tls_coverage" {
        check   = false
        message = "Some services are missing TLS configuration"
    }
}
```

#### 12.2.6 Conditionals with For Loops

For loops and conditionals compose freely:

```wcl
let services = [
    { name = "auth",     port = 8001, critical = true },
    { name = "payments", port = 8002, critical = true },
    { name = "debug",    port = 9999, critical = false },
]

for svc in services {
    service svc-${svc.name} "${svc.name}" {
        port = svc.port

        if svc.critical {
            @with_monitoring(threshold = 0.99)
            replicas = 3

            tls {
                cert_path = "/etc/certs/${svc.name}.pem"
            }
        } else {
            replicas = 1
        }
    }
}
```

#### 12.2.7 Conditional Expression Requirement

The condition expression MUST evaluate to `bool`. Non-boolean values are NOT implicitly coerced to boolean — this is a type error:

```wcl
let count = 5

if count {        // ✗ error: condition must be bool, got int
    // ...
}

if count > 0 {    // ✓ correct — explicit boolean expression
    // ...
}
```

#### 12.2.8 Conditional Expansion

Like for loops, conditional blocks expand during the **control flow expansion** phase. The condition is evaluated, and only the matching branch's body is included in the resulting AST. The other branch is discarded entirely — it does not participate in scope construction, evaluation, or validation.

This means:

- Attributes and blocks in the discarded branch do not need to be valid (e.g., they can reference undefined variables)
- Schemas are only validated against the included branch
- Queries do not see blocks from discarded branches

### 12.3 Control Flow in Attribute Macro Transform Bodies

For loops and conditionals are also available inside attribute macro `inject` blocks:

```wcl
macro @with_replicas(count: int = 3, regions: list(string) = ["ap-southeast-2"]) {
    set {
        replica_count = count
    }

    inject {
        for region in regions {
            replica replica-${region} {
                region = region
                count  = count
            }
        }
    }

    when count > 5 {
        set {
            needs_coordinator = true
        }
    }
}
```

### 12.4 Identifier Interpolation in For Loops

Within `for` loop bodies, iterator variables can be used in inline ID positions and block labels via `${}` interpolation. This is a special extension of string interpolation that applies to identifier literals:

```wcl
for name in ["auth", "payments"] {
    service svc-${name} "${name}" {
        port = 8080
    }
}
```

The interpolated identifier is validated after expansion — the resulting string MUST conform to IDENTIFIER_LIT rules (start with letter/underscore, contain only letters, digits, underscores, hyphens). If the expanded value produces an invalid identifier, it is an error:

```wcl
for name in ["auth", "pay ments"] {
    service svc-${name} {    // ✗ error on second iteration: 'svc-pay ments'
        port = 8080          //   contains a space, invalid identifier
    }
}
```

### 12.5 Depth and Iteration Limits

To prevent pathological expansion:

- **Nesting depth**: For loops and conditionals can nest up to a configurable maximum depth (default: 32).
- **Total iterations**: The total number of iterations across all for loops in a document is capped (default: 10,000). This prevents `for i in range(0, 1000000)` from generating a million blocks.
- **Per-loop limit**: Each individual for loop is capped at a configurable maximum (default: 1,000 iterations).

Exceeding these limits is an evaluation error with a diagnostic pointing to the offending loop.

### 12.6 AST Representation

```rust
#[derive(Debug, Clone)]
struct ForLoop {
    /// Iterator variable name
    iterator: Ident,
    /// Optional index variable name
    index: Option<Ident>,
    /// Expression that evaluates to a list
    iterable: Expr,
    /// Body to expand per iteration
    body: Vec<BodyItem>,
    trivia: Trivia,
    span: Span,
}

#[derive(Debug, Clone)]
struct Conditional {
    /// The condition expression (must evaluate to bool)
    condition: Expr,
    /// Body included when condition is true
    then_body: Vec<BodyItem>,
    /// Optional else branch — either another Conditional (else if) or a body (else)
    else_branch: Option<ElseBranch>,
    trivia: Trivia,
    span: Span,
}

#[derive(Debug, Clone)]
enum ElseBranch {
    /// else if condition { ... }
    ElseIf(Box<Conditional>),
    /// else { ... }
    Else(Vec<BodyItem>, Trivia, Span),
}
```

---

## 13. String Interpolation

### 13.1 Syntax

Within double-quoted strings and heredocs (non-raw), `${...}` sequences are evaluated as expressions and their result is converted to a string representation:

```wcl
name = "Alice"
greeting = "Hello, ${name}!"                    // "Hello, Alice!"
url = "https://${host}:${port}/api"             // expression references
calc = "result is ${2 + 2}"                     // arithmetic
cond = "mode: ${debug ? "debug" : "release"}"   // ternary
```

### 13.2 Nesting

Interpolation expressions may contain further string literals with their own interpolation:

```wcl
msg = "outer ${inner == true ? "yes-${suffix}" : "no"} end"
```

### 13.3 Escaping

To include a literal `${` in a string without triggering interpolation, escape the dollar sign:

```wcl
literal = "This is a literal \${not_interpolated}"
```

### 13.4 Type Coercion in Interpolation

Values interpolated into strings are converted using the following rules:

- `string` → inserted as-is
- `int` → decimal string representation
- `float` → decimal string representation (implementation-defined precision)
- `bool` → `"true"` or `"false"`
- `null` → `"null"`
- `identifier` → the identifier's string value
- `list`, `map`, `block_ref`, `function` → error; use explicit formatting functions

---

## 14. Built-in Functions

WCL provides a set of built-in functions available in all expression contexts. These functions have no side effects and are deterministic.

### 14.1 String Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `upper(s)` | `string → string` | Convert to uppercase |
| `lower(s)` | `string → string` | Convert to lowercase |
| `trim(s)` | `string → string` | Remove leading/trailing whitespace |
| `trim_prefix(s, prefix)` | `string, string → string` | Remove prefix if present |
| `trim_suffix(s, suffix)` | `string, string → string` | Remove suffix if present |
| `replace(s, old, new)` | `string, string, string → string` | Replace all occurrences |
| `split(sep, s)` | `string, string → list(string)` | Split string by separator |
| `join(sep, list)` | `string, list(string) → string` | Join list elements with separator |
| `starts_with(s, prefix)` | `string, string → bool` | Check if string starts with prefix |
| `ends_with(s, suffix)` | `string, string → bool` | Check if string ends with suffix |
| `contains(s, substr)` | `string, string → bool` | Check if string contains substring |
| `length(s)` | `string → int` | String length in characters |
| `substr(s, start, end?)` | `string, int, int? → string` | Extract substring |
| `format(fmt, args...)` | `string, any... → string` | Format string with positional args |
| `regex_match(s, pattern)` | `string, string → bool` | Test regex match |
| `regex_capture(s, pattern)` | `string, string → list(string)` | Return capture groups |

### 14.2 Math Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `abs(n)` | `int\|float → int\|float` | Absolute value |
| `min(a, b)` | `int\|float, int\|float → int\|float` | Minimum of two values |
| `max(a, b)` | `int\|float, int\|float → int\|float` | Maximum of two values |
| `floor(n)` | `float → int` | Floor (round down) |
| `ceil(n)` | `float → int` | Ceiling (round up) |
| `round(n)` | `float → int` | Round to nearest integer |
| `sqrt(n)` | `int\|float → float` | Square root |
| `pow(base, exp)` | `int\|float, int\|float → float` | Exponentiation |

### 14.3 Collection Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `len(c)` | `list\|map\|set → int` | Number of elements |
| `keys(m)` | `map → list(string)` | Map keys as list |
| `values(m)` | `map → list(any)` | Map values as list |
| `flatten(l)` | `list(list(T)) → list(T)` | Flatten one level of nesting |
| `concat(l1, l2)` | `list(T), list(T) → list(T)` | Concatenate two lists |
| `distinct(l)` | `list(T) → list(T)` | Remove duplicates, preserve order |
| `sort(l)` | `list(T) → list(T)` | Sort (ascending, natural order) |
| `reverse(l)` | `list(T) → list(T)` | Reverse a list |
| `contains(l, v)` | `list(T), T → bool` | Check if list contains value |
| `index_of(l, v)` | `list(T), T → int` | Index of first occurrence, -1 if absent |
| `range(start, end, step?)` | `int, int, int? → list(int)` | Generate integer range |
| `zip(l1, l2)` | `list(A), list(B) → list(list(any))` | Pair elements from two lists |

### 14.4 Higher-Order Collection Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `map(l, fn)` | `list(T), (T → U) → list(U)` | Transform each element |
| `filter(l, fn)` | `list(T), (T → bool) → list(T)` | Keep elements where fn is true |
| `every(l, fn)` | `list(T), (T → bool) → bool` | True if fn is true for all elements |
| `some(l, fn)` | `list(T), (T → bool) → bool` | True if fn is true for any element |
| `reduce(l, init, fn)` | `list(T), U, (U, T → U) → U` | Fold list to single value |

### 14.5 Aggregate Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `sum(l)` | `list(int\|float) → int\|float` | Sum of all elements |
| `avg(l)` | `list(int\|float) → float` | Average of all elements |
| `min_of(l)` | `list(int\|float) → int\|float` | Minimum element |
| `max_of(l)` | `list(int\|float) → int\|float` | Maximum element |
| `count(l, fn)` | `list(T), (T → bool) → int` | Count elements matching predicate |

### 14.6 Hash and Encoding Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `sha256(s)` | `string → string` | SHA-256 hex digest |
| `base64_encode(s)` | `string → string` | Base64 encode |
| `base64_decode(s)` | `string → string` | Base64 decode |
| `json_encode(v)` | `any → string` | Serialize value to JSON string |

### 14.7 Type Coercion Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `to_string(v)` | `any → string` | Convert to string representation |
| `to_int(v)` | `string\|float\|bool → int` | Convert to integer |
| `to_float(v)` | `string\|int → float` | Convert to float |
| `to_bool(v)` | `string\|int → bool` | Convert to boolean |
| `type_of(v)` | `any → string` | Return type name as string |

### 14.8 File Utility Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `import_table(path, separator?)` | `string, string? → table_rows` | Import tabular data from CSV/TSV |
| `import_raw(path)` | `string → string` | Import raw file content as string |

Note: WCL file imports use the top-level `import` directive, not an expression-level function. See Section 19 for full import semantics. The `import_table` and `import_raw` functions are expression-level utilities for importing non-WCL data.

### 14.9 Reference and Query Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `ref(id)` | `identifier → block_ref` | Resolve an identifier to a block reference |
| `query(pipeline)` | `pipeline → list(block_ref)\|list(value)` | Execute a query pipeline |
| `has(v, name)` | `block_ref, string → bool` | Check if block has an attribute |
| `has_decorator(v, name)` | `block_ref, string → bool` | Check if block has a decorator |

See Section 22 for full query semantics.

---

## 15. Decorators

Decorators are annotations that attach metadata to blocks, attributes, or tables. They are syntactically prefixed with `@` and appear on the line(s) immediately before the item they annotate.

### 15.1 Syntax

```
decorator = "@" IDENT decorator_args?
decorator_args = "(" arg_list ")"
arg_list = positional_args ("," named_args)?
         | named_args
positional_args = expression ("," expression)*
named_args = IDENT "=" expression ("," IDENT "=" expression)*
```

### 15.2 Decorator Forms

```wcl
// Marker decorator (no arguments)
@sensitive
password = "secret123"

// Positional argument
@deprecated("use new_field instead")
old_field = 42

// Named arguments
@validate(min = 1, max = 65535)
port = 8080

// Mixed positional and named
@deprecated("use v2", since = "1.4.0")
old_endpoint = "/api/v1"

// Multiple decorators stacked
@deprecated("use tls block")
@sensitive
@validate(pattern = "^[A-Za-z0-9]+$")
legacy_token = "abc123"
```

### 15.3 Decorator Targets

Decorators can be applied to:
- **Attributes**: `@sensitive password = "..."` — decorates the attribute
- **Blocks**: `@deprecated service svc-old { }` — decorates the block
- **Tables**: `@table_index(columns = ["role"]) table "perms" { }` — decorates the table
- **Schema fields**: Used within schema blocks to specify constraints

A decorator schema defines which targets it is valid for (see Section 16).

### 15.4 Decorator Evaluation

Decorator arguments are expressions and are evaluated during the expression evaluation phase. This means decorator arguments can reference variables and use expressions:

```wcl
let min_port = 1024

@validate(min = min_port, max = 65535)
port = 8080
```

### 15.5 Built-in Decorators

WCL ships with the following built-in decorators. Their schemas are always available and do not need to be imported.

| Decorator | Targets | Arguments | Description |
|-----------|---------|-----------|-------------|
| `@optional` | Schema fields | None | Field is not required |
| `@required` | Schema fields | None | Field is required (default for schema fields) |
| `@default(value)` | Schema fields | `value: any` | Default value if not specified |
| `@sensitive` | Attributes | `redact_in_logs: bool = true` | Marks value as sensitive for tooling |
| `@deprecated(msg, since?)` | Blocks, Attributes | `message: string, since: string?` | Marks as deprecated |
| `@validate(...)` | Attributes | `min, max, pattern, one_of, custom_msg` | Value constraints |
| `@doc(text)` | Any | `text: string` | Documentation (alternative to `///`) |
| `@example { ... }` | Decorator schemas, Schemas | Block body | Embedded usage example |
| `@allow(rule)` | Let bindings, Attributes | `rule: string` | Suppress a specific warning |
| `@id_pattern(glob)` | Schema `id` fields | `pattern: string` | Naming convention for IDs |
| `@ref(schema)` | Schema fields of type `identifier` | `schema: string` | Reference must resolve to given schema |
| `@partial_requires(fields)` | Partial blocks | `fields: list(string)` | Declare expected merge dependencies |
| `@merge_order(n)` | Partial blocks | `order: int` | Explicit ordering for partial merge |

---

## 16. Decorator Schemas

Decorator schemas define the valid structure of decorators — what arguments they accept, their types, defaults, and what AST nodes they can be applied to. Custom decorators MUST have a corresponding decorator schema or they are an error.

### 16.1 Syntax

```wcl
decorator_schema "name" {
    target = [target_list]

    param_name = type decorator*
    ...

    @constraint(...)
    @example { ... }
}
```

### 16.2 Examples

```wcl
decorator_schema "deprecated" {
    target = [block, attribute]

    message = string
    since   = string @optional

    @example {
        @deprecated("use v2", since = "1.4.0")
    }
}

decorator_schema "validate" {
    target = [attribute]

    min        = float   @optional
    max        = float   @optional
    pattern    = string  @optional
    one_of     = list(any) @optional
    custom_msg = string  @optional

    // At least one validation field required
    @constraint(any_of = ["min", "max", "pattern", "one_of"])
}

decorator_schema "sensitive" {
    target = [attribute]
    redact_in_logs = bool @optional @default(true)
}

decorator_schema "table_index" {
    target = [table]
    columns = list(string)
    unique  = bool @optional @default(false)
}
```

### 16.3 Target Types

The `target` attribute in a decorator schema accepts a list of the following values:

- `block` — can be applied to block declarations
- `attribute` — can be applied to attribute assignments
- `table` — can be applied to table declarations
- `schema` — can be applied to schema blocks

### 16.4 Parameter Definitions

Each attribute in a decorator schema body (other than `target`) defines a parameter:

- The attribute name is the parameter name
- The value is the expected type
- Decorators on the parameter define constraints: `@optional`, `@default(value)`
- Parameters without `@optional` are required

### 16.5 Positional Parameter Mapping

The first non-optional parameter in declaration order may be supplied positionally:

```wcl
decorator_schema "deprecated" {
    target  = [block, attribute]
    message = string            // first param → positional
    since   = string @optional  // second param → named only
}

// Both valid:
@deprecated("use v2")                          // positional
@deprecated(message = "use v2", since = "1.0") // all named
@deprecated("use v2", since = "1.0")           // mixed
```

### 16.6 Constraints

The `@constraint` decorator on the schema body defines cross-parameter constraints:

- `@constraint(any_of = ["a", "b", "c"])` — at least one of the listed parameters must be provided
- `@constraint(all_of = ["a", "b"])` — all listed parameters must be provided together
- `@constraint(one_of = ["a", "b"])` — exactly one of the listed parameters must be provided
- `@constraint(requires = { "a" = ["b", "c"] })` — if `a` is provided, `b` and `c` must also be provided

### 16.7 Validation

During the decorator validation phase, every decorator in the document is checked against its schema:

1. The decorator name must correspond to a built-in or user-defined decorator schema.
2. The decorator must be applied to a valid target type.
3. All required parameters must be provided.
4. All provided parameters must match their expected types.
5. No unknown parameters may be present.
6. All constraints must be satisfied.

Validation errors are collected (not fail-fast) and reported as diagnostics.

---

## 17. Schemas and Validation

Schemas define the expected structure and types of blocks. They enable type checking, required field validation, and documentation generation.

### 17.1 Syntax

```wcl
schema "name" {
    field_name = type decorator*
    ...
}
```

### 17.2 Examples

```wcl
schema "service" {
    @id_pattern("svc-*")
    id = identifier

    port       = int      @required @validate(min = 1, max = 65535)
    region     = string   @required
    env        = string   @optional @default("dev")
    tags       = list(string) @optional @default([])
    replicas   = int      @optional @default(1) @validate(min = 1, max = 100)
    debug      = bool     @optional @default(false)
    monitoring = ref("monitoring") @optional
}

schema "monitoring" {
    enabled         = bool  @required
    interval        = int   @optional @default(60) @validate(min = 1)
    alert_threshold = float @optional @default(0.95) @validate(min = 0.0, max = 1.0)
}

schema "dependency" {
    target_service = identifier @required @ref("service")
    priority       = int @optional @default(0)
}
```

### 17.3 Schema Application

Schemas are applied to blocks by matching the block type name to the schema name:

```wcl
schema "service" {
    port = int @required
}

// This block is validated against schema "service"
service svc-api "api" {
    port = 8080  // ✓ valid
}

service svc-broken "broken" {
    // ✗ error: missing required field 'port' (defined in schema "service")
}
```

### 17.4 Nested Schema References

Schemas can reference other schemas using `ref("schema_name")`:

```wcl
schema "service" {
    port   = int @required
    health = ref("health_check") @optional
}

schema "health_check" {
    path     = string @required
    interval = int    @optional @default(30)
}
```

### 17.5 Schema Validation Timing

Schemas validate the **fully resolved** document — after import resolution, macro expansion, partial merging, and expression evaluation. This means:

- Partial blocks are validated as a merged whole, not individually
- Expression results are validated, not the expressions themselves
- Variables are already erased and are not subject to schema validation

### 17.6 Validation Error Collection

Schema validation is accumulative — all errors are collected and reported together, not fail-fast:

```wcl
service svc-broken "broken" {
    // All of these errors are reported at once:
    port     = "not a number"   // ✗ expected int, got string
    replicas = -1               // ✗ validate: min is 1
    // ✗ missing required field: region
}
```

### 17.7 Schema Inheritance

WCL does not support schema inheritance. Composition is achieved through `ref()` types and partial declarations. If common fields are needed across schemas, extract them into a separate schema and reference it.

### 17.8 Open vs Closed Schemas

By default, schemas are **closed** — attributes not defined in the schema are not allowed:

```wcl
schema "config" {
    port = int @required
}

config {
    port  = 8080
    debug = true  // ✗ error: unknown attribute 'debug' in schema "config"
}
```

To allow additional attributes, use the `@open` decorator on the schema:

```wcl
@open
schema "config" {
    port = int @required
}

config {
    port  = 8080
    debug = true  // ✓ allowed — schema is open
}
```

---

## 18. Data Tables

Tables provide a compact syntax for tabular data within WCL documents. They are particularly useful for configuration data that is naturally expressed as rows and columns: permissions, routes, mappings, etc.

### 18.1 Syntax

Table column declarations use decorators and MUST always include explicit types. Columns are declared as individual typed attributes with decorators, not as a bare list:

```
table = decorator* ["partial"] "table" [IDENTIFIER_LIT] STRING_LIT* "{" table_body "}"
table_body = column_decl+ table_row*
column_decl = decorator* IDENT ":" type_expr
table_row = "|" expression { "|" expression } "|"
```

```wcl
table "name" {
    col1 : type1
    col2 : type2
    col3 : type3

    | value1 | value2 | value3 |
    | value1 | value2 | value3 |
}
```

### 18.2 Examples

```wcl
table "permissions" {
    role     : string
    resource : string
    action   : string
    allow    : bool

    | "admin"  | "*"       | "*"     | true  |
    | "viewer" | "reports" | "read"  | true  |
    | "viewer" | "reports" | "write" | false |
    | "editor" | "reports" | "read"  | true  |
    | "editor" | "reports" | "write" | true  |
}
```

### 18.3 Column Declarations

Each column is declared as an IDENT followed by `:` and a type expression. Column declarations define:

- The column name (used as the attribute name when deserializing rows)
- The column type (used for validation of every cell in that column)
- Optional decorators for additional metadata and constraints

Column declarations MUST appear before any row data. The number of values in each row MUST match the number of declared columns.

### 18.4 Column Decorators

Columns support the full decorator system, enabling per-column validation, documentation, and metadata:

```wcl
table "users" {
    @doc("Unique user identifier")
    @validate(pattern = "^[a-z][a-z0-9_]{2,30}$")
    username : string

    @doc("Display name shown in UI")
    display_name : string

    @validate(min = 0, max = 150)
    age : int

    @doc("User role in the system")
    @validate(one_of = ["admin", "editor", "viewer", "guest"])
    role : string

    @sensitive
    @doc("Hashed password — never log or display")
    password_hash : string

    @default(true)
    active : bool

    | "alice"   | "Alice Admin"    | 32  | "admin"  | "h$2b$12$abc..." | true  |
    | "bob"     | "Bob Builder"    | 28  | "editor" | "h$2b$12$def..." | true  |
    | "charlie" | "Charlie Reader" | 45  | "viewer" | "h$2b$12$ghi..." | false |
}
```

Decorators on columns are validated against their decorator schemas just like decorators on block attributes. The `@validate` decorator's constraints (min, max, pattern, one_of) are checked against every cell value in that column after expression evaluation.

### 18.5 Row Values

Each row is delimited by `|` characters. Values between pipes are expressions — they can be literals, variable references, or simple expressions:

```wcl
let default_role = "viewer"

table "users" {
    name   : string
    role   : string
    active : bool

    | "Alice"   | "admin"      | true  |
    | "Bob"     | default_role | true  |
    | "Charlie" | default_role | false |
}
```

### 18.6 Row Count Validation

The number of cell values in each row MUST exactly match the number of declared columns. A mismatch is a parse error:

```wcl
table "example" {
    a : int
    b : string
    c : bool

    | 1 | "hello" | true  |   // ✓ 3 values, 3 columns
    | 2 | "world"        |    // ✗ error: expected 3 values, got 2
    | 3 | "x" | true | 99 |   // ✗ error: expected 3 values, got 4
}
```

### 18.7 Type Validation

Every cell value is validated against the declared column type after expression evaluation. Type mismatches are reported as diagnostics with spans pointing to the offending cell:

```wcl
table "config" {
    name  : string
    port  : int
    debug : bool

    | "api"     | 8080    | true    |   // ✓ all types match
    | "worker"  | "oops"  | false   |   // ✗ error: column 'port' expected int, got string
}
```

### 18.8 Table Deserialization

Tables deserialize into `Vec<T>` where `T` is a struct matching the column names and types:

```rust
#[derive(Deserialize)]
struct Permission {
    role: String,
    resource: String,
    action: String,
    allow: bool,
}

// In serde: table "permissions" → Vec<Permission>
```

### 18.9 Tables in Queries

Tables can be queried like blocks, with column names acting as attribute names:

```wcl
let admin_perms = query(table."permissions" | .role == "admin")
let writable = query(table."permissions" | .action == "write" | .allow == true)
```

### 18.10 Inline ID on Tables

Tables may have an inline ID for use in references and queries:

```wcl
table perms-main "permissions" {
    role     : string
    resource : string
    action   : string
    allow    : bool

    | "admin" | "*" | "*" | true |
}

let main_perms = query(table#perms-main)
```

---

## 19. Import System

WCL supports composing documents from multiple files via top-level `import` directives. Imports are **top-level only** — they cannot appear inside blocks, expressions, or other nested contexts. Each imported file's top-level content is merged into the importing document's AST, creating a single unified document from multiple source files.

### 19.1 Import Directive

```
import_decl = "import" STRING_LIT
```

The `import` directive takes a single string literal path and merges the referenced file's top-level content into the current document:

```wcl
import "./schemas.wcl"
import "./macros.wcl"
import "./base/payments.wcl"
import "./monitoring/payments.wcl"
import "./security/payments.wcl"

// All top-level blocks, attributes, schemas, decorator_schemas,
// macros, tables, and validations from the imported files are
// now part of this document's AST.

service svc-gateway "gateway" {
    port = 8000
    // Can directly reference blocks/schemas/macros from any imported file
}
```

Import directives MUST appear at the top level of a file (not inside blocks). They are conventionally placed at the top of the file before any other declarations, but this is not enforced — they may appear anywhere at the top level. The position of the import directive determines where the imported content is spliced into the document body (which affects declaration order for merge precedence).

### 19.2 What Gets Merged

When a file is imported, the following top-level items from the imported file are merged into the importing document:

| Item | Merged? | Notes |
|------|---------|-------|
| Blocks | Yes | Merged at the import directive's position |
| Attributes (top-level) | Yes | Merged at the import directive's position |
| Schemas | Yes | Available for validation |
| Decorator schemas | Yes | Available for decorator validation |
| Macros | Yes | Registered in the global macro registry |
| Tables | Yes | Merged at the import directive's position |
| Validations | Yes | Executed during document validation |
| Exported variables (`export let`) | Yes | Available by name in importing document's scope |
| Re-exports (`export name`) | Yes | Transitively forwards names from upstream imports |
| Private variables (`let`) | No | Private to the defining file; evaluated during import but not visible |
| Import directives | Yes (transitively) | Imported files' own imports are resolved recursively |

Private `let` variables remain file-private because they are computation scaffolding — they get evaluated within their file's scope and may be used to compute exported values, attribute values, and block content, but the private variables themselves are not visible to the importing document.

Exported variables (`export let`) are explicitly made available. They participate in the importing document's scope and can be referenced by name directly. This is the primary mechanism for sharing computed values, configuration constants, and user-defined functions across files (see Section 10.2).

### 19.3 Utility Import Functions

While the `import` directive merges WCL files, two expression-level functions exist for importing non-WCL data:

#### `import_table(path, separator?)`

Reads tabular data from a CSV or TSV file and returns it as table row data. Default separator is `,`. This is an expression and can appear anywhere an expression is valid:

```wcl
table "regions" {
    name   : string
    code   : string
    active : bool

    // Rows loaded from CSV — columns must match the table's column declarations
    import_table("./data/regions.csv")
}
```

#### `import_raw(path)`

Reads a file's content as a raw string with no parsing. Useful for embedding SQL, templates, or other non-WCL content. This is an expression and can appear anywhere a string is expected:

```wcl
query_template {
    sql = import_raw("./queries/report.sql")
}

config {
    license_text = import_raw("./LICENSE")
}
```

### 19.4 Path Resolution Rules

All import paths (both `import` directives and utility functions) follow these rules:

1. **Relative paths only**: All import paths MUST be relative. Absolute paths (`/etc/...`), home-relative paths (`~/...`), and scheme-prefixed paths (`http://...`, `file://...`) are forbidden.
2. **Resolved relative to importing file**: The path is resolved relative to the directory containing the file that contains the import.
3. **Jail to root directory**: After resolution, the canonical path MUST be within the root directory (the directory of the top-level document being parsed). Paths that traverse above the root via `../` are forbidden.
4. **No symlink escapes**: Path canonicalization resolves symlinks before the jail check.
5. **No remote imports**: Any path containing `://` is rejected. WCL does not support fetching files over the network.

### 19.5 Import-Once Semantics

WCL uses **import-once** semantics. Each file is parsed and processed at most once per document. Subsequent imports of the same file are a no-op.

This means circular imports are safe and handled gracefully:

```
a.wcl imports b.wcl
b.wcl imports c.wcl
c.wcl imports a.wcl  → no error; a.wcl is already loaded, second import is a noop
```

The import resolver maintains a cache of all files that have been loaded, keyed by their canonical path:

- **First import** of a file: the file is read, parsed, and its top-level content is merged into the document. The file is marked as loaded in the cache.
- **Subsequent imports** of the same file (whether from the same file or a different one): the import is silently skipped. No re-parsing, no duplicate merging.

This enables natural multi-file composition without worrying about import ordering or duplication:

```wcl
// types.wcl — defines shared schemas
schema "address" { }
schema "contact" { }

// schemas.wcl
import "./types.wcl"          // loads types.wcl
schema "service" { }

// macros.wcl
import "./types.wcl"          // noop — types.wcl already loaded

// main.wcl
import "./schemas.wcl"        // loads schemas.wcl, which loads types.wcl
import "./macros.wcl"         // loads macros.wcl, types.wcl import is noop
import "./types.wcl"          // also noop — already loaded

// All schemas from all files are now available
```

### 19.6 Depth Limit

Nested imports (imports within imported files) are limited to a configurable maximum depth (default: 32). This prevents pathological cases of deeply nested import chains.

### 19.7 Merge Conflicts

Since imported content merges flat into the document, naming conflicts are possible. These are handled by existing WCL rules:

- **Blocks with duplicate IDs**: If both are `partial`, they merge (see Section 20). If either is non-partial, it is a duplicate ID error.
- **Duplicate top-level attributes**: Error — same as duplicate attributes within a block.
- **Duplicate schema names**: Error — schemas must be uniquely named.
- **Duplicate decorator schema names**: Error.
- **Duplicate macro names**: Error.
- **Exported variable conflicts**: If two imported files both export a variable with the same name, it is an error. Use re-exports through a single "prelude" file to avoid this.
- **Private variable conflicts**: Not possible — private `let` variables are file-private and never merged.

To avoid conflicts when composing many files, use inline IDs and partial declarations liberally — this is the intended composition pattern:

```wcl
// team-a.wcl
partial service svc-api "api" {
    port = 8080
}

// team-b.wcl
partial service svc-api "api" {
    monitoring { interval = 15 }
}

// main.wcl
import "./team-a.wcl"
import "./team-b.wcl"
// svc-api merges cleanly via the partial system
```

### 19.8 Import Resolver Configuration

The import resolver is configurable by consumers:

```rust
ParseOptions {
    root_dir: PathBuf,              // jail root directory
    max_import_depth: u32,          // default: 32
    allow_imports: bool,            // default: true; set false for untrusted input
}
```

When `allow_imports` is `false`, any `import` directive, `import_table()`, or `import_raw()` call is an error. This is essential for scenarios where WCL is used for API payloads or user-submitted configuration.

---

## 20. Partial Declarations and Merging

Partial declarations allow a block to be defined across multiple locations (within the same file or across multiple imported files) and merged into a single unified block.

### 20.1 Syntax

A block is declared as partial by prefixing the `partial` keyword:

```wcl
partial service svc-payments "payments" {
    port = 8443
}
```

### 20.2 Merge Rules

Multiple `partial` blocks with the same inline ID within the same scope are merged. The merge follows these rules:

#### 20.2.1 Prerequisites

- All blocks being merged MUST have the `partial` keyword
- All blocks being merged MUST have the same inline ID
- All blocks being merged MUST have the same block type (kind)
- Labels SHOULD be consistent; mismatched labels produce a warning

#### 20.2.2 Attribute Merging

In the default **strict** conflict mode:
- Each attribute may be defined in at most one fragment. Duplicate attribute names across fragments are an error (except for `id`, which is expected to duplicate).

In **last-wins** conflict mode:
- Duplicate attributes are resolved by the last fragment in merge order taking precedence.

The `id` attribute (whether inline or in body) is always deduplicated silently — it must match across fragments.

#### 20.2.3 Child Block Merging

- Child blocks with inline IDs are recursively merged if they appear in multiple fragments.
- Child blocks without inline IDs are appended in merge order.

#### 20.2.4 Decorator Merging

- Decorators are merged and deduplicated by name.
- If the same decorator appears in multiple fragments with different arguments, it is an error (in strict mode) or last-wins.

#### 20.2.5 Table and Comment Merging

- Tables are appended (rows from later fragments appear after rows from earlier fragments).
- Comments are preserved from all fragments.

### 20.3 Merge Order

Merge order is determined by:

1. **`@merge_order(n)` decorator**: If present, fragments are sorted by the `n` value (ascending). Lower numbers are merged first.
2. **Import order**: If no explicit merge order, fragments are merged in the order their containing files are imported.
3. **Declaration order**: Within a single file, fragments are merged in the order they appear.

```wcl
@merge_order(1)
partial service svc-payments "payments" {
    port = 8443
}

@merge_order(2)
partial service svc-payments "payments" {
    monitoring {
        interval = 15
    }
}
```

### 20.4 Post-Merge State

After merging:
- The `partial` flag is removed from the merged block.
- The merged block is placed at the position of the first fragment.
- All other fragment positions are removed from the body.
- The merged block is subject to schema validation as a whole.

### 20.5 Partial Requirements

The `@partial_requires` decorator documents what a fragment expects other fragments to provide:

```wcl
@partial_requires(["tls", "monitoring"])
partial service svc-payments "payments" {
    port = 8443
}
```

After merge, the validator checks that all `@partial_requires` fields are present in the merged block. If they are missing, it produces a warning or error indicating which fragment's requirements are unmet.

### 20.6 Conflict Mode Configuration

The merge conflict mode is configurable:

```rust
ParseOptions {
    merge_conflict_mode: ConflictMode::Strict,  // default
    // or ConflictMode::LastWins
}
```

---

## 21. Macros

WCL supports two kinds of macros for metaprogramming: **function macros** (template expansion) and **attribute macros** (AST transformation).

### 21.1 Function Macro Definition

Function macros produce AST fragments that are spliced into the document at the call site:

```
"macro" IDENT "(" param_list ")" "{" body "}"
```

```wcl
macro service_endpoint(name, port, protocol = "https") {
    endpoint "${name}" {
        url  = "${protocol}://service-${name}.internal:${port}"
        port = port

        health_check {
            path     = "/healthz"
            interval = 30
        }
    }
}
```

### 21.2 Attribute Macro Definition

Attribute macros transform the block they are attached to:

```
"macro" "@" IDENT "(" param_list ")" "{" transform_body "}"
```

```wcl
macro @with_monitoring(interval: int = 60, alert_threshold: float = 0.95) {
    inject {
        monitoring {
            enabled   = true
            interval  = interval
            threshold = alert_threshold
            target    = self.name
        }
    }

    set {
        monitored    = true
        monitor_tier = alert_threshold > 0.9 ? "critical" : "standard"
    }
}
```

### 21.3 Macro Parameters

```
param_list = param ("," param)*
param      = IDENT (":" type)? ("=" expression)?
```

- Parameters may have optional type constraints
- Parameters may have default values
- Parameters without defaults are required

When types are specified on parameters, the macro invocation arguments are type-checked at expansion time.

### 21.4 Function Macro Invocation

Function macros are called like functions, but at the statement level (not within expressions):

```wcl
config "gateway" {
    service_endpoint("auth", 8001)
    service_endpoint("users", 8002)
    service_endpoint("billing", 8003, "http")
}
```

The call is replaced by the expanded body of the macro with parameters substituted.

### 21.5 Attribute Macro Invocation

Attribute macros are invoked as decorators on blocks:

```wcl
@with_monitoring(interval = 30, alert_threshold = 0.99)
service svc-payments "payments" {
    port = 8443
}
```

The macro transforms the block by injecting, setting, or removing content.

### 21.6 Attribute Macro Transform Operations

Attribute macro bodies support the following transform directives:

#### `inject { ... }`

Adds child blocks or attributes into the target block's body:

```wcl
inject {
    monitoring {
        enabled = true
    }
}
```

#### `set { ... }`

Sets attributes on the target block (creates if absent, overwrites if present):

```wcl
set {
    monitored = true
    version   = "2.0"
}
```

#### `remove [ ... ]`

Removes attributes from the target block:

```wcl
remove [old_field, legacy_config]
```

#### `when condition { ... }`

Conditional transforms — only applied if the condition evaluates to true:

```wcl
when self.has("port") {
    inject {
        port_check {
            target = self.attr("port")
        }
    }
}

when alert_threshold > 0.9 {
    set {
        needs_paging = true
    }
}
```

### 21.7 The `self` Reference

Within attribute macros, `self` provides read-only access to the block the macro is attached to:

| Expression | Return Type | Description |
|------------|-------------|-------------|
| `self.name` | `string?` | First label of the block, or `null` |
| `self.kind` | `string` | Block type identifier |
| `self.id` | `identifier?` | Inline ID, or `null` |
| `self.attr(name)` | `value?` | Value of an attribute, or `null` |
| `self.has(name)` | `bool` | Whether the block has an attribute |
| `self.labels` | `list(string)` | All labels |
| `self.decorators` | `list(string)` | Names of all decorators on the block |

`self` is NOT available in function macros.

### 21.8 Macro Hygiene

WCL macros are **hygienic**: variables introduced inside a macro expansion do not leak into or capture names from the call site.

- Macro-internal `let` bindings exist in a scope that chains to the **macro definition site**, not the call site.
- Parameters shadow any same-named bindings from the definition site within the macro body.
- The call site's scope is not accessible from within the macro.

```wcl
let prefix = "global"

macro stamped(name) {
    let prefix = "svc"
    service "${prefix}-${name}" {
        tag = prefix
    }
}

config {
    let prefix = "local"
    stamped("auth")
    // Produces: service "svc-auth" { tag = "svc" }
    // NOT "local-auth" or "global-auth"
}
```

### 21.9 Macro Composition

Function macros can invoke other function macros:

```wcl
macro health_check(path = "/healthz") {
    health_check {
        path     = path
        interval = 30
    }
}

macro service_with_health(name, port) {
    service "${name}" {
        port = port
        health_check("/health/${name}")
    }
}
```

Attribute macros can inject content that carries further attribute macros:

```wcl
macro @production_ready() {
    inject {
        @with_monitoring(alert_threshold = 0.99)
        readiness {
            checks = ["health", "deps"]
        }
    }
}
```

### 21.10 Recursion and Depth Limits

- Direct recursion (a macro invoking itself) is forbidden and detected at expansion time.
- Indirect recursion (A calls B calls A) is also detected via an expansion stack.
- Macro expansion depth is limited (configurable, default: 64).
- The expander iterates until no macro calls remain, with the depth limit preventing infinite expansion.

### 21.11 Macros and Decorators

Macros themselves can carry decorators for metadata:

```wcl
@deprecated("use @observable instead", since = "2.0")
@doc("Adds monitoring to a service block")
macro @with_monitoring(interval: int = 60) {
    // ...
}
```

Macro decorators are regular decorators and are validated against their decorator schemas.

---

## 22. Query System

WCL includes a built-in query engine that provides jq-like syntax for searching and filtering blocks within a document. Queries can be used in expressions (to set variables or derive values) and via the CLI (for external tooling).

### 22.1 Query Syntax

```
query_expr = "query" "(" pipeline ")"
pipeline   = selector ("|" filter)*
```

### 22.2 Selectors

Selectors identify the starting set of blocks to search:

| Selector | Syntax | Description |
|----------|--------|-------------|
| Kind match | `service` | All blocks of type `service` |
| Kind + ID | `service#svc-auth` | Block of type `service` with id `svc-auth` |
| Kind + label | `service."payments"` | Block of type `service` with label `"payments"` |
| Path | `config.server.listener` | Nested block path |
| Recursive | `..health_check` | Find `health_check` blocks at any depth |
| Recursive + ID | `..listener#listener-internal` | Recursive search filtered by id |
| Root | `.` | The entire document |
| Wildcard | `*` | All blocks at the current level |
| Table | `table."permissions"` | Named table |
| Table by ID | `table#perms-main` | Table by inline ID |

### 22.3 Filters

Filters narrow the selected set. Multiple filters are chained with `|` and applied in sequence:

#### Attribute comparison

```wcl
query(service | .port > 8080)
query(service | .env == "prod")
query(service | .name != "legacy")
```

#### Regex match

```wcl
query(service | .name =~ "^api-")
```

#### Attribute existence

```wcl
query(service | has(.tls))
query(service | has(.monitoring))
```

#### Decorator existence

```wcl
query(service | has(@deprecated))
query(endpoint | has(@sensitive))
```

#### Decorator argument filtering

```wcl
query(service | @validate.min > 0)
query(endpoint | @deprecated.since == "1.0")
```

#### Compound filters

Multiple filters are AND-combined:

```wcl
query(service | .env == "prod" | .priority == "critical" | has(.tls))
```

### 22.4 Projections

A projection extracts a specific attribute value from matched blocks, changing the result from a list of block references to a list of values:

```wcl
let all_ports = query(service | .port)
// Returns: [8080, 8001, 8443, ...]

let prod_names = query(service | .env == "prod" | .name)
// Returns: ["payments", "auth", "gateway", ...]
```

A projection MUST be the last element in the pipeline.

### 22.5 Query Results

| Result Type | When | Value |
|-------------|------|-------|
| `list(block_ref)` | No projection | List of matching block references |
| `list(value)` | With projection | List of extracted attribute values |

Block references provide access to the matched block's attributes, decorators, ID, labels, and path within the document.

### 22.6 Aggregate Operations on Query Results

Query results can be used with aggregate and collection functions:

```wcl
let svc_count    = len(query(service))
let total_replicas = sum(query(service | .replicas))
let max_port     = max_of(query(service | .port))
let avg_cpu      = avg(query(service | .cpu_cores))
let all_unique   = distinct(query(service | .port))
let has_dupes    = len(query(service | .port)) != len(distinct(query(service | .port)))
```

### 22.7 Query in String Interpolation

Queries can be used within string interpolation for computed strings:

```wcl
let service_urls = map(
    query(service | .env == "prod"),
    s => "https://${s.name}.prod.internal:${s.port}"
)
```

### 22.8 CLI Query Interface

The query engine is also exposed as a CLI subcommand for external tooling:

```bash
# Find all service blocks
wcl query config.wcl 'service'

# Get all ports from prod services
wcl query config.wcl 'service | .env == "prod" | .port'

# Find deprecated things
wcl query config.wcl '.. | has(@deprecated)'

# Output as JSON
wcl query --format json config.wcl 'service | .priority == "critical"'

# Count results
wcl query --count config.wcl 'service'

# Query across multiple files
wcl query --recursive ./config/ 'service | .port > 8080'
```

Output formats supported by the CLI:
- `text` (default): Human-readable output
- `json`: JSON array of results
- `csv`: Tabular output (with projection)
- `wcl`: Results as WCL syntax

### 22.9 Evaluation Timing

Queries execute during the expression evaluation phase. The query engine operates on the partially-evaluated document — attribute values that have already been resolved can be filtered against.

If a query references attributes that depend on the result of that same query (directly or transitively), it is a cyclic dependency error, detected by the same dependency graph used for variable/attribute evaluation ordering.

---

## 23. Document Validation

In addition to per-field schema validation, WCL supports document-level validation rules that can express cross-cutting invariants using the query system.

### 23.1 Syntax

```wcl
validation "rule_name" {
    let ... // local variables for the check
    check   = boolean_expression
    message = "Human-readable error message"
}
```

### 23.2 Examples

```wcl
@doc("No two services may share a port within the same environment")
validation "unique_ports_per_env" {
    let envs = distinct(query(service | .env))

    check = every(envs, e =>
        len(query(service | .env == e | .port)) ==
        len(distinct(query(service | .env == e | .port)))
    )

    message = "Port conflict: multiple services share a port in the same environment"
}

@doc("All production services must have TLS and monitoring")
validation "prod_readiness" {
    let prod = query(service | .env == "prod")

    check = every(prod, s =>
        has(s, "tls") && has(s, "monitoring")
    )

    message = "Production services require tls and monitoring blocks"
}

@doc("At least one service must be defined")
validation "has_services" {
    check   = len(query(service)) > 0
    message = "No services defined — at least one service is required"
}
```

### 23.3 Validation Execution

Validation blocks execute after all other phases (import resolution, macro expansion, partial merging, expression evaluation, schema validation). The `check` attribute must evaluate to `bool`. If it evaluates to `false`, the `message` is emitted as a validation error.

### 23.4 Severity

Validations produce errors by default. A `@warning` decorator downgrades the severity:

```wcl
@warning
validation "prefer_tls" {
    check   = every(query(service), s => has(s, "tls"))
    message = "Not all services use TLS — consider enabling it"
}
```

---

## 24. Evaluation Pipeline

WCL processes a document through a series of well-defined phases. Each phase transforms the representation and feeds into the next. Errors are accumulated across phases and reported together.

### 24.1 Pipeline Phases

```
Source text
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 1: PARSE                                  │
│ Input:  Source text (UTF-8)                      │
│ Output: AST (with spans, trivia, all syntax)     │
│ Tool:   nom parser combinators                   │
│ Errors: Syntax errors                            │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 2: MACRO COLLECTION                       │
│ Input:  AST                                      │
│ Output: AST + MacroRegistry                      │
│ Action: Extract all macro definitions from the   │
│         body, register them, remove from AST     │
│ Errors: Duplicate macro names                    │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 3: IMPORT RESOLUTION                      │
│ Input:  AST + MacroRegistry                      │
│ Output: AST (single merged document)             │
│ Action: Process top-level import directives,     │
│         parse imported files recursively, merge  │
│         their top-level content (blocks, attrs,  │
│         schemas, macros, tables, validations,    │
│         exported variables) into the root AST.   │
│         Private let bindings remain file-local.  │
│         Process re-exports (export name) to      │
│         transitively forward names. Import-once  │
│         cache prevents duplicate processing.     │
│ Errors: File not found, jail escape, remote      │
│         import forbidden, max depth exceeded,    │
│         duplicate exported name, re-export of    │
│         undefined name                           │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 4: MACRO EXPANSION                        │
│ Input:  AST                                      │
│ Output: AST (all macros expanded)                │
│ Action: Expand function macro calls in-place,    │
│         apply attribute macros to blocks,        │
│         iterate until no macros remain           │
│ Errors: Undefined macros, type mismatches in     │
│         args, recursion, max depth               │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 5: CONTROL FLOW EXPANSION                 │
│ Input:  AST                                      │
│ Output: AST (all for/if structures expanded)     │
│ Action: Evaluate for-loop iterable expressions   │
│         and if/else condition expressions, then  │
│         expand loop bodies per iteration and     │
│         include/discard conditional branches.     │
│         Iterates until no control flow remains.  │
│         Validates identifier interpolation in    │
│         expanded inline IDs.                     │
│ Errors: Iterable not a list, condition not bool, │
│         invalid expanded identifier, max depth,  │
│         max iteration count exceeded             │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 6: PARTIAL MERGE                          │
│ Input:  AST                                      │
│ Output: AST (all partials merged)                │
│ Action: Group partial blocks by (scope, id),     │
│         merge attributes/children/decorators,    │
│         enforce id uniqueness                    │
│ Errors: Merge conflicts, kind mismatches,        │
│         mixed partial/non-partial                │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 7: SCOPE CONSTRUCTION                     │
│ Input:  AST                                      │
│ Output: AST + ScopeArena + IdRegistry            │
│ Action: Create scope hierarchy, register all     │
│         let bindings and attributes, check       │
│         shadowing, build dependency graph        │
│ Errors: Undefined references, cyclic deps,       │
│         shadowing warnings                       │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 8: EXPRESSION EVALUATION                  │
│ Input:  AST + ScopeArena                         │
│ Output: HIR (all expressions resolved to         │
│         concrete values, variables erased,       │
│         queries executed)                        │
│ Action: Topo-sort evaluation, evaluate all       │
│         expressions, execute query() calls,      │
│         resolve ref() calls, erase let bindings  │
│ Errors: Type errors, division by zero, unknown   │
│         functions, failed ref resolution         │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 9: DECORATOR VALIDATION                   │
│ Input:  HIR                                      │
│ Output: HIR (validated)                          │
│ Action: Check every decorator against its        │
│         schema: target validity, argument types, │
│         required params, constraints             │
│ Errors: Unknown decorators, wrong target,        │
│         type mismatches, missing params          │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 10: SCHEMA VALIDATION                     │
│ Input:  HIR                                      │
│ Output: HIR (validated)                          │
│ Action: Match blocks to schemas by type name,    │
│         check required fields, type-check values,│
│         validate @ref targets, check @validate   │
│         constraints, enforce open/closed schemas │
│ Errors: Missing required fields, type mismatches,│
│         unknown attributes (closed schema),      │
│         constraint violations                    │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 11: DOCUMENT VALIDATION                   │
│ Input:  HIR                                      │
│ Output: HIR (validated)                          │
│ Action: Execute validation blocks — evaluate     │
│         check expressions, emit messages for     │
│         failures                                 │
│ Errors: Validation rule failures                 │
└─────────────────────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────────────────────┐
│ Phase 12: SERDE DESERIALIZATION                 │
│ Input:  HIR                                      │
│ Output: Rust types (via serde Deserializer)      │
│ Action: Walk the concrete value tree, map to     │
│         serde data model                         │
│ Errors: Deserialization errors                   │
└─────────────────────────────────────────────────┘
```

### 24.2 Phase Ordering Rationale

1. **Parse** first — produce the raw AST.
2. **Macro collection** before imports — so imported files can reference macros defined in the root, and vice versa.
3. **Import resolution** before macro expansion — resolve all `import` directives, parse imported files recursively, and merge their top-level content into a single unified AST. After this phase, the document is self-contained.
4. **Macro expansion** before control flow expansion — macros may generate `for` loops and `if` blocks.
5. **Control flow expansion** before partial merge — `for` loops may generate `partial` blocks that need merging, and `if` blocks may conditionally include partials.
6. **Partial merge** before scope construction — the scope tree must see the merged blocks.
7. **Scope construction** before evaluation — the evaluator needs the dependency graph.
8. **Evaluation** before all validation — validators need concrete values.
9. **Decorator validation** before schema validation — decorators influence schema behavior (e.g., `@optional`).
10. **Schema validation** before document validation — individual blocks should be valid before cross-cutting rules run.
11. **Serde deserialization** last — consumers get fully validated data.

### 24.3 Control Flow Expansion Details

The control flow expansion phase (Phase 5) requires selective expression evaluation to determine loop iterables and conditional branches. This is handled as follows:

1. **Pre-evaluation pass**: The expander performs a limited evaluation of only the expressions used in `for ... in EXPR` and `if EXPR` positions. These expressions may reference module-level `let` bindings and literals, which are evaluated eagerly. If a control flow expression references an attribute that has not yet been evaluated, it is an error — control flow expressions MUST depend only on variables and literals that can be resolved without full document evaluation.

2. **Iterative expansion**: After evaluating control flow expressions, the expander replaces each `for` loop with its expanded body (one copy per iteration, with iterator variables substituted) and each `if/else` with only the matching branch. If expansion produces new `for` or `if` nodes (e.g., from nested structures), the expander re-scans until no control flow structures remain, subject to depth limits.

3. **Post-expansion cleanup**: Iterator variables and control flow nodes are removed from the AST. The result is a flat AST containing only blocks, attributes, tables, and let bindings — ready for partial merging and scope construction.

---

## 25. Error Handling and Diagnostics

### 25.1 Diagnostic Structure

All errors, warnings, and informational messages use a common diagnostic structure:

```rust
struct Diagnostic {
    severity: Severity,
    message: String,
    span: Span,
    labels: Vec<Label>,      // additional annotated spans
    notes: Vec<String>,      // free-form notes
    code: Option<String>,    // machine-readable error code, e.g. "E0042"
}

enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

struct Label {
    span: Span,
    message: String,
}

struct Span {
    file: FileId,
    start: usize,  // byte offset
    end: usize,    // byte offset
    line: u32,
    column: u32,
}
```

### 25.2 Error Accumulation

WCL uses an **accumulative** error model. Each phase collects all diagnostics rather than failing on the first error. This allows:

- IDEs to show all problems at once
- Batch validation to report comprehensively
- Users to fix multiple issues in a single edit cycle

If a phase produces errors that make subsequent phases impossible (e.g., parse errors preventing scope construction), later phases are skipped.

### 25.3 Error Codes

Each diagnostic category has a unique error code for tooling:

| Code | Phase | Description |
|------|-------|-------------|
| E001 | Parse | Syntax error |
| E002 | Parse | Unexpected token |
| E003 | Parse | Unterminated string |
| E010 | Import | File not found |
| E011 | Import | Jail escape (path outside root) |
| E013 | Import | Remote import forbidden |
| E014 | Import | Max import depth exceeded |
| E020 | Macro | Undefined macro |
| E021 | Macro | Recursive macro expansion |
| E022 | Macro | Max expansion depth exceeded |
| E023 | Macro | Wrong macro kind (function vs attribute) |
| E024 | Macro | Parameter type mismatch |
| E025 | Control Flow | For-loop iterable is not a list |
| E026 | Control Flow | If/else condition is not bool |
| E027 | Control Flow | Invalid expanded identifier (bad chars after interpolation) |
| E028 | Control Flow | Max iteration count exceeded |
| E029 | Control Flow | Max nesting depth exceeded |
| E030 | Merge | Duplicate ID (non-partial) |
| E031 | Merge | Attribute conflict in partial merge |
| E032 | Merge | Kind mismatch in partial merge |
| E033 | Merge | Mixed partial/non-partial with same ID |
| E034 | Export | Duplicate exported variable name across imports |
| E035 | Export | Re-export of undefined name |
| E036 | Export | Export declaration inside block (must be top-level) |
| E040 | Scope | Undefined reference |
| E041 | Scope | Cyclic dependency |
| E050 | Eval | Type error in expression |
| E051 | Eval | Division by zero |
| E052 | Eval | Unknown function |
| E053 | Eval | Ref resolution failed |
| E054 | Eval | Index out of bounds |
| E060 | Decorator | Unknown decorator |
| E061 | Decorator | Invalid target |
| E062 | Decorator | Missing required parameter |
| E063 | Decorator | Parameter type mismatch |
| E064 | Decorator | Constraint violation |
| E070 | Schema | Missing required field |
| E071 | Schema | Attribute type mismatch |
| E072 | Schema | Unknown attribute (closed schema) |
| E073 | Schema | Validate constraint violation |
| E074 | Schema | Ref target not found |
| E080 | Validation | Document validation failed |
| W001 | Scope | Shadowing warning |
| W002 | Scope | Unused variable |
| W003 | Merge | Label mismatch in partial merge |

### 25.4 Span-Based Error Reporting

All diagnostics carry spans that point to the exact source location. Multi-span diagnostics include both the error site and related locations:

```
error[E030]: duplicate id 'svc-payments' in scope
  ┌─ config.wcl:12:9
  │
5 │ service svc-payments "payments" {
  │         ^^^^^^^^^^^^ first defined here
  │
12│ service svc-payments "other" {
  │         ^^^^^^^^^^^^ duplicate definition
```

---

## 26. Serialization and Deserialization (Serde)

### 26.1 Deserialization

WCL implements the `serde::Deserializer` trait, allowing any Rust type that implements `serde::Deserialize` to be populated from a WCL document.

The deserializer operates on the fully-resolved HIR — all imports resolved, macros expanded, partials merged, expressions evaluated, variables erased, and validation passed.

```rust
// Basic usage
let config: ServerConfig = wcl::from_str(input)?;

// With options
let config: ServerConfig = wcl::from_str_with_options(input, ParseOptions {
    root_dir: PathBuf::from("./config"),
    max_import_depth: 10,
    allow_imports: true,
    merge_conflict_mode: ConflictMode::Strict,
})?;
```

### 26.2 Type Mapping

| WCL Type | Serde/Rust Type |
|----------|-----------------|
| `string` | `String`, `&str` |
| `int` | `i64`, `i32`, `i16`, `i8`, `u64`, `u32`, `u16`, `u8` |
| `float` | `f64`, `f32` |
| `bool` | `bool` |
| `null` | `Option<T>` (None) |
| `identifier` | `String` (or a newtype wrapper) |
| `list(T)` | `Vec<T>`, `HashSet<T>` |
| `map(K, V)` | `HashMap<K, V>`, `BTreeMap<K, V>` |
| Block | Struct (fields = attributes) |
| Table | `Vec<T>` (rows as structs) |
| `function` | Skipped — not representable in serde data model |

### 26.3 Block-to-Struct Mapping

```wcl
service svc-api "api" {
    port    = 8080
    env     = "prod"
    tags    = ["web", "public"]
}
```

```rust
#[derive(Deserialize)]
struct Service {
    id: String,           // from inline ID
    port: u16,
    env: String,
    tags: Vec<String>,
}
```

The inline ID is accessible as a field named `id` in the deserialized struct. Labels are accessible via a field named `labels` (type `Vec<String>`) or via the block type name if using a document-level struct.

### 26.4 Serialization

WCL implements the `serde::Serializer` trait for writing Rust values back to WCL format. The serializer produces valid WCL text.

```rust
let output = wcl::to_string(&config)?;
let pretty = wcl::to_string_pretty(&config)?;
```

Note: Serialization cannot preserve comments, decorators, macros, or variables — these are parse-time/validation-time constructs. The serializer produces a minimal WCL document containing only the data values.

### 26.5 Rich Document API

For consumers that need access to decorators, comments, spans, or other metadata, a rich document API is provided alongside serde:

```rust
// Parse into rich document (preserves everything)
let doc: wcl::Document = wcl::parse(input, options)?;

// Access blocks
for block in doc.blocks_of_type("service") {
    println!("Service: {:?}", block.labels());
    
    if block.has_decorator("deprecated") {
        let dep = block.decorator("deprecated").unwrap();
        println!("  Deprecated: {}", dep.arg("message"));
    }
    
    for attr in block.attributes() {
        if attr.has_decorator("sensitive") {
            println!("  [REDACTED] {}", attr.name());
        } else {
            println!("  {} = {}", attr.name(), attr.value());
        }
    }
}

// Query the document
let results = doc.query("service | .env == \"prod\" | .port")?;
```

---

## 27. Crate Architecture

The WCL implementation is organized as a Rust workspace with the following crates:

```
wcl/
├── Cargo.toml              # workspace root
├── wcl_core/               # AST, parser, spans, trivia, comments
│   ├── src/
│   │   ├── ast.rs          # AST node definitions
│   │   ├── lexer.rs        # Tokenizer (nom combinators)
│   │   ├── parser.rs       # Parser (nom combinators) → AST
│   │   ├── span.rs         # Span and source location tracking
│   │   ├── trivia.rs       # Comment and whitespace model
│   │   └── lib.rs
│   └── Cargo.toml
│
├── wcl_eval/               # Expression evaluation, import resolution, macro expansion
│   ├── src/
│   │   ├── evaluator.rs    # Expression evaluator with scope chain
│   │   ├── scope.rs        # Scope arena, name resolution, dependency graph
│   │   ├── imports.rs      # Import resolver with jail, caching, import-once
│   │   ├── macros.rs       # Macro expander (function + attribute)
│   │   ├── merge.rs        # Partial declaration merger
│   │   ├── functions.rs    # Built-in function registry
│   │   ├── query.rs        # Query engine (selector, filters, projection)
│   │   └── lib.rs
│   └── Cargo.toml
│
├── wcl_schema/             # Type system, decorator schemas, validation
│   ├── src/
│   │   ├── types.rs        # Type definitions and type checker
│   │   ├── schema.rs       # Schema definitions and validation
│   │   ├── decorator.rs    # Decorator schema validation
│   │   ├── document.rs     # Document-level validation (validation blocks)
│   │   ├── id.rs           # ID registry and uniqueness checking
│   │   └── lib.rs
│   └── Cargo.toml
│
├── wcl_serde/              # Serde Serializer + Deserializer implementations
│   ├── src/
│   │   ├── de.rs           # Deserializer implementation
│   │   ├── ser.rs          # Serializer implementation
│   │   ├── error.rs        # Serde error types
│   │   └── lib.rs
│   └── Cargo.toml
│
├── wcl_derive/             # Proc macros for derive support
│   ├── src/
│   │   └── lib.rs          # #[derive(WclDeserialize)] etc.
│   └── Cargo.toml
│
├── wcl_cli/                # Command-line tool
│   ├── src/
│   │   ├── main.rs
│   │   ├── fmt.rs          # Formatter
│   │   ├── validate.rs     # Validator
│   │   ├── query.rs        # Query command
│   │   └── inspect.rs      # AST/HIR inspector
│   └── Cargo.toml
│
└── wcl/                    # Facade crate (re-exports everything)
    ├── src/
    │   └── lib.rs          # pub use wcl_core, wcl_eval, etc.
    └── Cargo.toml
```

### 27.1 Dependency Graph

```
wcl_core       ← foundation, no deps on other wcl crates
wcl_eval       ← depends on wcl_core
wcl_schema     ← depends on wcl_core, wcl_eval
wcl_serde      ← depends on wcl_core, wcl_eval, wcl_schema
wcl_derive     ← depends on wcl_core (proc macro crate)
wcl_cli        ← depends on all crates
wcl            ← facade, depends on all library crates
```

### 27.2 External Dependencies

| Dependency | Crate | Purpose |
|------------|-------|---------|
| `nom` | wcl_core | Parser combinators |
| `serde` | wcl_serde | Serialization framework |
| `regex` | wcl_eval | Regex matching in expressions and queries |
| `sha2` | wcl_eval | SHA-256 built-in function |
| `base64` | wcl_eval | Base64 built-in functions |
| `clap` | wcl_cli | CLI argument parsing |
| `miette` or `ariadne` | wcl_cli | Pretty diagnostic rendering |

---

## 28. CLI Interface

The `wcl` CLI provides tools for working with WCL documents.

### 28.1 Commands

#### `wcl validate <file>`

Parse and validate a WCL document through all phases. Exits with code 0 if valid, non-zero if any errors.

```bash
wcl validate config.wcl
wcl validate --strict config.wcl       # warnings become errors
wcl validate --schema schema.wcl config.wcl  # explicit schema file
```

#### `wcl fmt <file>`

Format a WCL document. Preserves comments and blank line grouping. Outputs to stdout by default.

```bash
wcl fmt config.wcl                     # print to stdout
wcl fmt --write config.wcl             # format in place
wcl fmt --check config.wcl             # check if already formatted (exit code)
```

#### `wcl query <file> <query>`

Execute a query against a WCL document.

```bash
wcl query config.wcl 'service | .env == "prod"'
wcl query --format json config.wcl 'service | .port'
wcl query --count config.wcl 'service'
wcl query --recursive ./config/ '.. | has(@deprecated)'
```

#### `wcl inspect <file>`

Dump the AST or HIR for debugging and tooling development.

```bash
wcl inspect --ast config.wcl           # raw AST (post-parse)
wcl inspect --hir config.wcl           # resolved HIR (post-eval)
wcl inspect --scopes config.wcl        # scope tree
wcl inspect --deps config.wcl          # dependency graph
```

#### `wcl convert <file>`

Convert between WCL and other formats.

```bash
wcl convert --to json config.wcl       # WCL → JSON
wcl convert --to yaml config.wcl       # WCL → YAML
wcl convert --to toml config.wcl       # WCL → TOML
wcl convert --from json config.json    # JSON → WCL
```

#### `wcl lsp`

Start the Language Server Protocol server for IDE integration.

```bash
wcl lsp                                # stdio transport
wcl lsp --tcp 127.0.0.1:9257          # TCP transport
```

---

## 29. File Extension and MIME Type

| Property | Value |
|----------|-------|
| File extension | `.wcl` |
| MIME type | `application/wcl` |
| Text encoding | UTF-8 (always) |

---

## 30. EBNF Grammar Summary

This section provides a formal grammar summary in Extended Backus-Naur Form.

```ebnf
(* ===== Top-level ===== *)
document        = trivia { doc_item } trivia ;
doc_item        = import_decl | export_decl | body_item ;
body            = { body_item } ;
body_item       = attribute
                | block
                | table
                | let_binding
                | macro_def
                | macro_call
                | for_loop
                | conditional
                | validation
                | schema
                | decorator_schema
                | comment ;

(* ===== Import Directives (top-level only) ===== *)
import_decl     = "import" STRING_LIT ;

(* ===== Export Declarations (top-level only) ===== *)
export_decl     = "export" "let" IDENT "=" expression   (* export with assignment *)
                | "export" IDENT ;                        (* re-export *)

(* ===== Attributes ===== *)
attribute       = { decorator } IDENT "=" expression ;

(* ===== Blocks ===== *)
block           = { decorator } [ "partial" ] IDENT [ IDENTIFIER_LIT ]
                  { STRING_LIT } "{" body "}" ;

(* ===== Let Bindings ===== *)
let_binding     = "let" IDENT "=" expression ;

(* ===== Control Flow ===== *)
for_loop        = "for" IDENT [ "," IDENT ] "in" expression "{" body "}" ;
conditional     = "if" expression "{" body "}" [ else_branch ] ;
else_branch     = "else" ( conditional | "{" body "}" ) ;

(* ===== Tables ===== *)
table           = { decorator } [ "partial" ] "table" [ IDENTIFIER_LIT ]
                  { STRING_LIT } "{" table_body "}" ;
table_body      = { column_decl } { table_row } ;
column_decl     = { decorator } IDENT ":" type_expr ;
table_row       = "|" expression { "|" expression } "|" ;

(* ===== Schemas ===== *)
schema          = { decorator } "schema" STRING_LIT "{" { schema_field } "}" ;
schema_field    = { decorator } IDENT "=" type_expr { decorator } ;

(* ===== Decorator Schemas ===== *)
decorator_schema = { decorator } "decorator_schema" STRING_LIT
                   "{" decorator_schema_body "}" ;
decorator_schema_body = "target" "=" "[" target_list "]" { schema_field } ;
target_list     = target { "," target } ;
target          = "block" | "attribute" | "table" | "schema" ;

(* ===== Decorators ===== *)
decorator       = "@" IDENT [ "(" decorator_args ")" ] ;
decorator_args  = positional_args [ "," named_args ]
                | named_args ;
positional_args = expression { "," expression } ;
named_args      = named_arg { "," named_arg } ;
named_arg       = IDENT "=" expression ;

(* ===== Macros ===== *)
macro_def       = { decorator } "macro" ( func_macro_def | attr_macro_def ) ;
func_macro_def  = IDENT "(" param_list ")" "{" body "}" ;
attr_macro_def  = "@" IDENT "(" param_list ")" "{" transform_body "}" ;
param_list      = [ param { "," param } ] ;
param           = IDENT [ ":" type_expr ] [ "=" expression ] ;
macro_call      = IDENT "(" [ arg_list ] ")" ;
arg_list        = expression { "," expression } [ "," named_args ] ;

(* ===== Transform Body (attribute macros) ===== *)
transform_body  = { transform_directive } ;
transform_directive = inject_block | set_block | remove_block | when_block ;
inject_block    = "inject" "{" body "}" ;
set_block       = "set" "{" { attribute } "}" ;
remove_block    = "remove" "[" ident_list "]" ;
ident_list      = IDENT { "," IDENT } [ "," ] ;
when_block      = "when" expression "{" { transform_directive } "}" ;

(* ===== Validation ===== *)
validation      = { decorator } "validation" STRING_LIT "{"
                  { let_binding } "check" "=" expression
                  "message" "=" expression "}" ;

(* ===== Types ===== *)
type_expr       = "string" | "int" | "float" | "bool" | "null"
                | "identifier" | "any"
                | "list" "(" type_expr ")"
                | "map" "(" type_expr "," type_expr ")"
                | "set" "(" type_expr ")"
                | "ref" "(" STRING_LIT ")"
                | "union" "(" type_expr { "," type_expr } ")" ;

(* ===== Expressions ===== *)
expression      = ternary_expr ;
ternary_expr    = or_expr [ "?" expression ":" expression ] ;
or_expr         = and_expr { "||" and_expr } ;
and_expr        = equality_expr { "&&" equality_expr } ;
equality_expr   = comparison_expr { ( "==" | "!=" ) comparison_expr } ;
comparison_expr = additive_expr { ( "<" | ">" | "<=" | ">=" | "=~" )
                  additive_expr } ;
additive_expr   = multiplicative_expr { ( "+" | "-" ) multiplicative_expr } ;
multiplicative_expr = unary_expr { ( "*" | "/" | "%" ) unary_expr } ;
unary_expr      = ( "!" | "-" ) unary_expr | postfix_expr ;
postfix_expr    = primary_expr { ( "." IDENT | "[" expression "]"
                | "(" [ arg_list ] ")" ) } ;
primary_expr    = INT_LIT | FLOAT_LIT | STRING_LIT | BOOL_LIT | NULL_LIT
                | IDENTIFIER_LIT | IDENT
                | list_literal | map_literal
                | "(" expression ")"
                | query_expr
                | import_util_expr
                | ref_expr
                | lambda_expr ;

(* ===== Special Expressions ===== *)
query_expr      = "query" "(" pipeline ")" ;
pipeline        = selector { "|" filter } ;
selector        = [ ".." ] IDENT [ "#" IDENTIFIER_LIT ]
                  { "." ( IDENT | STRING_LIT ) }
                | "." | "*" ;
filter          = "." IDENT [ ( "==" | "!=" | "<" | ">" | "<=" | ">="
                  | "=~" ) expression ]
                | "has" "(" ( "." IDENT | "@" IDENT ) ")"
                | "@" IDENT "." IDENT ( "==" | "!=" | "<" | ">" | "<="
                  | ">=" ) expression ;

import_util_expr = "import_table" "(" STRING_LIT [ "," STRING_LIT ] ")"
                 | "import_raw" "(" STRING_LIT ")" ;

ref_expr        = "ref" "(" IDENTIFIER_LIT ")" ;

lambda_expr     = lambda_params "=>" ( expression | block_expr ) ;
lambda_params   = IDENT
                | "(" [ IDENT { "," IDENT } ] ")" ;
block_expr      = "{" { let_binding } expression "}" ;

(* ===== Collections ===== *)
list_literal    = "[" [ expression { "," expression } [ "," ] ] "]" ;
map_literal     = "{" [ map_entry { map_entry } ] "}" ;
map_entry       = ( IDENT | STRING_LIT ) "=" expression ;

(* ===== Trivia ===== *)
trivia          = { whitespace | comment } ;
comment         = line_comment | block_comment | doc_comment ;
line_comment    = "//" { any_char } newline ;
block_comment   = "/*" { any_char | block_comment } "*/" ;
doc_comment     = "///" { any_char } newline ;

(* ===== Terminals ===== *)
IDENT           = ( letter | "_" ) { letter | digit | "_" } ;
IDENTIFIER_LIT  = ( letter | "_" ) { letter | digit | "_" | "-" } ;
STRING_LIT      = '"' { string_char | escape_seq | interpolation } '"'
                | heredoc ;
INT_LIT         = digit { digit | "_" }
                | "0x" hex_digit { hex_digit | "_" }
                | "0o" oct_digit { oct_digit | "_" }
                | "0b" bin_digit { bin_digit | "_" } ;
FLOAT_LIT       = digit { digit } "." digit { digit }
                  [ ( "e" | "E" ) [ "+" | "-" ] digit { digit } ] ;
BOOL_LIT        = "true" | "false" ;
NULL_LIT        = "null" ;
interpolation   = "${" expression "}" ;
escape_seq      = "\\" ( '"' | "\\" | "n" | "r" | "t"
                | "u" hex4 | "U" hex8 ) ;
heredoc         = "<<" [ "-" ] marker newline { any_char } newline marker
                | "<<'" marker "'" newline { any_char } newline marker ;
```

---

## 31. Complete Examples

### 31.1 Microservice Configuration

```wcl
// ═══════════════════════════════════════
// schemas.wcl — Shared type definitions
// ═══════════════════════════════════════

schema "service" {
    @id_pattern("svc-*")
    id = identifier

    port       = int      @required @validate(min = 1, max = 65535)
    region     = string   @required
    env        = string   @required
    tags       = list(string) @optional @default([])
    replicas   = int      @optional @default(1) @validate(min = 1)
    debug      = bool     @optional @default(false)
}

schema "health_check" {
    path     = string @required
    interval = int    @optional @default(30) @validate(min = 1)
    timeout  = int    @optional @default(5)
}

schema "monitoring" {
    enabled         = bool  @required
    interval        = int   @optional @default(60)
    alert_threshold = float @optional @default(0.95) @validate(min = 0.0, max = 1.0)
}

schema "tls" {
    cert_path   = string @required
    key_path    = string @optional
    min_version = string @optional @default("1.2")
}

decorator_schema "rate_limit" {
    target = [block]

    requests_per_second = int    @optional
    burst               = int    @optional
    window              = string @optional @default("1m")

    @constraint(any_of = ["requests_per_second", "burst"])
}
```

```wcl
// ═══════════════════════════════════════
// macros.wcl — Reusable macro definitions
// ═══════════════════════════════════════

/// Adds a standard health check to a service block.
macro standard_health(path = "/healthz", interval = 30) {
    health_check {
        path     = path
        interval = interval
        timeout  = 5
    }
}

/// Adds monitoring configuration to the attached block.
macro @with_monitoring(interval: int = 60, threshold: float = 0.95) {
    inject {
        monitoring {
            enabled         = true
            interval        = interval
            alert_threshold = threshold
            target          = self.name
        }
    }

    set {
        monitored = true
    }

    when threshold > 0.9 {
        set {
            critical = true
        }
    }
}

/// Adds TLS configuration to the attached block.
macro @with_tls(cert_dir: string = "/etc/certs") {
    inject {
        tls {
            cert_path   = "${cert_dir}/${self.name}.pem"
            key_path    = "${cert_dir}/${self.name}.key"
            min_version = "1.3"
        }
    }
}
```

```wcl
// ═══════════════════════════════════════
// constants.wcl — Shared constants and helpers
// ═══════════════════════════════════════

export let default_region = "ap-southeast-2"
export let env = "prod"
export let base_port = 8000

// Exported helper functions
export let port_for = (offset) => base_port + offset
export let is_prod = env == "prod"

// Private helper — not visible to importers
let internal_suffix = is_prod ? "" : "-${env}"
```

```wcl
// ═══════════════════════════════════════
// config.wcl — Main configuration file
// ═══════════════════════════════════════

import "./schemas.wcl"
import "./macros.wcl"
import "./constants.wcl"

// default_region, env, base_port, port_for, is_prod are
// all available directly from the constants.wcl exports

// ─── Auth Service ───

@with_monitoring(interval = 15, threshold = 0.99)
@with_tls
@rate_limit(requests_per_second = 1000, burst = 50)
service svc-auth "auth" {
    port     = port_for(1)
    region   = default_region
    env      = env
    tags     = ["auth", "critical"]
    replicas = is_prod ? 3 : 1

    standard_health("/health/auth")
}

// ─── Payments Service ───

@with_monitoring(interval = 10, threshold = 0.999)
@with_tls
service svc-payments "payments" {
    port     = port_for(2)
    region   = default_region
    env      = env
    tags     = ["payments", "critical", "pci"]
    replicas = 5

    standard_health("/health/payments", 10)
}

// ─── Gateway ───

service svc-gateway "gateway" {
    port     = base_port
    region   = default_region
    env      = env
    tags     = ["gateway", "public"]
    replicas = 3

    standard_health()

    // Cross-references via inline IDs
    upstream_auth     = ref(svc-auth)
    upstream_payments = ref(svc-payments)

    monitoring {
        enabled         = true
        interval        = 5
        alert_threshold = 0.999
    }
}

// ─── Permissions Table ───

table perms-api "api_permissions" {
    @validate(one_of = ["admin", "service", "viewer", "editor"])
    role     : string

    resource : string
    action   : string
    allow    : bool

    | "admin"   | "*"          | "*"     | true  |
    | "service" | "auth"       | "read"  | true  |
    | "service" | "payments"   | "read"  | true  |
    | "service" | "payments"   | "write" | true  |
    | "viewer"  | "dashboard"  | "read"  | true  |
    | "viewer"  | "dashboard"  | "write" | false |
}

// ─── Dynamic Worker Pool (for loop) ───

let worker_regions = ["ap-southeast-2", "us-east-1", "eu-west-1"]

for region, idx in worker_regions {
    service svc-worker-${region} "worker-${region}" {
        port     = base_port + 100 + idx
        region   = region
        env      = env
        tags     = ["worker", "batch"]
        replicas = env == "prod" ? 3 : 1

        standard_health("/health/worker")
    }
}

// ─── Conditional Infrastructure ───

if env == "prod" {
    load_balancer lb-prod "production" {
        algorithm = "round_robin"
        targets   = query(service | .env == "prod" | .port)

        tls {
            cert_path   = "/etc/certs/lb.pem"
            min_version = "1.3"
        }
    }
}

if env != "prod" {
    service svc-debug "debug-tools" {
        port  = 9999
        env   = env
        debug = true
    }
} else {
    // In prod, deploy the admin dashboard instead
    service svc-admin "admin-dashboard" {
        port = 9000
        env  = env
        tags = ["admin", "internal"]
    }
}

// ─── Document-Level Validations ───

validation "unique_ports" {
    let ports = query(service | .port)
    check   = len(ports) == len(distinct(ports))
    message = "Port conflict detected: two or more services share the same port"
}

validation "prod_readiness" {
    let prod_services = query(service | .env == "prod")

    check = every(prod_services, s =>
        has(s, "tls") && has(s, "monitoring") && has(s, "health_check")
    )

    message = "All production services must have tls, monitoring, and health_check"
}

// ─── Computed Outputs ───

let all_ports = query(service | .port)
let critical_services = query(service | .tags | contains("critical"))
let total_replicas = sum(query(service | .replicas))

/// Summary block for dashboards and tooling
summary "cluster" {
    service_count   = len(query(service))
    total_replicas  = total_replicas
    has_gateway     = len(query(service#svc-gateway)) > 0
    all_ports       = all_ports
    environment     = env
    region          = default_region
}
```

### 31.2 Partial Declarations Across Files

```wcl
// ═══════════════════════════════════════
// base/payments.wcl — Platform team
// ═══════════════════════════════════════

partial service svc-payments "payments" {
    port     = 8443
    region   = "ap-southeast-2"
    env      = "prod"
    replicas = 5

    standard_health("/health/payments", 10)
}
```

```wcl
// ═══════════════════════════════════════
// observability/payments.wcl — SRE team
// ═══════════════════════════════════════

partial service svc-payments "payments" {
    monitoring {
        enabled         = true
        interval        = 10
        alert_threshold = 0.999
    }

    @sensitive
    pagerduty_key = "PD-abc123"
}
```

```wcl
// ═══════════════════════════════════════
// security/payments.wcl — Security team
// ═══════════════════════════════════════

partial service svc-payments "payments" {
    tls {
        cert_path   = "/etc/certs/payments.pem"
        key_path    = "/etc/certs/payments.key"
        min_version = "1.3"
    }

    allowed_origins = ["https://app.example.com"]
}
```

```wcl
// ═══════════════════════════════════════
// main.wcl — Assembles everything
// ═══════════════════════════════════════

import "./schemas.wcl"
import "./macros.wcl"
import "./base/payments.wcl"
import "./observability/payments.wcl"
import "./security/payments.wcl"

// After import resolution and partial merge, svc-payments
// is a single unified service block containing all fragments.

validation "payments_complete" {
    let payments = query(service#svc-payments)

    check = len(payments) == 1
        && has(payments[0], "tls")
        && has(payments[0], "monitoring")
        && has(payments[0], "health_check")

    message = "Payments service must have tls, monitoring, and health_check after assembly"
}
```

---

## Appendix A: Reserved for Future Extension

The following features are NOT part of the v0.1.0 specification but are considered for future versions:

- **Enum types**: Named sets of allowed values as a type
- **LSP specification**: Detailed protocol for WCL language server features
- **Plugin system**: User-defined functions loaded from WASM modules
- **Format-preserving AST modification API**: Programmatic refactoring that preserves all formatting

---

## Appendix B: Comparison with Other Formats

| Feature | WCL | HCL | YAML | TOML | JSON |
|---------|-----|-----|------|------|------|
| Block structure | ✓ | ✓ | ✗ | Partial | ✗ |
| Type system / schemas | ✓ | ✗ | ✗ | ✗ | ✗ (JSON Schema external) |
| Decorators / annotations | ✓ | ✗ | ✗ | ✗ | ✗ |
| Expressions | ✓ | ✓ | ✗ | ✗ | ✗ |
| Variables (with export) | ✓ (first-class, export/private) | ✓ (locals) | ✗ | ✗ | ✗ |
| User-defined functions | ✓ (lambdas) | ✗ | ✗ | ✗ | ✗ |
| Conditional blocks | ✓ (if/else) | ✗ | ✗ | ✗ | ✗ |
| For-loop expansion | ✓ | ✗ | ✗ | ✗ | ✗ |
| Macros | ✓ | ✗ | ✗ | ✗ | ✗ |
| Data tables | ✓ (typed columns) | ✗ | ✗ | ✗ | ✗ |
| Import system | ✓ (top-level merge, secure) | ✗ | ✗ | ✗ | ✗ |
| Partial declarations | ✓ | ✗ | ✗ | ✗ | ✗ |
| Query engine | ✓ | ✗ | ✗ | ✗ | ✗ |
| Comment preservation | ✓ | ✓ | Partial | ✗ | ✗ |
| Serde integration | ✓ (native) | ✗ | ✓ | ✓ | ✓ |
| Unique block IDs | ✓ | ✗ | ✗ | ✗ | ✗ |

---

*WCL Specification v0.1.0 — End of Document*
