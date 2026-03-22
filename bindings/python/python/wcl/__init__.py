import json
import os

from wcl._types import BlockRef, Decorator, Diagnostic, Document
from wcl._wasm_runtime import WasmRuntime
from wcl._callback import set_functions, clear_functions

__all__ = [
    "parse",
    "parse_file",
    "Document",
    "BlockRef",
    "Decorator",
    "Diagnostic",
]


def parse(source, *, root_dir=None, allow_imports=None, max_import_depth=None,
          max_macro_depth=None, max_loop_depth=None, max_iterations=None,
          functions=None, variables=None, lib_paths=None,
          no_default_lib_paths=None):
    """Parse a WCL source string and return a Document."""
    options = {}
    if root_dir is not None:
        options["rootDir"] = str(root_dir)
    if allow_imports is not None:
        options["allowImports"] = allow_imports
    if max_import_depth is not None:
        options["maxImportDepth"] = max_import_depth
    if max_macro_depth is not None:
        options["maxMacroDepth"] = max_macro_depth
    if max_loop_depth is not None:
        options["maxLoopDepth"] = max_loop_depth
    if max_iterations is not None:
        options["maxIterations"] = max_iterations
    if variables is not None:
        options["variables"] = variables
    if lib_paths is not None:
        options["libPaths"] = [str(p) for p in lib_paths]
    if no_default_lib_paths is not None:
        options["noDefaultLibPaths"] = no_default_lib_paths

    options_json = json.dumps(options) if options else None
    runtime = WasmRuntime.get()

    if functions:
        set_functions(functions)
        try:
            func_names_json = json.dumps(list(functions.keys()))
            handle = runtime.parse_with_functions(source, options_json, func_names_json)
        finally:
            clear_functions()
    else:
        handle = runtime.parse(source, options_json)

    return Document(handle)


def parse_file(path, **kwargs):
    """Parse a WCL file and return a Document."""
    path = str(path)
    try:
        with open(path) as f:
            source = f.read()
    except OSError as e:
        raise IOError(f"{path}: {e}") from e

    if "root_dir" not in kwargs:
        parent = os.path.dirname(os.path.abspath(path))
        kwargs["root_dir"] = parent

    return parse(source, **kwargs)
