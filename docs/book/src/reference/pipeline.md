# Evaluation Pipeline

WCL processes a document through eleven sequential phases. Each phase transforms or validates the document before passing it to the next. Understanding the pipeline is useful when interpreting error messages and when reasoning about what is possible at each stage.

## Overview

```
Source text
    │
    ▼
 1. Parse              → AST
    │
    ▼
 2. Macro collection   → MacroRegistry
    │
    ▼
 3. Import resolution  → merged AST
    │
    ▼
 4. Macro expansion    → expanded AST
    │
    ▼
 5. Control flow       → flattened AST
    │
    ▼
 6. Partial merge      → unified AST
    │
    ▼
 7. Scope construction → ScopeArena + dependency graph
    │
    ▼
 8. Expression eval    → resolved values
    │
    ▼
 9. Decorator valid.   → (errors or ok)
    │
    ▼
10. Schema validation  → (errors or ok)
    │
    ▼
11. Document valid.    → final Document
```

## Phase 1: Parse

**Input**: raw source text
**Output**: Abstract Syntax Tree (AST)

The lexer tokenizes the source, and the parser produces an unambiguous AST. This phase handles:

- All syntactic constructs: blocks, attributes, let bindings, expressions, macros, schemas, tables, etc.
- Comments and trivia (preserved for formatting)
- Span information for every node (for diagnostics and LSP)

Errors here are E001 (syntax error), E002 (unexpected token), E003 (unterminated string).

**Rationale**: parsing first gives a clean representation to work with. No evaluation can occur until syntax is confirmed valid.

## Phase 2: Macro Collection

**Input**: AST
**Output**: MacroRegistry

All macro definitions (`macro name(...)`) are collected and registered before any expansion occurs. Both function macros and attribute macros are collected.

**Rationale**: macros must be fully registered before expansion so that call sites that appear before the definition in textual order can still be expanded. This enables any-order macro definitions, consistent with WCL's general any-order philosophy.

## Phase 3: Import Resolution

**Input**: AST with `import` directives
**Output**: merged AST

Each `import "path"` directive is resolved to a file, parsed, and its top-level items are merged into the importing document's AST. Imports are processed recursively. The same file is not re-parsed if it has already been imported (import deduplication).

Errors: E010 (file not found), E011 (path escapes jail), E013 (remote import), E014 (max depth exceeded).

**Rationale**: imports are resolved before macro expansion so that macros imported from other files are available during expansion.

## Phase 4: Macro Expansion

**Input**: merged AST + MacroRegistry
**Output**: expanded AST (no macro calls remain)

Every macro call site is replaced with the expanded body of the macro, with parameters substituted. Attribute macros (`@macro_name`) are applied to their annotated items, injecting, setting, or removing attributes per their transform body.

Errors: E020 (undefined macro), E021 (recursive expansion), E022 (max depth exceeded), E023 (wrong macro kind), E024 (parameter type mismatch).

**Rationale**: macro expansion must happen before control flow so that macros can generate `for` loops and `if` blocks, and before scope construction so that generated items participate in normal scoping.

## Phase 5: Control Flow Expansion

**Input**: expanded AST
**Output**: flattened AST (no `for`/`if` constructs remain)

`for` loops are unrolled: the loop body is repeated once per element in the iterable, with the loop variable substituted. `if`/`else` chains are evaluated and only the selected branch is retained.

Errors: E025 (non-list iterable), E026 (non-bool condition), E027 (invalid expanded identifier), E028 (max iteration count), E029 (max nesting depth).

**Rationale**: control flow must be resolved before partial merge and scope construction so that all generated blocks and attributes are known as concrete AST nodes before name resolution.

## Phase 6: Partial Merge

**Input**: flattened AST
**Output**: unified AST (partial blocks merged)

Blocks declared as `partial` with the same type and ID are merged into a single block. Attributes from each partial declaration are combined. The order of attributes follows declaration order across partials.

Errors: E030 (duplicate non-partial ID), E031 (attribute conflict), E032 (kind mismatch), E033 (mixed partial/non-partial). Warning W003 (label mismatch).

**Rationale**: partial merging happens after control flow (which can generate partial blocks) and before scope construction so that the merged block is the single entity that participates in scope and evaluation.

## Phase 7: Scope Construction

**Input**: unified AST
**Output**: ScopeArena (scope tree) + dependency graph

A scope tree is built: one module scope plus one child scope per block. For each scope, all defined names are recorded. A dependency graph is constructed by inspecting which names each expression references.

Errors: E040 (undefined reference), E041 (cyclic dependency), E034/E035/E036 (export errors). Warning W001 (shadowing), W002 (unused variable).

**Rationale**: scope construction is separated from evaluation so that the full dependency graph can be built before any expression is evaluated, enabling correct topological ordering.

## Phase 8: Expression Evaluation

**Input**: ScopeArena + dependency graph
**Output**: resolved values for all attributes and let bindings

Expressions are evaluated in topological order (dependencies before dependents) within each scope. Query expressions (`query(...)`) are evaluated against the partially-resolved document. Ref expressions (`ref(id)`) are resolved to their target values.

Errors: E050 (type error), E051 (division by zero), E052 (unknown function), E053 (ref not found), E054 (index out of bounds).

**Rationale**: topo-sorted evaluation ensures each expression is evaluated only after its dependencies are known, without requiring the author to declare them in order.

## Phase 9: Decorator Validation

**Input**: resolved document + decorator schemas
**Output**: (errors or ok)

Each decorator applied to a block, attribute, table, or schema is validated against its decorator schema (if one is registered). Required parameters are checked, types are verified, and constraint rules are applied.

Errors: E060 (unknown decorator), E061 (invalid target), E062 (missing required parameter), E063 (parameter type mismatch), E064 (constraint violation).

**Rationale**: decorator validation happens after full evaluation so that decorator argument values (which may be expressions) are fully resolved before checking.

## Phase 10: Schema Validation

**Input**: resolved document + schema definitions
**Output**: (errors or ok)

Every block that is annotated with a schema reference is validated against that schema. Field presence, types, and constraints (min, max, pattern, one_of, ref) are checked.

Errors: E070 (missing required field), E071 (type mismatch), E072 (unknown attribute in closed schema), E073 (constraint violation), E074 (ref target not found).

**Rationale**: schema validation is the final structural check. It happens after evaluation because schema constraints may involve evaluated expressions (e.g., computed field values).

## Phase 11: Document Validation

**Input**: resolved document + validation blocks
**Output**: final Document

Each `validation` block is executed: its `check` expression is evaluated in the context of the resolved document, and if it returns `false`, the `message` expression is evaluated and reported as an error (E080).

**Rationale**: document-level validation is the last phase because it can express cross-cutting invariants over the fully-evaluated document — things that cannot be expressed in per-block schemas alone.
