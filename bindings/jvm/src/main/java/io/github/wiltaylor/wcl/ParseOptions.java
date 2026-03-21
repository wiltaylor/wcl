package io.github.wiltaylor.wcl;

import io.github.wiltaylor.wcl.eval.WclValue;
import io.github.wiltaylor.wcl.wasm.JsonConvert;

import java.util.ArrayList;
import java.util.Map;
import java.util.function.Function;

public final class ParseOptions {
    private String rootDir;
    private Boolean allowImports;
    private Integer maxImportDepth;
    private Integer maxMacroDepth;
    private Integer maxLoopDepth;
    private Integer maxIterations;
    private Map<String, Function<WclValue[], WclValue>> functions;

    public ParseOptions rootDir(String rootDir) { this.rootDir = rootDir; return this; }
    public ParseOptions allowImports(boolean allowImports) { this.allowImports = allowImports; return this; }
    public ParseOptions maxImportDepth(int maxImportDepth) { this.maxImportDepth = maxImportDepth; return this; }
    public ParseOptions maxMacroDepth(int maxMacroDepth) { this.maxMacroDepth = maxMacroDepth; return this; }
    public ParseOptions maxLoopDepth(int maxLoopDepth) { this.maxLoopDepth = maxLoopDepth; return this; }
    public ParseOptions maxIterations(int maxIterations) { this.maxIterations = maxIterations; return this; }
    public ParseOptions functions(Map<String, Function<WclValue[], WclValue>> functions) {
        this.functions = functions;
        return this;
    }

    public String getRootDir() { return rootDir; }
    public Boolean getAllowImports() { return allowImports; }
    public Integer getMaxImportDepth() { return maxImportDepth; }
    public Integer getMaxMacroDepth() { return maxMacroDepth; }
    public Integer getMaxLoopDepth() { return maxLoopDepth; }
    public Integer getMaxIterations() { return maxIterations; }
    public Map<String, Function<WclValue[], WclValue>> getFunctions() { return functions; }

    String toJson() {
        var parts = new ArrayList<String>();
        try {
            if (rootDir != null)
                parts.add("\"rootDir\":" + JsonConvert.MAPPER.writeValueAsString(rootDir));
            if (allowImports != null)
                parts.add("\"allowImports\":" + (allowImports ? "true" : "false"));
            if (maxImportDepth != null)
                parts.add("\"maxImportDepth\":" + maxImportDepth);
            if (maxMacroDepth != null)
                parts.add("\"maxMacroDepth\":" + maxMacroDepth);
            if (maxLoopDepth != null)
                parts.add("\"maxLoopDepth\":" + maxLoopDepth);
            if (maxIterations != null)
                parts.add("\"maxIterations\":" + maxIterations);
        } catch (Exception e) {
            throw new WclException("failed to serialize parse options", e);
        }
        if (parts.isEmpty()) return null;
        return "{" + String.join(",", parts) + "}";
    }
}
