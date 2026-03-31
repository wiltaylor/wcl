package io.github.wiltaylor.wcl.eval;

import java.util.*;

public final class WclValue {
    private final WclValueKind kind;
    private final String stringValue;
    private final long intValue;
    private final double floatValue;
    private final boolean boolValue;
    private final List<WclValue> listValue;
    private final LinkedHashMap<String, WclValue> mapValue;
    private final BlockRef blockRef;

    public static final WclValue NULL = new WclValue(WclValueKind.NULL, null, 0, 0, false, null, null, null);

    private WclValue(WclValueKind kind, String s, long i, double d, boolean b,
                     List<WclValue> list, LinkedHashMap<String, WclValue> map, BlockRef br) {
        this.kind = kind;
        this.stringValue = s;
        this.intValue = i;
        this.floatValue = d;
        this.boolValue = b;
        this.listValue = list;
        this.mapValue = map;
        this.blockRef = br;
    }

    // Factory methods
    public static WclValue ofString(String value) {
        return new WclValue(WclValueKind.STRING, value, 0, 0, false, null, null, null);
    }

    public static WclValue ofInt(long value) {
        return new WclValue(WclValueKind.INT, null, value, 0, false, null, null, null);
    }

    public static WclValue ofFloat(double value) {
        return new WclValue(WclValueKind.FLOAT, null, 0, value, false, null, null, null);
    }

    public static WclValue ofBool(boolean value) {
        return new WclValue(WclValueKind.BOOL, null, 0, 0, value, null, null, null);
    }

    public static WclValue ofList(List<WclValue> items) {
        return new WclValue(WclValueKind.LIST, null, 0, 0, false, items, null, null);
    }

    public static WclValue ofMap(LinkedHashMap<String, WclValue> map) {
        return new WclValue(WclValueKind.MAP, null, 0, 0, false, null, map, null);
    }

    public static WclValue ofSet(List<WclValue> items) {
        return new WclValue(WclValueKind.SET, null, 0, 0, false, items, null, null);
    }

    public static WclValue ofBlockRef(BlockRef blockRef) {
        return new WclValue(WclValueKind.BLOCK_REF, null, 0, 0, false, null, null, blockRef);
    }

    public static WclValue ofBigInt(long value) {
        return new WclValue(WclValueKind.BIG_INT, null, value, 0, false, null, null, null);
    }

    public static WclValue ofDate(String value) {
        return new WclValue(WclValueKind.DATE, value, 0, 0, false, null, null, null);
    }

    public static WclValue ofDuration(String value) {
        return new WclValue(WclValueKind.DURATION, value, 0, 0, false, null, null, null);
    }

    // Accessors
    public WclValueKind getKind() { return kind; }

    public String asString() {
        if (kind != WclValueKind.STRING) throw new IllegalStateException("expected string, got " + typeName());
        return stringValue;
    }

    public long asInt() {
        if (kind != WclValueKind.INT) throw new IllegalStateException("expected int, got " + typeName());
        return intValue;
    }

    public double asFloat() {
        if (kind != WclValueKind.FLOAT) throw new IllegalStateException("expected float, got " + typeName());
        return floatValue;
    }

    public boolean asBool() {
        if (kind != WclValueKind.BOOL) throw new IllegalStateException("expected bool, got " + typeName());
        return boolValue;
    }

    public List<WclValue> asList() {
        if (kind != WclValueKind.LIST) throw new IllegalStateException("expected list, got " + typeName());
        return listValue;
    }

    public LinkedHashMap<String, WclValue> asMap() {
        if (kind != WclValueKind.MAP) throw new IllegalStateException("expected map, got " + typeName());
        return mapValue;
    }

    public List<WclValue> asSet() {
        if (kind != WclValueKind.SET) throw new IllegalStateException("expected set, got " + typeName());
        return listValue;
    }

    public BlockRef asBlockRef() {
        if (kind != WclValueKind.BLOCK_REF) throw new IllegalStateException("expected block_ref, got " + typeName());
        return blockRef;
    }

    public long asBigInt() {
        if (kind != WclValueKind.BIG_INT) throw new IllegalStateException("expected bigint, got " + typeName());
        return intValue;
    }

    public String asDate() {
        if (kind != WclValueKind.DATE) throw new IllegalStateException("expected date, got " + typeName());
        return stringValue;
    }

    public String asDuration() {
        if (kind != WclValueKind.DURATION) throw new IllegalStateException("expected duration, got " + typeName());
        return stringValue;
    }

    // Try accessors
    public Optional<String> tryAsString() {
        return kind == WclValueKind.STRING ? Optional.of(stringValue) : Optional.empty();
    }

    public OptionalLong tryAsInt() {
        return kind == WclValueKind.INT ? OptionalLong.of(intValue) : OptionalLong.empty();
    }

    public OptionalDouble tryAsFloat() {
        return kind == WclValueKind.FLOAT ? OptionalDouble.of(floatValue) : OptionalDouble.empty();
    }

    public boolean isNull() {
        return kind == WclValueKind.NULL;
    }

    public String typeName() {
        return switch (kind) {
            case STRING -> "string";
            case INT -> "int";
            case FLOAT -> "float";
            case BOOL -> "bool";
            case NULL -> "null";
            case LIST -> "list";
            case MAP -> "map";
            case SET -> "set";
            case BLOCK_REF -> "block_ref";
            case BIG_INT -> "bigint";
            case DATE -> "date";
            case DURATION -> "duration";
        };
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) return true;
        if (!(obj instanceof WclValue other)) return false;
        if (kind != other.kind) return false;
        return switch (kind) {
            case STRING, DATE, DURATION -> Objects.equals(stringValue, other.stringValue);
            case INT, BIG_INT -> intValue == other.intValue;
            case FLOAT -> Double.compare(floatValue, other.floatValue) == 0;
            case BOOL -> boolValue == other.boolValue;
            case NULL -> true;
            case LIST, SET -> Objects.equals(listValue, other.listValue);
            case MAP -> Objects.equals(mapValue, other.mapValue);
            case BLOCK_REF -> Objects.equals(blockRef, other.blockRef);
        };
    }

    @Override
    public int hashCode() {
        return kind.hashCode();
    }

    @Override
    public String toString() {
        return switch (kind) {
            case STRING, DATE, DURATION -> stringValue;
            case INT, BIG_INT -> Long.toString(intValue);
            case FLOAT -> Double.toString(floatValue);
            case BOOL -> boolValue ? "true" : "false";
            case NULL -> "null";
            case LIST -> {
                var sb = new StringBuilder("[");
                for (int i = 0; i < listValue.size(); i++) {
                    if (i > 0) sb.append(", ");
                    sb.append(listValue.get(i));
                }
                sb.append(']');
                yield sb.toString();
            }
            case MAP -> {
                var sb = new StringBuilder("{");
                int idx = 0;
                for (var entry : mapValue.entrySet()) {
                    if (idx++ > 0) sb.append(", ");
                    sb.append(entry.getKey()).append(" = ").append(entry.getValue());
                }
                sb.append('}');
                yield sb.toString();
            }
            case SET -> {
                var sb = new StringBuilder("set(");
                for (int i = 0; i < listValue.size(); i++) {
                    if (i > 0) sb.append(", ");
                    sb.append(listValue.get(i));
                }
                sb.append(')');
                yield sb.toString();
            }
            case BLOCK_REF -> {
                var sb = new StringBuilder(blockRef.getKind());
                if (blockRef.getId() != null) sb.append(' ').append(blockRef.getId());
                sb.append(" {");
                int idx = 0;
                for (var entry : blockRef.getAttributes().entrySet()) {
                    if (idx++ > 0) sb.append(',');
                    sb.append(' ').append(entry.getKey()).append(" = ").append(entry.getValue());
                }
                sb.append(" }");
                yield sb.toString();
            }
        };
    }
}
