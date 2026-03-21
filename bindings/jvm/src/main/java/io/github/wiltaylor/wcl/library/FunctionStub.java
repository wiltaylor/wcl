package io.github.wiltaylor.wcl.library;

import java.util.List;
import java.util.stream.Collectors;

public final class FunctionStub {
    private final String name;
    private final List<Param> params;
    private final String returnType;
    private final String doc;

    public FunctionStub(String name, List<Param> params, String returnType, String doc) {
        this.name = name;
        this.params = params;
        this.returnType = returnType;
        this.doc = doc;
    }

    public FunctionStub(String name, List<Param> params) {
        this(name, params, null, null);
    }

    public String toWcl() {
        var paramStr = params.stream()
                .map(p -> p.name() + ": " + p.type())
                .collect(Collectors.joining(", "));
        var ret = returnType != null ? " -> " + returnType : "";
        return "declare " + name + "(" + paramStr + ")" + ret + "\n";
    }

    public record Param(String name, String type) {}
}
