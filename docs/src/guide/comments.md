# Comments

WCL has three comment forms. All comments are preserved in the AST, enabling round-trip formatting and IDE tooling.

## Line Comments

A line comment starts with `//` and extends to the end of the line:

```wcl
// This is a line comment.
port = 8080  // inline comment after a value
```

## Block Comments

A block comment is delimited by `/*` and `*/`. Block comments are **nestable**, so you can comment out a region of code that already contains block comments:

```wcl
/*
  This entire section is commented out.

  server {
    /* nested block comment */
    port = 8080
  }
*/
```

Nesting depth is tracked accurately, so only the outermost `*/` closes the comment:

```wcl
/* outer /* inner */ still in outer */ // now outside
```

## Doc Comments

A doc comment starts with `///` and attaches to the declaration that immediately follows it:

```wcl
/// The primary web server.
/// Listens on all interfaces.
server #web-1 {
  /// The TCP port to bind.
  port = 8080
}
```

Doc comments are used by the WCL language server to populate hover documentation in editors that support the LSP. Multiple consecutive `///` lines are merged into a single doc string.

## Comment Attachment

The parser classifies comments into three categories based on their position relative to declarations:

| Category  | Position                                                      | Example                             |
|-----------|---------------------------------------------------------------|-------------------------------------|
| Leading   | One or more comment lines immediately before a declaration    | `// comment\nport = 8080`           |
| Trailing  | A comment on the same line as a declaration, after the value  | `port = 8080  // comment`           |
| Floating  | A comment separated from any declaration by blank lines       | A comment in the middle of a block  |

Floating comments are associated with the surrounding block rather than a specific declaration.

## Round-Trip Preservation

All three comment categories — leading, trailing, and floating — are stored in the AST. The WCL formatter (`wcl fmt`) reads these nodes and restores them in their original positions, so formatting a file never discards comments.

## Blank Line Preservation

The parser records blank lines between declarations. The formatter uses this information to maintain vertical spacing, preventing it from collapsing logically grouped sections together.

```wcl
// Database settings
host = "db.internal"
port = 5432

// Connection pool settings
pool_min = 2
pool_max = 10
```

After formatting, the blank line between the two groups is preserved.

## Disabling Linting with Comments

Some diagnostics can be suppressed inline using the `@allow` decorator on a block. For example, to suppress a shadowing warning on a specific block:

```wcl
@allow(shadowing)
server {
  let port = 8080
}
```

See [Built-in Decorators](./decorators-builtin.md) for the full list of suppressible diagnostics.
