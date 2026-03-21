using System;
using System.IO;
using System.Text;
using Wasmtime;

namespace Wcl.Wasm
{
    internal sealed class WasmRuntime : IDisposable
    {
        private static readonly Lazy<WasmRuntime> _instance = new Lazy<WasmRuntime>(() => new WasmRuntime());
        internal static WasmRuntime Instance => _instance.Value;

        private readonly Engine _engine;
        private readonly Module _module;
        private readonly Linker _linker;
        private Store _store;
        private Instance? _instance_wasm;
        private readonly object _lock = new object();

        // Cached function references
        private Func<int, int>? _alloc;
        private Action<int, int>? _dealloc;
        private Func<int, int, int>? _parse;
        private Func<int, int, int, int>? _parseWithFunctions;
        private Action<int>? _documentFree;
        private Func<int, int>? _documentValues;
        private Func<int, int>? _documentHasErrors;
        private Func<int, int>? _documentDiagnostics;
        private Func<int, int, int>? _documentQuery;
        private Func<int, int>? _documentBlocks;
        private Func<int, int, int>? _documentBlocksOfType;
        private Action<int>? _stringFree;

        private WasmRuntime()
        {
            _engine = new Engine();
            var wasmBytes = LoadEmbeddedWasm();
            _module = Module.FromBytes(_engine, "wcl_wasm", wasmBytes);
            _linker = new Linker(_engine);
            _linker.DefineWasi();
            DefineHostFunctions();
            _store = new Store(_engine);
            _store.SetWasiConfiguration(new WasiConfiguration());
            _instance_wasm = _linker.Instantiate(_store, _module);
            CacheFunctions();
        }

        private static byte[] LoadEmbeddedWasm()
        {
            var assembly = typeof(WasmRuntime).Assembly;
            var resourceName = "Wcl.Wasm.wcl_wasm.wasm";

            using var stream = assembly.GetManifestResourceStream(resourceName);
            if (stream == null)
            {
                // Fall back to checking all resource names
                var names = assembly.GetManifestResourceNames();
                foreach (var name in names)
                {
                    if (name.EndsWith("wcl_wasm.wasm"))
                    {
                        using var s = assembly.GetManifestResourceStream(name);
                        if (s != null)
                        {
                            using var ms2 = new MemoryStream();
                            s.CopyTo(ms2);
                            return ms2.ToArray();
                        }
                    }
                }
                throw new FileNotFoundException(
                    $"Embedded WASM resource not found. Available: [{string.Join(", ", names)}]");
            }

            using var ms = new MemoryStream();
            stream.CopyTo(ms);
            return ms.ToArray();
        }

        private void DefineHostFunctions()
        {
            _linker.DefineFunction("env", "host_call_function",
                (Caller caller, int namePtr, int nameLen, int argsPtr, int argsLen, int resultPtrOut, int resultLenOut) =>
                {
                    var memory = caller.GetMemory("memory");
                    if (memory == null) return -1;

                    var nameBytes = memory.GetSpan(namePtr, nameLen);
                    var name = Encoding.UTF8.GetString(nameBytes);

                    var argsBytes = memory.GetSpan(argsPtr, argsLen);
                    var argsJson = Encoding.UTF8.GetString(argsBytes);

                    var (success, resultJson) = WasmCallbackBridge.Invoke(name, argsJson);

                    if (resultJson != null)
                    {
                        var resultBytes = Encoding.UTF8.GetBytes(resultJson);
                        var allocFn = caller.GetFunction("wcl_wasm_alloc");
                        if (allocFn == null) return -1;
                        var ptr = (int)allocFn.Invoke(resultBytes.Length)!;

                        var dest = memory.GetSpan(ptr, resultBytes.Length);
                        resultBytes.CopyTo(dest);

                        // Write pointer and length to output params
                        var memSpan = memory.GetSpan<byte>(0, (int)memory.GetLength());
                        BitConverter.TryWriteBytes(memSpan.Slice(resultPtrOut), ptr);
                        BitConverter.TryWriteBytes(memSpan.Slice(resultLenOut), resultBytes.Length);
                    }

                    return success ? 0 : -1;
                });
        }

        private void CacheFunctions()
        {
            _alloc = _instance_wasm!.GetFunction<int, int>("wcl_wasm_alloc");
            _dealloc = _instance_wasm.GetAction<int, int>("wcl_wasm_dealloc");
            _parse = _instance_wasm.GetFunction<int, int, int>("wcl_wasm_parse");
            _parseWithFunctions = _instance_wasm.GetFunction<int, int, int, int>("wcl_wasm_parse_with_functions");
            _documentFree = _instance_wasm.GetAction<int>("wcl_wasm_document_free");
            _documentValues = _instance_wasm.GetFunction<int, int>("wcl_wasm_document_values");
            _documentHasErrors = _instance_wasm.GetFunction<int, int>("wcl_wasm_document_has_errors");
            _documentDiagnostics = _instance_wasm.GetFunction<int, int>("wcl_wasm_document_diagnostics");
            _documentQuery = _instance_wasm.GetFunction<int, int, int>("wcl_wasm_document_query");
            _documentBlocks = _instance_wasm.GetFunction<int, int>("wcl_wasm_document_blocks");
            _documentBlocksOfType = _instance_wasm.GetFunction<int, int, int>("wcl_wasm_document_blocks_of_type");
            _stringFree = _instance_wasm.GetAction<int>("wcl_wasm_string_free");
        }

        private Memory GetMemory()
        {
            return _instance_wasm!.GetMemory("memory")
                ?? throw new InvalidOperationException("WASM memory not found");
        }

        internal int WriteString(string? s)
        {
            if (s == null) return 0;
            var bytes = Encoding.UTF8.GetBytes(s);
            var ptr = _alloc!(bytes.Length + 1);
            var memory = GetMemory();
            var dest = memory.GetSpan(ptr, bytes.Length + 1);
            bytes.CopyTo(dest);
            dest[bytes.Length] = 0; // null terminator
            return ptr;
        }

        internal string ReadCString(int ptr)
        {
            if (ptr == 0) return "";
            var memory = GetMemory();
            var span = memory.GetSpan<byte>(0, (int)memory.GetLength());
            int start = ptr;
            int end = start;
            while (end < span.Length && span[end] != 0) end++;
            return Encoding.UTF8.GetString(span.Slice(start, end - start));
        }

        internal string ConsumeString(int ptr)
        {
            if (ptr == 0) return "";
            var s = ReadCString(ptr);
            _stringFree!(ptr);
            return s;
        }

        internal void FreeWasmString(int ptr)
        {
            if (ptr != 0) _stringFree!(ptr);
        }

        // ── Public API ──────────────────────────────────────────────────────

        internal int Parse(string source, string? optionsJson)
        {
            lock (_lock)
            {
                var srcPtr = WriteString(source);
                var optsPtr = WriteString(optionsJson);
                try
                {
                    return _parse!(srcPtr, optsPtr);
                }
                finally
                {
                    if (srcPtr != 0) _dealloc!(srcPtr, Encoding.UTF8.GetByteCount(source) + 1);
                    if (optsPtr != 0 && optionsJson != null) _dealloc!(optsPtr, Encoding.UTF8.GetByteCount(optionsJson) + 1);
                }
            }
        }

        internal int ParseWithFunctions(string source, string? optionsJson, string funcNamesJson)
        {
            lock (_lock)
            {
                var srcPtr = WriteString(source);
                var optsPtr = WriteString(optionsJson);
                var namesPtr = WriteString(funcNamesJson);
                try
                {
                    return _parseWithFunctions!(srcPtr, optsPtr, namesPtr);
                }
                finally
                {
                    if (srcPtr != 0) _dealloc!(srcPtr, Encoding.UTF8.GetByteCount(source) + 1);
                    if (optsPtr != 0 && optionsJson != null) _dealloc!(optsPtr, Encoding.UTF8.GetByteCount(optionsJson) + 1);
                    if (namesPtr != 0) _dealloc!(namesPtr, Encoding.UTF8.GetByteCount(funcNamesJson) + 1);
                }
            }
        }

        internal void DocumentFree(int handle)
        {
            lock (_lock) { _documentFree!(handle); }
        }

        internal string DocumentValues(int handle)
        {
            lock (_lock)
            {
                var ptr = _documentValues!(handle);
                return ConsumeString(ptr);
            }
        }

        internal bool DocumentHasErrors(int handle)
        {
            lock (_lock) { return _documentHasErrors!(handle) != 0; }
        }

        internal string DocumentDiagnostics(int handle)
        {
            lock (_lock)
            {
                var ptr = _documentDiagnostics!(handle);
                return ConsumeString(ptr);
            }
        }

        internal string DocumentQuery(int handle, string query)
        {
            lock (_lock)
            {
                var qPtr = WriteString(query);
                try
                {
                    var ptr = _documentQuery!(handle, qPtr);
                    return ConsumeString(ptr);
                }
                finally
                {
                    if (qPtr != 0) _dealloc!(qPtr, Encoding.UTF8.GetByteCount(query) + 1);
                }
            }
        }

        internal string DocumentBlocks(int handle)
        {
            lock (_lock)
            {
                var ptr = _documentBlocks!(handle);
                return ConsumeString(ptr);
            }
        }

        internal string DocumentBlocksOfType(int handle, string kind)
        {
            lock (_lock)
            {
                var kPtr = WriteString(kind);
                try
                {
                    var ptr = _documentBlocksOfType!(handle, kPtr);
                    return ConsumeString(ptr);
                }
                finally
                {
                    if (kPtr != 0) _dealloc!(kPtr, Encoding.UTF8.GetByteCount(kind) + 1);
                }
            }
        }

        public void Dispose()
        {
            _instance_wasm = null;
            _store?.Dispose();
            _module?.Dispose();
            _engine?.Dispose();
        }
    }
}
