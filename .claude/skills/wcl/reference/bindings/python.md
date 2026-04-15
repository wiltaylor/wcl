# Python Binding (`pywcl`)

Pure-Python wrapper over the embedded WASM module via `wasmtime`. Source: `bindings/python/python/wcl/__init__.py`, `_types.py`.

## Install

```bash
pip install pywcl         # pypi package name
```

Requires Python ≥ 3.9. Dependency: `wasmtime>=29`.

Import as `wcl`:

```python
import wcl
```

## Minimal Example

```python
import wcl

src = '''
server web {
    host = "0.0.0.0"
    port = 8080
}
'''

doc = wcl.parse(src)
if doc.has_errors:
    for d in doc.errors:
        print(d)
else:
    print(doc.values)                    # { "server": {...} }
    for block in doc.blocks():
        print(block.kind, block.id)
```

File:

```python
doc = wcl.parse_file("./config.wcl")     # root_dir defaults to the file's parent
```

## Core API

Module-level (`wcl.__init__`, line 8):

| Function | Signature |
|----------|-----------|
| `parse(source, *, root_dir=None, allow_imports=None, max_import_depth=None, max_macro_depth=None, max_loop_depth=None, max_iterations=None, functions=None, variables=None, lib_paths=None, no_default_lib_paths=None)` | Parse and evaluate a string |
| `parse_file(path, **kwargs)` | Read file, default `root_dir` to parent |

`Document` (`_types.py:9`):

| Member | Kind | Description |
|--------|------|-------------|
| `values` | property | Evaluated top-level values (dict) |
| `has_errors` | property | `bool` |
| `errors` | property | list of `Diagnostic` with `severity == "error"` |
| `diagnostics` | property | All diagnostics |
| `query(query_str)` | method | Execute a WCL query; raises `ValueError` on error |
| `blocks()` | method | `list[BlockRef]` top-level |
| `blocks_of_type(kind)` | method | Filtered list |
| `__del__` | Auto-frees the underlying handle |

`BlockRef`: `kind`, `id`, `attributes` (dict), `children` (list), `decorators` (list). Helpers: `has_decorator(name)`, `get(key)`.

`Diagnostic`: `severity`, `message`, `code`.

## Custom Functions

```python
def upper_rev(args):
    s = args[0]
    return s.upper()[::-1]

doc = wcl.parse('x = upper_rev("hi")', functions={"upper_rev": upper_rev})
```

Function receives Python-native args, returns any JSON-serializable value.

## Variables

```python
doc = wcl.parse('port = PORT', variables={"PORT": 8080})
```

`variables` override top-level `let` bindings of the same name.

## Value Type Mapping

| WCL | Python |
|-----|--------|
| string | `str` |
| int | `int` |
| float | `float` |
| bool | `bool` |
| null | `None` |
| list | `list` |
| map | `dict` |
| block | `BlockRef` (in `blocks()` result) / `dict` (in `values`) |
| date | ISO `str` |
| duration | ISO `str` |
| symbol | `str` (prefixed `:`) |

## Error Handling

- `wcl.parse_file` raises `IOError` for missing/unreadable files.
- `wcl.parse` never raises for WCL errors — always check `doc.has_errors` and iterate `doc.errors`.
- `doc.query(...)` raises `ValueError` with the error message if the query fails.

## Gotchas

- Import path is `wcl`, not `pywcl`. PyPI name is `pywcl`, module name is `wcl`.
- Custom functions must be set before parse; they're installed globally on the WASM instance for the duration of the call and cleared after.
- Large int values (> 2^53) may lose precision when they round-trip through WASM JSON encoding.
