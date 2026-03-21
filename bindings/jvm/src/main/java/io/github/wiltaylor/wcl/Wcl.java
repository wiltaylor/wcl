package io.github.wiltaylor.wcl;

import io.github.wiltaylor.wcl.eval.WclValue;
import io.github.wiltaylor.wcl.serde.WclDeserializer;
import io.github.wiltaylor.wcl.serde.WclSerializer;
import io.github.wiltaylor.wcl.wasm.JsonConvert;
import io.github.wiltaylor.wcl.wasm.WasmCallbackBridge;
import io.github.wiltaylor.wcl.wasm.WasmRuntime;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.stream.Collectors;

public final class Wcl {
    private Wcl() {}

    public static WclDocument parse(String source) {
        return parse(source, null);
    }

    public static WclDocument parse(String source, ParseOptions options) {
        var optsJson = options != null ? options.toJson() : null;

        if (options != null && options.getFunctions() != null && !options.getFunctions().isEmpty()) {
            return parseWithFunctions(source, optsJson, options);
        }

        var handle = WasmRuntime.getInstance().parse(source, optsJson);
        if (handle == 0) throw new WclException("wcl: parse returned invalid handle");
        return new WclDocument(handle);
    }

    public static WclDocument parseFile(String path) {
        return parseFile(path, null);
    }

    public static WclDocument parseFile(String path, ParseOptions options) {
        var filePath = Path.of(path);
        String source;
        try {
            source = Files.readString(filePath);
        } catch (IOException e) {
            throw new WclException("failed to read file: " + path, e);
        }

        var opts = options != null ? options : new ParseOptions();
        if (opts.getRootDir() == null) {
            var parent = filePath.toAbsolutePath().getParent();
            if (parent != null) opts.rootDir(parent.toString());
        }

        return parse(source, opts);
    }

    public static <T> T fromString(String source, Class<T> type) {
        return fromString(source, type, null);
    }

    public static <T> T fromString(String source, Class<T> type, ParseOptions options) {
        try (var doc = parse(source, options)) {
            if (doc.hasErrors()) {
                var msgs = doc.getErrors().stream()
                        .map(d -> d.message())
                        .collect(Collectors.joining("; "));
                throw new WclException("parse errors: " + msgs);
            }
            return WclDeserializer.fromValue(WclValue.ofMap(doc.getValues()), type);
        }
    }

    public static <T> String toString(T value) {
        return WclSerializer.serialize(value, false);
    }

    public static <T> String toStringPretty(T value) {
        return WclSerializer.serialize(value, true);
    }

    private static WclDocument parseWithFunctions(String source, String optsJson, ParseOptions options) {
        var funcNames = new ArrayList<>(options.getFunctions().keySet());
        String funcNamesJson;
        try {
            funcNamesJson = JsonConvert.MAPPER.writeValueAsString(funcNames);
        } catch (Exception e) {
            throw new WclException("failed to serialize function names", e);
        }

        WasmCallbackBridge.setFunctions(options.getFunctions());
        try {
            var handle = WasmRuntime.getInstance().parseWithFunctions(source, optsJson, funcNamesJson);
            if (handle == 0) throw new WclException("wcl: parse returned invalid handle");
            return new WclDocument(handle);
        } finally {
            WasmCallbackBridge.clearFunctions();
        }
    }
}
