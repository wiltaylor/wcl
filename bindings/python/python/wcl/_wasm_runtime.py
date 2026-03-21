"""Low-level WASM runtime wrapper using wasmtime-py."""

import threading
from pathlib import Path

from wasmtime import Engine, Linker, Module, Store, WasiConfig

from wcl import _callback


class WasmRuntime:
    """Singleton WASM runtime that loads and manages the wcl_wasm module."""

    _instance = None
    _init_lock = threading.Lock()

    @classmethod
    def get(cls):
        if cls._instance is None:
            with cls._init_lock:
                if cls._instance is None:
                    cls._instance = cls()
        return cls._instance

    def __init__(self):
        self._lock = threading.Lock()
        self._engine = Engine()

        wasm_path = Path(__file__).parent / "wcl_wasm.wasm"
        self._module = Module.from_file(self._engine, str(wasm_path))

        self._linker = Linker(self._engine)
        self._linker.define_wasi()
        self._define_host_functions()

        self._store = Store(self._engine)
        self._store.set_wasi(WasiConfig())
        self._instance = self._linker.instantiate(self._store, self._module)

        # Cache exported functions
        self._alloc = self._instance.exports(self._store)["wcl_wasm_alloc"]
        self._dealloc = self._instance.exports(self._store)["wcl_wasm_dealloc"]
        self._string_free = self._instance.exports(self._store)["wcl_wasm_string_free"]
        self._parse = self._instance.exports(self._store)["wcl_wasm_parse"]
        self._parse_with_functions = self._instance.exports(self._store)["wcl_wasm_parse_with_functions"]
        self._doc_free = self._instance.exports(self._store)["wcl_wasm_document_free"]
        self._doc_values = self._instance.exports(self._store)["wcl_wasm_document_values"]
        self._doc_has_errors = self._instance.exports(self._store)["wcl_wasm_document_has_errors"]
        self._doc_diagnostics = self._instance.exports(self._store)["wcl_wasm_document_diagnostics"]
        self._doc_query = self._instance.exports(self._store)["wcl_wasm_document_query"]
        self._doc_blocks = self._instance.exports(self._store)["wcl_wasm_document_blocks"]
        self._doc_blocks_of_type = self._instance.exports(self._store)["wcl_wasm_document_blocks_of_type"]
        self._memory = self._instance.exports(self._store)["memory"]

    def _define_host_functions(self):
        def host_call_function(caller, name_ptr, name_len, args_ptr, args_len, result_ptr_out, result_len_out):
            import ctypes

            memory = caller.get("memory")
            if memory is None:
                return -1

            mem = memory.data_ptr(caller)
            name = bytes(mem[name_ptr:name_ptr + name_len]).decode("utf-8")
            args_json = bytes(mem[args_ptr:args_ptr + args_len]).decode("utf-8")

            success, result_json = _callback.invoke(name, args_json)

            if result_json is not None:
                result_bytes = result_json.encode("utf-8")
                alloc_fn = caller.get("wcl_wasm_alloc")
                ptr = alloc_fn(caller, len(result_bytes))

                # Re-fetch data_ptr after alloc (memory may have grown)
                mem = memory.data_ptr(caller)
                for i, b in enumerate(result_bytes):
                    mem[ptr + i] = b

                # Write pointer and length as i32 LE via ctypes
                ptr_val = ctypes.c_int32(ptr)
                len_val = ctypes.c_int32(len(result_bytes))
                ctypes.memmove(ctypes.addressof(mem.contents) + result_ptr_out, ctypes.byref(ptr_val), 4)
                ctypes.memmove(ctypes.addressof(mem.contents) + result_len_out, ctypes.byref(len_val), 4)

            return 0 if success else -1

        from wasmtime import FuncType, ValType
        func_type = FuncType(
            [ValType.i32(), ValType.i32(), ValType.i32(), ValType.i32(), ValType.i32(), ValType.i32()],
            [ValType.i32()],
        )
        self._linker.define_func("env", "host_call_function", func_type, host_call_function, access_caller=True)

    def _get_memory_data(self):
        return self._memory.data_ptr(self._store)

    def write_string(self, s):
        """Write a string into WASM memory, returning its pointer."""
        if s is None:
            return 0
        encoded = s.encode("utf-8")
        ptr = self._alloc(self._store, len(encoded) + 1)
        mem = self._get_memory_data()
        for i, b in enumerate(encoded):
            mem[ptr + i] = b
        mem[ptr + len(encoded)] = 0  # null terminator
        return ptr

    def read_c_string(self, ptr):
        """Read a null-terminated C string from WASM memory."""
        if ptr == 0:
            return ""
        mem = self._get_memory_data()
        end = ptr
        while mem[end] != 0:
            end += 1
        return bytes(mem[ptr:end]).decode("utf-8")

    def consume_string(self, ptr):
        """Read a C string and then free it in WASM."""
        if ptr == 0:
            return ""
        s = self.read_c_string(ptr)
        self._string_free(self._store, ptr)
        return s

    # ── Public API ──────────────────────────────────────────────────────

    def parse(self, source, options_json):
        with self._lock:
            src_ptr = self.write_string(source)
            opts_ptr = self.write_string(options_json)
            try:
                return self._parse(self._store, src_ptr, opts_ptr)
            finally:
                if src_ptr:
                    self._dealloc(self._store, src_ptr, len(source.encode("utf-8")) + 1)
                if opts_ptr and options_json:
                    self._dealloc(self._store, opts_ptr, len(options_json.encode("utf-8")) + 1)

    def parse_with_functions(self, source, options_json, func_names_json):
        with self._lock:
            src_ptr = self.write_string(source)
            opts_ptr = self.write_string(options_json)
            names_ptr = self.write_string(func_names_json)
            try:
                return self._parse_with_functions(self._store, src_ptr, opts_ptr, names_ptr)
            finally:
                if src_ptr:
                    self._dealloc(self._store, src_ptr, len(source.encode("utf-8")) + 1)
                if opts_ptr and options_json:
                    self._dealloc(self._store, opts_ptr, len(options_json.encode("utf-8")) + 1)
                if names_ptr:
                    self._dealloc(self._store, names_ptr, len(func_names_json.encode("utf-8")) + 1)

    def document_free(self, handle):
        with self._lock:
            self._doc_free(self._store, handle)

    def document_values(self, handle):
        with self._lock:
            ptr = self._doc_values(self._store, handle)
            return self.consume_string(ptr)

    def document_has_errors(self, handle):
        with self._lock:
            return self._doc_has_errors(self._store, handle) != 0

    def document_diagnostics(self, handle):
        with self._lock:
            ptr = self._doc_diagnostics(self._store, handle)
            return self.consume_string(ptr)

    def document_query(self, handle, query):
        with self._lock:
            q_ptr = self.write_string(query)
            try:
                ptr = self._doc_query(self._store, handle, q_ptr)
                return self.consume_string(ptr)
            finally:
                if q_ptr:
                    self._dealloc(self._store, q_ptr, len(query.encode("utf-8")) + 1)

    def document_blocks(self, handle):
        with self._lock:
            ptr = self._doc_blocks(self._store, handle)
            return self.consume_string(ptr)

    def document_blocks_of_type(self, handle, kind):
        with self._lock:
            k_ptr = self.write_string(kind)
            try:
                ptr = self._doc_blocks_of_type(self._store, handle, k_ptr)
                return self.consume_string(ptr)
            finally:
                if k_ptr:
                    self._dealloc(self._store, k_ptr, len(kind.encode("utf-8")) + 1)
