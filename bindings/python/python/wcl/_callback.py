"""Callback bridge for custom functions invoked from WASM."""

import json
import threading

_local = threading.local()


def set_functions(fn_dict):
    """Register Python functions for the current thread."""
    _local.functions = fn_dict


def clear_functions():
    """Clear registered functions for the current thread."""
    _local.functions = None


def invoke(name, args_json):
    """Invoke a registered function by name with JSON-encoded args.

    Returns (success: bool, result_json: str).
    """
    functions = getattr(_local, "functions", None)
    if functions is None or name not in functions:
        return (False, f"callback not found: {name}")

    try:
        args = json.loads(args_json)
        py_args = [_json_to_py(a) for a in args]
        result = functions[name](py_args)
        result_json = json.dumps(_py_to_json(result))
        return (True, result_json)
    except Exception as e:
        return (False, str(e))


def _json_to_py(val):
    """Convert a JSON value to a Python value for function arguments."""
    if val is None:
        return None
    if isinstance(val, bool):
        return val
    if isinstance(val, (int, float)):
        return val
    if isinstance(val, str):
        return val
    if isinstance(val, list):
        return [_json_to_py(v) for v in val]
    if isinstance(val, dict):
        return {k: _json_to_py(v) for k, v in val.items()}
    return val


def _py_to_json(val):
    """Convert a Python value to a JSON-serializable value for function return."""
    if val is None:
        return None
    if isinstance(val, bool):
        return val
    if isinstance(val, int):
        return val
    if isinstance(val, float):
        return val
    if isinstance(val, str):
        return val
    if isinstance(val, list):
        return [_py_to_json(v) for v in val]
    if isinstance(val, dict):
        return {k: _py_to_json(v) for k, v in val.items()}
    if isinstance(val, set):
        return {"__type": "set", "items": [_py_to_json(v) for v in val]}
    return val
