"""JSON-to-Python value conversion for WCL WASM binding."""

import json


def json_to_py(val):
    """Convert a parsed JSON value to a Python native value."""
    if val is None:
        return None
    if isinstance(val, bool):
        return val
    if isinstance(val, int):
        return val
    if isinstance(val, float):
        # Preserve ints that JSON parsed as float
        if val == int(val) and not isinstance(val, bool):
            return int(val)
        return val
    if isinstance(val, str):
        return val
    if isinstance(val, list):
        return [json_to_py(v) for v in val]
    if isinstance(val, dict):
        # Check for set encoding
        if val.get("__type") == "set" and "items" in val:
            items = [json_to_py(v) for v in val["items"]]
            try:
                return set(items)
            except TypeError:
                return items
        # Check for block ref encoding
        if "kind" in val and ("attributes" in val or "children" in val or "decorators" in val):
            return _json_to_block_ref(val)
        return {k: json_to_py(v) for k, v in val.items()}
    return val


def json_to_values(json_str):
    """Parse a JSON string and convert to a Python dict of values."""
    data = json.loads(json_str)
    return {k: json_to_py(v) for k, v in data.items()}


def json_to_blocks(json_str):
    """Parse a JSON array string and convert to BlockRef objects."""
    from wcl._types import BlockRef
    data = json.loads(json_str)
    return [_json_to_block_ref(b) for b in data]


def json_to_diagnostics(json_str):
    """Parse a JSON array string and convert to Diagnostic objects."""
    from wcl._types import Diagnostic
    data = json.loads(json_str)
    return [
        Diagnostic(
            severity=d["severity"],
            message=d["message"],
            code=d.get("code"),
        )
        for d in data
    ]


def _json_to_block_ref(obj):
    """Convert a JSON block ref object to a BlockRef."""
    from wcl._types import BlockRef, Decorator
    attrs_raw = obj.get("attributes", {})
    attributes = {k: json_to_py(v) for k, v in attrs_raw.items()}
    children_raw = obj.get("children", [])
    children = [_json_to_block_ref(c) for c in children_raw]
    decorators_raw = obj.get("decorators", [])
    decorators = [
        Decorator(
            name=d["name"],
            args={k: json_to_py(v) for k, v in d.get("args", {}).items()},
        )
        for d in decorators_raw
    ]
    return BlockRef(
        kind=obj["kind"],
        id=obj.get("id"),
        labels=obj.get("labels", []),
        attributes=attributes,
        children=children,
        decorators=decorators,
    )
