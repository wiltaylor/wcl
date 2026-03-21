package io.github.wiltaylor.wcl.wasm;

import io.github.wiltaylor.wcl.eval.WclValue;

import java.util.Map;
import java.util.function.Function;

public final class WasmCallbackBridge {
    private static final ThreadLocal<Map<String, Function<WclValue[], WclValue>>> FUNCTIONS =
            new ThreadLocal<>();

    private WasmCallbackBridge() {}

    public static void setFunctions(Map<String, Function<WclValue[], WclValue>> functions) {
        FUNCTIONS.set(functions);
    }

    public static void clearFunctions() {
        FUNCTIONS.remove();
    }

    public static CallResult invoke(String name, String argsJson) {
        var functions = FUNCTIONS.get();
        if (functions == null || !functions.containsKey(name)) {
            return new CallResult(false, "callback not found: " + name);
        }

        try {
            var argsNode = JsonConvert.MAPPER.readTree(argsJson);

            var args = new WclValue[argsNode.size()];
            for (int i = 0; i < argsNode.size(); i++) {
                args[i] = JsonConvert.toWclValue(argsNode.get(i));
            }

            var result = functions.get(name).apply(args);
            var resultJson = JsonConvert.wclValueToJson(result);
            return new CallResult(true, resultJson);
        } catch (Exception ex) {
            return new CallResult(false, ex.getMessage());
        }
    }

    public record CallResult(boolean success, String resultJson) {}
}
