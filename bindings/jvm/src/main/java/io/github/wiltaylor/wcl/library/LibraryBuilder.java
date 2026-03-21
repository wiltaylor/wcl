package io.github.wiltaylor.wcl.library;

import java.util.ArrayList;
import java.util.List;

public final class LibraryBuilder {
    private final String name;
    private final List<String> schemaTexts = new ArrayList<>();
    private final List<FunctionStub> stubs = new ArrayList<>();

    public LibraryBuilder(String name) {
        this.name = name;
    }

    public LibraryBuilder addSchemaText(String text) {
        schemaTexts.add(text);
        return this;
    }

    public LibraryBuilder addFunctionStub(FunctionStub stub) {
        stubs.add(stub);
        return this;
    }

    public String build() {
        var sb = new StringBuilder();
        for (var schema : schemaTexts) {
            sb.append(schema);
            if (!schema.endsWith("\n")) sb.append('\n');
        }
        for (var stub : stubs) {
            sb.append(stub.toWcl());
        }
        return sb.toString();
    }

    public String install() {
        return LibraryManager.install(name, build());
    }
}
