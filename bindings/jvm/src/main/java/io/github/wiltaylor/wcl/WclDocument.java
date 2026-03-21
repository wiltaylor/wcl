package io.github.wiltaylor.wcl;

import io.github.wiltaylor.wcl.core.Diagnostic;
import io.github.wiltaylor.wcl.eval.BlockRef;
import io.github.wiltaylor.wcl.eval.WclValue;
import io.github.wiltaylor.wcl.wasm.JsonConvert;
import io.github.wiltaylor.wcl.wasm.WasmRuntime;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.stream.Collectors;

public final class WclDocument implements AutoCloseable {

    private int handle;
    private boolean closed;
    private final Object lock = new Object();

    private LinkedHashMap<String, WclValue> cachedValues;
    private List<Diagnostic> cachedDiagnostics;

    WclDocument(int handle) {
        this.handle = handle;
    }

    public LinkedHashMap<String, WclValue> getValues() {
        synchronized (lock) {
            checkClosed();
            if (cachedValues == null) {
                try {
                    var json = WasmRuntime.getInstance().documentValues(handle);
                    var node = JsonConvert.MAPPER.readTree(json);
                    cachedValues = JsonConvert.toValues(node);
                } catch (Exception e) {
                    throw new WclException("failed to read document values", e);
                }
            }
            return cachedValues;
        }
    }

    public List<Diagnostic> getDiagnostics() {
        synchronized (lock) {
            checkClosed();
            if (cachedDiagnostics == null) {
                try {
                    var json = WasmRuntime.getInstance().documentDiagnostics(handle);
                    var node = JsonConvert.MAPPER.readTree(json);
                    cachedDiagnostics = new ArrayList<>();
                    for (var el : node) {
                        cachedDiagnostics.add(JsonConvert.toDiagnostic(el));
                    }
                } catch (Exception e) {
                    throw new WclException("failed to read diagnostics", e);
                }
            }
            return cachedDiagnostics;
        }
    }

    public boolean hasErrors() {
        synchronized (lock) {
            checkClosed();
            return WasmRuntime.getInstance().documentHasErrors(handle);
        }
    }

    public List<Diagnostic> getErrors() {
        return getDiagnostics().stream().filter(Diagnostic::isError).collect(Collectors.toList());
    }

    public WclValue query(String query) {
        synchronized (lock) {
            checkClosed();
            try {
                var resultJson = WasmRuntime.getInstance().documentQuery(handle, query);
                var node = JsonConvert.MAPPER.readTree(resultJson);
                if (node.has("error")) {
                    throw new WclException("query error: " + node.get("error").textValue());
                }
                if (node.has("ok")) {
                    return JsonConvert.toWclValue(node.get("ok"));
                }
                throw new WclException("unexpected query result format");
            } catch (WclException e) {
                throw e;
            } catch (Exception e) {
                throw new WclException("query failed", e);
            }
        }
    }

    public List<BlockRef> getBlocks() {
        synchronized (lock) {
            checkClosed();
            try {
                var json = WasmRuntime.getInstance().documentBlocks(handle);
                var node = JsonConvert.MAPPER.readTree(json);
                var result = new ArrayList<BlockRef>();
                for (var el : node) result.add(JsonConvert.toBlockRef(el));
                return result;
            } catch (Exception e) {
                throw new WclException("failed to read blocks", e);
            }
        }
    }

    public List<BlockRef> getBlocksOfType(String kind) {
        synchronized (lock) {
            checkClosed();
            try {
                var json = WasmRuntime.getInstance().documentBlocksOfType(handle, kind);
                var node = JsonConvert.MAPPER.readTree(json);
                var result = new ArrayList<BlockRef>();
                for (var el : node) result.add(JsonConvert.toBlockRef(el));
                return result;
            } catch (Exception e) {
                throw new WclException("failed to read blocks of type: " + kind, e);
            }
        }
    }

    private void checkClosed() {
        if (closed) throw new IllegalStateException("WclDocument is closed");
    }

    @Override
    public void close() {
        synchronized (lock) {
            if (closed) return;
            closed = true;
            if (handle != 0) {
                WasmRuntime.getInstance().documentFree(handle);
                handle = 0;
            }
        }
    }

}
