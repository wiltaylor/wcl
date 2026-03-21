"""Pure Python types for WCL binding."""

import json

from wcl._convert import json_to_blocks, json_to_diagnostics, json_to_py, json_to_values
from wcl._wasm_runtime import WasmRuntime


class Document:
    """A parsed and evaluated WCL document."""

    def __init__(self, handle):
        self._handle = handle
        self._values = None
        self._diagnostics = None

    @property
    def values(self):
        if self._values is None:
            json_str = WasmRuntime.get().document_values(self._handle)
            self._values = json_to_values(json_str)
        return self._values

    @property
    def has_errors(self):
        return WasmRuntime.get().document_has_errors(self._handle)

    @property
    def errors(self):
        return [d for d in self.diagnostics if d.severity == "error"]

    @property
    def diagnostics(self):
        if self._diagnostics is None:
            json_str = WasmRuntime.get().document_diagnostics(self._handle)
            self._diagnostics = json_to_diagnostics(json_str)
        return self._diagnostics

    def query(self, query_str):
        json_str = WasmRuntime.get().document_query(self._handle, query_str)
        result = json.loads(json_str)
        if "error" in result:
            raise ValueError(result["error"])
        return json_to_py(result["ok"])

    def blocks(self):
        json_str = WasmRuntime.get().document_blocks(self._handle)
        return json_to_blocks(json_str)

    def blocks_of_type(self, kind):
        json_str = WasmRuntime.get().document_blocks_of_type(self._handle, kind)
        return json_to_blocks(json_str)

    def __del__(self):
        if hasattr(self, "_handle") and self._handle:
            try:
                WasmRuntime.get().document_free(self._handle)
            except Exception:
                pass


class BlockRef:
    """A reference to a WCL block with its attributes."""

    def __init__(self, kind, id=None, attributes=None, children=None, decorators=None):
        self.kind = kind
        self.id = id
        self.attributes = attributes or {}
        self.children = children or []
        self.decorators = decorators or []

    def has_decorator(self, name):
        return any(d.name == name for d in self.decorators)

    def get(self, key):
        return self.attributes.get(key)

    def __repr__(self):
        if self.id:
            return f"BlockRef({self.kind} {self.id})"
        return f"BlockRef({self.kind})"

    def __eq__(self, other):
        if isinstance(other, list) and len(other) == 0 and not isinstance(self, list):
            return False
        return NotImplemented


class Decorator:
    """A WCL decorator with name and arguments."""

    def __init__(self, name, args=None):
        self.name = name
        self.args = args or {}

    def __repr__(self):
        return f"Decorator(@{self.name})"


class Diagnostic:
    """A WCL diagnostic (error, warning, etc.)."""

    def __init__(self, severity, message, code=None):
        self.severity = severity
        self.message = message
        self.code = code

    def __repr__(self):
        if self.code:
            return f"Diagnostic({self.severity}: [{self.code}] {self.message})"
        return f"Diagnostic({self.severity}: {self.message})"
