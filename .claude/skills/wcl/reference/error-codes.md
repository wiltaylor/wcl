# Error Codes

Source of truth: `docs/appendix-error-codes.wcl`. All codes currently emitted by the WCL pipeline, grouped by phase.

Run with `--strict` to promote warnings to errors.

## Errors

### Parse
| Code | Description |
|------|-------------|
| E002 | Unexpected token |
| E003 | Unterminated string |
| E102 | Duplicate symbol_set name |
| E103 | Duplicate symbol within a symbol_set |

### Import
| Code | Description |
|------|-------------|
| E010 | File not found |
| E011 | Jail escape (path outside root) |
| E013 | Remote import forbidden |
| E014 | Max import depth exceeded |
| E015 | Library not found in search paths |
| E016 | Glob pattern matched no files (non-optional import) |
| E017 | Lazy import requires a namespace path |

### Macro
| Code | Description |
|------|-------------|
| E020 | Undefined macro |
| E021 | Recursive macro expansion |
| E022 | Max expansion depth exceeded (default 64) |
| E023 | Wrong macro kind (function vs attribute) |
| E024 | Parameter type mismatch |

### Control Flow
| Code | Description |
|------|-------------|
| E025 | For-loop iterable is not a list |
| E026 | If/else condition is not bool |
| E027 | Invalid expanded identifier |
| E028 | Max iteration count exceeded |
| E029 | Max nesting depth exceeded |
| E105 | For-loop iterable could not be resolved |

### Merge
| Code | Description |
|------|-------------|
| E030 | Duplicate ID (non-partial) |
| E031 | Attribute conflict in partial merge |
| E032 | Kind mismatch in partial merge |
| E033 | Mixed partial / non-partial |
| E038 | Partial let value must be a list |
| E039 | Let declared both partial and non-partial |

### Export
| Code | Description |
|------|-------------|
| E034 | Duplicate exported variable name |
| E035 | Re-export of undefined name |
| E036 | Export inside block |

### Scope
| Code | Description |
|------|-------------|
| E040 | Undefined reference |
| E041 | Cyclic dependency |

### Eval
| Code | Description |
|------|-------------|
| E050 | Type error in expression |
| E051 | Division by zero |
| E052 | Unknown function |
| E053 | Declared-but-unregistered function (library `declare`) |
| E054 | Index out of bounds |

### Decorator
| Code | Description |
|------|-------------|
| E060 | Unknown decorator |
| E061 | Invalid target (decorator applied to wrong kind) |
| E062 | Missing required parameter |
| E063 | Parameter type mismatch |
| E064 | Constraint violation (AnyOf / AllOf / OneOf / Requires) |

### Schema
| Code | Description |
|------|-------------|
| E001 | Duplicate schema name |
| E070 | Missing required field |
| E071 | Attribute type mismatch |
| E072 | Unknown attribute (closed schema) |
| E073 | Validate constraint violation |
| E074 | Ref target not found |
| E093 | Block uses text block syntax but schema has no `@text` field |
| E094 | `@text` field validation errors |
| E095 | Child not allowed by parent's `@children` |
| E096 | Item not allowed by its own `@parent` |
| E097 | Child count below `@child` minimum |
| E098 | Child count above `@child` maximum |
| E099 | Self-nesting exceeds `@child max_depth` |
| E100 | Symbol value not in declared symbol_set |
| E101 | Referenced symbol_set does not exist |
| E104 | `@embedded_lsp` on non-string field or empty language |

### Validation
| Code | Description |
|------|-------------|
| E080 | Document validation failed |

### Table
| Code | Description |
|------|-------------|
| E090 | `@table_index` references nonexistent column |
| E091 | Duplicate value in unique table index |
| E092 | Inline columns defined when schema is applied |

### Namespace
| Code | Description |
|------|-------------|
| E120 | `use` target not found in namespace |
| E121 | Namespace not found in `use` or qualified access |
| E123 | File-level namespace must appear before other items |

## Warnings

| Code | Phase | Description |
|------|-------|-------------|
| W001 | Scope | Shadowing warning |
| W002 | Scope | Unused variable |
| W003 | Merge | Inline args mismatch in partial merge |

## Diagnostic Output

Rust-style with source spans, e.g.:

```
error[E070]: missing required field `port`
  --> config.wcl:12:3
   |
12 |   service svc-api {
   |   ^^^^^^^^^^^^^^^ missing field `port`
   |
   = required by schema `ServiceSchema`
```
