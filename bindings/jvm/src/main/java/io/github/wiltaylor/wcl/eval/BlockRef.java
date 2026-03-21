package io.github.wiltaylor.wcl.eval;

import java.util.LinkedHashMap;
import java.util.List;

public final class BlockRef {
    private final String kind;
    private final String id;
    private final LinkedHashMap<String, WclValue> attributes;
    private final List<BlockRef> children;
    private final List<DecoratorValue> decorators;

    public BlockRef(String kind, String id,
                    LinkedHashMap<String, WclValue> attributes,
                    List<BlockRef> children, List<DecoratorValue> decorators) {
        this.kind = kind;
        this.id = id;
        this.attributes = attributes;
        this.children = children;
        this.decorators = decorators;
    }

    public String getKind() { return kind; }
    public String getId() { return id; }
    public LinkedHashMap<String, WclValue> getAttributes() { return attributes; }
    public List<BlockRef> getChildren() { return children; }
    public List<DecoratorValue> getDecorators() { return decorators; }

    public WclValue get(String key) {
        return attributes.get(key);
    }

    public boolean hasDecorator(String name) {
        return decorators.stream().anyMatch(d -> d.name().equals(name));
    }

    public DecoratorValue getDecorator(String name) {
        return decorators.stream()
                .filter(d -> d.name().equals(name))
                .findFirst()
                .orElse(null);
    }
}
