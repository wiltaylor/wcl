package io.github.wiltaylor.wcl.wasm;

import com.dylibso.chicory.runtime.ExportFunction;
import com.dylibso.chicory.runtime.HostFunction;
import com.dylibso.chicory.runtime.ImportFunction;
import com.dylibso.chicory.runtime.ImportValues;
import com.dylibso.chicory.runtime.Instance;
import com.dylibso.chicory.runtime.Memory;
import com.dylibso.chicory.wasm.Parser;
import com.dylibso.chicory.wasm.types.ValueType;
import com.dylibso.chicory.wasi.WasiOptions;
import com.dylibso.chicory.wasi.WasiPreview1;
import io.github.wiltaylor.wcl.WclException;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

public final class WasmRuntime {
    private static final WasmRuntime INSTANCE = new WasmRuntime();

    public static WasmRuntime getInstance() { return INSTANCE; }

    private final Instance instance;
    private final Object lock = new Object();

    // Cached export references
    private final ExportFunction alloc;
    private final ExportFunction dealloc;
    private final ExportFunction parse;
    private final ExportFunction parseWithFunctions;
    private final ExportFunction documentFree;
    private final ExportFunction documentValues;
    private final ExportFunction documentHasErrors;
    private final ExportFunction documentDiagnostics;
    private final ExportFunction documentQuery;
    private final ExportFunction documentBlocks;
    private final ExportFunction documentBlocksOfType;
    private final ExportFunction stringFree;

    private WasmRuntime() {
        try {
            var wasmBytes = loadEmbeddedWasm();
            var module = Parser.parse(wasmBytes);

            var wasi = WasiPreview1.builder()
                    .withOptions(WasiOptions.builder().build())
                    .build();

            var hostCallFunction = new HostFunction(
                    "env",
                    "host_call_function",
                    List.of(ValueType.I32, ValueType.I32, ValueType.I32,
                            ValueType.I32, ValueType.I32, ValueType.I32),
                    List.of(ValueType.I32),
                    (inst, args) -> {
                        var memory = inst.memory();
                        int namePtr = (int) args[0];
                        int nameLen = (int) args[1];
                        int argsPtr = (int) args[2];
                        int argsLen = (int) args[3];
                        int resultPtrOut = (int) args[4];
                        int resultLenOut = (int) args[5];

                        var nameBytes = memory.readBytes(namePtr, nameLen);
                        var name = new String(nameBytes, StandardCharsets.UTF_8);

                        var argsBytes = memory.readBytes(argsPtr, argsLen);
                        var argsJson = new String(argsBytes, StandardCharsets.UTF_8);

                        var result = WasmCallbackBridge.invoke(name, argsJson);

                        if (result.resultJson() != null) {
                            var resultBytes = result.resultJson().getBytes(StandardCharsets.UTF_8);
                            var allocFn = inst.export("wcl_wasm_alloc");
                            var ptr = (int) allocFn.apply(resultBytes.length)[0];

                            memory.write(ptr, resultBytes);
                            memory.writeI32(resultPtrOut, ptr);
                            memory.writeI32(resultLenOut, resultBytes.length);
                        }

                        return new long[] { result.success() ? 0 : -1 };
                    }
            );

            // Combine WASI host functions with our custom host function
            var wasiHostFunctions = wasi.toHostFunctions();
            var allFunctions = new ArrayList<ImportFunction>(Arrays.asList(wasiHostFunctions));
            allFunctions.add(hostCallFunction);

            var imports = ImportValues.builder()
                    .withFunctions(allFunctions)
                    .build();

            instance = Instance.builder(module)
                    .withImportValues(imports)
                    .build();

            alloc = instance.export("wcl_wasm_alloc");
            dealloc = instance.export("wcl_wasm_dealloc");
            parse = instance.export("wcl_wasm_parse");
            parseWithFunctions = instance.export("wcl_wasm_parse_with_functions");
            documentFree = instance.export("wcl_wasm_document_free");
            documentValues = instance.export("wcl_wasm_document_values");
            documentHasErrors = instance.export("wcl_wasm_document_has_errors");
            documentDiagnostics = instance.export("wcl_wasm_document_diagnostics");
            documentQuery = instance.export("wcl_wasm_document_query");
            documentBlocks = instance.export("wcl_wasm_document_blocks");
            documentBlocksOfType = instance.export("wcl_wasm_document_blocks_of_type");
            stringFree = instance.export("wcl_wasm_string_free");

        } catch (IOException e) {
            throw new WclException("failed to load WASM module", e);
        }
    }

    private static byte[] loadEmbeddedWasm() throws IOException {
        var stream = WasmRuntime.class.getResourceAsStream(
                "/io/github/wiltaylor/wcl/wasm/wcl_wasm.wasm");
        if (stream == null) {
            throw new IOException("embedded WASM resource not found");
        }
        try (stream) {
            return stream.readAllBytes();
        }
    }

    private Memory getMemory() {
        return instance.memory();
    }

    int writeString(String s) {
        if (s == null) return 0;
        var bytes = s.getBytes(StandardCharsets.UTF_8);
        var ptr = (int) alloc.apply(bytes.length + 1)[0];
        var memory = getMemory();
        memory.write(ptr, bytes);
        memory.write(ptr + bytes.length, new byte[] { 0 }, 0, 1); // null terminator
        return ptr;
    }

    String readCString(int ptr) {
        if (ptr == 0) return "";
        var memory = getMemory();
        int end = ptr;
        while (memory.read(end) != 0) end++;
        var bytes = memory.readBytes(ptr, end - ptr);
        return new String(bytes, StandardCharsets.UTF_8);
    }

    String consumeString(int ptr) {
        if (ptr == 0) return "";
        var s = readCString(ptr);
        stringFree.apply(ptr);
        return s;
    }

    // Public API

    public int parse(String source, String optionsJson) {
        synchronized (lock) {
            var srcPtr = writeString(source);
            var optsPtr = writeString(optionsJson);
            try {
                return (int) parse.apply(srcPtr, optsPtr)[0];
            } finally {
                if (srcPtr != 0) dealloc.apply(srcPtr, source.getBytes(StandardCharsets.UTF_8).length + 1);
                if (optsPtr != 0 && optionsJson != null)
                    dealloc.apply(optsPtr, optionsJson.getBytes(StandardCharsets.UTF_8).length + 1);
            }
        }
    }

    public int parseWithFunctions(String source, String optionsJson, String funcNamesJson) {
        synchronized (lock) {
            var srcPtr = writeString(source);
            var optsPtr = writeString(optionsJson);
            var namesPtr = writeString(funcNamesJson);
            try {
                return (int) parseWithFunctions.apply(srcPtr, optsPtr, namesPtr)[0];
            } finally {
                if (srcPtr != 0) dealloc.apply(srcPtr, source.getBytes(StandardCharsets.UTF_8).length + 1);
                if (optsPtr != 0 && optionsJson != null)
                    dealloc.apply(optsPtr, optionsJson.getBytes(StandardCharsets.UTF_8).length + 1);
                if (namesPtr != 0)
                    dealloc.apply(namesPtr, funcNamesJson.getBytes(StandardCharsets.UTF_8).length + 1);
            }
        }
    }

    public void documentFree(int handle) {
        synchronized (lock) { documentFree.apply(handle); }
    }

    public String documentValues(int handle) {
        synchronized (lock) {
            var ptr = (int) documentValues.apply(handle)[0];
            return consumeString(ptr);
        }
    }

    public boolean documentHasErrors(int handle) {
        synchronized (lock) { return documentHasErrors.apply(handle)[0] != 0; }
    }

    public String documentDiagnostics(int handle) {
        synchronized (lock) {
            var ptr = (int) documentDiagnostics.apply(handle)[0];
            return consumeString(ptr);
        }
    }

    public String documentQuery(int handle, String query) {
        synchronized (lock) {
            var qPtr = writeString(query);
            try {
                var ptr = (int) documentQuery.apply(handle, qPtr)[0];
                return consumeString(ptr);
            } finally {
                if (qPtr != 0) dealloc.apply(qPtr, query.getBytes(StandardCharsets.UTF_8).length + 1);
            }
        }
    }

    public String documentBlocks(int handle) {
        synchronized (lock) {
            var ptr = (int) documentBlocks.apply(handle)[0];
            return consumeString(ptr);
        }
    }

    public String documentBlocksOfType(int handle, String kind) {
        synchronized (lock) {
            var kPtr = writeString(kind);
            try {
                var ptr = (int) documentBlocksOfType.apply(handle, kPtr)[0];
                return consumeString(ptr);
            } finally {
                if (kPtr != 0) dealloc.apply(kPtr, kind.getBytes(StandardCharsets.UTF_8).length + 1);
            }
        }
    }
}
