# Reflection builtins

Reflection builtins read a type's schema back at evaluation time — used by libraries (like wdoc) that dispatch on a block's declared kind, and to auto-generate documentation that can't drift from the implementation.

| Function | Use |
| --- | --- |
| `type_fields(T)` | The fields of type `T` (name, type, …) as records |
| `fn_signature(name)` | A builtin's signature, params, and return-value docs |
| `decl_info(T)` | Declaration metadata — block kind, `is_document`, name |
| `builtin_names()` | The names of every builtin function |
| `child_types(T)` | The element types of `T`'s `@child`/`@children` slots |
| `decorator_names(T)` | The names of the decorators on declaration `T` |
| `doc_comment(T)` | The doc comment above a type or one of its fields |

## Related

- [Schema & Decorators](../references/concept_schema.md)

- [Namespaces](../references/concept_namespaces.md)

[← All facts](../references/facts_ref.md)
