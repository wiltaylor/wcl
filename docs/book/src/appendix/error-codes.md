# Error Codes

This page lists all diagnostic codes produced by the WCL pipeline, grouped by phase.

## Errors

| Code | Phase | Description |
|------|-------|-------------|
| E001 | Parse | Syntax error |
| E002 | Parse | Unexpected token |
| E003 | Parse | Unterminated string |
| E010 | Import | File not found |
| E011 | Import | Jail escape (path outside root) |
| E013 | Import | Remote import forbidden |
| E014 | Import | Max import depth exceeded |
| E015 | Import | Library not found in search paths |
| E016 | Import | Glob pattern matched no files (non-optional import) |
| E020 | Macro | Undefined macro |
| E021 | Macro | Recursive macro expansion |
| E022 | Macro | Max expansion depth exceeded |
| E023 | Macro | Wrong macro kind |
| E024 | Macro | Parameter type mismatch |
| E025 | Control Flow | For-loop iterable is not a list |
| E026 | Control Flow | If/else condition is not bool |
| E027 | Control Flow | Invalid expanded identifier |
| E028 | Control Flow | Max iteration count exceeded |
| E029 | Control Flow | Max nesting depth exceeded |
| E030 | Merge | Duplicate ID (non-partial) |
| E031 | Merge | Attribute conflict in partial merge |
| E032 | Merge | Kind mismatch in partial merge |
| E033 | Merge | Mixed partial/non-partial |
| E034 | Export | Duplicate exported variable name |
| E035 | Export | Re-export of undefined name |
| E036 | Export | Export inside block |
| E038 | Merge | Partial let binding value must be a list |
| E039 | Merge | Let binding declared as both partial and non-partial with same name |
| E040 | Scope | Undefined reference |
| E041 | Scope | Cyclic dependency |
| E050 | Eval | Type error in expression |
| E051 | Eval | Division by zero |
| E052 | Eval | Unknown function |
| E053 | Eval | Declared-but-unregistered function (from `declare` in library) |
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
| E090 | Table | @table_index references nonexistent column |
| E091 | Table | Duplicate value in unique table index |
| E092 | Table | Inline columns defined when schema is applied |
| E093 | Schema | Block uses text block syntax but schema has no `@text` field |
| E094 | Schema | `@text` field validation errors (wrong name or type) |
| E095 | Schema | Child not allowed by parent's `@children` constraint |
| E096 | Schema | Item not allowed by its own `@parent` constraint |
| E097 | Schema | Child count below `@child` minimum |
| E098 | Schema | Child count above `@child` maximum |
| E099 | Schema | Self-nesting exceeds `@child` max_depth |
| E100 | Schema | Symbol value not in declared `symbol_set` |
| E101 | Schema | Referenced `symbol_set` does not exist |
| E102 | Parse  | Duplicate `symbol_set` name |
| E103 | Parse  | Duplicate symbol within a `symbol_set` |
| E120 | Namespace | `use` target not found in namespace |
| E121 | Namespace | Namespace not found in `use` or qualified access |
| E123 | Namespace | File-level `namespace` must appear before other items |

## Warnings

| Code | Phase | Description |
|------|-------|-------------|
| W001 | Scope | Shadowing warning |
| W002 | Scope | Unused variable |
| W003 | Merge | Inline args mismatch in partial merge |

## Diagnostic Output Format

WCL diagnostics use a Rust-style format with source spans:

```
error[E070]: missing required field `port`
  --> config.wcl:12:3
   |
12 |   service svc-api {
   |   ^^^^^^^^^^^^^^^ missing field `port`
   |
   = required by schema `ServiceSchema`

warning[W001]: `port` shadows outer binding
  --> config.wcl:18:7
   |
18 |   let port = 9090
   |       ^^^^ shadows binding at config.wcl:3:5
```

Each diagnostic includes:

- **Severity** — `error` or `warning`
- **Code** — e.g. `[E070]`
- **Message** — human-readable description
- **Location** — file path, line, and column (`file:line:col`)
- **Source snippet** — the relevant source lines with a caret pointing to the problem
- **Notes** — optional additional context (prefixed with `=`)

## Using `--strict`

Running `wcl validate --strict` promotes all warnings to errors. This is useful in CI pipelines where zero warnings are required.

```bash
wcl validate --strict config.wcl
echo $?  # 0 if no errors or warnings; 1 if any
```
