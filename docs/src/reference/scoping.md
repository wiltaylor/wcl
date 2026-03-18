# Scoping Rules

This page describes how WCL constructs scopes, resolves names, and orders evaluation.

## Scope Kinds

### Module Scope

The top level of a WCL document forms the module scope. It contains:

- All top-level `let` bindings
- All top-level attributes
- All block declarations (accessible by type and ID)
- All imported names (merged from `import` directives)
- All exported names

Module scope is the root of the scope tree.

### Block Scope

Each block (`service`, `database`, etc.) creates a child scope. Block scopes:

- Inherit all names from the enclosing module scope (or parent block scope)
- Define their own attributes and let bindings, which are local to the block
- Can reference names from any enclosing scope

Blocks can be nested. Inner blocks have access to all names in all enclosing scopes.

```wcl
let base_port = 8000

service svc-api {
  // base_port is visible here from module scope
  port = base_port + 80

  let path_prefix = "/api"

  endpoint health {
    // both base_port and path_prefix are visible here
    path = path_prefix + "/health"
  }
}
```

### Macro Scope

When a macro is called, a new scope is created for the macro body. This scope:

- Contains the macro parameters bound to the call arguments
- Does not inherit from the call site scope
- Has read access to the module scope at the point of definition

Macro expansion happens before scope construction for the main document, so macro-generated items are treated as if they were written directly in the source.

## Name Resolution Algorithm

When an identifier `x` is referenced in an expression:

1. Search the current (innermost) scope for a binding named `x`.
2. If not found, walk up to the parent scope and repeat.
3. Continue until the module scope is reached.
4. If still not found, produce error E040 (undefined reference).

First match wins. The search is purely lexical (static scoping).

## Shadowing

A name in an inner scope may shadow a name with the same identifier in an outer scope. This is permitted but produces warning W001 to alert the author.

```wcl
let port = 8080

service svc-api {
  let port = 9090  // W001: shadows outer `port`
  exposed_port = port  // resolves to 9090
}
```

## Dependency-Ordered Evaluation

Within a scope, WCL does not require declarations to appear in evaluation order. Instead:

1. The evaluator builds a dependency graph by inspecting which names each expression references.
2. The graph is topologically sorted.
3. Expressions are evaluated in dependency order.

This means forward references are fully supported:

```wcl
service svc-api {
  base_url = "http://${host}:${port}"
  port     = 8080
  host     = "localhost"
}
```

`base_url` is evaluated after `host` and `port` regardless of their textual order.

If a cycle exists in the dependency graph, error E041 is produced for all names involved in the cycle.

## Import Merging and Scope

`import` directives are resolved before scope construction. The imported document's top-level items are merged into the current module scope. Imported names are visible throughout the entire importing document, including in items that textually precede the import declaration.

Name conflicts between an imported document and the importing document are resolved in favour of the importing document (local definitions win).

## Export Visibility

`export let name = expr` and `export name` make names available to documents that import this file. Exported names are part of the module's public interface.

Exports are only permitted at the top level. Exporting a name from inside a block produces error E036.

Duplicate export names produce error E034. Exporting an undefined name produces error E035.

## Unused Variables

A `let` binding that is defined but never referenced produces warning W002. This warning is suppressed for exported names, since they may be consumed by other documents.
